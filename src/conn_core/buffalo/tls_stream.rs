use crate::conn_core::connect_params::{ConnectParams, ServerCerts};
use rustls::{ClientConfig, ClientSession, Session};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use webpki::DNSNameRef;

pub struct TlsStream {
    is_handshaking: bool,
    tcpstream: TcpStream,
    tlsconfig: Arc<ClientConfig>,
    tlssession: Arc<Mutex<ClientSession>>,
}
impl TlsStream {
    pub fn try_new(params: &ConnectParams) -> std::io::Result<TlsStream> {
        let (tcpstream, tlsconfig, tlssession) = connect_tcp(params)?;
        Ok(TlsStream {
            is_handshaking: true,
            tcpstream,
            tlsconfig,
            tlssession: Arc::new(Mutex::new(tlssession)),
        })
    }
    pub fn try_clone(&self) -> std::io::Result<TlsStream> {
        Ok(TlsStream {
            is_handshaking: false,
            tcpstream: self.tcpstream.try_clone()?,
            tlsconfig: Arc::clone(&self.tlsconfig),
            tlssession: Arc::clone(&self.tlssession),
        })
    }
}

fn connect_tcp(
    params: &ConnectParams,
) -> std::io::Result<(TcpStream, Arc<ClientConfig>, ClientSession)> {
    debug!("connect_tcp(): Connecting to {:?}", params.addr());

    let tcpstream = TcpStream::connect(params.addr())?;

    trace!("tcpstream working");

    let mut config = ClientConfig::new();
    match params.server_certs() {
        None => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "No server certificates provided",
            ));
        }
        Some(ServerCerts::Direct(pem)) => {
            let mut cursor = std::io::Cursor::new(pem);
            let (n_ok, n_err) = config.root_store.add_pem_file(&mut cursor).unwrap();
            if n_ok == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "None of the provided server certificates was accepted",
                ));
            }
            if n_err > 0 {
                warn!("Not all provided server certificates were accepted");
            }
        }
        Some(ServerCerts::Environment(env_var)) => match std::env::var(env_var) {
            Ok(value) => {
                trace!("trying with env var {:?}", env_var);
                let mut cursor = std::io::Cursor::new(value);
                let (n_ok, n_err) = config.root_store.add_pem_file(&mut cursor).unwrap();
                if n_ok == 0 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "None of the provided server certificates was accepted",
                    ));
                }
                if n_err > 0 {
                    warn!("Not all provided server certificates were accepted");
                }
            }
            Err(e) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("Environment variable {} not found, reason: {}", env_var, e),
                ));
            }
        },
        Some(ServerCerts::Directory(trust_anchor_dir)) => {
            debug!("Trust anchor directory = {}", trust_anchor_dir);

            let trust_anchor_files: Vec<PathBuf> = std::fs::read_dir(trust_anchor_dir)?
                .filter_map(|r_dir_entry| r_dir_entry.ok())
                .filter(|dir_entry| {
                    dir_entry.file_type().is_ok() && dir_entry.file_type().unwrap().is_file()
                })
                .filter(|dir_entry| {
                    let path = dir_entry.path();
                    let ext = path.extension();
                    ext.is_some() && ext.unwrap() == "pem"
                })
                .map(|dir_entry| dir_entry.path())
                .collect();

            let mut t_ok = 0;
            let mut t_err = 0;
            for trust_anchor_file in trust_anchor_files {
                trace!("Trying trust anchor file {:?}", trust_anchor_file);
                let mut rd = std::io::BufReader::new(std::fs::File::open(trust_anchor_file)?);
                let (n_ok, n_err) = config.root_store.add_pem_file(&mut rd).map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "server certificates could not be parsed",
                    )
                })?;
                t_ok += n_ok;
                t_err += n_err;
            }
            if t_ok == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "None of the provided server certificates was accepted",
                ));
            }
            if t_err > 0 {
                warn!("Not all provided server certificates were accepted");
            }
        }
    }

    let tlsconfig = Arc::new(config);

    let tlssession = ClientSession::new(
        &tlsconfig,
        DNSNameRef::try_from_ascii_str(params.host()).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Cannot use {} for DNSNameRef", params.host()),
            )
        })?,
    );

    Ok((tcpstream, tlsconfig, tlssession))
}

impl std::io::Write for TlsStream {
    fn write(&mut self, request: &[u8]) -> std::io::Result<usize> {
        trace!(
            "std::io::Write::write() with request size {}",
            request.len()
        );
        let mut tlssession = self.tlssession.lock().unwrap();

        std::io::Write::write_all(&mut *tlssession, request)?;

        while tlssession.wants_write() {
            let count = tlssession.write_tls(&mut self.tcpstream)?;
            trace!("std::io::Write::write(): wrote tls ({})", count);
        }

        trace!("std::io::Write::write() done");
        Ok(request.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        trace!("std::io::Write::flush()");
        let mut tlssession = self.tlssession.lock().unwrap();

        loop {
            while tlssession.wants_write() {
                let count = tlssession.write_tls(&mut self.tcpstream)?;
                trace!("std::io::Write::flush(): wrote tls ({})", count);
            }

            if self.is_handshaking && !tlssession.is_handshaking() {
                self.is_handshaking = false;

                match tlssession.get_protocol_version() {
                    Some(protocol) => debug!("Protocol {:?} negotiated", protocol),
                    None => debug!("No TLS Protocol negotiated"),
                }
            }

            if tlssession.wants_read() {
                let count = tlssession.read_tls(&mut self.tcpstream)?;
                trace!("std::io::Write::flush(): read_tls() -> {}", count);
                if count == 0 {
                    break;
                }

                if let Err(err) = tlssession.process_new_packets() {
                    return Err(std::io::Error::new(std::io::ErrorKind::Other, err));
                }
            } else {
                break;
            }
        }

        trace!("std::io::Write::flush() done");
        Ok(())
    }
}

impl std::io::Read for TlsStream {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        trace!("std::io::Read::read() with buf size {}", buffer.len());
        let mut tlssession = self.tlssession.lock().unwrap();

        loop {
            if tlssession.wants_read() {
                let count = tlssession.read_tls(&mut self.tcpstream)?;
                trace!("transfer_read(): read_tls() -> {}", count);
                if count == 0 {
                    break;
                }

                if let Err(err) = tlssession.process_new_packets() {
                    return Err(std::io::Error::new(std::io::ErrorKind::Other, err));
                }
            } else {
                break;
            }
        }

        let read_bytes = tlssession.read(&mut buffer[..])?;
        trace!("std::io::Read::read() done");
        Ok(read_bytes)
    }
}

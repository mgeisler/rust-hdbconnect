use crate::conn_core::am_conn_core::AmConnCore;
use crate::conn_core::buffalo::Buffalo;
use crate::conn_core::connect_params::ConnectParams;
use crate::conn_core::initial_request;
use crate::conn_core::session_state::SessionState;
use crate::protocol::argument::Argument;
use crate::protocol::part::Part;
use crate::protocol::part::Parts;
use crate::protocol::partkind::PartKind;
use crate::protocol::parts::client_info::ClientInfo;
use crate::protocol::parts::connect_options::ConnectOptions;
use crate::protocol::parts::execution_result::ExecutionResult;
use crate::protocol::parts::parameter_descriptor::ParameterDescriptor;
use crate::protocol::parts::resultset::ResultSet;
use crate::protocol::parts::resultset_metadata::ResultSetMetadata;
use crate::protocol::parts::server_error::{ServerError, Severity};
use crate::protocol::parts::statement_context::StatementContext;
use crate::protocol::parts::topology::Topology;
use crate::protocol::parts::transactionflags::TransactionFlags;
use crate::protocol::reply::parse_message_and_sequence_header;
use crate::protocol::reply::Reply;
use crate::protocol::request::Request;
use crate::protocol::server_resource_consumption_info::ServerResourceConsumptionInfo;
use crate::{HdbError, HdbResult};
use std::cell::RefCell;
use std::io;
use std::mem;

pub const DEFAULT_FETCH_SIZE: u32 = 32;
pub const DEFAULT_LOB_READ_LENGTH: i32 = 1_000_000;

#[derive(Debug)]
pub(crate) struct ConnectionCore {
    authenticated: bool,
    session_id: i64,
    client_info: ClientInfo,
    client_info_touched: bool,
    seq_number: i32,
    auto_commit: bool,
    server_resource_consumption_info: ServerResourceConsumptionInfo,
    fetch_size: u32,
    lob_read_length: i32,
    session_state: SessionState,
    statement_sequence: Option<i64>, // statement sequence within the transaction
    connect_options: ConnectOptions,
    topology: Option<Topology>,
    pub warnings: Vec<ServerError>,
    buffalo: Buffalo,
}

impl<'a> ConnectionCore {
    pub fn try_new(params: ConnectParams) -> HdbResult<ConnectionCore> {
        let mut buffalo = Buffalo::try_new(params)?;
        initial_request::send_and_receive(&mut buffalo)?;

        Ok(ConnectionCore {
            authenticated: false,
            session_id: 0,
            seq_number: 0,
            auto_commit: true,
            server_resource_consumption_info: Default::default(),
            fetch_size: DEFAULT_FETCH_SIZE,
            lob_read_length: DEFAULT_LOB_READ_LENGTH,
            client_info: Default::default(),
            client_info_touched: false,
            session_state: Default::default(),
            statement_sequence: None,
            connect_options: Default::default(),
            topology: None,
            warnings: Vec::<ServerError>::new(),
            buffalo,
        })
    }

    pub fn set_application_version(&mut self, version: &str) -> HdbResult<()> {
        self.client_info.set_application_version(version);
        self.client_info_touched = true;
        Ok(())
    }

    pub fn set_application_source(&mut self, source: &str) -> HdbResult<()> {
        self.client_info.set_application_source(source);
        self.client_info_touched = true;
        Ok(())
    }

    pub fn set_application_user(&mut self, application_user: &str) -> HdbResult<()> {
        self.client_info.set_application_user(application_user);
        self.client_info_touched = true;
        Ok(())
    }

    pub fn is_client_info_touched(&self) -> bool {
        self.client_info_touched
    }
    pub fn get_client_info_for_sending(&mut self) -> ClientInfo {
        debug!("cloning client info for sending");
        self.client_info_touched = false;
        self.client_info.clone()
    }

    pub fn evaluate_statement_context(&mut self, stmt_ctx: &StatementContext) -> HdbResult<()> {
        trace!(
            "Received StatementContext with sequence_info = {:?}",
            stmt_ctx.get_statement_sequence_info()
        );
        self.set_statement_sequence(stmt_ctx.get_statement_sequence_info());
        self.server_resource_consumption_info.update(
            stmt_ctx.get_server_processing_time(),
            stmt_ctx.get_server_cpu_time(),
            stmt_ctx.get_server_memory_usage(),
        );
        // FIXME do not ignore the other content of StatementContext
        // StatementContextId::SchemaName => 3,
        // StatementContextId::FlagSet => 4,
        // StatementContextId::QueryTimeout => 5,
        // StatementContextId::ClientReconnectionWaitTimeout => 6,

        Ok(())
    }

    pub fn set_auto_commit(&mut self, ac: bool) {
        self.auto_commit = ac;
    }

    pub fn is_auto_commit(&self) -> bool {
        self.auto_commit
    }

    pub fn server_resource_consumption_info(&self) -> &ServerResourceConsumptionInfo {
        &self.server_resource_consumption_info
    }

    #[deprecated]
    pub fn get_server_proc_time(&self) -> i32 {
        self.server_resource_consumption_info.acc_server_proc_time
    }

    pub fn get_fetch_size(&self) -> u32 {
        self.fetch_size
    }

    pub fn set_fetch_size(&mut self, fetch_size: u32) {
        self.fetch_size = fetch_size;
    }

    pub fn get_lob_read_length(&self) -> i32 {
        self.lob_read_length
    }

    pub fn set_lob_read_length(&mut self, lob_read_length: i32) {
        self.lob_read_length = lob_read_length;
    }

    pub fn set_session_id(&mut self, session_id: i64) {
        self.session_id = session_id;
    }

    pub fn set_topology(&mut self, topology: Topology) {
        self.topology = Some(topology);
    }

    pub fn transfer_server_connect_options(&mut self, conn_opts: ConnectOptions) -> HdbResult<()> {
        self.connect_options
            .transfer_server_connect_options(conn_opts)
    }

    pub fn is_authenticated(&self) -> bool {
        self.authenticated
    }

    pub fn set_authenticated(&mut self, authenticated: bool) {
        self.authenticated = authenticated;
    }

    pub fn statement_sequence(&self) -> &Option<i64> {
        &self.statement_sequence
    }

    fn set_statement_sequence(&mut self, statement_sequence: Option<i64>) {
        self.statement_sequence = statement_sequence;
    }

    pub fn session_id(&self) -> i64 {
        self.session_id
    }

    fn reader(&self) -> &RefCell<io::BufRead> {
        self.buffalo.reader()
    }

    fn writer(&self) -> &RefCell<io::Write> {
        self.buffalo.writer()
    }

    pub fn next_seq_number(&mut self) -> i32 {
        self.seq_number += 1;
        self.seq_number
    }
    pub fn last_seq_number(&self) -> i32 {
        self.seq_number
    }

    pub fn evaluate_ta_flags(&mut self, ta_flags: TransactionFlags) -> HdbResult<()> {
        self.session_state.update(ta_flags);
        if self.session_state.dead {
            Err(HdbError::DbIssue(
                "SessionclosingTaError received".to_owned(),
            ))
        } else {
            Ok(())
        }
    }

    pub fn get_database_name(&self) -> &str {
        self.connect_options
            .get_database_name()
            .map(|s| s.as_ref())
            .unwrap_or("")
    }
    pub fn get_system_id(&self) -> &str {
        self.connect_options
            .get_system_id()
            .map(|s| s.as_ref())
            .unwrap_or("")
    }

    pub fn get_connection_id(&self) -> i32 {
        self.connect_options.get_connection_id().unwrap_or(-1)
    }

    pub fn get_full_version_string(&self) -> &str {
        self.connect_options
            .get_full_version_string()
            .map(|s| s.as_ref())
            .unwrap_or("")
    }

    pub fn pop_warnings(&mut self) -> HdbResult<Option<Vec<ServerError>>> {
        if self.warnings.is_empty() {
            Ok(None)
        } else {
            let mut v = Vec::<ServerError>::new();
            mem::swap(&mut v, &mut self.warnings);
            Ok(Some(v))
        }
    }

    pub fn roundtrip(
        &mut self,
        request: Request<'a>,
        am_conn_core: &AmConnCore,
        o_rs_md: Option<&ResultSetMetadata>,
        o_par_md: Option<&Vec<ParameterDescriptor>>,
        o_rs: &mut Option<&mut ResultSet>,
    ) -> HdbResult<Reply> {
        let auto_commit_flag: i8 = if self.is_auto_commit() { 1 } else { 0 };
        let nsn = self.next_seq_number();
        {
            let writer = &mut *(self.writer().borrow_mut());
            request.serialize(self.session_id(), nsn, auto_commit_flag, writer)?;
        }
        let mut reply = {
            let rdr = &mut *(self.reader().borrow_mut());
            Reply::parse(o_rs_md, o_par_md, o_rs, am_conn_core, rdr)?
        };
        self.handle_db_error(&mut reply.parts)?;
        Ok(reply)
    }

    fn handle_db_error(&mut self, parts: &mut Parts) -> HdbResult<()> {
        self.warnings.clear();

        // Retrieve errors from returned parts
        let mut errors = {
            let opt_error_part = parts.extract_first_part_of_type(PartKind::Error);
            match opt_error_part {
                None => {
                    // No error part found, reply evaluation happens elsewhere
                    return Ok(());
                }
                Some(error_part) => {
                    let (_, argument) = error_part.into_elements();
                    if let Argument::Error(server_errors) = argument {
                        // filter out warnings and add them to conn_core
                        let errors: Vec<ServerError> = server_errors
                            .into_iter()
                            .filter_map(|se| match se.severity() {
                                Severity::Warning => {
                                    self.warnings.push(se);
                                    None
                                }
                                _ => Some(se),
                            })
                            .collect();
                        if errors.is_empty() {
                            // Only warnings, so return Ok(())
                            return Ok(());
                        } else {
                            errors
                        }
                    } else {
                        unreachable!("129837938423")
                    }
                }
            }
        };

        // Evaluate the other parts
        let mut opt_rows_affected = None;
        parts.reverse(); // digest with pop
        while let Some(part) = parts.pop() {
            let (kind, arg) = part.into_elements();
            match arg {
                Argument::StatementContext(ref stmt_ctx) => {
                    self.evaluate_statement_context(stmt_ctx)?;
                }
                Argument::TransactionFlags(ta_flags) => {
                    self.evaluate_ta_flags(ta_flags)?;
                }
                Argument::ExecutionResult(vec) => {
                    opt_rows_affected = Some(vec);
                }
                arg => warn!(
                    "Reply::handle_db_error(): ignoring unexpected part of kind {:?}, arg = {:?}",
                    kind, arg
                ),
            }
        }

        match opt_rows_affected {
            Some(rows_affected) => {
                // mix errors into rows_affected
                let mut err_iter = errors.into_iter();
                let mut rows_affected = rows_affected
                    .into_iter()
                    .map(|ra| match ra {
                        ExecutionResult::Failure(_) => ExecutionResult::Failure(err_iter.next()),
                        _ => ra,
                    })
                    .collect::<Vec<ExecutionResult>>();
                for e in err_iter {
                    warn!(
                        "Reply::handle_db_error(): \
                         found more errors than instances of ExecutionResult::Failure"
                    );
                    rows_affected.push(ExecutionResult::Failure(Some(e)));
                }
                Err(HdbError::MixedResults(rows_affected))
            }
            None => {
                if errors.len() == 1 {
                    Err(HdbError::DbError(errors.remove(0)))
                } else {
                    unreachable!("hopefully...")
                }
            }
        }
    }

    fn drop_impl(&mut self) -> HdbResult<()> {
        trace!("Drop of ConnectionCore, session_id = {}", self.session_id);
        if self.authenticated {
            let request = Request::new_for_disconnect();
            {
                let nsn = self.next_seq_number();
                let mut writer = self.buffalo.writer().borrow_mut();
                request.serialize(self.session_id, nsn, 0, &mut *writer)?;
                writer.flush()?;
                trace!("Disconnect: request successfully sent");
            }
            {
                let mut reader = self.buffalo.reader().borrow_mut();
                match parse_message_and_sequence_header(&mut *reader) {
                    Ok((no_of_parts, mut reply)) => {
                        trace!(
                            "Disconnect: response header parsed, now parsing {} parts",
                            no_of_parts
                        );
                        for i in 0..no_of_parts {
                            let part = Part::parse(
                                &mut (reply.parts),
                                None,
                                None,
                                None,
                                &mut None,
                                i == no_of_parts - 1,
                                &mut *reader,
                            )?;
                            debug!("Drop of connection: got Part {:?}", part);
                        }
                        trace!("Disconnect: response successfully parsed");
                    }
                    Err(e) => {
                        trace!("Disconnect: could not parse response due to {:?}", e);
                    }
                }
            }
        }
        Ok(())
    }
}

impl Drop for ConnectionCore {
    // try to send a disconnect to the database, ignore all errors
    fn drop(&mut self) {
        if let Err(e) = self.drop_impl() {
            warn!("Disconnect request failed with {:?}", e);
        }
    }
}

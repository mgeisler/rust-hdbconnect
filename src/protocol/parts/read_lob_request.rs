use crate::hdb_error::HdbResult;
use byteorder::{LittleEndian, WriteBytesExt};

#[derive(Debug)]
pub struct ReadLobRequest {
    locator_id: u64,
    offset: u64,
    length: u32,
}
impl ReadLobRequest {
    pub fn new(locator_id: u64, offset: u64, length: u32) -> ReadLobRequest {
        trace!("Offset = {}, length = {}", offset, length);
        ReadLobRequest {
            locator_id,
            offset,
            length,
        }
    }
    pub fn emit<T: std::io::Write>(&self, w: &mut T) -> HdbResult<()> {
        trace!("read_lob_request::emit() {:?}", self);
        w.write_u64::<LittleEndian>(self.locator_id)?;
        w.write_u64::<LittleEndian>(self.offset)?;
        w.write_u32::<LittleEndian>(self.length)?;
        w.write_u32::<LittleEndian>(0_u32)?; // FILLER
        Ok(())
    }
    pub fn size(&self) -> usize {
        24
    }
}

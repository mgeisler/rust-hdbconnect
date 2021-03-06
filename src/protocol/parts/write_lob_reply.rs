use crate::HdbResult;

use byteorder::{LittleEndian, ReadBytesExt};
use std::io::BufRead;

#[derive(Debug)]
pub(crate) struct WriteLobReply {
    locator_ids: Vec<u64>,
}
impl WriteLobReply {
    pub fn into_locator_ids(self) -> Vec<u64> {
        self.locator_ids
    }
}

impl WriteLobReply {
    pub fn parse<T: BufRead>(count: usize, rdr: &mut T) -> HdbResult<WriteLobReply> {
        debug!("called with count = {}", count);
        let mut locator_ids: Vec<u64> = Default::default();
        for _ in 0..count {
            let locator_id = rdr.read_u64::<LittleEndian>()?; // I8
            locator_ids.push(locator_id);
        }

        Ok(WriteLobReply { locator_ids })
    }
}

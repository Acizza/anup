use super::{RemoteService, SeriesEntry, SeriesInfo};
use crate::err::{self, Result};

pub struct Offline {}

impl Offline {
    pub fn new() -> Offline {
        Offline {}
    }
}

impl RemoteService for Offline {
    fn search_info_by_name(&self, _: &str) -> Result<Vec<SeriesInfo>> {
        Err(err::Error::RunWithPrefetch {})
    }

    fn search_info_by_id(&self, _: u32) -> Result<SeriesInfo> {
        Err(err::Error::RunWithPrefetch {})
    }

    fn get_list_entry(&self, _: u32) -> Result<Option<SeriesEntry>> {
        Ok(None)
    }

    fn update_list_entry(&self, _: &SeriesEntry) -> Result<()> {
        Ok(())
    }

    fn is_offline(&self) -> bool {
        true
    }
}

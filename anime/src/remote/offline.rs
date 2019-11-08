use super::{RemoteService, ScoreParser, SeriesEntry, SeriesID, SeriesInfo};
use crate::err::{self, Result};

/// A remote service that will not connect to the internet.
///
/// This service is intended to make it easier to implement an offline mode
/// for your application by letting you seamlessly switch between online and offline
/// modes.
///
/// Note that the `search_info_by_name` and `search_info_by_id` methods will always
/// return an error with the variant `NeedExistingSeriesData`. All other methods simply
/// do nothing.
#[derive(Default)]
pub struct Offline;

impl Offline {
    pub fn new() -> Offline {
        Offline {}
    }
}

impl RemoteService for Offline {
    fn search_info_by_name(&self, _: &str) -> Result<Vec<SeriesInfo>> {
        Err(err::Error::NeedExistingSeriesData)
    }

    fn search_info_by_id(&self, _: SeriesID) -> Result<SeriesInfo> {
        Err(err::Error::NeedExistingSeriesData)
    }

    fn get_list_entry(&self, _: SeriesID) -> Result<Option<SeriesEntry>> {
        Ok(None)
    }

    fn update_list_entry(&self, _: &SeriesEntry) -> Result<()> {
        Ok(())
    }

    fn is_offline(&self) -> bool {
        true
    }
}

impl ScoreParser for Offline {}

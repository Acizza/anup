pub mod anilist;
pub mod offline;

use super::detect;
use crate::err::{self, Result};
use serde_derive::{Deserialize, Serialize};
use snafu::OptionExt;

pub trait RemoteService {
    fn search_info_by_name(&self, name: &str) -> Result<Vec<SeriesInfo>>;
    fn search_info_by_id(&self, id: u32) -> Result<SeriesInfo>;

    fn get_list_entry(&self, id: u32) -> Result<Option<SeriesEntry>>;
    fn update_list_entry(&self, entry: &SeriesEntry) -> Result<()>;

    fn is_offline(&self) -> bool {
        false
    }
}

pub type Minutes = u32;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SeriesInfo {
    pub id: u32,
    pub title: String,
    pub episodes: u32,
    pub episode_length: Minutes,
    pub sequel: Option<u32>,
}

impl SeriesInfo {
    pub fn best_matching_from_remote<R, S>(remote: R, name: S) -> Result<SeriesInfo>
    where
        R: AsRef<RemoteService>,
        S: AsRef<str>,
    {
        let remote = remote.as_ref();
        let name = name.as_ref();

        let mut results = remote.search_info_by_name(name)?;
        let index = detect::best_matching_info(name, results.as_slice())
            .context(err::NoMatchingSeries { name })?;

        let info = results.swap_remove(index);
        Ok(info)
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SeriesEntry {
    pub id: u32,
    pub watched_eps: u32,
    pub score: Option<u8>,
    pub status: Status,
    pub times_rewatched: u32,
    pub start_date: Option<chrono::NaiveDate>,
    pub end_date: Option<chrono::NaiveDate>,
}

impl SeriesEntry {
    pub fn new(id: u32) -> SeriesEntry {
        SeriesEntry {
            id,
            watched_eps: 0,
            score: None,
            status: Status::default(),
            times_rewatched: 0,
            start_date: None,
            end_date: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum Status {
    Watching,
    Completed,
    OnHold,
    Dropped,
    PlanToWatch,
    Rewatching,
}

impl Default for Status {
    fn default() -> Status {
        Status::PlanToWatch
    }
}

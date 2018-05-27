use chrono::{NaiveDate, Utc};
use config::Config;
use error::BackendError;

pub mod anilist;

pub trait SyncBackend
where
    Self: Sized,
{
    fn init(config: &mut Config) -> Result<Self, BackendError>;

    fn search_by_name(&self, name: &str) -> Result<Vec<AnimeInfo>, BackendError>;
    fn get_series_info_by_id(&self, id: u32) -> Result<AnimeInfo, BackendError>;

    fn get_list_entry(&self, info: AnimeInfo) -> Result<Option<AnimeEntry>, BackendError>;
    fn update_list_entry(&self, entry: &AnimeEntry) -> Result<(), BackendError>;
}

#[derive(Clone, Debug)]
pub struct AnimeInfo {
    pub id: u32,
    pub title: String,
    pub episodes: u32,
}

#[derive(Debug)]
pub struct AnimeEntry {
    pub info: AnimeInfo,
    pub watched_episodes: u32,
    pub score: f32,
    pub status: Status,
    pub start_date: Option<NaiveDate>,
    pub finish_date: Option<NaiveDate>,
}

impl AnimeEntry {
    pub fn new(info: AnimeInfo) -> AnimeEntry {
        AnimeEntry {
            info,
            watched_episodes: 0,
            score: 0.0,
            status: Status::PlanToWatch,
            start_date: Some(Utc::now().naive_utc().date()),
            finish_date: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Watching,
    Completed,
    OnHold,
    Dropped,
    PlanToWatch,
    Rewatching,
}

use crate::config::Config;
use crate::error::BackendError;
use chrono::NaiveDate;
use serde_derive::{Deserialize, Serialize};
use std::borrow::Cow;

pub mod anilist;

pub trait SyncBackend
where
    Self: Sized + ScoreParser,
{
    fn name() -> &'static str;

    fn init(offline_mode: bool, config: &mut Config) -> Result<Self, BackendError>;

    fn search_by_name(&self, name: &str) -> Result<Vec<AnimeInfo>, BackendError>;
    fn get_series_info_by_id(&self, id: u32) -> Result<AnimeInfo, BackendError>;

    fn get_list_entry(&self, info: AnimeInfo) -> Result<Option<AnimeEntry>, BackendError>;
    fn update_list_entry(&self, entry: &AnimeEntry) -> Result<(), BackendError>;
}

pub trait ScoreParser {
    fn formatted_score_range(&self) -> (Cow<str>, Cow<str>);
    fn parse_score(&self, input: &str) -> Result<f32, BackendError>;
    fn format_score(&self, raw_score: f32) -> Result<String, BackendError>;
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AnimeInfo {
    pub id: u32,
    pub title: String,
    pub episodes: Option<u32>,
}

impl Default for AnimeInfo {
    fn default() -> AnimeInfo {
        AnimeInfo {
            id: 0,
            title: String::new(),
            episodes: None,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AnimeEntry {
    #[serde(flatten)]
    pub info: AnimeInfo,
    pub watched_episodes: u32,
    pub score: Option<f32>,
    pub status: Status,
    pub start_date: Option<NaiveDate>,
    pub finish_date: Option<NaiveDate>,
}

impl AnimeEntry {
    pub fn new(info: AnimeInfo) -> AnimeEntry {
        AnimeEntry {
            info,
            watched_episodes: 0,
            score: None,
            status: Status::PlanToWatch,
            start_date: None,
            finish_date: None,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum Status {
    Watching,
    Completed,
    OnHold,
    Dropped,
    PlanToWatch,
    Rewatching,
}

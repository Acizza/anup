pub mod anilist;
pub mod offline;

use crate::err::Result;
use serde_derive::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt;

pub trait RemoteService: ScoreParser {
    fn search_info_by_name(&self, name: &str) -> Result<Vec<SeriesInfo>>;
    fn search_info_by_id(&self, id: u32) -> Result<SeriesInfo>;

    fn get_list_entry(&self, id: u32) -> Result<Option<SeriesEntry>>;
    fn update_list_entry(&self, entry: &SeriesEntry) -> Result<()>;

    fn is_offline(&self) -> bool {
        false
    }
}

pub trait ScoreParser {
    fn parse_score(&self, score: &str) -> Option<u8> {
        score
            .parse()
            .ok()
            .and_then(|score| if score <= 100 { Some(score) } else { None })
    }

    fn score_to_str(&self, score: u8) -> Cow<str> {
        Cow::Owned(score.to_string())
    }
}

pub type Minutes = u32;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SeriesInfo {
    pub id: u32,
    pub title: SeriesTitle,
    pub episodes: u32,
    pub episode_length: Minutes,
    pub sequel: Option<u32>,
}

impl<'a> Into<Cow<'a, SeriesInfo>> for SeriesInfo {
    fn into(self) -> Cow<'a, SeriesInfo> {
        Cow::Owned(self)
    }
}

impl<'a> Into<Cow<'a, SeriesInfo>> for &'a SeriesInfo {
    fn into(self) -> Cow<'a, SeriesInfo> {
        Cow::Borrowed(self)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SeriesTitle {
    pub romaji: String,
    pub preferred: String,
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

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Status::Watching => "Watching",
            Status::Completed => "Completed",
            Status::OnHold => "On Hold",
            Status::Dropped => "Dropped",
            Status::PlanToWatch => "Plan To Watch",
            Status::Rewatching => "Rewatching",
        };

        write!(f, "{}", value)
    }
}

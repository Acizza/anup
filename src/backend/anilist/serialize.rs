use super::{AnimeEntry, AnimeInfo, Status};
use chrono::{Date, Datelike, Local, NaiveDate, TimeZone};

#[derive(Deserialize)]
pub struct User {
    pub id: u32,
    #[serde(rename = "mediaListOptions")]
    pub list_options: MediaListOptions,
}

impl Default for User {
    fn default() -> User {
        User {
            id: 0,
            list_options: MediaListOptions::default(),
        }
    }
}

#[derive(Deserialize)]
pub struct MediaListOptions {
    #[serde(rename = "scoreFormat")]
    pub score_format: ScoreFormat,
}

impl Default for MediaListOptions {
    fn default() -> MediaListOptions {
        MediaListOptions {
            score_format: ScoreFormat::default(),
        }
    }
}

#[derive(Serialize, Deserialize, Copy, Clone)]
pub enum ScoreFormat {
    #[serde(rename = "POINT_100")]
    Point100,
    #[serde(rename = "POINT_10_DECIMAL")]
    Point10Decimal,
    #[serde(rename = "POINT_10")]
    Point10,
    #[serde(rename = "POINT_5")]
    Point5,
    #[serde(rename = "POINT_3")]
    Point3,
}

impl ScoreFormat {
    pub fn max_score(&self) -> u8 {
        use self::ScoreFormat::*;

        match self {
            Point100 => 100,
            Point10Decimal | Point10 => 10,
            Point5 => 5,
            Point3 => 3,
        }
    }
}

impl Default for ScoreFormat {
    fn default() -> ScoreFormat {
        ScoreFormat::Point100
    }
}

#[derive(Deserialize)]
pub struct Media {
    pub id: u32,
    pub title: MediaTitle,
    pub episodes: Option<u32>,
}

#[derive(Deserialize)]
pub struct MediaTitle {
    romaji: String,
}

impl Into<AnimeInfo> for Media {
    fn into(self) -> AnimeInfo {
        AnimeInfo {
            id: self.id,
            title: self.title.romaji,
            episodes: self.episodes,
        }
    }
}

#[derive(Deserialize)]
pub struct MediaListEntry {
    pub progress: u32,
    pub status: MediaStatus,
    pub score: f32,
    #[serde(rename = "startedAt")]
    pub start_date: MediaDate,
    #[serde(rename = "completedAt")]
    pub finish_date: MediaDate,
}

impl MediaListEntry {
    pub fn into_generic_entry(self, info: AnimeInfo) -> AnimeEntry {
        // AniList uses 0.0 to represent a non-set score
        let score = if self.score >= 0.1 {
            Some(self.score)
        } else {
            None
        };

        AnimeEntry {
            info,
            watched_episodes: self.progress,
            score,
            status: self.status.into(),
            start_date: self.start_date.into_date(),
            finish_date: self.finish_date.into_date(),
        }
    }
}

#[derive(Serialize, Deserialize, Copy, Clone, PartialEq)]
pub enum MediaStatus {
    #[serde(rename = "CURRENT")]
    Current,
    #[serde(rename = "COMPLETED")]
    Completed,
    #[serde(rename = "PAUSED")]
    Paused,
    #[serde(rename = "DROPPED")]
    Dropped,
    #[serde(rename = "PLANNING")]
    Planning,
    #[serde(rename = "REPEATING")]
    Repeating,
}

impl Into<Status> for MediaStatus {
    fn into(self) -> Status {
        match self {
            MediaStatus::Current => Status::Watching,
            MediaStatus::Completed => Status::Completed,
            MediaStatus::Paused => Status::OnHold,
            MediaStatus::Dropped => Status::Dropped,
            MediaStatus::Planning => Status::PlanToWatch,
            MediaStatus::Repeating => Status::Rewatching,
        }
    }
}

impl From<Status> for MediaStatus {
    fn from(status: Status) -> MediaStatus {
        match status {
            Status::Watching => MediaStatus::Current,
            Status::Completed => MediaStatus::Completed,
            Status::OnHold => MediaStatus::Paused,
            Status::Dropped => MediaStatus::Dropped,
            Status::PlanToWatch => MediaStatus::Planning,
            Status::Rewatching => MediaStatus::Repeating,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct MediaDate {
    pub year: Option<i32>,
    pub month: Option<u32>,
    pub day: Option<u32>,
}

impl MediaDate {
    pub fn into_date(self) -> Option<Date<Local>> {
        match (self.year, self.month, self.day) {
            (Some(year), Some(month), Some(day)) => Some(Local.ymd(year, month, day)),
            _ => None,
        }
    }

    pub fn from_date(date: Option<Date<Local>>) -> MediaDate {
        match date {
            Some(date) => MediaDate {
                year: Some(date.year()),
                month: Some(date.month()),
                day: Some(date.day()),
            },
            None => MediaDate {
                year: None,
                month: None,
                day: None,
            },
        }
    }
}

impl From<NaiveDate> for MediaDate {
    fn from(date: NaiveDate) -> MediaDate {
        MediaDate {
            year: Some(date.year()),
            month: Some(date.month()),
            day: Some(date.day()),
        }
    }
}

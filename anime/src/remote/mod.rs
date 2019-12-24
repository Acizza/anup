pub mod anilist;
pub mod offline;

use crate::err::{self, Result};
use serde_derive::{Deserialize, Serialize};
use snafu::ResultExt;
use std::borrow::Cow;
use std::fmt;

#[cfg(feature = "rusqlite-support")]
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};

/// Type representing the ID of an anime series.
pub type SeriesID = u32;

/// Core functionality to interact with an anime tracking service.
pub trait RemoteService: ScoreParser {
    // TODO: convert return type to impl Iterator when associated type defaults are stable  (https://github.com/rust-lang/rust/issues/29661)
    /// Search for an anime's information by title and return all of the matches.
    fn search_info_by_name(&self, name: &str) -> Result<Box<dyn Iterator<Item = SeriesInfo>>>;

    /// Get an anime's information by its ID.
    /// Note that the ID will differ from service to service.
    fn search_info_by_id(&self, id: SeriesID) -> Result<SeriesInfo>;

    /// Retrieve the anime list entry from the currently authenticated user.
    ///
    /// `id` is the ID of the anime, which differs from service to service.
    fn get_list_entry(&self, id: SeriesID) -> Result<Option<SeriesEntry>>;

    /// Upload `entry` to the currently authenticated user's anime list.
    ///
    /// Please ensure that the `SeriesEntry` you are using comes from the current service
    /// in use, or you may overwrite a completely different list entry.
    fn update_list_entry(&self, entry: &SeriesEntry) -> Result<()>;

    /// Indicates whether or not this service is meant to be used without an internet connection.
    ///
    /// Returns false by default.
    fn is_offline(&self) -> bool {
        false
    }
}

/// Functionality to deal with scores from an anime tracking service.
pub trait ScoreParser {
    /// Parse the given `score` string to a u8 between 0 - 100.
    ///
    /// By default, it will simply map `score` to its equivalent u8 value.
    fn parse_score(&self, score: &str) -> Option<u8> {
        score
            .parse()
            .ok()
            .and_then(|score| if score <= 100 { Some(score) } else { None })
    }

    /// Map the given `score` to its string equivalent.
    ///
    /// By default, it will simply return `score` as a string.
    fn score_to_str(&self, score: u8) -> Cow<str> {
        Cow::Owned(score.to_string())
    }
}

/// General information for an anime series.
#[derive(Clone, Debug)]
pub struct SeriesInfo {
    /// The ID of the series.
    pub id: SeriesID,
    /// The titles of the series.
    pub title: SeriesTitle,
    /// The number of episodes.
    pub episodes: u32,
    /// The length of a single episode in minutes.
    pub episode_length: u32,
    /// An ID pointing to the sequel of this series.
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

/// Various title formats for an anime series.
#[derive(Clone, Debug)]
pub struct SeriesTitle {
    /// The title in romaji.
    pub romaji: String,
    /// The title in the user's preferred format.
    pub preferred: String,
}

/// A list entry for an anime series.
#[derive(Debug)]
pub struct SeriesEntry {
    /// The ID of the anime.
    pub id: u32,
    /// The number of episodes that have been watched.
    pub watched_eps: u32,
    /// The score given by the user.
    pub score: Option<u8>,
    /// The user's current watch status of the series.
    pub status: Status,
    /// The number of times the user has rewatched the series.
    pub times_rewatched: u32,
    /// The date the user started watching the series.
    pub start_date: Option<chrono::NaiveDate>,
    /// The date the user finished watching the series.
    pub end_date: Option<chrono::NaiveDate>,
}

impl SeriesEntry {
    /// Create a new `SeriesEntry` associated to the anime with the specified `id`.
    #[inline]
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

/// The watch status of an anime series.
#[derive(Clone, Copy, Debug, PartialEq)]
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

#[cfg(feature = "rusqlite-support")]
impl FromSql for Status {
    fn column_result(value: ValueRef) -> FromSqlResult<Self> {
        match value.as_i64()? {
            1 => Ok(Status::Watching),
            2 => Ok(Status::Completed),
            3 => Ok(Status::OnHold),
            4 => Ok(Status::Dropped),
            5 => Ok(Status::PlanToWatch),
            6 => Ok(Status::Rewatching),
            _ => Err(FromSqlError::InvalidType),
        }
    }
}

#[cfg(feature = "rusqlite-support")]
impl ToSql for Status {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput> {
        let value = match self {
            Status::Watching => 1,
            Status::Completed => 2,
            Status::OnHold => 3,
            Status::Dropped => 4,
            Status::PlanToWatch => 5,
            Status::Rewatching => 6,
        };

        Ok(value.into())
    }
}

/// A user's access token for a remote service.
///
/// Most remote services will require you to use this in order to make changes to
/// a user's list.
#[derive(Clone, Default, Deserialize, Serialize)]
pub struct AccessToken {
    encoded_token: String,
}

impl AccessToken {
    /// Encode a new `AccessToken`.
    #[inline]
    pub fn encode<S>(token: S) -> AccessToken
    where
        S: AsRef<str>,
    {
        AccessToken {
            encoded_token: base64::encode(token.as_ref()),
        }
    }

    /// Get the content of the `AccessToken`.
    ///
    /// # Example
    ///
    /// ```
    /// use anime::remote::AccessToken;
    ///
    /// let token = AccessToken::encode("test");
    /// assert_eq!(token.decode().unwrap(), "test");
    /// ```
    #[inline]
    pub fn decode(&self) -> Result<String> {
        let bytes = base64::decode(&self.encoded_token).context(err::Base64Decode)?;
        let string = String::from_utf8(bytes).context(err::UTF8Decode)?;

        Ok(string)
    }
}

// Better to not accidently expose a base64 encoded token..
impl fmt::Debug for AccessToken {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "AccessToken {{}}")
    }
}

pub mod anilist;
pub mod offline;

use crate::err::{self, Result};
use crate::SeriesKind;
use anilist::AniList;
use enum_dispatch::enum_dispatch;
use offline::Offline;
use serde_derive::{Deserialize, Serialize};
use snafu::ResultExt;
use std::borrow::Cow;
use std::fmt;

#[cfg(feature = "diesel-support")]
use {
    diesel::{
        deserialize::{self, FromSql},
        serialize::{self, Output, ToSql},
        sql_types::{Date, SmallInt},
    },
    std::io::Write,
};

/// Type representing the ID of an anime series.
pub type SeriesID = u32;

/// Enum representing each remote service.
#[enum_dispatch]
#[derive(Debug)]
pub enum Remote {
    AniList,
    Offline,
}

impl Remote {
    #[inline(always)]
    pub fn offline() -> Self {
        Offline::new().into()
    }
}

/// Core functionality to interact with an anime tracking service.
#[enum_dispatch(Remote)]
pub trait RemoteService: ScoreParser {
    /// Search for an anime's information by title and return all of the matches.
    fn search_info_by_name(&self, name: &str) -> Result<Vec<SeriesInfo>>;

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
#[enum_dispatch(Remote)]
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
    /// The type of series.
    pub kind: SeriesKind,
    /// An ID pointing to the sequel of this series.
    pub sequels: Vec<Sequel>,
}

impl SeriesInfo {
    #[inline]
    pub fn closest_match<'a, I, S>(
        name: S,
        min_confidence: f32,
        items: I,
    ) -> Option<(usize, Cow<'a, Self>)>
    where
        I: Iterator<Item = Cow<'a, Self>>,
        S: Into<String>,
    {
        let mut name = name.into();
        name.make_ascii_lowercase();

        crate::closest_match(items, min_confidence, |info| {
            let title = info.title.romaji.to_ascii_lowercase();
            Some(strsim::jaro_winkler(&title, &name) as f32)
        })
    }

    /// Returns the first sequel that is the same kind as the current series.
    ///
    /// This can be used to follow sequel trails of seasons.
    #[inline(always)]
    pub fn direct_sequel(&self) -> Option<&Sequel> {
        self.sequel_by_kind(self.kind)
    }

    /// Returns the first sequel matching the specified `kind`.
    #[inline]
    pub fn sequel_by_kind(&self, kind: SeriesKind) -> Option<&Sequel> {
        self.sequels.iter().find(|sequel| sequel.kind == kind)
    }
}

impl<'a> Into<Cow<'a, Self>> for SeriesInfo {
    fn into(self) -> Cow<'a, Self> {
        Cow::Owned(self)
    }
}

impl<'a> Into<Cow<'a, SeriesInfo>> for &'a SeriesInfo {
    fn into(self) -> Cow<'a, SeriesInfo> {
        Cow::Borrowed(self)
    }
}

/// A sequel to a series.
#[derive(Clone, Debug)]
pub struct Sequel {
    /// The kind of sequel this is.
    pub kind: SeriesKind,
    /// The series ID of the sequel.
    pub id: SeriesID,
}

impl Sequel {
    #[inline(always)]
    pub fn new(kind: SeriesKind, id: SeriesID) -> Self {
        Self { kind, id }
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
    pub start_date: Option<SeriesDate>,
    /// The date the user finished watching the series.
    pub end_date: Option<SeriesDate>,
}

impl SeriesEntry {
    /// Create a new `SeriesEntry` associated to the anime with the specified `id`.
    #[inline]
    pub fn new(id: u32) -> Self {
        Self {
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
#[cfg_attr(
    feature = "diesel-support",
    derive(AsExpression, FromSqlRow),
    sql_type = "SmallInt"
)]
pub enum Status {
    Watching,
    Completed,
    OnHold,
    Dropped,
    PlanToWatch,
    Rewatching,
}

impl Default for Status {
    fn default() -> Self {
        Self::PlanToWatch
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

#[cfg(feature = "diesel-support")]
impl<DB> FromSql<SmallInt, DB> for Status
where
    DB: diesel::backend::Backend,
    i16: FromSql<SmallInt, DB>,
{
    fn from_sql(bytes: Option<&DB::RawValue>) -> deserialize::Result<Self> {
        match i16::from_sql(bytes)? {
            1 => Ok(Status::Watching),
            2 => Ok(Status::Completed),
            3 => Ok(Status::OnHold),
            4 => Ok(Status::Dropped),
            5 => Ok(Status::PlanToWatch),
            6 => Ok(Status::Rewatching),
            other => Err(format!("invalid status: {}", other).into()),
        }
    }
}

#[cfg(feature = "diesel-support")]
impl<DB> ToSql<SmallInt, DB> for Status
where
    DB: diesel::backend::Backend,
    i16: ToSql<SmallInt, DB>,
{
    fn to_sql<W: Write>(&self, out: &mut Output<W, DB>) -> serialize::Result {
        let value = match self {
            Status::Watching => 1,
            Status::Completed => 2,
            Status::OnHold => 3,
            Status::Dropped => 4,
            Status::PlanToWatch => 5,
            Status::Rewatching => 6,
        };

        value.to_sql(out)
    }
}

#[derive(Copy, Clone, Debug)]
#[cfg_attr(
    feature = "diesel-support",
    derive(AsExpression, FromSqlRow),
    sql_type = "Date"
)]
/// A date on a series.
pub struct SeriesDate {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

impl SeriesDate {
    #[inline(always)]
    pub fn from_ymd(year: u16, month: u8, day: u8) -> Self {
        Self { year, month, day }
    }
}

#[cfg(feature = "diesel-support")]
impl<DB> FromSql<Date, DB> for SeriesDate
where
    DB: diesel::backend::Backend,
    String: FromSql<Date, DB>,
{
    fn from_sql(bytes: Option<&DB::RawValue>) -> deserialize::Result<Self> {
        let value = String::from_sql(bytes)?;
        let mut separator = value.split('-');

        let year = separator
            .next()
            .ok_or_else(|| "no year found while parsing date")?
            .parse()?;

        let month = separator
            .next()
            .ok_or_else(|| "no month found while parsing date")?
            .parse()?;

        let day = separator
            .next()
            .ok_or_else(|| "no day found while parsing date")?
            .parse()?;

        Ok(Self::from_ymd(year, month, day))
    }
}

#[cfg(feature = "diesel-support")]
impl<DB> ToSql<Date, DB> for SeriesDate
where
    DB: diesel::backend::Backend,
    String: ToSql<Date, DB>,
{
    fn to_sql<W: Write>(&self, out: &mut Output<W, DB>) -> serialize::Result {
        format!("{}-{}-{}", self.year, self.month, self.day).to_sql(out)
    }
}

#[cfg(feature = "chrono-support")]
impl From<chrono::NaiveDate> for SeriesDate {
    fn from(date: chrono::NaiveDate) -> Self {
        use chrono::Datelike;

        Self {
            year: (date.year().max(0) as u16).min(u16::MAX),
            month: date.month().min(u8::MAX as u32) as u8,
            day: date.day().min(u8::MAX as u32) as u8,
        }
    }
}

#[cfg(feature = "chrono-support")]
impl Into<chrono::NaiveDate> for SeriesDate {
    fn into(self) -> chrono::NaiveDate {
        use chrono::NaiveDate;
        NaiveDate::from_ymd(self.year as i32, self.month as u32, self.day as u32)
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
    pub fn encode<S>(token: S) -> Self
    where
        S: AsRef<[u8]>,
    {
        Self {
            encoded_token: base64::encode(token),
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

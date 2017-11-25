use chrono::NaiveDate;
use failure::{Error, SyncFailure};
use minidom::Element;
use SeriesInfo;

/// Represents information about an anime series on a user's list.
#[derive(Debug, Clone)]
pub struct AnimeEntry {
    /// The general series information.
    pub info: SeriesInfo,
    /// The number of episodes watched.
    pub watched_episodes: u32,
    /// The date the user started watching the series.
    pub start_date: Option<NaiveDate>,
    /// The date the user finished watching the series.
    pub end_date: Option<NaiveDate>,
    /// The current watch status of the series.
    pub status: Status,
}

impl PartialEq for AnimeEntry {
    fn eq(&self, other: &AnimeEntry) -> bool {
        self.info == other.info
    }
}

#[derive(Fail, Debug)]
#[fail(display = "{} does not map to any Status enum variants", _0)]
pub struct InvalidStatus(pub i32);

/// Represents the watch status of an anime on a user's list.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Status {
    Watching = 1,
    Completed,
    OnHold,
    Dropped,
    PlanToWatch = 6,
}

impl Status {
    /// Attempts to convert an i32 to a `Status`.
    ///
    /// Note that the i32 value of each `Status` variant is mapped
    /// to the one provided by the MyAnimeList API, so they do not increment naturally.
    ///
    /// # Example
    ///
    /// ```
    /// use mal::list::Status;
    ///
    /// let status = Status::from_i32(1).unwrap();
    /// assert_eq!(status, Status::Watching);
    /// ```
    pub fn from_i32(value: i32) -> Result<Status, InvalidStatus> {
        match value {
            1 => Ok(Status::Watching),
            2 => Ok(Status::Completed),
            3 => Ok(Status::OnHold),
            4 => Ok(Status::Dropped),
            6 => Ok(Status::PlanToWatch),
            i => Err(InvalidStatus(i)),
        }
    }
}

/// Represents a specific value of an anime on a user's anime list.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum EntryTag {
    /// The number of watched episodes.
    Episode(u32),
    /// The current watch status.
    Status(Status),
    /// The date the user started watching the anime.
    StartDate(NaiveDate),
    /// The date the user finished watching the anime.
    FinishDate(NaiveDate),
    /// The score to give the anime.
    Score(u8),
    /// Indicates whether or not the user is rewatching the anime.
    Rewatching(bool),
}

macro_rules! elem_with_txt {
    ($name:expr, $value:expr) => {{
        let mut elem = Element::bare($name);
        elem.append_text_node($value);
        elem
    }};
}

impl EntryTag {
    // TODO: adjust visibility
    pub fn build_xml_resp(stats: &[EntryTag]) -> Result<String, Error> {
        let mut entry = Element::bare("entry");

        for stat in stats {
            use self::EntryTag::*;

            let child = match *stat {
                Episode(num) => elem_with_txt!("episode", num.to_string()),
                Status(ref status) => elem_with_txt!("status", (status.clone() as i32).to_string()),
                StartDate(date) => elem_with_txt!("date_start", date_to_str(&date)),
                FinishDate(date) => elem_with_txt!("date_finish", date_to_str(&date)),
                Score(score) => elem_with_txt!("score", score.to_string()),
                Rewatching(v) => elem_with_txt!("enable_rewatching", (v as u8).to_string()),
            };

            entry.append_child(child);
        }

        let mut buffer = Vec::new();
        entry.write_to(&mut buffer).map_err(SyncFailure::new)?;

        Ok(String::from_utf8(buffer)?)
    }
}

fn date_to_str(date: &NaiveDate) -> String {
    date.format("%m%d%Y").to_string()
}

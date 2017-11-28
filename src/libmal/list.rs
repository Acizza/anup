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
    /// The current rating given by the user.
    pub score: u8,
    /// Indicates whether or not the anime is currently being rewatched.
    pub rewatching: bool,
}

impl AnimeEntry {
    /// Creates a new `EntryChangeset` builder.
    pub fn new_changeset(&mut self) -> EntryChangeset {
        EntryChangeset::new(self)
    }
}

impl PartialEq for AnimeEntry {
    fn eq(&self, other: &AnimeEntry) -> bool {
        self.info == other.info
    }
}

/// A builder for generating an `EntryTag` list while keeping an `AnimeEntry` in sync.
///
/// When adding / updating an anime on a user's list, you'll often
/// want to syncronize the changes that will appear on the user's list with
/// the `AnimeEntry` values. This builder will generate an `EntryTag` list that
/// can be used with `MAL::add_anime` / `MAL::update_anime` and set the
/// appropriate values in the `AnimeEntry` struct at the same time.
///
/// # Example
///
/// ```
/// use mal::SeriesInfo;
/// use mal::list::{AnimeEntry, EntryTag, Status};
///
/// let mut entry = AnimeEntry {
///     info: SeriesInfo {
///         id: 1234,
///         title: "Test Anime".into(),
///         episodes: 12
///     },
///     watched_episodes: 0,
///     start_date: None,
///     end_date: None,
///     status: Status::PlanToWatch,
///     score: 0,
///     rewatching: false
/// };
///
/// let tags = entry.new_changeset()
///     .episode(1)
///     .status(Status::Watching)
///     .build();
///
/// assert_eq!(tags, vec![EntryTag::Episode(1), EntryTag::Status(Status::Watching)]);
///
/// assert_eq!(entry.watched_episodes, 1);
/// assert_eq!(entry.status, Status::Watching);
/// ```
#[derive(Debug)]
pub struct EntryChangeset<'a> {
    entry: &'a mut AnimeEntry,
    tags: Vec<EntryTag>,
}

impl<'a> EntryChangeset<'a> {
    fn new(entry: &'a mut AnimeEntry) -> EntryChangeset<'a> {
        EntryChangeset {
            entry,
            tags: Vec::new(),
        }
    }

    /// Set the number of episodes watched.
    pub fn episode(mut self, ep: u32) -> EntryChangeset<'a> {
        self.tags.push(EntryTag::Episode(ep));
        self.entry.watched_episodes = ep;
        self
    }

    /// Set the date the user started watching the anime.
    pub fn start_date(mut self, date: Option<NaiveDate>) -> EntryChangeset<'a> {
        self.tags.push(EntryTag::StartDate(date));
        self.entry.start_date = date;
        self
    }

    /// Set the date the user finished watching the anime.
    pub fn finish_date(mut self, date: Option<NaiveDate>) -> EntryChangeset<'a> {
        self.tags.push(EntryTag::FinishDate(date));
        self.entry.end_date = date;
        self
    }

    /// Set the current watch status of the anime.
    pub fn status(mut self, status: Status) -> EntryChangeset<'a> {
        self.tags.push(EntryTag::Status(status));
        self.entry.status = status;
        self
    }

    /// Set the user's rating of an anime.
    pub fn score(mut self, score: u8) -> EntryChangeset<'a> {
        self.tags.push(EntryTag::Score(score));
        self.entry.score = score;
        self
    }

    /// Set whether or not the anime is being rewatched.
    pub fn rewatching(mut self, rewatching: bool) -> EntryChangeset<'a> {
        self.tags.push(EntryTag::Rewatching(rewatching));
        self.entry.rewatching = rewatching;
        self
    }

    /// Consume the builder and get the created `EntryTag` list.
    pub fn build(self) -> Vec<EntryTag> {
        self.tags
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
    StartDate(Option<NaiveDate>),
    /// The date the user finished watching the anime.
    FinishDate(Option<NaiveDate>),
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
                StartDate(date) => elem_with_txt!("date_start", date_to_str(date)),
                FinishDate(date) => elem_with_txt!("date_finish", date_to_str(date)),
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

fn date_to_str(date: Option<NaiveDate>) -> String {
    match date {
        Some(date) => date.format("%m%d%Y").to_string(),
        None => {
            // MAL uses an all-zero date to represent a non-set one
            "00000000".into()
        }
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

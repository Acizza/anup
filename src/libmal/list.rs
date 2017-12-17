use chrono::NaiveDate;
use failure::{Error, SyncFailure};
use get_xml_child_text;
use MAL;
use minidom::Element;
use request;
use RequestURL;
use SeriesInfo;

/// Used to perform operations on a user's anime list.
/// 
/// Note that since the `AnimeList` struct stores a reference to a `MAL` instance,
/// the `MAL` instance must live as long as the `AnimeList`.
pub struct AnimeList<'a> {
    mal: &'a MAL,
}

impl<'a> AnimeList<'a> {
    /// Creates a new instance of the `AnimeList` struct and stores the provided `MAL` reference
    /// so authorization can be handled automatically.
    /// 
    /// # Examples
    /// 
    /// ```no_run
    /// use mal::MAL;
    /// use mal::list::AnimeList;
    /// 
    /// // Create a new MAL instance
    /// let mal = MAL::new("username", "password");
    /// 
    /// // Create a new AnimeList instance.
    /// // Keep in mind that the MAL instance must now live for as long as the AnimeList
    /// let anime_list = AnimeList::new(&mal);
    /// ```
    #[inline]
    pub fn new(mal: &'a MAL) -> AnimeList<'a> {
        AnimeList {
            mal
        }
    }

    /// Requests and parses all entries on the user's anime list.
    /// 
    /// # Examples
    /// 
    /// ```no_run
    /// use mal::MAL;
    /// use mal::list::AnimeList;
    /// 
    /// // Create a new MAL instance
    /// let mal = MAL::new("username", "password");
    /// 
    /// // Create a new AnimeList instance
    /// let anime_list = AnimeList::new(&mal);
    /// 
    /// // Read all list entries from the user's list
    /// let entries = anime_list.read_entries().unwrap();
    /// 
    /// assert!(entries.len() > 0);
    /// ```
    pub fn read_entries(&self) -> Result<Vec<ListEntry>, Error> {
        let resp = request::get_verify(self.mal, RequestURL::AnimeList(&self.mal.username))?.text()?;
        let root: Element = resp.parse().map_err(SyncFailure::new)?;

        let mut entries = Vec::new();

        for child in root.children().skip(1) {
            let get_child = |name| get_xml_child_text(child, name);

            let entry = ListEntry {
                series_info: SeriesInfo {
                    id: get_child("series_animedb_id")?.parse()?,
                    title: get_child("series_title")?,
                    episodes: get_child("series_episodes")?.parse()?,
                },
                watched_episodes: get_child("my_watched_episodes")?.parse()?,
                start_date: parse_str_date(&get_child("my_start_date")?),
                finish_date: parse_str_date(&get_child("my_finish_date")?),
                status: Status::from_i32(get_child("my_status")?.parse()?)?,
                score: get_child("my_score")?.parse()?,
                rewatching: get_child("my_rewatching")?.parse::<u8>()? == 1,
                changeset: Changeset::new(),
            };

            entries.push(entry);
        }

        Ok(entries)
    }

    /// Adds an anime to the user's list.
    /// 
    /// If the anime is already on the user's list, nothing will happen.
    /// 
    /// # Examples
    /// 
    /// ```no_run
    /// use mal::{MAL, SeriesInfo};
    /// use mal::list::{AnimeList, Changeset, Status};
    /// 
    /// // Create a new MAL instance
    /// let mal = MAL::new("username", "password");
    /// 
    /// /// // Search for "Toradora" on MyAnimeList
    /// let mut search_results = mal.search("Toradora").unwrap();
    /// 
    /// // Use the first result's info
    /// let toradora_info = search_results.swap_remove(0);
    /// 
    /// // Create a new AnimeList instance
    /// let anime_list = AnimeList::new(&mal);
    /// 
    /// // Construct the initial set of values the anime will have when added
    /// let mut changeset = Changeset::new();
    /// 
    /// // Set the watched episode count to 5 and the status to watching
    /// changeset.watched(5).status(Status::Watching);
    /// 
    /// // Perform the add action on MyAnimeList and store the new ListEntry created
    /// let entry = anime_list.add(toradora_info, changeset).unwrap();
    /// 
    /// assert_eq!(entry.series_info.title, "Toradora!");
    /// ```
    #[inline]
    pub fn add(&self, info: SeriesInfo, values: Changeset) -> Result<ListEntry, Error> {
        let body = values.build_xml_resp()?;
        request::auth_post_verify(self.mal, RequestURL::Add(info.id), body)?;

        Ok(ListEntry::from_changeset(info, values))
    }

    /// Updates the specified anime on the user's list.
    /// 
    /// This will synchronize the values in the `entry`'s changeset with the `entry`'s general values.
    /// It will also clear the `entry`'s changeset afterwards.
    /// 
    /// Note that if the specified anime is already on the user's list, nothing will happen.
    /// 
    /// # Examples
    /// 
    /// ```no_run
    /// use mal::{MAL, SeriesInfo};
    /// use mal::list::{AnimeList, Changeset, Status};
    /// 
    /// // Create a new MAL instance
    /// let mal = MAL::new("username", "password");
    /// 
    /// // Create a new AnimeList instance
    /// let anime_list = AnimeList::new(&mal);
    /// 
    /// // Get and parse all of the list entries
    /// let entries = anime_list.read_entries().unwrap();
    /// 
    /// // Find Toradora in the list entries
    /// let mut toradora_entry = entries.into_iter().find(|e| e.series_info.id == 4224).unwrap();
    /// 
    /// // Queue the entry items to update.
    /// // In this case, the episode count will be updated to 25, the score will be set to 10, and the status will be set to completed
    /// toradora_entry.changeset
    ///               .watched(25)
    ///               .score(10)
    ///               .status(Status::Completed);
    /// 
    /// // Update the anime on the user's list and clear the modified changeset
    /// anime_list.update(&mut toradora_entry).unwrap();
    /// 
    /// assert_eq!(toradora_entry.watched_episodes, 25);
    /// assert_eq!(toradora_entry.status, Status::Completed);
    /// assert_eq!(toradora_entry.score, 10);
    /// ```
    #[inline]
    pub fn update(&self, entry: &mut ListEntry) -> Result<(), Error> {
        let body = entry.changeset.build_xml_resp()?;
        request::auth_post_verify(self.mal, RequestURL::Update(entry.series_info.id), body)?;

        entry.sync_to_changeset();
        entry.changeset.clear();

        Ok(())
    }
}

fn parse_str_date(date: &str) -> Option<NaiveDate> {
    if date != "0000-00-00" {
        NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()
    } else {
        None
    }
}

/// Represents information about an anime series on a user's list.
#[derive(Debug, Clone)]
pub struct ListEntry {
    /// The general series information.
    pub series_info: SeriesInfo,
    /// The number of episodes watched.
    pub watched_episodes: u32,
    /// The date the user started watching the series.
    pub start_date: Option<NaiveDate>,
    /// The date the user finished watching the series.
    pub finish_date: Option<NaiveDate>,
    /// The current watch status of the series.
    pub status: Status,
    /// The current rating given by the user.
    pub score: u8,
    /// Indicates whether or not the anime is currently being rewatched.
    pub rewatching: bool,
    /// Represents pending changes to make to the `ListEntry`.
    pub changeset: Changeset,
}

impl ListEntry {
    fn from_changeset(info: SeriesInfo, values: Changeset) -> ListEntry {
        macro_rules! gen_info {
            ($($field:ident),+) => {
                ListEntry {
                    series_info: info,
                    $($field: values.$field.unwrap_or(Default::default()),)+
                    changeset: Changeset::new(),
                }
            };
        }

        gen_info! {
            watched_episodes,
            start_date,
            finish_date,
            status,
            score,
            rewatching
        }
    }

    /// Updates the `ListEntry`'s values with ones that are set in its changeset.
    #[inline]
    pub fn sync_to_changeset(&mut self) {
        macro_rules! sync_fields {
            ($($field:ident),+) => {
                $(if let Some(v) = self.changeset.$field {
                    self.$field = v;
                })+
            };
        }

        sync_fields! {
            watched_episodes,
            status,
            start_date,
            finish_date,
            score,
            rewatching
        };
    }
}

impl PartialEq for ListEntry {
    #[inline]
    fn eq(&self, other: &ListEntry) -> bool {
        self.series_info == other.series_info
    }
}

/// Represents which values should be sent to MyAnimeList when updating a `ListEntry`.
/// 
/// Using a builder like this will save bandwidth when updating an anime on a user's list
/// when you don't need to update every field.
#[derive(Debug, Copy, Clone)]
pub struct Changeset {
    /// Represents the watched episode count.
    pub watched_episodes: Option<u32>,
    /// Represents the current watch status of a series.
    pub status: Option<Status>,
    /// Represents the date the series started being watched.
    pub start_date: Option<Option<NaiveDate>>,
    /// Represents the date the series was finished being watched.
    pub finish_date: Option<Option<NaiveDate>>,
    /// Represents the score the user has given the series.
    pub score: Option<u8>,
    /// Indicates whether or not the user is rewatching the series.
    pub rewatching: Option<bool>,
}

impl Changeset {
    /// Creates a new `Changeset`.
    #[inline]
    pub fn new() -> Changeset {
        Changeset {
            watched_episodes: None,
            status: None,
            start_date: None,
            finish_date: None,
            score: None,
            rewatching: None,
        }
    }

    /// Clears all modifications to the `Changeset`.
    #[inline]
    pub fn clear<'a>(&'a mut self) -> &'a mut Changeset {
        *self = Changeset::new();
        self
    }

    /// Sets the `watched_episodes` field.
    #[inline]
    pub fn watched<'a>(&'a mut self, episodes: u32) -> &'a mut Changeset {
        self.watched_episodes = Some(episodes);
        self
    }

    /// Sets the `status` field.
    #[inline]
    pub fn status<'a>(&'a mut self, status: Status) -> &'a mut Changeset {
        self.status = Some(status);
        self
    }

    /// Sets the `start_date` field.
    #[inline]
    pub fn start_date<'a>(&'a mut self, start_date: Option<NaiveDate>) -> &'a mut Changeset {
        self.start_date = Some(start_date);
        self
    }

    /// Sets the `finish_date` field.
    #[inline]
    pub fn finish_date<'a>(&'a mut self, finish_date: Option<NaiveDate>) -> &'a mut Changeset {
        self.finish_date = Some(finish_date);
        self
    }

    /// Sets the `score` field.
    #[inline]
    pub fn score<'a>(&'a mut self, score: u8) -> &'a mut Changeset {
        self.score = Some(score);
        self
    }

    /// Sets the `rewatching` field.
    #[inline]
    pub fn rewatching<'a>(&'a mut self, rewatching: bool) -> &'a mut Changeset {
        self.rewatching = Some(rewatching);
        self
    }

    fn build_xml_resp(&self) -> Result<String, Error> {
        macro_rules! gen_update_xml {
            ($entry:ident, $xml_elem:ident, $($field:ident($val_name:ident): $xml_name:expr => $xml_val:expr),+) => {
                $(if let Some($val_name) = $entry.$field {
                    let mut elem = Element::bare($xml_name);
                    elem.append_text_node($xml_val);
                    $xml_elem.append_child(elem);
                })+
            };
        }

        let mut entry = Element::bare("entry");

        gen_update_xml!(self, entry,
            watched_episodes(num): "episode" => num.to_string(),
            status(status): "status" => (status as i32).to_string(),
            start_date(date): "date_start" => date_to_str(date),
            finish_date(date): "date_finish" => date_to_str(date),
            score(score): "score" => score.to_string(),
            rewatching(v): "enable_rewatching" => (v as u8).to_string()
        );

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
    #[inline]
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

impl Default for Status {
    #[inline]
    fn default() -> Self {
        Status::PlanToWatch
    }
}

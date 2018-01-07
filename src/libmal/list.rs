use chrono::NaiveDate;
use failure::{Error, SyncFailure};
use get_xml_child_text;
use MAL;
use minidom::Element;
use request;
use RequestURL;
use SeriesInfo;
use std::fmt::Debug;

/// Used to perform operations on a user's anime list.
/// 
/// Note that since the `AnimeList` struct stores a reference to a [MAL] instance,
/// the [MAL] instance must live as long as the `AnimeList`.
/// 
/// [MAL]: ../struct.MAL.html
pub struct AnimeList<'a> {
    /// A reference to the MyAnimeList client used to add and update anime on a user's list.
    pub mal: &'a MAL,
}

impl<'a> AnimeList<'a> {
    /// Creates a new instance of the `AnimeList` struct and stores the provided [MAL] reference
    /// so authorization can be handled automatically.
    /// 
    /// [MAL]: ../struct.MAL.html
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
        let resp = request::get_verify(&self.mal.client, RequestURL::AnimeList(&self.mal.username))?.text()?;
        let root: Element = resp.parse().map_err(SyncFailure::new)?;

        let mut entries = Vec::new();

        for child in root.children().skip(1) {
            let get_child = |name| get_xml_child_text(child, name);

            let info = SeriesInfo {
                id: get_child("series_animedb_id")?.parse()?,
                title: get_child("series_title")?,
                episodes: get_child("series_episodes")?.parse()?,
            };

            let entry = ListEntry {
                series_info: info,
                watched_episodes: get_child("my_watched_episodes")?.parse::<u32>()?.into(),
                start_date: parse_str_date(&get_child("my_start_date")?).into(),
                finish_date: parse_str_date(&get_child("my_finish_date")?).into(),
                status: Status::from_i32(get_child("my_status")?.parse()?)?.into(),
                score: get_child("my_score")?.parse::<u8>()?.into(),
                rewatching: (get_child("my_rewatching")?.parse::<u8>()? == 1).into(),
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
    /// use mal::list::{AnimeList, ListEntry, Status};
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
    /// // Create a new anime list entry with Toradora's info
    /// let mut entry = ListEntry::new(toradora_info);
    /// 
    /// // Set the entry's watched episodes to 5 and status to watching
    /// entry.set_watched_episodes(5).set_status(Status::Watching);
    /// 
    /// // Add the entry to the user's anime list
    /// anime_list.add(&entry).unwrap();
    /// ```
    #[inline]
    pub fn add(&self, entry: &ListEntry) -> Result<(), Error> {
        let body = entry.generate_xml()?;

        request::auth_post_verify(self.mal,
            RequestURL::Add(entry.series_info.id),
            &body)?;

        Ok(())
    }

    /// Updates the specified anime on the user's list.
    /// 
    /// If the anime is already on the user's list, nothing will happen.
    /// 
    /// # Examples
    /// 
    /// ```no_run
    /// use mal::{MAL, SeriesInfo};
    /// use mal::list::{AnimeList, ListEntry, Status};
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
    /// // Set new values for the list entry
    /// // In this case, the episode count will be updated to 25, the score will be set to 10, and the status will be set to completed
    /// toradora_entry.set_watched_episodes(25)
    ///               .set_score(10)
    ///               .set_status(Status::Completed);
    /// 
    /// // Update the anime on the user's list and clear the modified changeset
    /// anime_list.update(&mut toradora_entry).unwrap();
    /// 
    /// assert_eq!(toradora_entry.watched_episodes(), 25);
    /// assert_eq!(toradora_entry.status(), Status::Completed);
    /// assert_eq!(toradora_entry.score(), 10);
    /// ```
    #[inline]
    pub fn update(&self, entry: &mut ListEntry) -> Result<(), Error> {
        let body = entry.generate_xml()?;
        
        request::auth_post_verify(self.mal,
            RequestURL::Update(entry.series_info.id),
            &body)?;

        entry.reset_changed_status();
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

#[derive(Debug, Clone)]
struct ChangeTracker<T: Debug + Clone> {
    value: T,
    changed: bool,
}

impl<T: Debug + Clone> ChangeTracker<T> {
    fn new(value: T) -> ChangeTracker<T> {
        ChangeTracker {
            value,
            changed: false,
        }
    }

    fn get(&self) -> &T {
        &self.value
    }

    fn set(&mut self, value: T) {
        self.value = value;
        self.changed = true;
    }
}

impl<T: Debug + Clone> From<T> for ChangeTracker<T> {
    fn from(value: T) -> Self {
        ChangeTracker::new(value)
    }
}

/// Represents information about an anime series on a user's list.
#[derive(Debug, Clone)]
pub struct ListEntry {
    /// The general series information.
    pub series_info: SeriesInfo,
    watched_episodes: ChangeTracker<u32>,
    start_date: ChangeTracker<Option<NaiveDate>>,
    finish_date: ChangeTracker<Option<NaiveDate>>,
    status: ChangeTracker<Status>,
    score: ChangeTracker<u8>,
    rewatching: ChangeTracker<bool>,
}

impl ListEntry {
    /// Creates a new `ListEntry` instance with [SeriesInfo] obtained from [MAL].
    /// 
    /// [MAL]: ../struct.MAL.html
    /// [SeriesInfo]: ../struct.SeriesInfo.html
    /// 
    /// # Examples
    /// 
    /// ```no_run
    /// use mal::MAL;
    /// use mal::list::ListEntry;
    /// 
    /// // Create a new MAL instance
    /// let mal = MAL::new("username", "password");
    /// 
    /// // Search for Toradora on MAL
    /// let mut results = mal.search("Toradora").unwrap();
    /// 
    /// // Select the first result
    /// let toradora_info = results.swap_remove(0);
    /// 
    /// // Create a new ListEntry that represents Toradora with default values
    /// let entry = ListEntry::new(toradora_info);
    /// ```
    #[inline]
    pub fn new(info: SeriesInfo) -> ListEntry {
        ListEntry {
            series_info: info,
            watched_episodes: 0.into(),
            start_date: None.into(),
            finish_date: None.into(),
            status: Status::default().into(),
            score: 0.into(),
            rewatching: false.into(),
        }
    }

    fn generate_xml(&self) -> Result<String, Error> {
        macro_rules! gen_xml {
            ($entry:ident, $xml_elem:ident, $($field:ident($val_name:ident): $xml_name:expr => $xml_val:expr),+) => {
                $(if $entry.$field.changed {
                    let $val_name = $entry.$field.get();

                    let mut elem = Element::bare($xml_name);
                    elem.append_text_node($xml_val);
                    $xml_elem.append_child(elem);
                })+
            };
        }

        let mut entry = Element::bare("entry");

        gen_xml!(self, entry,
            watched_episodes(num): "episode" => num.to_string(),
            status(status): "status" => (*status as i32).to_string(),
            start_date(date): "date_start" => date_to_str(*date),
            finish_date(date): "date_finish" => date_to_str(*date),
            score(score): "score" => score.to_string(),
            rewatching(v): "enable_rewatching" => (*v as u8).to_string()
        );

        let mut buffer = Vec::new();
        entry.write_to(&mut buffer).map_err(SyncFailure::new)?;

        Ok(String::from_utf8(buffer)?)
    }

    fn reset_changed_status(&mut self) {
        macro_rules! reset {
            ($($name:ident),+) => ($(self.$name.changed = false;)+);
        }

        reset! {
            watched_episodes,
            start_date,
            finish_date,
            status,
            score,
            rewatching
        }
    }

    /// Returns the number of episodes watched.
    #[inline]
    pub fn watched_episodes(&self) -> u32 {
        *self.watched_episodes.get()
    }

    /// Sets the watched episode count.
    #[inline]
    pub fn set_watched_episodes(&mut self, watched: u32) -> &mut ListEntry {
        self.watched_episodes.set(watched);
        self
    }

    /// Returns the date the anime started being watched.
    #[inline]
    pub fn start_date(&self) -> &Option<NaiveDate> {
        self.start_date.get()
    }

    /// Sets the date the user started watching the anime.
    #[inline]
    pub fn set_start_date(&mut self, date: Option<NaiveDate>) -> &mut ListEntry {
        self.start_date.set(date);
        self
    }

    /// Returns the date the anime finished being watched.
    #[inline]
    pub fn finish_date(&self) -> &Option<NaiveDate> {
        self.finish_date.get()
    }

    /// Sets the date the user finished watching the anime.
    #[inline]
    pub fn set_finish_date(&mut self, date: Option<NaiveDate>) -> &mut ListEntry {
        self.finish_date.set(date);
        self
    }

    /// Returns the current watch status of the anime.
    #[inline]
    pub fn status(&self) -> Status {
        *self.status.get()
    }

    /// Sets the current watch status for the anime.
    #[inline]
    pub fn set_status(&mut self, status: Status) -> &mut ListEntry {
        self.status.set(status);
        self
    }

    /// Returns the user's score of the anime.
    #[inline]
    pub fn score(&self) -> u8 {
        *self.score.get()
    }

    /// Sets the user's score for the anime.
    #[inline]
    pub fn set_score(&mut self, score: u8) -> &mut ListEntry {
        self.score.set(score);
        self
    }

    /// Returns true if the anime is currently being rewatched.
    #[inline]
    pub fn rewatching(&self) -> bool {
        *self.rewatching.get()
    }

    /// Sets whether or not the user is currently rewatching the anime.
    #[inline]
    pub fn set_rewatching(&mut self, rewatching: bool) -> &mut ListEntry {
        self.rewatching.set(rewatching);
        self
    }
}

impl PartialEq for ListEntry {
    #[inline]
    fn eq(&self, other: &ListEntry) -> bool {
        self.series_info == other.series_info
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

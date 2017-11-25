#[macro_use]
extern crate failure_derive;

pub mod list;

mod request;

extern crate chrono;
extern crate failure;
extern crate minidom;
extern crate reqwest;

use chrono::NaiveDate;
use failure::{Error, SyncFailure};
use list::{AnimeEntry, EntryTag, Status};
use minidom::Element;
use request::RequestURL;
use std::convert::Into;

/// Represents basic information of an anime series on MyAnimeList.
#[derive(Debug, Clone)]
pub struct SeriesInfo {
    /// The ID of the anime series.
    pub id: u32,
    /// The title of the anime series.
    pub title: String,
    /// The number of episodes in the anime series.
    pub episodes: u32,
}

impl PartialEq for SeriesInfo {
    fn eq(&self, other: &SeriesInfo) -> bool {
        self.id == other.id
    }
}

#[derive(Fail, Debug)]
#[fail(display = "unable to find XML node named '{}' in MAL response", _0)]
pub struct MissingXMLNode(pub String);

#[derive(Fail, Debug)]
#[fail(display = "received bad response from MAL: {} {}", _0, _1)]
pub struct BadResponse(pub u16, pub String);

/// Used to interact with the MyAnimeList API with authorization being handled automatically.
#[derive(Debug)]
pub struct MAL {
    /// The user's name on MyAnimeList
    pub username: String,
    password: String,
    client: reqwest::Client,
}

impl MAL {
    /// Creates a new instance of the MAL struct for interacting with the MyAnimeList API.
    ///
    /// If you only need to call `MAL::get_anime_list`, then the `password` field can be an empty string.
    pub fn new<S: Into<String>>(username: S, password: S) -> MAL {
        MAL {
            username: username.into(),
            password: password.into(),
            client: reqwest::Client::new(),
        }
    }

    /// Searches MyAnimeList for an anime and returns all found results.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use mal::MAL;
    ///
    /// let mal = MAL::new("username", "password");
    /// let found = mal.search("Cowboy Bebop").unwrap();
    ///
    /// assert!(found.len() > 0);
    /// ```
    pub fn search(&self, name: &str) -> Result<Vec<SeriesInfo>, Error> {
        let resp = request::auth_get(&self, RequestURL::Search(name))?.text()?;
        let root: Element = resp.parse().map_err(SyncFailure::new)?;

        let mut entries = Vec::new();

        for child in root.children() {
            let get_child = |name| get_xml_child_text(child, name);

            let entry = SeriesInfo {
                id: get_child("id")?.parse()?,
                title: get_child("title")?,
                episodes: get_child("episodes")?.parse()?,
            };

            entries.push(entry);
        }

        Ok(entries)
    }

    /// Retrieves the user's anime list and returns every entry as an AnimeEntry.
    ///
    /// If this is the only function you need to call in `MAL`, you don't need
    /// to provide a valid password when calling `MAL::new`.
    pub fn get_anime_list(&self) -> Result<Vec<AnimeEntry>, Error> {
        let resp = request::auth_get(&self, RequestURL::AnimeList(&self.username))?.text()?;
        let root: Element = resp.parse().map_err(SyncFailure::new)?;

        let mut entries = Vec::new();

        for child in root.children().skip(1) {
            let get_child = |name| get_xml_child_text(child, name);

            let entry = AnimeEntry {
                info: SeriesInfo {
                    id: get_child("series_animedb_id")?.parse()?,
                    title: get_child("series_title")?,
                    episodes: get_child("series_episodes")?.parse()?,
                },
                watched_episodes: get_child("my_watched_episodes")?.parse()?,
                start_date: parse_str_date(&get_child("my_start_date")?),
                end_date: parse_str_date(&get_child("my_finish_date")?),
                status: Status::from_i32(get_child("my_status")?.parse()?)?,
            };

            entries.push(entry);
        }

        Ok(entries)
    }

    /// Adds an anime to the user's list.
    /// If the specified anime is already on the user's list, the function will return an HTTP 400 error.
    ///
    /// # Arguments
    ///
    /// * `id` - The MyAnimeList ID for the anime to add
    /// * `tags` - The values to set on the specified anime
    ///
    /// # Example
    ///
    /// ```no_run
    /// use mal::MAL;
    /// use mal::list::{EntryTag, Status};
    ///
    /// // ID for Cowboy Bebop
    /// let id = 1;
    ///
    /// let mal = MAL::new("username", "password");
    /// mal.add_anime(id, &[EntryTag::Status(Status::Watching)]).unwrap();
    /// ```
    pub fn add_anime(&self, id: u32, tags: &[EntryTag]) -> Result<(), Error> {
        let body = EntryTag::build_xml_resp(tags)?;
        request::auth_post(&self, RequestURL::Add(id), body)?;

        Ok(())
    }

    /// Updates an existing anime on the user's list.
    /// Note that if the specified anime isn't already on the user's list, nothing will happen.
    ///
    /// # Arguments
    ///
    /// * `id` - The MyAnimeList ID for the anime to update
    /// * `tags` - The values to set on the specified anime
    ///
    /// # Example
    ///
    /// ```no_run
    /// use mal::MAL;
    /// use mal::list::{EntryTag, Status};
    ///
    /// // ID for Cowboy Bebop
    /// let id = 1;
    ///
    /// let mal = MAL::new("username", "password");
    /// mal.update_anime(id, &[EntryTag::Episode(5)]).unwrap();
    /// ```
    pub fn update_anime(&self, id: u32, tags: &[EntryTag]) -> Result<(), Error> {
        let body = EntryTag::build_xml_resp(tags)?;
        request::auth_post(&self, RequestURL::Update(id), body)?;

        Ok(())
    }
}

fn get_xml_child_text(elem: &minidom::Element, name: &str) -> Result<String, MissingXMLNode> {
    elem.children()
        .find(|c| c.name() == name)
        .map(|c| c.text())
        .ok_or(MissingXMLNode(name.into()))
}

fn parse_str_date(date: &str) -> Option<NaiveDate> {
    if date != "0000-00-00" {
        NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()
    } else {
        None
    }
}

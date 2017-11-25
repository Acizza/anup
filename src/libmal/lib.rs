#[macro_use]
extern crate failure_derive;

mod request;

extern crate chrono;
extern crate failure;
extern crate minidom;
extern crate reqwest;

pub mod list;

use chrono::NaiveDate;
use failure::{Error, SyncFailure};
use list::{AnimeEntry, EntryTag, Status};
use minidom::Element;
use request::RequestURL;

#[derive(Debug)]
pub struct SeriesInfo {
    pub id: u32,
    pub title: String,
    pub episodes: u32,
}

#[derive(Fail, Debug)]
#[fail(display = "unable to find XML node named '{}' in MAL response", _0)]
pub struct MissingXMLNode(pub String);

#[derive(Fail, Debug)]
#[fail(display = "received bad response from MAL: {} {}", _0, _1)]
pub struct BadResponse(pub u16, pub String);

#[derive(Debug)]
pub struct MAL {
    pub username: String,
    pub password: String,
    client: reqwest::Client,
}

impl MAL {
    pub fn new(username: String, password: String) -> MAL {
        MAL {
            username,
            password,
            client: reqwest::Client::new(),
        }
    }

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
    /// let mal = MAL::new("username".into(), "password".into());
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
    /// let mal = MAL::new("username".into(), "password".into());
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

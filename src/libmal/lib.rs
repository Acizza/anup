#[macro_use]
extern crate failure;
#[macro_use]
extern crate lazy_static;

pub mod list;

mod request;

extern crate chrono;
extern crate minidom;
extern crate reqwest;

use failure::{Error, SyncFailure};
use minidom::Element;
use request::RequestURL;
use reqwest::StatusCode;
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
    #[inline]
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
    /// The user's name on MyAnimeList.
    pub username: String,
    /// The user's password on MyAnimeList.
    pub password: String,
    client: reqwest::Client,
}

impl MAL {
    /// Creates a new instance of the MAL struct for interacting with the MyAnimeList API.
    ///
    /// If you only need to call `MAL::get_anime_list`, then the `password` field can be an empty string.
    #[inline]
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
        let mut resp = request::auth_get(self, RequestURL::Search(name))?;

        if resp.status() == StatusCode::NoContent {
            return Ok(Vec::new());
        }

        let root: Element = resp.text()?.parse().map_err(SyncFailure::new)?;

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

    /// Returns true if the provided account credentials are correct.
    /// 
    /// # Examples
    /// 
    /// ```no_run
    /// use mal::MAL;
    /// 
    /// // Create a new MAL instance
    /// let mal = MAL::new("username", "password");
    /// 
    /// // Verify that the username and password are valid
    /// let valid = mal.verify_credentials().unwrap();
    /// 
    /// assert_eq!(valid, false);
    /// ```
    #[inline]
    pub fn verify_credentials(&self) -> Result<bool, Error> {
        let resp = request::auth_get(self, RequestURL::VerifyCredentials)?;
        Ok(resp.status() == StatusCode::Ok)
    }
}

fn get_xml_child_text(elem: &minidom::Element, name: &str) -> Result<String, MissingXMLNode> {
    elem.children()
        .find(|c| c.name() == name)
        .map(|c| c.text())
        .ok_or_else(|| MissingXMLNode(name.into()))
}

#[macro_use]
extern crate failure_derive;

extern crate chrono;
extern crate failure;
extern crate minidom;
extern crate reqwest;

use chrono::NaiveDate;
use failure::{Error, SyncFailure};
use minidom::Element;
use reqwest::{RequestBuilder, Response, Url};
use std::string::ToString;

pub type ID = u32;

#[derive(Debug)]
enum RequestURL<'a> {
    AnimeList(&'a str),
    Search(&'a str),
    Add(ID),
}

impl<'a> RequestURL<'a> {
    const BASE_URL: &'static str = "https://myanimelist.net";
}

impl<'a> ToString for RequestURL<'a> {
    fn to_string(&self) -> String {
        let mut url = Url::parse(RequestURL::BASE_URL).unwrap();

        match *self {
            RequestURL::AnimeList(ref uname) => {
                url.set_path("/malappinfo.php");

                url.query_pairs_mut()
                    .append_pair("u", &uname)
                    .append_pair("status", "all")
                    .append_pair("type", "anime");
            }
            RequestURL::Search(ref name) => {
                url.set_path("/api/anime/search.xml");
                url.query_pairs_mut().append_pair("q", &name);
            }
            RequestURL::Add(id) => {
                url.set_path(&format!("/api/animelist/add/{}.xml", id));
            }
        }

        url.into_string()
    }
}

#[derive(Debug)]
pub struct SeriesInfo {
    pub id: u32,
    pub title: String,
    pub episodes: u32,
}

#[derive(Debug)]
pub struct ListEntry {
    pub info: SeriesInfo,
    pub watched_episodes: u32,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub status: Status,
}

#[derive(Fail, Debug)]
#[fail(display = "{} does not map to any Status enum variants", _0)]
pub struct InvalidStatus(pub i32);

#[derive(Debug)]
pub enum Status {
    Watching = 1,
    Completed,
    OnHold,
    Dropped,
    PlanToWatch = 6,
}

impl Status {
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
        let resp = self.send_get_auth_req(RequestURL::Search(name))?.text()?;
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

    pub fn get_anime_list(&self) -> Result<Vec<ListEntry>, Error> {
        let resp = self.send_get_auth_req(RequestURL::AnimeList(&self.username))?
            .text()?;

        let root: Element = resp.parse().map_err(SyncFailure::new)?;
        let mut entries = Vec::new();

        for child in root.children().skip(1) {
            let get_child = |name| get_xml_child_text(child, name);

            let entry = ListEntry {
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

    fn send_get_auth_req(&self, req_type: RequestURL) -> Result<Response, Error> {
        self.send_auth_req(self.client.get(&req_type.to_string()))
    }

    fn send_post_auth_req(&self, req_type: RequestURL) -> Result<Response, Error> {
        self.send_auth_req(self.client.post(&req_type.to_string()))
    }

    fn send_auth_req(&self, mut req: RequestBuilder) -> Result<Response, Error> {
        let resp = req.basic_auth(self.username.clone(), Some(self.password.clone()))
            .send()?;

        let status = resp.status();

        if status.is_success() {
            Ok(resp)
        } else {
            let reason = status.canonical_reason().unwrap_or("Unknown Error").into();
            Err(BadResponse(status.as_u16(), reason).into())
        }
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

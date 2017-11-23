#[macro_use]
extern crate failure_derive;

extern crate chrono;
extern crate failure;
extern crate minidom;
extern crate reqwest;

pub mod list;

use failure::{Error, SyncFailure};
use list::ListEntry;
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

#[derive(Fail, Debug)]
#[fail(display = "unable to find XML node named '{}' in MAL response", _0)]
pub struct MissingXMLNode(pub &'static str);

#[derive(Debug)]
pub struct MAL {
    pub username: String,
    password: String,
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
            let get_child = |name| {
                child
                    .children()
                    .find(|c| c.name() == name)
                    .map(|c| c.text())
                    .ok_or(MissingXMLNode(name))
            };

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
        list::get_for_user(&self.username)
    }

    fn send_get_auth_req(&self, req_type: RequestURL) -> reqwest::Result<Response> {
        self.send_auth_req(self.client.get(&req_type.to_string()))
    }

    fn send_post_auth_req(&self, req_type: RequestURL) -> reqwest::Result<Response> {
        self.send_auth_req(self.client.post(&req_type.to_string()))
    }

    // TODO: handle invalid credentials
    fn send_auth_req(&self, mut req: RequestBuilder) -> reqwest::Result<Response> {
        req.basic_auth(self.username.clone(), Some(self.password.clone()))
            .send()
    }
}

#[derive(Debug)]
pub struct SeriesInfo {
    pub id: u32,
    pub title: String,
    pub episodes: u32,
}

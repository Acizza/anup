#[macro_use] extern crate failure_derive;
extern crate failure;
extern crate minidom;
extern crate reqwest;

use std::string::ToString;
use failure::{Error, SyncFailure};
use minidom::Element;
use reqwest::{Url, Response};

#[derive(Fail, Debug)]
#[fail(display = "unable to find XML node named '{}'", _0)]
pub struct MissingXMLNode(&'static str);

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

    pub fn search(&self, name: &str) -> Result<Vec<AnimeEntry>, Error> {
        let resp = self.exec_request(RequestURL::Search(name))?.text()?;
        let root: Element = resp.parse().map_err(SyncFailure::new)?;

        let mut entries = Vec::new();

        for child in root.children() {
            let get_child = |name| {
                child.children()
                     .find(|c| c.name() == name)
                     .map(|c| c.text())
                     .ok_or(MissingXMLNode(name))
            };

            let entry = AnimeEntry {
                id:       get_child("id")?.parse()?,
                title:    get_child("title")?,
                episodes: get_child("episodes")?.parse()?,
            };

            entries.push(entry);
        }

        Ok(entries)
    }

    fn exec_request(&self, req_type: RequestURL) -> reqwest::Result<Response> {
        let mut req = match req_type {
            RequestURL::Search(_) => self.client.get(&req_type.to_string()),
            RequestURL::Add(_)    => self.client.post(&req_type.to_string()),
        };

        req.basic_auth(self.username.clone(), Some(self.password.clone()))
           .send()
    }
}

#[derive(Debug)]
pub struct AnimeEntry {
    pub id:       u32,
    pub title:    String,
    pub episodes: u32,
}

pub type ID = u32;

#[derive(Debug)]
enum RequestURL<'a> {
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
            RequestURL::Search(ref name) => {
                url.set_path("/api/anime/search.xml");
                url.query_pairs_mut().append_pair("q", &name);
            },
            RequestURL::Add(id) => {
                url.set_path(&format!("/api/animelist/add/{}.xml", id));
            },
        }

        url.into_string()
    }
}
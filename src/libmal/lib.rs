#[macro_use] extern crate failure_derive;
extern crate failure;
extern crate reqwest;
extern crate rquery;

use std::io::Read;
use std::string::ToString;
use failure::Error;
use reqwest::{Url, Response};
use rquery::Document;

#[derive(Debug, Fail)]
pub enum XmlError {
    #[fail(display = "failed to load XML: {:?}", _0)]
    Load(rquery::DocumentError),
    #[fail(display = "failed to parse XML: {:?}", _0)]
    Parse(rquery::SelectError),
}

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

    pub fn search(&self, name: &str) -> Result<Vec<AnimeEntry>, Error> {
        let resp = self.perform_request(Request::Find(name))?;
        let entries = AnimeEntry::all_from_xml(resp)?;

        Ok(entries)
    }

    fn perform_request(&self, req_type: Request) -> Result<Response, Error> {
        let mut req = match req_type {
            Request::Find(_) => self.client.get(&req_type.to_string()),
            Request::Add(_)  => self.client.post(&req_type.to_string()),
        };

        let resp = req
            .basic_auth(
                self.username.clone(),
                Some(self.password.clone()))
            .send()?;

        Ok(resp)
    }
}

#[derive(Debug)]
pub struct AnimeEntry {
    pub id:       u32,
    pub title:    String,
    pub episodes: u32,
}

impl AnimeEntry {
    fn all_from_xml<R: Read>(xml: R) -> Result<Vec<AnimeEntry>, Error> {
        let doc = Document::new_from_xml_stream(xml)
            .map_err(|e| XmlError::Load(e))?;

        let mut entries = Vec::new();

        for entry in doc.select_all("entry").map_err(|e| XmlError::Parse(e))? {
            let select = |n| entry.select(n).map_err(|e| XmlError::Parse(e));

            let anime_entry = AnimeEntry {
                id:       select("id")?.text().parse()?,
                title:    select("title")?.text().clone(),
                episodes: select("episodes")?.text().parse()?,
            };

            entries.push(anime_entry);
        }
        
        Ok(entries)
    }
}

pub type ID = u32;

#[derive(Debug)]
enum Request<'a> {
    Find(&'a str),
    Add(ID),
}

impl<'a> Request<'a> {
    const BASE_URL: &'static str = "https://myanimelist.net";
}

impl<'a> ToString for Request<'a> {
    fn to_string(&self) -> String {
        let mut url = Url::parse(Request::BASE_URL).unwrap();

        match *self {
            Request::Find(ref name) => {
                url.set_path("/api/anime/search.xml");
                url.query_pairs_mut().append_pair("q", &name);
            },
            Request::Add(id) => {
                url.set_path(&format!("/api/animelist/add/{}.xml", id));
            },
        }

        url.into_string()
    }
}
#[macro_use] extern crate failure_derive;
extern crate failure;
extern crate reqwest;
extern crate rquery;

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
        let resp = self.exec_request(RequestURL::Search(name))?;

        let doc = Document::new_from_xml_stream(resp)
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
#[macro_use] extern crate error_chain;
extern crate reqwest;

use std::string::ToString;
use reqwest::Url;

error_chain! {
    foreign_links {
        Reqwest(reqwest::Error);
    }
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

    pub fn search(&self, name: &str) -> Result<Vec<AnimeEntry>> {
        let body = self.perform_request(Request::Find(name))?;
        println!("{}", body);

        Ok(vec![])
    }

    fn perform_request(&self, req_type: Request) -> Result<String> {
        let mut req = match req_type {
            Request::Find(_) => self.client.get(&req_type.to_string()),
            Request::Add(_)  => self.client.post(&req_type.to_string()),
        };

        let mut resp = req
            .basic_auth(
                self.username.clone(),
                Some(self.password.clone()))
            .send()?;

        Ok(resp.text()?)
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
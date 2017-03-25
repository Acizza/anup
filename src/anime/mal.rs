extern crate hyper;
extern crate hyper_native_tls;

use std::io::Read;
use self::hyper::Url;
use self::hyper::client::Client;
use self::hyper::header::{Authorization, Basic};
use self::hyper::net::HttpsConnector;
use self::hyper_native_tls::NativeTlsClient;

const BASE_URL: &'static str = "https://myanimelist.net";

enum RequestType {
    Search(String),
}

fn get_url(req_type: &RequestType) -> String {
    let mut url = Url::parse(BASE_URL).unwrap();

    match *req_type {
        RequestType::Search(ref name) => {
            url.set_path("/api/anime/search.xml");
            url.query_pairs_mut().append_pair("q", &name);
            url.into_string()
        },
    }
}

fn perform_request(req_type: RequestType, username: String, password: String) -> String {
    let url = get_url(&req_type);

    // TODO: Isolate?
    let ssl = NativeTlsClient::new().unwrap();
    let connector = HttpsConnector::new(ssl);
    let client = Client::with_connector(connector);

    let mut body = String::new();

    let mut res = client
        .get(&url)
        .header(Authorization(
            Basic {
                username: username,
                password: Some(password),
            }
        ))
        .send()
        .unwrap();

    res.read_to_string(&mut body).unwrap();
    body
}

#[derive(Debug)]
pub struct AnimeInfo {
    pub id:             u32,
    pub name:           String,
    pub episodes:       u32,
    pub episode_length: u32,
}

impl AnimeInfo {
    pub fn request(name: &str, username: String, password: String) -> AnimeInfo {
        println!("{:?}", perform_request(
            RequestType::Search(name.into()),
            username,
            password)
        );

        AnimeInfo {
            id: 0,
            name: "".into(),
            episodes: 0,
            episode_length: 0,
        }
    }
}
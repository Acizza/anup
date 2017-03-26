extern crate hyper;
extern crate hyper_native_tls;

use self::hyper::Url;
use self::hyper::client::{Client, Response};
use self::hyper::header::{Authorization, Basic};
use self::hyper::net::HttpsConnector;
use self::hyper_native_tls::NativeTlsClient;

const BASE_URL: &'static str = "https://myanimelist.net";

pub type Name = String;

pub enum RequestType {
    Search(Name),
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

pub fn execute(req_type: RequestType, username: String, password: String) -> Response {
    let url = get_url(&req_type);

    // TODO: Isolate?
    let ssl = NativeTlsClient::new().unwrap();
    let connector = HttpsConnector::new(ssl);
    let client = Client::with_connector(connector);

    client.get(&url)
        .header(Authorization(
            Basic {
                username: username,
                password: Some(password),
            }
        ))
        .send()
        .unwrap()
}
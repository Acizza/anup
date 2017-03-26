extern crate hyper;
extern crate hyper_native_tls;

use self::hyper::Url;
use self::hyper::client::{Client, Response};
use self::hyper::header::{Authorization, Basic};
use self::hyper::net::HttpsConnector;
use self::hyper::status::StatusCode;
use self::hyper_native_tls::NativeTlsClient;

const BASE_URL: &'static str = "https://myanimelist.net";

error_chain! {
    foreign_links {
        Hyper(self::hyper::error::Error);
        HyperParse(self::hyper::error::ParseError);
        HyperTLS(self::hyper_native_tls::native_tls::Error);
    }

    errors {
        InvalidPassword {
            description("invalid password")
            display("invalid password")
        }

        BadStatus(code: StatusCode) {
            description("received a non-ok status")
            display("received a non-ok status code from MAL: {:?}", code)
        }
    }
}

pub type Name = String;

pub enum RequestType {
    Search(Name),
}

fn get_url(req_type: &RequestType) -> Result<String> {
    let mut url = Url::parse(BASE_URL)?;

    match *req_type {
        RequestType::Search(ref name) => {
            url.set_path("/api/anime/search.xml");
            url.query_pairs_mut().append_pair("q", &name);

            Ok(url.into_string())
        },
    }
}

pub fn execute(req_type: RequestType, username: String, password: String) -> Result<Response> {
    let url = get_url(&req_type)?;

    // TODO: Isolate?
    let ssl = NativeTlsClient::new()?;
    let connector = HttpsConnector::new(ssl);
    let client = Client::with_connector(connector);

    let res = client
        .get(&url)
        .header(Authorization(
            Basic {
                username: username,
                password: Some(password),
            }
        ))
        .send()?;

    match res.status {
        StatusCode::Ok           => Ok(res),
        StatusCode::Unauthorized => bail!(ErrorKind::InvalidPassword),
        other                    => bail!(ErrorKind::BadStatus(other)),
    }
}
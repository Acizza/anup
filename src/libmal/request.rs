extern crate hyper;
extern crate hyper_native_tls;

use self::hyper::Url;
use self::hyper::client::{Client, Response};
use self::hyper::header::{Authorization, Basic, ContentType};
use self::hyper::net::HttpsConnector;
use self::hyper::status::StatusCode;
use self::hyper_native_tls::NativeTlsClient;

pub const BASE_URL: &'static str = "https://myanimelist.net";

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

pub type Name     = String;
pub type Username = String;
pub type ID       = u32;

#[derive(Debug)]
pub enum RequestType {
    Find(Name),
    GetList(Username),
    Add(ID),
    Update(ID),
}

impl RequestType {
    fn get_url(&self) -> Result<String> {
        let mut url = Url::parse(BASE_URL)?;
        use self::RequestType::*;

        match *self {
            Find(ref name) => {
                url.set_path("/api/anime/search.xml");
                url.query_pairs_mut().append_pair("q", &name);
                Ok(url.into_string())
            },
            GetList(ref name) => {
                url.set_path("/malappinfo.php");

                url.query_pairs_mut()
                    .append_pair("u", name)
                    .append_pair("status", "all")
                    .append_pair("type", "anime");

                Ok(url.into_string())
            },
            Add(id) => {
                url.set_path(&format!("/api/animelist/add/{}.xml", id));
                Ok(url.into_string())
            },
            Update(id) => {
                url.set_path(&format!("/api/animelist/update/{}.xml", id));
                Ok(url.into_string())
            },
        }
    }
}

pub fn get(req_type: RequestType, username: String, password: Option<String>)
    -> Result<Response> {

    let url = req_type.get_url()?;

    // TODO: Isolate?
    let ssl       = NativeTlsClient::new()?;
    let connector = HttpsConnector::new(ssl);
    let client    = Client::with_connector(connector);

    let request = {
        let mut req = client.get(&url);

        match password {
            Some(password) => {
                req = req.header(Authorization(
                    Basic {
                        username: username,
                        password: Some(password),
                    }
                ));
            },
            None => (),
        }

        req.send()?
    };

    match request.status {
        StatusCode::Ok           => Ok(request),
        StatusCode::Unauthorized => bail!(ErrorKind::InvalidPassword),
        other                    => bail!(ErrorKind::BadStatus(other)),
    }
}

pub fn post(req_type: RequestType, body: &str, username: String, password: String)
    -> Result<Response> {
    
    let url = req_type.get_url()?;

    // TODO: Isolate?
    let ssl       = NativeTlsClient::new()?;
    let connector = HttpsConnector::new(ssl);
    let client    = Client::with_connector(connector);

    let body = match req_type {
        RequestType::Add(_) |
        RequestType::Update(_) => format!("data={}", body),
        RequestType::Find(_) |
        RequestType::GetList(_) => body.into(),
    };

    let request = client
        .post(&url)
        .header(ContentType("application/x-www-form-urlencoded".parse().unwrap()))
        .body(&body)
        .header(Authorization(
            Basic {
                username: username,
                password: Some(password),
            }
        ))
        .send()?;

    match request.status {
        StatusCode::Ok |
        StatusCode::Created      => Ok(request),
        StatusCode::Unauthorized => bail!(ErrorKind::InvalidPassword),
        other                    => bail!(ErrorKind::BadStatus(other)),
    }
}
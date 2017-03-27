extern crate hyper;
extern crate hyper_native_tls;

use self::hyper::client::{Client, Response};
use self::hyper::header::{Authorization, Basic, ContentType};
use self::hyper::net::HttpsConnector;
use self::hyper::status::StatusCode;
use self::hyper_native_tls::NativeTlsClient;
use super::RequestType;

error_chain! {
    foreign_links {
        Hyper(self::hyper::error::Error);
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

pub fn get(req_type: RequestType, username: String, password: Option<String>)
    -> Result<Response> {

    let url = req_type.get_url();

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
    
    let url = req_type.get_url();

    // TODO: Isolate?
    let ssl       = NativeTlsClient::new()?;
    let connector = HttpsConnector::new(ssl);
    let client    = Client::with_connector(connector);

    let body = match req_type {
        RequestType::Add(_) => format!("data={}", body),
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
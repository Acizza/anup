use failure::Error;
use MAL;
use reqwest::{RequestBuilder, Response, Url};
use reqwest::header::{ContentType, Headers};

pub type ID = u32;

#[derive(Debug)]
pub enum RequestURL<'a> {
    AnimeList(&'a str),
    Search(&'a str),
    Add(ID),
}

impl<'a> RequestURL<'a> {
    pub const BASE_URL: &'static str = "https://myanimelist.net";
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
#[fail(display = "received bad response from MAL: {} {}", _0, _1)]
pub struct BadResponse(pub u16, pub String);

pub fn auth_get(mal: &MAL, req_type: RequestURL) -> Result<Response, Error> {
    send_auth_req(mal, &mut mal.client.get(&req_type.to_string()))
}

pub fn auth_post(mal: &MAL, req_type: RequestURL, body: String) -> Result<Response, Error> {
    let mut headers = Headers::new();
    headers.set(ContentType::form_url_encoded());

    send_auth_req(
        mal,
        mal.client
            .post(&req_type.to_string())
            .body(format!("data={}", body))
            .headers(headers),
    )
}

fn send_auth_req(mal: &MAL, req: &mut RequestBuilder) -> Result<Response, Error> {
    let resp = req.basic_auth(mal.username.clone(), Some(mal.password.clone()))
        .send()?;

    let status = resp.status();

    if status.is_success() {
        Ok(resp)
    } else {
        let reason = status.canonical_reason().unwrap_or("Unknown Error").into();
        Err(BadResponse(status.as_u16(), reason).into())
    }
}

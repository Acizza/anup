use failure::Error;
use MAL;
use reqwest::{RequestBuilder, Response, StatusCode, Url};
use reqwest::header::{ContentType, Headers};

pub type ID = u32;

#[derive(Debug)]
pub enum RequestURL<'a> {
    AnimeList(&'a str),
    Search(&'a str),
    Add(ID),
    Update(ID),
    VerifyCredentials,
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
            RequestURL::Update(id) => {
                url.set_path(&format!("/api/animelist/update/{}.xml", id));
            }
            RequestURL::VerifyCredentials => {
                url.set_path("/api/account/verify_credentials.xml");
            }
        }

        url.into_string()
    }
}

pub fn get(mal: &MAL, req_type: RequestURL) -> Result<Response, Error> {
    Ok(mal.client.get(&req_type.to_string()).send()?)
}

pub fn get_verify(mal: &MAL, req_type: RequestURL) -> Result<Response, Error> {
    let resp = get(mal, req_type)?;
    verify_good_response(&resp)?;

    Ok(resp)
}

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

pub fn auth_post_verify(mal: &MAL, req_type: RequestURL, body: String) -> Result<Response, Error> {
    let resp = auth_post(mal, req_type, body)?;
    verify_good_response(&resp)?;

    Ok(resp)
}

fn send_auth_req(mal: &MAL, req: &mut RequestBuilder) -> Result<Response, Error> {
    let resp = req.basic_auth(mal.username.clone(), Some(mal.password.clone()))
        .send()?;

    Ok(resp)
}

#[derive(Fail, Debug)]
#[fail(display = "received bad response from MAL: {} {}", _0, _1)]
pub struct BadResponse(pub u16, pub String);

pub fn verify_good_response(resp: &Response) -> Result<(), BadResponse> {
    match resp.status() {
        StatusCode::Ok | StatusCode::Created => Ok(()),
        status => {
            let reason = status.canonical_reason().unwrap_or("Unknown Error").into();
            Err(BadResponse(status.as_u16(), reason).into())
        }
    }
}

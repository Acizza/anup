use super::{AnimeInfo, SyncBackend};
use config::Config;
use error::BackendError;
use input;
use reqwest::header::{Authorization, Bearer, ContentType, Headers};
use reqwest::{Client, Response};
use serde_json;
use std::io;
use std::process::{Command, ExitStatus};

const LOGIN_URL: &str =
    "https://anilist.co/api/v2/oauth/authorize?client_id=427&response_type=token";

const API_URL: &str = "https://graphql.anilist.co";

pub struct Anilist {
    client: Client,
    access_token: String,
}

impl Anilist {
    fn send_request(
        &self,
        query_str: &str,
        variables: &serde_json::Value,
    ) -> Result<Response, BackendError> {
        let body = json!({
            "query": query_str,
            "variables": variables,
        }).to_string();

        let mut headers = Headers::new();
        headers.set(ContentType::json());
        headers.set(Authorization(Bearer {
            token: self.access_token.to_owned(),
        }));

        let response = self
            .client
            .post(API_URL)
            .header(ContentType::json())
            .body(body)
            .send()?;
        Ok(response)
    }
}

impl SyncBackend for Anilist {
    fn init(config: &mut Config) -> Result<Anilist, BackendError> {
        let access_token = match config.user.access_token {
            Some(_) => config.user.decode_access_token()?,
            None => {
                // TODO: add better error reporting
                open_url(LOGIN_URL)?;

                println!("please authorize your account in the opened browser tab and paste the code below:");
                let token = input::read_line()?;
                config.user.encode_access_token(&token);

                token
            }
        };

        let instance = Anilist {
            client: Client::new(),
            access_token,
        };

        Ok(instance)
    }

    fn find_series_by_name(&self, name: &str) -> Result<Vec<AnimeInfo>, BackendError> {
        let query = r#"
            query ($name: String) {
                Page (page: 1, perPage: 30) {
                    media (search: $name, type: ANIME) {
                        id
                        title {
                            romaji
                        }
                        episodes
                    }
                }
            }
        "#;

        let vars = json!({ "name": name });

        let resp: serde_json::Value = {
            let text = self.send_request(query, &vars)?.text()?;
            serde_json::from_str(&text)?
        };

        use serde_json::Value;
        let mut series = Vec::new();

        match resp["data"]["Page"]["media"] {
            Value::Array(ref entries) => {
                for entry in entries {
                    let series_info: MediaData = serde_json::from_value(entry.clone())?;
                    series.push(series_info.into());
                }
            }
            _ => return Err(BackendError::InvalidJsonResponse),
        }

        Ok(series)
    }
}

#[derive(Deserialize)]
struct MediaData {
    id: u32,
    title: Title,
    episodes: u32,
}

#[derive(Deserialize)]
struct Title {
    romaji: String,
}

impl Into<AnimeInfo> for MediaData {
    fn into(self) -> AnimeInfo {
        AnimeInfo {
            id: self.id,
            title: self.title.romaji,
            episodes: self.episodes,
        }
    }
}

fn open_url(url: &str) -> io::Result<ExitStatus> {
    // TODO: add support for Windows / macOS
    Command::new("xdg-open").arg(url).status()
}

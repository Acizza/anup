mod serialize;

use self::serialize::{Media, MediaDate, MediaListEntry, MediaStatus, ScoreFormat, User};
use super::{AnimeEntry, AnimeInfo, ScoreParser, Status, SyncBackend};
use config::Config;
use error::BackendError;
use input;
use process;
use reqwest::header::{Accept, Authorization, Bearer, ContentType, Headers};
use reqwest::{Client, Response};
use serde_json as json;
use std::borrow::Cow;

const LOGIN_URL: &str =
    "https://anilist.co/api/v2/oauth/authorize?client_id=427&response_type=token";

const API_URL: &str = "https://graphql.anilist.co";

macro_rules! send_query {
    ($backend:expr, $query_str:expr, {$($vars:tt)*}, $($response_root:expr)=>*) => {{
        let vars = json!({
            $($vars)*
        });

        $backend.send_json_request($query_str, &vars).map(|json| json$([$response_root])*.clone())
    }};
}

pub struct AniList {
    client: Client,
    user: User,
    access_token: String,
}

impl AniList {
    fn send_request(
        &self,
        query_str: &str,
        variables: &json::Value,
    ) -> Result<Response, BackendError> {
        let body = json!({
            "query": query_str,
            "variables": variables,
        }).to_string();

        let mut headers = Headers::new();
        headers.set(ContentType::json());
        headers.set(Accept::json());
        headers.set(Authorization(Bearer {
            token: self.access_token.to_owned(),
        }));

        let response = self
            .client
            .post(API_URL)
            .headers(headers)
            .body(body)
            .send()?;

        Ok(response)
    }

    fn send_json_request(
        &self,
        query_str: &str,
        variables: &json::Value,
    ) -> Result<json::Value, BackendError> {
        let text = self.send_request(query_str, variables)?.text()?;
        let json: json::Value = json::from_str(&text)?;

        if json["errors"] != json::Value::Null {
            // TODO: add error chaining
            let err = &json["errors"][0];

            let msg = err["message"].as_str().unwrap_or("unknown error");
            let status_code = err["status"].as_u64().unwrap_or(0) as u32;

            return Err(BackendError::BadResponse(status_code, msg.into()));
        }

        Ok(json)
    }

    fn request_user_info(&self) -> Result<User, BackendError> {
        let resp = send_query!(self,
            r#"
                query {
                    Viewer {
                        id
                        mediaListOptions {
                            scoreFormat
                        }
                    }
                }
            "#,
            {},
            "data" => "Viewer"
        )?;

        let user = json::from_value(resp)?;
        Ok(user)
    }

    fn prompt_for_access_token(open_url: bool) -> Result<String, BackendError> {
        if open_url {
            try_open_url(LOGIN_URL);
        }

        println!(
            "please authorize your account in the opened browser tab and paste the code below:"
        );

        let token = input::read_line()?;
        Ok(token)
    }

    fn login(&mut self, is_first_launch: bool, config: &mut Config) -> Result<(), BackendError> {
        let mut times_token_incorrect = 0;

        loop {
            match self.request_user_info() {
                Ok(user) => {
                    self.user = user;
                    break;
                }
                // As bad as checking for a specific error via its message is, the API does not provide
                // anything else to narrow it down to an invalid token error
                Err(BackendError::BadResponse(400, ref msg))
                    if msg.to_lowercase() == "invalid token" =>
                {
                    times_token_incorrect += 1;
                    println!("\ninvalid access token");

                    let should_open_url = !is_first_launch && times_token_incorrect <= 1;
                    let token = AniList::prompt_for_access_token(should_open_url)?;

                    self.access_token = token;
                }
                Err(err) => return Err(err),
            }
        }

        if times_token_incorrect > 0 {
            config.anilist.token.encode(&self.access_token);
        }

        Ok(())
    }
}

impl SyncBackend for AniList {
    fn name() -> &'static str {
        "AniList"
    }

    fn init(config: &mut Config) -> Result<AniList, BackendError> {
        let is_first_launch = !config.anilist.token.is_set();

        let access_token = if is_first_launch {
            let token = AniList::prompt_for_access_token(true)?;
            config.anilist.token.encode(&token);
            token
        } else {
            config.anilist.token.decode()?
        };

        let mut anilist = AniList {
            client: Client::new(),
            user: User::default(),
            access_token,
        };

        anilist.login(is_first_launch, config)?;

        Ok(anilist)
    }

    fn search_by_name(&self, name: &str) -> Result<Vec<AnimeInfo>, BackendError> {
        let resp = send_query!(self,
            r#"
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
            "#,
            { "name": name },
            "data" => "Page" => "media"
        )?;

        use self::json::Value;
        let mut series = Vec::new();

        match resp {
            Value::Array(ref entries) => {
                for entry in entries {
                    let series_info: Media = json::from_value(entry.clone())?;
                    series.push(series_info.into());
                }
            }
            _ => return Err(BackendError::InvalidJsonResponse),
        }

        Ok(series)
    }

    fn get_series_info_by_id(&self, id: u32) -> Result<AnimeInfo, BackendError> {
        let resp = send_query!(self,
            r#"
                query ($id: Int) {
                    Media (id: $id) {
                        id
                        title {
                            romaji
                        }
                        episodes
                    }
                }
            "#,
            { "id": id },
            "data" => "Media"
        )?;

        let info: Media = json::from_value(resp)?;
        Ok(info.into())
    }

    fn get_list_entry(&self, info: AnimeInfo) -> Result<Option<AnimeEntry>, BackendError> {
        let resp = send_query!(self,
            r#"
                query ($id: Int, $userID: Int) {
                    MediaList(mediaId: $id, userId: $userID) {
                        progress
                        status
                        score
                        startedAt {
                            year
                            month
                            day
                        }
                        completedAt {
                            year
                            month
                            day
                        }
                    }
                }
            "#,
            { "id": info.id, "userID": self.user.id },
            "data" => "MediaList"
        );

        match resp {
            Ok(entry_json) => {
                let media_entry: MediaListEntry = json::from_value(entry_json)?;
                Ok(Some(media_entry.into_generic_entry(info)))
            }
            Err(BackendError::BadResponse(404, _)) => Ok(None),
            Err(err) => Err(err),
        }
    }

    fn update_list_entry(&self, entry: &AnimeEntry) -> Result<(), BackendError> {
        send_query!(self,
            r#"
                mutation (
                    $mediaId: Int,
                    $watched_eps: Int,
                    $score: Float,
                    $status: MediaListStatus,
                    $start_date: FuzzyDateInput,
                    $finish_date: FuzzyDateInput) {

                    SaveMediaListEntry (
                        mediaId: $mediaId,
                        progress: $watched_eps,
                        score: $score,
                        status: $status,
                        startedAt: $start_date,
                        completedAt: $finish_date) {
                            
                        mediaId
                    }
                }
            "#,
            {
                "mediaId": entry.info.id,
                "watched_eps": entry.watched_episodes,
                "score": entry.score.unwrap_or(0.0),
                "status": MediaStatus::from(entry.status),
                "start_date": MediaDate::from_date(entry.start_date),
                "finish_date": MediaDate::from_date(entry.finish_date),
            },
        )?;

        Ok(())
    }
}

impl ScoreParser for AniList {
    fn formatted_score_range(&self) -> (Cow<str>, Cow<str>) {
        match self.user.list_options.score_format {
            ScoreFormat::Point3 => (":(".into(), ":)".into()),
            ScoreFormat::Point10Decimal => ("1.0".into(), "10.0".into()),
            format => ("1".into(), format.max_score().to_string().into()),
        }
    }

    fn parse_score(&self, input: &str) -> Result<f32, BackendError> {
        match self.user.list_options.score_format {
            ScoreFormat::Point3 => match input {
                ":(" => Ok(1.0),
                ":|" => Ok(2.0),
                ":)" => Ok(3.0),
                _ => Err(BackendError::UnknownScoreValue(input.into())),
            },
            format => {
                let value = input.parse::<f32>()?;
                let max_score = f32::from(format.max_score());

                if value < 1.0 || value > max_score {
                    return Err(BackendError::OutOfRangeScore);
                }

                Ok(value)
            }
        }
    }

    fn format_score(&self, raw_score: f32) -> Result<String, BackendError> {
        match self.user.list_options.score_format {
            ScoreFormat::Point3 => match raw_score.round() as u32 {
                1 => Ok(":(".into()),
                2 => Ok(":|".into()),
                3 => Ok(":)".into()),
                _ => Err(BackendError::OutOfRangeScore),
            },
            ScoreFormat::Point5 | ScoreFormat::Point10 | ScoreFormat::Point100 => {
                let value = raw_score.round() as u32;
                Ok(value.to_string())
            }
            ScoreFormat::Point10Decimal => Ok(raw_score.to_string()),
        }
    }
}

fn try_open_url(url: &str) {
    match process::open_with_default(url) {
        Ok(status) if status.success() => (),
        result => {
            eprintln!(
                "failed to open URL in default browser. please open it manually: {}",
                url
            );

            if let Err(err) = result {
                eprintln!("error message: {}", err);
            }
        }
    }
}

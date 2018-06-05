use super::{AnimeEntry, AnimeInfo, Status, SyncBackend};
use chrono::{Date, Datelike, Local, NaiveDate, TimeZone};
use config::Config;
use error::BackendError;
use input;
use reqwest::header::{Accept, Authorization, Bearer, ContentType, Headers};
use reqwest::{Client, Response};
use serde_json as json;
use std::io;
use std::process::{Command, ExitStatus};

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

pub struct Anilist {
    client: Client,
    user_id: u32,
    access_token: String,
}

impl Anilist {
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

    fn request_user_id(&self) -> Result<u32, BackendError> {
        let resp = send_query!(self,
            r#"
                query {
                    Viewer {
                        id
                    }
                }
            "#,
            {},
            "data" => "Viewer" => "id"
        )?;

        let id = json::from_value(resp)?;
        Ok(id)
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
            match self.request_user_id() {
                Ok(user_id) => {
                    self.user_id = user_id;
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
                    let token = Anilist::prompt_for_access_token(should_open_url)?;

                    self.access_token = token;
                }
                Err(err) => return Err(err),
            }
        }

        if times_token_incorrect > 0 {
            config.user.encode_access_token(&self.access_token);
        }

        Ok(())
    }
}

impl SyncBackend for Anilist {
    fn name() -> &'static str {
        "AniList"
    }

    fn max_score(&self) -> u8 {
        // TODO: add support for other scoring types
        10
    }

    fn init(config: &mut Config) -> Result<Anilist, BackendError> {
        let is_first_launch = config.user.access_token.is_none();

        let access_token = if is_first_launch {
            let token = Anilist::prompt_for_access_token(true)?;
            config.user.encode_access_token(&token);
            token
        } else {
            config.user.decode_access_token()?
        };

        let mut anilist = Anilist {
            client: Client::new(),
            user_id: 0,
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
                    let series_info: MediaData = json::from_value(entry.clone())?;
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

        let info: MediaData = json::from_value(resp)?;
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
            { "id": info.id, "userID": self.user_id },
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
                mutation ($mediaId: Int, $watched_eps: Int, $score: Float, $status: MediaListStatus, $start_date: FuzzyDateInput, $finish_date: FuzzyDateInput) {
                    SaveMediaListEntry (mediaId: $mediaId, progress: $watched_eps, score: $score, status: $status, startedAt: $start_date, completedAt: $finish_date) {
                        mediaId
                    }
                }
            "#,
            {
                "mediaId": entry.info.id,
                "watched_eps": entry.watched_episodes,
                "score": entry.score,
                "status": MediaStatus::from(entry.status),
                "start_date": MediaDate::from_date(entry.start_date),
                "finish_date": MediaDate::from_date(entry.finish_date),
            },
        )?;

        Ok(())
    }
}

fn open_url(url: &str) -> io::Result<ExitStatus> {
    #[cfg(target_os = "windows")]
    const LAUNCH_PROGRAM: &str = "start";
    #[cfg(target_os = "macos")]
    const LAUNCH_PROGRAM: &str = "open";
    #[cfg(target_os = "linux")]
    const LAUNCH_PROGRAM: &str = "xdg-open";

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    compile_error!("support for opening URL's not implemented for this platform");

    Command::new(LAUNCH_PROGRAM).arg(url).status()
}

fn try_open_url(url: &str) {
    match open_url(url) {
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

#[derive(Deserialize)]
struct MediaData {
    id: u32,
    title: Title,
    episodes: Option<u32>,
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

#[derive(Deserialize)]
struct MediaListEntry {
    progress: u32,
    status: MediaStatus,
    score: f32,
    #[serde(rename = "startedAt")]
    start_date: MediaDate,
    #[serde(rename = "completedAt")]
    finish_date: MediaDate,
}

impl MediaListEntry {
    fn into_generic_entry(self, info: AnimeInfo) -> AnimeEntry {
        AnimeEntry {
            info,
            watched_episodes: self.progress,
            score: self.score,
            status: self.status.into(),
            start_date: self.start_date.into_date(),
            finish_date: self.finish_date.into_date(),
        }
    }
}

#[derive(Serialize, Deserialize, Copy, Clone, PartialEq)]
enum MediaStatus {
    #[serde(rename = "CURRENT")]
    Current,
    #[serde(rename = "COMPLETED")]
    Completed,
    #[serde(rename = "PAUSED")]
    Paused,
    #[serde(rename = "DROPPED")]
    Dropped,
    #[serde(rename = "PLANNING")]
    Planning,
    #[serde(rename = "REPEATING")]
    Repeating,
}

impl Into<Status> for MediaStatus {
    fn into(self) -> Status {
        match self {
            MediaStatus::Current => Status::Watching,
            MediaStatus::Completed => Status::Completed,
            MediaStatus::Paused => Status::OnHold,
            MediaStatus::Dropped => Status::Dropped,
            MediaStatus::Planning => Status::PlanToWatch,
            MediaStatus::Repeating => Status::Rewatching,
        }
    }
}

impl From<Status> for MediaStatus {
    fn from(status: Status) -> MediaStatus {
        match status {
            Status::Watching => MediaStatus::Current,
            Status::Completed => MediaStatus::Completed,
            Status::OnHold => MediaStatus::Paused,
            Status::Dropped => MediaStatus::Dropped,
            Status::PlanToWatch => MediaStatus::Planning,
            Status::Rewatching => MediaStatus::Repeating,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct MediaDate {
    year: Option<i32>,
    month: Option<u32>,
    day: Option<u32>,
}

impl MediaDate {
    fn into_date(self) -> Option<Date<Local>> {
        match (self.year, self.month, self.day) {
            (Some(year), Some(month), Some(day)) => Some(Local.ymd(year, month, day)),
            _ => None,
        }
    }

    fn from_date(date: Option<Date<Local>>) -> MediaDate {
        match date {
            Some(date) => MediaDate {
                year: Some(date.year()),
                month: Some(date.month()),
                day: Some(date.day()),
            },
            None => MediaDate {
                year: None,
                month: None,
                day: None,
            },
        }
    }
}

impl From<NaiveDate> for MediaDate {
    fn from(date: NaiveDate) -> MediaDate {
        MediaDate {
            year: Some(date.year()),
            month: Some(date.month()),
            day: Some(date.day()),
        }
    }
}

use super::{AnimeEntry, AnimeInfo, Status, SyncBackend};
use chrono::{Date, Datelike, Local, NaiveDate, TimeZone};
use config::Config;
use error::BackendError;
use input;
use reqwest::header::{Accept, Authorization, Bearer, ContentType, Headers};
use reqwest::{Client, Response};
use serde_json;
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

        let text = $backend.send_request($query_str, &vars)?.text()?;
        let json: serde_json::Value = serde_json::from_str(&text)?;

        json$([$response_root])*.clone()
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
        variables: &serde_json::Value,
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
        );

        let id = serde_json::from_value(resp)?;
        Ok(id)
    }
}

impl SyncBackend for Anilist {
    fn init(config: &mut Config) -> Result<Anilist, BackendError> {
        let access_token = match config.user.access_token {
            Some(_) => config.user.decode_access_token()?,
            None => {
                match open_url(LOGIN_URL) {
                    Ok(status) if status.success() => (),
                    result => {
                        eprintln!(
                            "failed to open URL in default browser. please open it manually: {}",
                            LOGIN_URL
                        );

                        if let Err(err) = result {
                            eprintln!("error message: {}", err);
                        }
                    }
                }

                println!("please authorize your account in the opened browser tab and paste the code below:");
                let token = input::read_line()?;
                config.user.encode_access_token(&token);

                token
            }
        };

        let mut instance = Anilist {
            client: Client::new(),
            user_id: 0,
            access_token,
        };

        instance.user_id = instance.request_user_id()?;

        Ok(instance)
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
        );

        use serde_json::Value;
        let mut series = Vec::new();

        match resp {
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
        );

        let info: MediaData = serde_json::from_value(resp)?;
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
            serde_json::Value::Null => Ok(None),
            _ => {
                let media_entry: MediaListEntry = serde_json::from_value(resp)?;
                Ok(Some(media_entry.into_generic_entry(info)))
            }
        }
    }

    fn update_list_entry(&self, entry: &AnimeEntry) -> Result<(), BackendError> {
        let resp = send_query!(self,
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
                "status": MediaStatus::from(entry.status.clone()),
                "start_date": MediaDate::from_date(entry.start_date),
                "finish_date": MediaDate::from_date(entry.finish_date),
            },
        );

        if !resp["errors"].is_null() {
            let msg = resp["errors"][0]["message"].to_string();
            return Err(BackendError::ListUpdate(msg));
        }

        Ok(())
    }

    fn max_score(&self) -> u8 {
        // TODO: add support for other scoring types
        10
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

use super::{RemoteService, SeriesEntry, SeriesInfo, Status};
use crate::err::{self, Result};
use crate::file::{FileType, SaveDir, SaveFile};
use chrono::{Datelike, NaiveDate};
use lazy_static::lazy_static;
use reqwest::Client;
use serde_derive::{Deserialize, Serialize};
use serde_json as json;
use serde_json::json;
use snafu::ResultExt;
use std::convert::TryInto;
use std::fmt;
use std::result;

pub const LOGIN_URL: &str =
    "https://anilist.co/api/v2/oauth/authorize?client_id=427&response_type=token";

pub const API_URL: &str = "https://graphql.anilist.co";

macro_rules! send {
    ($token:expr, $file:expr, {$($vars:tt)*}, $($resp_root:expr)=>*) => {{
        if cfg!(debug_assertions) {
            println!("DEBUG: AniList request: {}", $file);
        }

        let vars = json!({
            $($vars)*
        });

        let query = include_str!(concat!("../../../graphql/anilist/", $file, ".gql"));

        // We must bind the json variable mutably, but the compiler warns that it can be removed.
        #[allow(unused_mut)]
        match send_gql_request(query, &vars, $token) {
            Ok(mut json) => {
                $(json = json[$resp_root].take();)*
                Ok(json)
            },
            Err(err) => Err(err),
        }
    }};
}

macro_rules! query {
    ($token:expr, $file:expr, {$($vars:tt)*}, $($resp_root:expr)=>*) => {
        send!($token, $file, {$($vars)*}, $($resp_root)=>*).and_then(|json| {
            json::from_value(json).context(err::JsonDecode)
        })
    };
}

#[derive(Debug)]
pub struct AniList {
    config: AniListConfig,
    user: User,
}

impl AniList {
    pub fn login(config: AniListConfig) -> Result<AniList> {
        let token = config.token.decode()?;
        let user = query!(&token, "user", {}, "data" => "Viewer")?;

        Ok(AniList { config, user })
    }
}

impl RemoteService for AniList {
    fn search_info_by_name(&self, name: &str) -> Result<Vec<SeriesInfo>> {
        let token = self.config.token.decode()?;
        let entries: Vec<Media> = query!(
            &token,
            "info_by_name",
            { "name": name },
            "data" => "Page" => "media"
        )?;

        let entries = entries.into_iter().map(|entry| entry.into()).collect();
        Ok(entries)
    }

    fn search_info_by_id(&self, id: u32) -> Result<SeriesInfo> {
        let token = self.config.token.decode()?;
        let info: Media = query!(&token, "info_by_id", { "id": id }, "data" => "Media")?;

        Ok(info.into())
    }

    fn get_list_entry(&self, id: u32) -> Result<Option<SeriesEntry>> {
        let token = self.config.token.decode()?;
        let query: Result<MediaEntry> = query!(
            &token,
            "get_list_entry",
            { "id": id, "userID": self.user.id },
            "data" => "MediaList"
        );

        match query {
            Ok(entry) => Ok(Some(entry.into_series_entry(id))),
            Err(ref err) if err.is_http_code(404) => Ok(None),
            Err(err) => Err(err),
        }
    }

    fn update_list_entry(&self, entry: &SeriesEntry) -> Result<()> {
        let token = self.config.token.decode()?;

        send!(
            &token,
            "update_list_entry",
            {
                "mediaId": entry.id,
                "watched_eps": entry.watched_eps,
                "score": entry.score.unwrap_or(0.0),
                "status": MediaStatus::from(entry.status),
                "times_rewatched": entry.times_rewatched,
                "start_date": entry.start_date.map(|date| MediaDate::from(&date)),
                "finish_date": entry.end_date.map(|date| MediaDate::from(&date)),
            },
        )?;

        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AniListConfig {
    #[serde(flatten)]
    pub token: AccessToken,
}

impl AniListConfig {
    pub fn new(token: AccessToken) -> AniListConfig {
        AniListConfig { token }
    }
}

impl SaveFile for AniListConfig {
    fn filename() -> &'static str {
        "anilist.toml"
    }

    fn save_dir() -> SaveDir {
        SaveDir::Config
    }

    fn file_type() -> FileType {
        FileType::Toml
    }
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct AccessToken {
    encoded_token: String,
}

impl AccessToken {
    pub fn new<S>(token: S) -> AccessToken
    where
        S: AsRef<str>,
    {
        AccessToken {
            encoded_token: AccessToken::encode(token),
        }
    }

    fn encode<S>(value: S) -> String
    where
        S: AsRef<str>,
    {
        base64::encode(value.as_ref())
    }

    pub fn decode(&self) -> Result<String> {
        let bytes = base64::decode(&self.encoded_token).context(err::Base64Decode)?;
        let string = String::from_utf8(bytes).context(err::UTF8Decode)?;

        Ok(string)
    }
}

impl fmt::Debug for AccessToken {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "AccessToken {{}}")
    }
}

fn send_gql_request<S, R>(query: S, vars: &json::Value, token: R) -> Result<json::Value>
where
    S: Into<String>,
    R: AsRef<str>,
{
    lazy_static! {
        static ref CLIENT: Client = Client::new();
    }

    let query = minimize_query(query);

    let body = json!({
        "query": query,
        "variables": vars,
    })
    .to_string();

    let json: json::Value = CLIENT
        .post(API_URL)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .bearer_auth(token.as_ref())
        .body(body)
        .send()
        .context(err::Reqwest)?
        .json()
        .context(err::Reqwest)?;

    if json["errors"] != json::Value::Null {
        let err = &json["errors"][0];

        let message = err["message"].as_str().unwrap_or("unknown").to_string();
        let code = err["status"].as_u64().unwrap_or(0) as u16;

        return Err(err::Error::BadAniListResponse { code, message });
    }

    Ok(json)
}

fn minimize_query<S>(value: S) -> String
where
    S: Into<String>,
{
    let mut value = value.into();
    value.retain(|c| c != ' ' && c != '\n');
    value
}

#[derive(Debug, Deserialize)]
struct User {
    id: u32,
    #[serde(rename = "mediaListOptions")]
    options: ListOptions,
}

#[derive(Debug, Deserialize)]
struct ListOptions {
    #[serde(rename = "scoreFormat")]
    score_format: ScoreFormat,
}

#[derive(Debug, Deserialize)]
#[allow(clippy::enum_variant_names)]
enum ScoreFormat {
    #[serde(rename = "POINT_100")]
    Point100,
    #[serde(rename = "POINT_10_DECIMAL")]
    Point10Decimal,
    #[serde(rename = "POINT_10")]
    Point10,
    #[serde(rename = "POINT_5")]
    Point5,
    #[serde(rename = "POINT_3")]
    Point3,
}

#[derive(Debug, Deserialize)]
struct Media {
    id: u32,
    title: MediaTitle,
    episodes: Option<u32>,
    relations: Option<MediaRelation>,
    format: String,
}

impl Media {
    /// Returns the media ID of the series that is listed as a sequel and matches the same format.
    fn direct_sequel_id(&self) -> Option<u32> {
        let relations = match &self.relations {
            Some(relations) => relations,
            None => return None,
        };

        let is_direct_sequel =
            |edge: &&MediaEdge| edge.is_sequel() && edge.node.format == self.format;

        relations
            .edges
            .iter()
            .find(is_direct_sequel)
            .map(|edge| edge.node.id)
    }
}

impl Into<SeriesInfo> for Media {
    fn into(self) -> SeriesInfo {
        let sequel = self.direct_sequel_id();

        SeriesInfo {
            id: self.id,
            title: self.title.romaji,
            episodes: self.episodes.unwrap_or(1),
            sequel,
        }
    }
}

#[derive(Debug, Deserialize)]
struct MediaTitle {
    romaji: String,
}

#[derive(Debug, Deserialize)]
struct MediaRelation {
    edges: Vec<MediaEdge>,
}

#[derive(Debug, Deserialize)]
struct MediaEdge {
    #[serde(rename = "relationType")]
    relation: MediaRelationType,
    node: MediaNode,
}

impl MediaEdge {
    fn is_sequel(&self) -> bool {
        self.relation == MediaRelationType::Sequel
    }
}

#[derive(Debug, Deserialize, PartialEq)]
enum MediaRelationType {
    #[serde(rename = "SEQUEL")]
    Sequel,
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
struct MediaNode {
    id: u32,
    format: String,
}

#[derive(Debug, Deserialize)]
struct MediaEntry {
    status: MediaStatus,
    score: f32,
    progress: u32,
    repeat: u32,
    #[serde(rename = "startedAt")]
    start_date: Option<MediaDate>,
    #[serde(rename = "completedAt")]
    complete_date: Option<MediaDate>,
}

impl MediaEntry {
    fn into_series_entry(self, id: u32) -> SeriesEntry {
        let score = if self.score > 0.0 {
            Some(self.score)
        } else {
            None
        };

        SeriesEntry {
            id,
            watched_eps: self.progress,
            score,
            status: self.status.into(),
            times_rewatched: self.repeat,
            start_date: self.start_date.and_then(|d| d.try_into().ok()),
            end_date: self.complete_date.and_then(|d| d.try_into().ok()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Serialize, Deserialize)]
struct MediaDate {
    year: Option<i32>,
    month: Option<u32>,
    day: Option<u32>,
}

impl TryInto<NaiveDate> for MediaDate {
    type Error = ();

    fn try_into(self) -> result::Result<NaiveDate, Self::Error> {
        match (self.year, self.month, self.day) {
            (Some(y), Some(m), Some(d)) => Ok(NaiveDate::from_ymd(y, m, d)),
            _ => Err(()),
        }
    }
}

impl From<&NaiveDate> for MediaDate {
    fn from(date: &NaiveDate) -> MediaDate {
        MediaDate {
            year: Some(date.year()),
            month: Some(date.month()),
            day: Some(date.day()),
        }
    }
}

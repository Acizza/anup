use super::{
    AccessToken, RemoteService, ScoreParser, SeriesEntry, SeriesID, SeriesInfo, SeriesTitle, Status,
};
use crate::err::{self, Result};
use chrono::{Datelike, NaiveDate};
use serde_derive::{Deserialize, Serialize};
use serde_json as json;
use serde_json::json;
use snafu::ResultExt;
use std::borrow::Cow;
use std::convert::TryInto;
use std::result;

/// The URL to the API endpoint.
pub const API_URL: &str = "https://graphql.anilist.co";

/// Returns the URL that the user needs to go to in order to authenticate their account
/// so the API can make changes to it.
///
/// `client_id` is the ID of the application you wish to use the API with.
/// It can be retrieved from the `Developer` section of your account settings.
#[inline]
pub fn auth_url(client_id: u32) -> String {
    format!(
        "https://anilist.co/api/v2/oauth/authorize?client_id={}&response_type=token",
        client_id
    )
}

/// Send an API query to AniList, without attemping to parse a response.
macro_rules! send {
    ($token:expr, $file:expr, {$($vars:tt)*}, $($resp_root:expr)=>*) => {{
        if cfg!(debug_assertions) && cfg!(feature = "print-requests-debug") {
            println!("DEBUG: AniList request: {}", $file);
        }

        let vars = json!({
            $($vars)*
        });

        let query = include_str!(concat!("../../graphql/anilist/", $file, ".gql"));

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

/// Send an API query to AniList, and attempt to parse the response into a specified type.
macro_rules! query {
    ($token:expr, $file:expr, {$($vars:tt)*}, $($resp_root:expr)=>*) => {
        send!($token, $file, {$($vars)*}, $($resp_root)=>*).and_then(|json| {
            json::from_value(json).context(err::JsonDecode)
        })
    };
}

/// A connection to the AniList API.
#[derive(Debug)]
pub struct AniList {
    /// The currently authenticated user.
    pub auth: Option<Auth>,
}

impl AniList {
    /// Create a new unauthenticated `AniList` instance.
    ///
    /// When unauthenticated, you can only search for series info by name and by ID.
    /// Trying to make any other request will return a `NeedAuthentication` error.
    #[inline]
    pub fn unauthenticated() -> Self {
        Self { auth: None }
    }

    /// Create a new authenticated `AniList` instance with the specified user `token`.
    ///
    /// This will allow you to update the specified user's list.
    /// To get a user's token, they will need to visit the URL provided by
    /// the `auth_url` function and provide it to you. The token should then be
    /// stored as it is only visible once.
    pub fn authenticated(token: AccessToken) -> Result<Self> {
        let user = query!(Some(&token), "user", {}, "data" => "Viewer")?;
        let auth = Auth::new(user, token);

        Ok(Self { auth: Some(auth) })
    }

    fn score_format(&self) -> ScoreFormat {
        match &self.auth {
            Some(auth) => auth.user.options.score_format,
            None => ScoreFormat::default(),
        }
    }
}

impl RemoteService for AniList {
    fn search_info_by_name(&self, name: &str) -> Result<Box<dyn Iterator<Item = SeriesInfo>>> {
        let entries: Vec<Media> = query!(
            None,
            "info_by_name",
            { "name": name },
            "data" => "Page" => "media"
        )?;

        let entries = entries.into_iter().map(|entry| entry.into());
        Ok(Box::new(entries))
    }

    fn search_info_by_id(&self, id: SeriesID) -> Result<SeriesInfo> {
        let info: Media = query!(None, "info_by_id", { "id": id }, "data" => "Media")?;
        Ok(info.into())
    }

    fn get_list_entry(&self, id: SeriesID) -> Result<Option<SeriesEntry>> {
        let auth = match &self.auth {
            Some(auth) => auth,
            None => return Err(err::Error::NeedAuthentication),
        };

        let query: Result<MediaEntry> = query!(
            Some(&auth.token),
            "get_list_entry",
            { "id": id, "userID": auth.user.id },
            "data" => "MediaList"
        );

        match query {
            Ok(entry) => Ok(Some(entry.into_series_entry(id))),
            Err(ref err) if err.is_http_code(404) => Ok(None),
            Err(err) => Err(err),
        }
    }

    fn update_list_entry(&self, entry: &SeriesEntry) -> Result<()> {
        let token = match &self.auth {
            Some(auth) => &auth.token,
            None => return Err(err::Error::NeedAuthentication),
        };

        send!(
            Some(token),
            "update_list_entry",
            {
                "mediaId": entry.id,
                "watched_eps": entry.watched_eps,
                "score": entry.score.unwrap_or(0),
                "status": MediaStatus::from(entry.status),
                "times_rewatched": entry.times_rewatched,
                "start_date": entry.start_date.map(|date| MediaDate::from(&date)),
                "finish_date": entry.end_date.map(|date| MediaDate::from(&date)),
            },
        )?;

        Ok(())
    }
}

impl ScoreParser for AniList {
    fn parse_score(&self, score: &str) -> Option<u8> {
        self.score_format().points_value(score)
    }

    fn score_to_str(&self, score: u8) -> Cow<str> {
        match self.score_format() {
            ScoreFormat::Point100 => score.to_string().into(),
            ScoreFormat::Point10 => (score / 10).to_string().into(),
            ScoreFormat::Point10Decimal => format!("{:.1}", f32::from(score) / 10.0).into(),
            ScoreFormat::Point5 => {
                let num_stars = score / 20;
                "★".repeat(num_stars as usize).into()
            }
            ScoreFormat::Point3 => {
                if score <= 33 {
                    ":(".into()
                } else if score <= 66 {
                    ":|".into()
                } else {
                    ":)".into()
                }
            }
        }
    }
}

/// An authenticated user.
#[derive(Debug)]
pub struct Auth {
    /// The AniList user's account information.
    pub user: User,
    token: AccessToken,
}

impl Auth {
    fn new(user: User, token: AccessToken) -> Self {
        Self { user, token }
    }
}

/// An AniList user.
#[derive(Debug, Deserialize)]
pub struct User {
    /// The user's account ID.
    pub id: u32,
    /// Settings related to the user's anime list.
    #[serde(rename = "mediaListOptions")]
    pub options: ListOptions,
}

/// Anime list settings for a user.
#[derive(Debug, Deserialize)]
pub struct ListOptions {
    /// The user's preferred scoring format.
    #[serde(rename = "scoreFormat")]
    pub score_format: ScoreFormat,
}

/// AniList score formats.
#[derive(Clone, Copy, Debug, Deserialize)]
pub enum ScoreFormat {
    /// Range between 0 - 100.
    #[serde(rename = "POINT_100")]
    Point100,
    /// Range between 0.0 - 10.0.
    #[serde(rename = "POINT_10_DECIMAL")]
    Point10Decimal,
    /// Range between 0 - 10.
    #[serde(rename = "POINT_10")]
    Point10,
    /// Range between 0 - 5.
    #[serde(rename = "POINT_5")]
    Point5,
    /// Range between 0 - 100. This variant is unique in that it is
    /// represented by an ASCII-style face. Value ranges for each face
    /// are shown below:
    ///
    /// | Range    | Face |
    /// | -------- | ---- |
    /// | 0 - 33   | :(   |
    /// | 34 - 66  | :\|  |
    /// | 67 - 100 | :)   |
    #[serde(rename = "POINT_3")]
    Point3,
}

impl ScoreFormat {
    fn points_value<S>(self, score: S) -> Option<u8>
    where
        S: AsRef<str>,
    {
        let score = score.as_ref();

        let raw_score = match self {
            Self::Point100 => score.parse().ok()?,
            Self::Point10Decimal => {
                let score = score.parse::<f32>().ok()?;
                (score * 10.0).round() as u8
            }
            Self::Point10 => {
                let score = score.parse::<u8>().ok()?;
                score.saturating_mul(10)
            }
            Self::Point5 => {
                let score = score.parse::<u8>().ok()?;
                score.saturating_mul(20)
            }
            Self::Point3 => match score {
                ":(" => 33,
                ":|" => 50, // When set to 66, AniList interprets this as the ":)" rating
                ":)" => 100,
                _ => return None,
            },
        };

        Some(raw_score.min(100))
    }
}

impl Default for ScoreFormat {
    fn default() -> ScoreFormat {
        ScoreFormat::Point100
    }
}

fn send_gql_request<S>(
    query: S,
    vars: &json::Value,
    token: Option<&AccessToken>,
) -> Result<json::Value>
where
    S: Into<String>,
{
    const REQ_TIMEOUT_MS: u64 = 15_000;

    let query = minimize_query(query);

    let body = json!({
        "query": query,
        "variables": vars,
    });

    let mut request = ureq::post(API_URL);
    request.timeout_connect(REQ_TIMEOUT_MS);
    request.timeout_read(REQ_TIMEOUT_MS);
    request.timeout_write(REQ_TIMEOUT_MS);
    request.set("Content-Type", "application/json");
    request.set("Accept", "application/json");

    if let Some(token) = token {
        request.auth_kind("Bearer", &token.decode()?);
    }

    let resp = request.send_json(body);

    if let Some(err) = resp.synthetic_error() {
        return Err(err.into());
    }

    let json = resp.into_json().context(err::HttpIO)?;

    if json["errors"] != json::Value::Null {
        let err = &json["errors"][0];

        let message = err["message"].as_str().unwrap_or("unknown").to_string();
        let code = err["status"].as_u64().unwrap_or(0) as u16;

        return Err(err::Error::BadAniListResponse { code, message });
    }

    Ok(json)
}

// TODO: convert to const fn when mutable references can be used
fn minimize_query<S>(value: S) -> String
where
    S: Into<String>,
{
    let mut value = value.into();
    value.retain(|c| c != ' ' && c != '\n');
    value
}

#[derive(Debug, Deserialize)]
struct Media {
    id: u32,
    title: MediaTitle,
    episodes: Option<u32>,
    duration: Option<u32>,
    relations: Option<MediaRelation>,
    format: String,
}

impl Media {
    /// Returns the media ID of the series that is listed as a sequel and matches the same format.
    fn direct_sequel_id(&self) -> Option<u32> {
        let relations = self.relations.as_ref()?;

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
            title: self.title.into(),
            episodes: self.episodes.unwrap_or(1),
            episode_length: self.duration.unwrap_or(24),
            sequel,
        }
    }
}

#[derive(Debug, Deserialize)]
struct MediaTitle {
    romaji: String,
    #[serde(rename = "userPreferred")]
    preferred: String,
}

impl Into<SeriesTitle> for MediaTitle {
    fn into(self) -> SeriesTitle {
        SeriesTitle {
            romaji: self.romaji,
            preferred: self.preferred,
        }
    }
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
    score: u8,
    progress: u32,
    repeat: u32,
    #[serde(rename = "startedAt")]
    start_date: Option<MediaDate>,
    #[serde(rename = "completedAt")]
    complete_date: Option<MediaDate>,
}

impl MediaEntry {
    fn into_series_entry(self, id: u32) -> SeriesEntry {
        let score = if self.score > 0 {
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
            Self::Current => Status::Watching,
            Self::Completed => Status::Completed,
            Self::Paused => Status::OnHold,
            Self::Dropped => Status::Dropped,
            Self::Planning => Status::PlanToWatch,
            Self::Repeating => Status::Rewatching,
        }
    }
}

impl From<Status> for MediaStatus {
    fn from(status: Status) -> Self {
        match status {
            Status::Watching => Self::Current,
            Status::Completed => Self::Completed,
            Status::OnHold => Self::Paused,
            Status::Dropped => Self::Dropped,
            Status::PlanToWatch => Self::Planning,
            Status::Rewatching => Self::Repeating,
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
    fn from(date: &NaiveDate) -> Self {
        Self {
            year: Some(date.year()),
            month: Some(date.month()),
            day: Some(date.day()),
        }
    }
}

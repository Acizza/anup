use super::{
    AccessToken, RemoteService, ScoreParser, Sequel, SeriesDate, SeriesEntry, SeriesID, SeriesInfo,
    SeriesKind, SeriesTitle, Status,
};
use crate::err::{Error, Result};
use serde_derive::{Deserialize, Serialize};
use serde_json as json;
use serde_json::json;
use std::borrow::Cow;
use std::convert::TryInto;
use std::result;
use std::str;
use std::time::Duration;

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

// This macro tests how far you can go with const functions for things like string manipulation.
// It is a lot more complicated than the original naive implementation, but it saves us from an O(n) operation with allocations
// that would otherwise be performed for each API query.
//
// The solution can be simplified as new Rust versions add more features for const functions.
macro_rules! minimize_query {
    ($value:expr) => {{
        const LEN: usize = $value.len();

        // This function needs to be generated on a per-string basis so our array length can be semi-close to our minimized result.
        // If we don't do this, LLVM seems to emit assembly that uses way more stack space than necessary on Rust 1.46.0+
        const fn minimize() -> [u8; LEN] {
            let bytes = $value.as_bytes();
            let mut result = [0; LEN];
            let mut result_index = 0;
            let mut index = 0;

            while index < bytes.len() {
                let byte = bytes[index];

                index += 1;

                if byte == b' ' || byte == b'\n' {
                    continue;
                }

                result[result_index] = byte;
                result_index += 1;
            }

            result
        }

        // Store the result in a constant to guarantee that it will be ran at compile time
        const MINIMIZED: [u8; LEN] = minimize();

        // Since minimize() returns an array the size of the original string length, we need to find where the minimized one ends.
        // For release builds, this should be computed at compile time given that we're working with constants.
        let end_pos = LEN - MINIMIZED.iter().rev().position(|&b| b != 0).unwrap_or(0);

        unsafe {
            str::from_utf8_unchecked(&MINIMIZED[..end_pos])
        }
    }};
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

        let query = minimize_query!(include_str!(concat!("../../graphql/anilist/", $file, ".gql")));

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
            json::from_value(json).map_err(Into::into)
        })
    };
}

/// A connection to the AniList API.
#[derive(Debug)]
pub enum AniList {
    /// An `AniList` connection with authentication.
    ///
    /// This mode will allow you to update the specified user's list.
    /// To get a user's token, they will need to visit the URL provided by
    /// the `auth_url` function and provide it to you. The token should then be
    /// stored as it is only visible once.
    Authenticated(Auth),
    /// An `AniList` connection without any authentication.
    ///
    /// In this mode, you can only search for series info by name and by ID.
    /// Trying to make any other request will return a `NeedAuthentication` error.
    Unauthenticated,
}

impl AniList {
    fn auth(&self) -> Result<&Auth> {
        match &self {
            Self::Authenticated(auth) => Ok(auth),
            Self::Unauthenticated => Err(Error::NeedAuthentication),
        }
    }

    fn auth_token(&self) -> Result<&AccessToken> {
        self.auth().map(|auth| &auth.token)
    }

    fn score_format(&self) -> ScoreFormat {
        match &self {
            Self::Authenticated(auth) => auth.user.options.score_format,
            Self::Unauthenticated => ScoreFormat::default(),
        }
    }
}

impl RemoteService for AniList {
    fn search_info_by_name(&self, name: &str) -> Result<Vec<SeriesInfo>> {
        let entries: Vec<Media> = query!(
            self.auth_token().ok(),
            "info_by_name",
            { "name": name },
            "data" => "Page" => "media"
        )?;

        let entries = entries
            .into_iter()
            .filter_map(|entry| entry.try_into().ok())
            .collect();

        Ok(entries)
    }

    fn search_info_by_id(&self, id: SeriesID) -> Result<SeriesInfo> {
        let info: Media =
            query!(self.auth_token().ok(), "info_by_id", { "id": id }, "data" => "Media")?;

        info.try_into().map_err(|_| Error::NotAnAnime)
    }

    fn get_list_entry(&self, id: SeriesID) -> Result<Option<SeriesEntry>> {
        let auth = self.auth()?;

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
        let token = self.auth_token()?;

        send!(
            Some(token),
            "update_list_entry",
            {
                "mediaId": entry.id,
                "watched_eps": entry.watched_eps,
                "score": entry.score.unwrap_or(0),
                "status": MediaStatus::from(entry.status),
                "times_rewatched": entry.times_rewatched,
                "start_date": entry.start_date.map(MediaDate::from),
                "finish_date": entry.end_date.map(MediaDate::from),
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
    #[inline(always)]
    pub fn new(user: User, token: AccessToken) -> Self {
        Self { user, token }
    }

    /// Retrieve the current authorization from AniList using the specified `token`.
    pub fn retrieve(token: AccessToken) -> Result<Self> {
        let user = query!(Some(&token), "user", {}, "data" => "Viewer")?;
        Ok(Self::new(user, token))
    }
}

/// An AniList user.
#[derive(Debug, Deserialize)]
pub struct User {
    /// The user's account ID.
    pub id: u32,
    /// The user's account name.
    pub name: String,
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
    fn points_value(self, score: &str) -> Option<u8> {
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
    S: AsRef<str>,
{
    const REQ_TIMEOUT_SEC: u64 = 15;

    let body = json!({
        "query": query.as_ref(),
        "variables": vars,
    });

    let mut request = attohttpc::post(API_URL)
        .timeout(Duration::from_secs(REQ_TIMEOUT_SEC))
        .json(&body)?;

    if let Some(token) = token {
        request = request.bearer_auth(&token.decode()?);
    }

    let json: json::Value = request.send()?.json()?;

    if json["errors"] != json::Value::Null {
        let err = &json["errors"][0];

        let message = err["message"].as_str().unwrap_or("unknown").to_string();
        let code = err["status"].as_u64().unwrap_or(0) as u16;

        return Err(Error::BadAniListResponse { code, message });
    }

    Ok(json)
}

#[derive(Debug, Deserialize)]
struct Media {
    id: u32,
    title: MediaTitle,
    episodes: Option<u32>,
    duration: Option<u32>,
    relations: Option<MediaRelation>,
    format: MediaFormat,
}

impl Media {
    fn sequels(&self) -> Vec<Sequel> {
        let relations = match self.relations.as_ref() {
            Some(relations) => relations,
            None => return Vec::new(),
        };

        relations
            .edges
            .iter()
            .filter_map(|edge| edge.try_into().ok())
            .collect()
    }
}

impl TryInto<SeriesInfo> for Media {
    type Error = ();

    fn try_into(self) -> result::Result<SeriesInfo, Self::Error> {
        let kind = self.format.try_into()?;
        let sequels = self.sequels();

        Ok(SeriesInfo {
            id: self.id,
            title: self.title.into(),
            episodes: self.episodes.unwrap_or(1),
            episode_length: self.duration.unwrap_or(24),
            kind,
            sequels,
        })
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

impl TryInto<Sequel> for &MediaEdge {
    type Error = ();

    fn try_into(self) -> result::Result<Sequel, Self::Error> {
        // It doesn't make sense to consider this media edge a sequel
        // if its an alternative, source, or character relation
        if !self.relation.is_sequential() {
            return Err(());
        }

        let kind = match self.node.format {
            Some(fmt) => fmt.try_into()?,
            None => return Err(()),
        };

        let sequel = Sequel::new(kind, self.node.id);

        Ok(sequel)
    }
}

#[derive(Copy, Clone, Debug, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
enum MediaRelationType {
    Sequel,
    #[serde(rename = "SIDE_STORY")]
    SideStory,
    Other,
    #[serde(other)]
    Unknown,
}

impl MediaRelationType {
    /// Returns true if the relation is considered to be some kind of sequel. Ex: a second season, OVA, ONA, movie, etc
    fn is_sequential(self) -> bool {
        match self {
            Self::Sequel | Self::SideStory | Self::Other => true,
            Self::Unknown => false,
        }
    }
}

#[derive(Debug, Deserialize)]
struct MediaNode {
    id: u32,
    format: Option<MediaFormat>,
}

#[derive(Copy, Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
enum MediaFormat {
    TV,
    #[serde(rename = "TV_SHORT")]
    TVShort,
    Movie,
    Special,
    OVA,
    ONA,
    Music,
    #[serde(other)]
    Other,
}

impl TryInto<SeriesKind> for MediaFormat {
    type Error = ();

    fn try_into(self) -> result::Result<SeriesKind, Self::Error> {
        match self {
            Self::TV | Self::TVShort => Ok(SeriesKind::Season),
            Self::Movie => Ok(SeriesKind::Movie),
            Self::Special => Ok(SeriesKind::Special),
            Self::OVA => Ok(SeriesKind::OVA),
            Self::ONA => Ok(SeriesKind::ONA),
            Self::Music => Ok(SeriesKind::Music),
            Self::Other => Err(()),
        }
    }
}

#[derive(Debug, Deserialize)]
struct MediaEntry {
    status: MediaStatus,
    score: u8,
    progress: u32,
    repeat: u32,
    #[serde(rename = "startedAt")]
    start_date: MediaDate,
    #[serde(rename = "completedAt")]
    complete_date: MediaDate,
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
            start_date: self.start_date.try_into().ok(),
            end_date: self.complete_date.try_into().ok(),
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

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
struct MediaDate {
    year: Option<u16>,
    month: Option<u8>,
    day: Option<u8>,
}

impl From<SeriesDate> for MediaDate {
    fn from(date: SeriesDate) -> Self {
        Self {
            year: Some(date.year),
            month: Some(date.month),
            day: Some(date.day),
        }
    }
}

impl TryInto<SeriesDate> for MediaDate {
    type Error = ();

    fn try_into(self) -> result::Result<SeriesDate, Self::Error> {
        match (self.year, self.month, self.day) {
            (Some(y), Some(m), Some(d)) => Ok(SeriesDate::from_ymd(y, m, d)),
            _ => Err(()),
        }
    }
}

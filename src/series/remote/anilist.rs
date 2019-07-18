use super::{RemoteService, SeriesEntry, SeriesInfo};
use crate::err::{self, Result};
use crate::file::{FileType, SaveDir, SaveFile};
use lazy_static::lazy_static;
use reqwest::Client;
use serde_derive::{Deserialize, Serialize};
use serde_json as json;
use serde_json::json;
use snafu::ResultExt;
use std::fmt;

pub const LOGIN_URL: &str =
    "https://anilist.co/api/v2/oauth/authorize?client_id=427&response_type=token";

pub const API_URL: &str = "https://graphql.anilist.co";

macro_rules! send_query {
    ($token:expr, $file:expr, {$($vars:tt)*}, $($resp_root:expr)=>*) => {{
        let vars = json!({
            $($vars)*
        });

        let query = include_str!(concat!("../../../graphql/anilist/", $file, ".gql"));

        match send_gql_request(query, &vars, $token) {
            Ok(mut json) => {
                $(json = json[$resp_root].take();)*
                json::from_value(json).context(err::JsonDecode)
            },
            Err(err) => Err(err),
        }
    }};
}

#[derive(Debug)]
pub struct AniList {
    config: AniListConfig,
    user: User,
}

impl AniList {
    pub fn login(config: AniListConfig) -> Result<AniList> {
        let token = config.token.decode()?;
        let user = send_query!(&token, "user", {}, "data" => "Viewer")?;

        Ok(AniList { config, user })
    }
}

impl RemoteService for AniList {
    fn search_info_by_name(&self, name: &str) -> Result<Vec<SeriesInfo>> {
        let token = self.config.token.decode()?;
        let entries: Vec<Media> = send_query!(
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
        let info: Media = send_query!(&token, "info_by_id", { "id": id }, "data" => "Media")?;

        Ok(info.into())
    }

    fn update_list_entry(&self, _: &SeriesEntry) -> Result<()> {
        unimplemented!()
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
        let code = err["status"].as_u64().unwrap_or(0) as u32;

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
}

impl Into<SeriesInfo> for Media {
    fn into(self) -> SeriesInfo {
        let sequel = self
            .relations
            .and_then(|rel| rel.edges.into_iter().find(|e| e.is_sequel()))
            .map(|e| e.node.id);

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
    relation: String,
    node: MediaNode,
}

impl MediaEdge {
    fn is_sequel(&self) -> bool {
        self.relation == "SEQUEL"
    }
}

#[derive(Debug, Deserialize)]
struct MediaNode {
    id: u32,
}

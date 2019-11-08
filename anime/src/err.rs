use snafu::{Backtrace, Snafu};
use std::io;
use std::path;
use std::result;
use std::string;

pub type Result<T> = result::Result<T, Error>;

#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
pub enum Error {
    #[snafu(display("file io error [{:?}]: {}", path, source))]
    FileIO {
        path: path::PathBuf,
        source: io::Error,
        backtrace: Backtrace,
    },

    #[snafu(display("dir entry error [{:?}]: {}", dir, source))]
    EntryIO {
        dir: path::PathBuf,
        source: io::Error,
        backtrace: Backtrace,
    },

    #[snafu(display("base64 decode error: {}", source))]
    Base64Decode {
        source: base64::DecodeError,
        backtrace: Backtrace,
    },

    #[snafu(display("utf8 decode error: {}", source))]
    UTF8Decode {
        source: string::FromUtf8Error,
        backtrace: Backtrace,
    },

    #[snafu(display("json decode error: {}", source))]
    JsonDecode {
        source: serde_json::Error,
        backtrace: Backtrace,
    },

    #[snafu(display("failed to create regex pattern \"{}\": {}", pattern, source))]
    Regex {
        pattern: String,
        source: regex::Error,
        backtrace: Backtrace,
    },

    #[snafu(display("reqwest error: {}", source))]
    Reqwest {
        source: reqwest::Error,
        backtrace: Backtrace,
    },

    #[snafu(display("failed to parse episode title: {}", name))]
    NoEpisodeTitle { name: String },

    #[snafu(display("expected episode number for file: {}", name))]
    ExpectedEpNumber { name: String },

    #[snafu(display("failed to parse episode: {}", name))]
    NoEpMatches { name: String },

    #[snafu(display(
        "found different episode titles:\n\texpecting: {}\n\tfound: {}",
        expecting,
        found
    ))]
    MultipleTitles { expecting: String, found: String },

    #[snafu(display("need existing series info to use offline backend"))]
    NeedExistingSeriesData,

    #[snafu(display("received bad response from AniList (code {}): {}", code, message))]
    BadAniListResponse { code: u16, message: String },

    #[snafu(display("missing group \"{}\" in custom episode matcher", group))]
    MissingCustomMatcherGroup { group: &'static str },
}

impl Error {
    pub fn is_http_code(&self, http_code: u16) -> bool {
        match self {
            Error::BadAniListResponse { code, .. } if http_code == *code => true,
            Error::Reqwest { source, .. } => {
                let status = match source.status() {
                    Some(status) => status,
                    None => return false,
                };

                status.as_u16() == http_code
            }
            _ => false,
        }
    }
}

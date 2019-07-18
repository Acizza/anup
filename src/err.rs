use snafu::{Backtrace, ErrorCompat, Snafu};
use std::io;
use std::path;
use std::result;
use std::string;

pub type Result<T> = result::Result<T, Error>;

#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
pub enum Error {
    #[snafu(display("io error: {}", source))]
    IO {
        source: io::Error,
        backtrace: Backtrace,
    },

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

    #[snafu(display("toml decode error [{:?}]: {}", path, source))]
    TomlDecode {
        path: path::PathBuf,
        source: toml::de::Error,
        backtrace: Backtrace,
    },

    #[snafu(display("toml encode error [{:?}]: {}", path, source))]
    TomlEncode {
        path: path::PathBuf,
        source: toml::ser::Error,
        backtrace: Backtrace,
    },

    #[snafu(display("message pack encode error [{:?}]: {}", path, source))]
    RMPEncode {
        path: path::PathBuf,
        source: rmp_serde::encode::Error,
        backtrace: Backtrace,
    },

    #[snafu(display("message pack decode error [{:?}]: {}", path, source))]
    RMPDecode {
        path: path::PathBuf,
        source: rmp_serde::decode::Error,
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

    #[snafu(display("failed to create regex matcher \"{}\": {}", matcher, source))]
    Regex {
        matcher: String,
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

    #[snafu(display("series should only have one episode, but found multiple\n\tshould only be: {}\n\tfound: {}", original, conflict))]
    MultipleEpsInSingleEpSeries { original: String, conflict: String },

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

    #[snafu(display("no episodes found at path: {:?}", path))]
    NoEpisodes { path: path::PathBuf },

    #[snafu(display("episode {} not found for {}", episode, series))]
    EpisodeNotFound { episode: u32, series: String },

    #[snafu(display("failed to play episode {} of {}: {}", episode, series, source))]
    FailedToPlayEpisode {
        episode: u32,
        series: String,
        source: io::Error,
    },

    #[snafu(display("video player didn't exit normally while playing: {:?}", path))]
    AbnormalPlayerExit { path: path::PathBuf },

    #[snafu(display("no series with id {} found", id))]
    NoSeriesWithID { id: u32 },

    #[snafu(display("no series found with name similar to {}", name))]
    NoMatchingSeries { name: String },

    #[snafu(display("need series data to run in offline mode\nrun the program with --prefetch first when an internet connection is available"))]
    RunWithPrefetch {},

    #[snafu(display("received bad response from AniList (code {}): {}", code, message))]
    BadAniListResponse { code: u16, message: String },

    #[snafu(display("no data found for season {}", season))]
    NoSeason { season: usize },
}

impl Error {
    pub fn is_file_nonexistant(&self) -> bool {
        match self {
            Error::FileIO { source, .. } => source.kind() == io::ErrorKind::NotFound,
            _ => false,
        }
    }

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

pub fn display_error(err: Error) {
    eprintln!("{}", err);

    if let Some(backtrace) = err.backtrace() {
        eprintln!("backtrace:\n{}", backtrace);
    }
}

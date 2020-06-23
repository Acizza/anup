use snafu::{Backtrace, GenerateBacktrace, Snafu};
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

    #[snafu(display("http error: {}", source))]
    Http {
        source: attohttpc::Error,
        backtrace: Backtrace,
    },

    #[snafu(display("failed to parse episode title: {}", filename))]
    NoEpisodeTitle { filename: String },

    #[snafu(display("expected episode number for file: {}", filename))]
    ExpectedEpNumber { filename: String },

    #[snafu(display("failed to parse episode: {}", filename))]
    EpisodeParseFailed { filename: String },

    #[snafu(display(
        "found different episode titles:\n\texpecting: {}\n\tfound: {}",
        expecting,
        found
    ))]
    MultipleTitles { expecting: String, found: String },

    #[snafu(display("need existing series info to use offline backend"))]
    NeedExistingSeriesData,

    #[snafu(display("bad AniList response ({}): {}", code, message))]
    BadAniListResponse { code: u16, message: String },

    #[snafu(display(
        "custom episode matcher must specify the episode and (optionally) the title group"
    ))]
    MissingMatcherGroups,

    #[snafu(display("title group must be specified to parse episodes"))]
    NeedTitleGroup,

    #[snafu(display("must be authorized to make this request"))]
    NeedAuthentication,

    #[snafu(display("requested series is not an anime"))]
    NotAnAnime,
}

impl Error {
    pub fn is_http_code(&self, http_code: u16) -> bool {
        use attohttpc::ErrorKind;

        match self {
            Error::BadAniListResponse { code, .. } if http_code == *code => true,
            Error::Http { source, .. } => match source.kind() {
                ErrorKind::StatusCode(status) => status.as_u16() == http_code,
                _ => false,
            },
            _ => false,
        }
    }
}

impl From<attohttpc::Error> for Error {
    fn from(source: attohttpc::Error) -> Self {
        Self::Http {
            source,
            backtrace: Backtrace::generate(),
        }
    }
}

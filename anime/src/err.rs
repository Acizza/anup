use std::io;
use std::result;
use std::string;
use thiserror::Error;

pub type Result<T> = result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] io::Error),

    #[error("base64 decode error: {0}")]
    Base64Decode(#[from] base64::DecodeError),

    #[error("utf8 decode error: {0}")]
    UTF8Decode(#[from] string::FromUtf8Error),

    #[error("json decode error: {0}")]
    JsonDecode(#[from] serde_json::Error),

    #[error("http error: {0}")]
    Http(#[from] attohttpc::Error),

    #[error("failed to parse episode: {filename}")]
    EpisodeParseFailed { filename: String },

    #[error("found different episode titles:\nexpecting: {expecting}\nfound: {found}")]
    MultipleTitles { expecting: String, found: String },

    #[error("need existing series info to use offline backend")]
    NeedExistingSeriesData,

    #[error("bad AniList response ({code}): {message}")]
    BadAniListResponse { code: u16, message: String },

    #[error("must be authorized to make this request")]
    NeedAuthentication,

    #[error("requested series is not an anime")]
    NotAnAnime,
}

impl Error {
    #[must_use]
    pub fn is_http_code(&self, http_code: u16) -> bool {
        use attohttpc::ErrorKind;

        match self {
            Error::BadAniListResponse { code, .. } if http_code == *code => true,
            Error::Http(source) => match source.kind() {
                ErrorKind::StatusCode(status) => status.as_u16() == http_code,
                _ => false,
            },
            _ => false,
        }
    }
}

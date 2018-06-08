macro_rules! impl_error_conversion {
    ($err_name:ident, $($from_ty:ty => $to_ty:ident,)+) => {
        $(
        impl From<$from_ty> for $err_name {
            fn from(f: $from_ty) -> $err_name {
                $err_name::$to_ty(f)
            }
        }
        )+
    };
}

#[derive(Fail, Debug)]
pub enum Error {
    #[fail(display = "io error")]
    Io(#[cause] ::std::io::Error),

    #[fail(display = "sync service error")]
    BackendError(#[cause] BackendError),

    #[fail(display = "error processing series")]
    SeriesError(#[cause] SeriesError),

    #[fail(display = "config error")]
    ConfigError(#[cause] ConfigError),

    #[fail(display = "path to [{}] not found. Try setting it with -p", _0)]
    SeriesNotFound(String),

    #[fail(display = "no information about the series to watch was provided")]
    NoSeriesInfoProvided,
}

impl_error_conversion!(Error,
    ::std::io::Error => Io,
    BackendError => BackendError,
    SeriesError => SeriesError,
    ConfigError => ConfigError,
);

#[derive(Fail, Debug)]
pub enum SeriesError {
    #[fail(display = "io error")]
    Io(#[cause] ::std::io::Error),

    #[fail(display = "input error")]
    InputError(#[cause] InputError),

    #[fail(display = "error serializing toml")]
    TomlSerialize(#[cause] ::toml::ser::Error),

    #[fail(display = "error deserializing toml")]
    TomlDeserialize(#[cause] ::toml::de::Error),

    #[fail(display = "failed to open video player")]
    FailedToOpenPlayer(#[cause] ::std::io::Error),

    #[fail(display = "episode number parse failed")]
    EpisodeNumParseFailed(#[cause] ::std::num::ParseIntError),

    #[fail(display = "sync service error")]
    Backend(#[cause] BackendError),

    #[fail(display = "no episodes found")]
    NoEpisodesFound,

    #[fail(display = "multiple series found")]
    MultipleSeriesFound,

    #[fail(display = "unable to get filename")]
    UnableToGetFilename,

    #[fail(display = "failed to get regex captures on episode")]
    EpisodeRegexCaptureFailed,

    #[fail(display = "specified path is not a directory: {}", _0)]
    NotADirectory(String),

    #[fail(display = "episode {} not found", _0)]
    EpisodeNotFound(u32),
}

impl_error_conversion!(SeriesError,
    ::std::io::Error => Io,
    InputError => InputError,
    ::toml::ser::Error => TomlSerialize,
    ::toml::de::Error => TomlDeserialize,
    BackendError => Backend,
);

#[derive(Fail, Debug)]
pub enum InputError {
    #[fail(display = "failed to read line")]
    ReadFailed(#[cause] ::std::io::Error),

    #[fail(display = "failed to parse type: {}", _0)]
    ParseFailed(String),
}

#[derive(Fail, Debug)]
pub enum ConfigError {
    #[fail(display = "io error")]
    Io(#[cause] ::std::io::Error),

    #[fail(display = "input is not valid UTF8")]
    FromUtf8Error(#[cause] ::std::string::FromUtf8Error),

    #[fail(display = "error serializing toml")]
    TomlSerialize(#[cause] ::toml::ser::Error),

    #[fail(display = "error deserializing toml")]
    TomlDeserialize(#[cause] ::toml::de::Error),

    #[fail(display = "access token decode failed")]
    FailedTokenDecode(#[cause] ::base64::DecodeError),

    #[fail(display = "access token not set for sync service")]
    TokenNotSet,
}

impl_error_conversion!(ConfigError,
    ::std::io::Error => Io,
    ::std::string::FromUtf8Error => FromUtf8Error,
    ::toml::ser::Error => TomlSerialize,
    ::toml::de::Error => TomlDeserialize,
);

#[derive(Fail, Debug)]
pub enum BackendError {
    #[fail(display = "io error")]
    Io(#[cause] ::std::io::Error),

    #[fail(display = "float parse error")]
    ParseFloat(#[cause] ::std::num::ParseFloatError),

    #[fail(display = "config error")]
    ConfigError(#[cause] ConfigError),

    #[fail(display = "HTTP error")]
    HttpError(#[cause] ::reqwest::Error),

    #[fail(display = "json error")]
    Json(#[cause] ::serde_json::Error),

    #[fail(display = "received invalid JSON response")]
    InvalidJsonResponse,

    #[fail(display = "received bad response from API: {} {}", _0, _1)]
    BadResponse(u32, String),

    #[fail(display = "unknown score value: {}", _0)]
    UnknownScoreValue(String),

    #[fail(display = "score out of range")]
    OutOfRangeScore,
}

impl_error_conversion!(BackendError,
    ::std::io::Error => Io,
    ::std::num::ParseFloatError => ParseFloat,
    ConfigError => ConfigError,
    ::reqwest::Error => HttpError,
    ::serde_json::Error => Json,
);

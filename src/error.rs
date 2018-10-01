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
    Backend(#[cause] BackendError),

    #[fail(display = "error processing series")]
    Series(#[cause] SeriesError),

    #[fail(display = "config error")]
    Config(#[cause] ConfigError),

    #[fail(display = "path to [{}] not found. Try setting it with -p", _0)]
    SeriesNotFound(String),

    #[fail(display = "no information about the series to watch was provided")]
    NoSeriesInfoProvided,
}

impl_error_conversion!(Error,
    ::std::io::Error => Io,
    BackendError => Backend,
    SeriesError => Series,
    ConfigError => Config,
);

#[derive(Fail, Debug)]
pub enum SeriesError {
    #[fail(display = "input error")]
    Input(#[cause] InputError),

    #[fail(display = "sync service error")]
    Backend(#[cause] BackendError),

    #[fail(display = "io error")]
    Io(#[cause] ::std::io::Error),

    #[fail(display = "error serializing toml")]
    TomlSerialize(#[cause] ::toml::ser::Error),

    #[fail(display = "error deserializing toml")]
    TomlDeserialize(#[cause] ::toml::de::Error),

    #[fail(display = "failed to open video player")]
    FailedToOpenPlayer(#[cause] ::std::io::Error),

    #[fail(display = "episode number parse failed")]
    EpisodeNumParseFailed(#[cause] ::std::num::ParseIntError),

    #[fail(display = "regex error")]
    Regex(#[cause] ::regex::Error),

    #[fail(display = "exit requested (note: this is a bug)")]
    RequestExit,

    #[fail(display = "no series were found")]
    NoSeriesFound,

    #[fail(display = "no episodes were found for the specified series")]
    NoSeriesEpisodes,

    #[fail(display = "unable to get filename")]
    UnableToGetFilename,

    #[fail(display = "failed to get regex captures on episode")]
    EpisodeRegexCaptureFailed,

    #[fail(display = "specified path is not a directory: {}", _0)]
    NotADirectory(String),

    #[fail(display = "episode {} not found", _0)]
    EpisodeNotFound(u32),

    #[fail(display = "no regex capture named \"{}\"", _0)]
    UnknownRegexCapture(String),
}

impl_error_conversion!(SeriesError,
    InputError => Input,
    BackendError => Backend,
    ::std::io::Error => Io,
    ::toml::ser::Error => TomlSerialize,
    ::toml::de::Error => TomlDeserialize,
    ::regex::Error => Regex,
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
    FromUtf8(#[cause] ::std::string::FromUtf8Error),

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
    ::std::string::FromUtf8Error => FromUtf8,
    ::toml::ser::Error => TomlSerialize,
    ::toml::de::Error => TomlDeserialize,
);

#[derive(Fail, Debug)]
pub enum BackendError {
    #[fail(display = "config error")]
    Config(#[cause] ConfigError),

    #[fail(display = "io error")]
    Io(#[cause] ::std::io::Error),

    #[fail(display = "float parse error")]
    ParseFloat(#[cause] ::std::num::ParseFloatError),

    #[fail(display = "HTTP error")]
    Http(#[cause] ::reqwest::Error),

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
    ConfigError => Config,
    ::std::io::Error => Io,
    ::std::num::ParseFloatError => ParseFloat,
    ::reqwest::Error => Http,
    ::serde_json::Error => Json,
);

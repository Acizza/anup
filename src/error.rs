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

    #[fail(display = "MAL error")]
    MALError(#[cause] ::mal::MALError),

    #[fail(display = "error processing series")]
    SeriesError(#[cause] SeriesError),

    #[fail(display = "config error")]
    ConfigError(#[cause] ConfigError),

    #[fail(display = "failed to get current working directory")]
    FailedToGetCWD(#[cause] ::std::io::Error),
}

impl_error_conversion!(Error,
    ::std::io::Error => Io,
    ::mal::MALError => MALError,
    SeriesError => SeriesError,
    ConfigError => ConfigError,
);

#[derive(Fail, Debug)]
pub enum SeriesError {
    #[fail(display = "io error")]
    Io(#[cause] ::std::io::Error),

    #[fail(display = "input prompt error")]
    PromptError(#[cause] PromptError),

    #[fail(display = "MAL error")]
    MALError(#[cause] ::mal::MALError),

    #[fail(display = "json error")]
    Json(#[cause] ::serde_json::Error),

    #[fail(display = "failed to open video player")]
    FailedToOpenPlayer(#[cause] ::std::io::Error),

    #[fail(display = "episode number parse failed")]
    EpisodeNumParseFailed(#[cause] ::std::num::ParseIntError),

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

    #[fail(display = "no anime with id {} found with name [{}] on MAL", _0, _1)]
    UnknownAnimeID(u32, String),
}

impl_error_conversion!(SeriesError,
    ::std::io::Error => Io,
    ::serde_json::Error => Json,
    ::mal::MALError => MALError,
    PromptError => PromptError,
);

#[derive(Fail, Debug)]
pub enum PromptError {
    #[fail(display = "io error")]
    Io(#[cause] ::std::io::Error),

    #[fail(display = "MAL error")]
    MALError(#[cause] ::mal::MALError),

    #[fail(display = "input error")]
    InputError(#[cause] InputError),

    #[fail(display = "no series named [{}] found", _0)]
    NoSeriesFound(String),
}

impl_error_conversion!(PromptError,
    ::std::io::Error => Io,
    ::mal::MALError => MALError,
    InputError => InputError,
);

#[derive(Fail, Debug)]
pub enum InputError {
    #[fail(display = "failed to read line")]
    ReadFailed(#[cause] ::std::io::Error),

    #[fail(display = "failed to parse input string to number")]
    IntParseFailed(#[cause] ::std::num::ParseIntError),
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

    #[fail(display = "password decode failed")]
    FailedPasswordDecode(#[cause] ::base64::DecodeError),

    #[fail(display = "failed to get executable path")]
    FailedToGetExePath(#[cause] ::std::io::Error),
}

impl_error_conversion!(ConfigError,
    ::std::io::Error => Io,
    ::std::string::FromUtf8Error => FromUtf8Error,
    ::toml::ser::Error => TomlSerialize,
    ::toml::de::Error => TomlDeserialize,
);

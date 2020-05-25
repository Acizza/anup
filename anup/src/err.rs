use anime::remote::anilist;
use snafu::{Backtrace, ErrorCompat, GenerateBacktrace, Snafu};
use std::io;
use std::num;
use std::path;
use std::result;
use std::sync::mpsc;

pub type Result<T> = result::Result<T, Error>;

#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
pub enum Error {
    #[snafu(display("{}", source))]
    Anime {
        source: anime::Error,
        backtrace: Backtrace,
    },

    #[snafu(display("io error: {}", source))]
    IO {
        source: io::Error,
        backtrace: Backtrace,
    },

    #[snafu(display("file io error at {}: {}", path.display(), source))]
    FileIO {
        path: path::PathBuf,
        source: io::Error,
        backtrace: Backtrace,
    },

    #[snafu(display("dir entry error at {}: {}", dir.display(), source))]
    EntryIO {
        dir: path::PathBuf,
        source: io::Error,
        backtrace: Backtrace,
    },

    #[snafu(display("toml decode error at {}: {}", path.display(), source))]
    TomlDecode {
        path: path::PathBuf,
        source: toml::de::Error,
        backtrace: Backtrace,
    },

    #[snafu(display("toml encode error at {}: {}", path.display(), source))]
    TomlEncode {
        path: path::PathBuf,
        source: toml::ser::Error,
        backtrace: Backtrace,
    },

    #[snafu(display("mpsc channel receive error: {}", source))]
    MPSCRecv {
        source: mpsc::RecvError,
        backtrace: Backtrace,
    },

    #[snafu(display("diesel error: {}", source))]
    Diesel {
        source: diesel::result::Error,
        backtrace: Backtrace,
    },

    #[snafu(display("diesel connection error: {}", source))]
    DieselConnection {
        source: diesel::result::ConnectionError,
        backtrace: Backtrace,
    },

    #[snafu(display("error parsing int: {}", source))]
    ParseNum {
        source: num::ParseIntError,
        backtrace: Backtrace,
    },

    #[snafu(display("no series found on disk matching name: {}", name))]
    NoMatchingSeriesOnDisk { name: String },

    #[snafu(display("{} must be added to the program first\nyou can do this in the TUI by using the add command", name))]
    MustAddSeries { name: String },

    #[snafu(display("path must be a directory"))]
    NotADirectory,

    #[snafu(display("series name must be specified"))]
    MustSpecifySeriesName,

    #[snafu(display("command can only be ran in online mode"))]
    MustRunOnline,

    #[snafu(display("failed to play episode {}: {}", episode, source))]
    FailedToPlayEpisode { episode: u32, source: io::Error },

    #[snafu(display("episode {} not found", episode))]
    EpisodeNotFound { episode: u32 },

    #[snafu(display(
        "need access token for AniList\ngo to {} and specify your token with the -t flag",
        anilist::auth_url(crate::ANILIST_CLIENT_ID)
    ))]
    NeedAniListToken,

    #[snafu(display("no command specified"))]
    NoCommandSpecified,

    #[snafu(display("command not found: {}", command))]
    CommandNotFound { command: String },

    #[snafu(display("{} argument(s) specified, need at least {}", has, need))]
    NotEnoughArguments { has: usize, need: usize },

    #[snafu(display("unknown argument: {}", value))]
    UnknownCmdPromptArg { value: String },

    #[snafu(display("series already exists as {}", name))]
    SeriesAlreadyExists { name: String },

    #[snafu(display("must be online to {}", reason))]
    MustBeOnlineTo { reason: String },

    #[snafu(display("invalid score"))]
    InvalidScore,

    #[snafu(display("since this is a new series, you must specify the series ID\nyou can use the add command in the TUI instead to avoid this"))]
    NewSeriesNeedsID,
}

impl Error {
    pub fn is_file_nonexistant(&self) -> bool {
        match self {
            Error::FileIO { source, .. } => source.kind() == io::ErrorKind::NotFound,
            _ => false,
        }
    }
}

impl From<anime::Error> for Error {
    fn from(source: anime::Error) -> Error {
        Error::Anime {
            source,
            backtrace: Backtrace::generate(),
        }
    }
}

impl From<diesel::result::Error> for Error {
    fn from(source: diesel::result::Error) -> Self {
        Self::Diesel {
            source,
            backtrace: Backtrace::generate(),
        }
    }
}

impl From<diesel::result::ConnectionError> for Error {
    fn from(source: diesel::result::ConnectionError) -> Self {
        Self::DieselConnection {
            source,
            backtrace: Backtrace::generate(),
        }
    }
}

impl From<num::ParseIntError> for Error {
    fn from(source: num::ParseIntError) -> Self {
        Self::ParseNum {
            source,
            backtrace: Backtrace::generate(),
        }
    }
}

pub fn display_error(err: Error) {
    eprintln!("{}", err);

    let mut print_backtrace = true;

    if let Error::Anime { source, .. } = &err {
        if let anime::Error::NeedExistingSeriesData = source {
            eprintln!("run the program with the --prefetch flag first when an internet connection is available");
            print_backtrace = false;
        }
    }

    if !print_backtrace {
        return;
    }

    if let Some(backtrace) = err.backtrace() {
        eprintln!("backtrace:\n{}", backtrace);
    }
}

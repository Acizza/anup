use snafu::{Backtrace, ErrorCompat, Snafu};
use std::io;
use std::path;
use std::result;
use std::sync::mpsc;

pub type Result<T> = result::Result<T, Error>;

#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
pub enum Error {
    #[snafu(display("backend lib error: {}", source))]
    Anime {
        source: anime::Error,
        backtrace: Backtrace,
    },

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

    #[snafu(display("mpsc channel receive error: {}", source))]
    MPSCRecv {
        source: mpsc::RecvError,
        backtrace: Backtrace,
    },

    #[snafu(display("no series found with name: {}", name))]
    NoMatchingSeries { name: String },

    #[snafu(display("need existing series info to run in offline mode\nrun the program with --prefetch first when an internet connection is available"))]
    RunWithPrefetch,

    #[snafu(display("series name must be specified"))]
    MustSpecifySeriesName,

    #[snafu(display("command can only be ran in online mode"))]
    MustRunOnline,

    #[snafu(display("failed to play episode {}: {}", episode, source))]
    FailedToPlayEpisode { episode: u32, source: io::Error },

    #[snafu(display("video player didn't exit normally"))]
    AbnormalPlayerExit,

    #[snafu(display("episode {} not found", episode))]
    EpisodeNotFound { episode: u32 },

    #[snafu(display("please run the program in CLI mode to set your AniList token"))]
    GetAniListTokenFromCLI,

    #[snafu(display("no command specified"))]
    NoCommandSpecified,

    #[snafu(display("command not found: {}", command))]
    CommandNotFound { command: String },

    #[snafu(display("{} argument(s) specified, need at least {}", has, need))]
    NotEnoughArguments { has: usize, need: usize },

    #[snafu(display("unknown argument: {}", value))]
    UnknownCmdPromptArg { value: String },

    #[snafu(display("missing \"{}\" group for custom episode matcher\nmake sure both {{title}} and {{episode}} are specified", group))]
    MissingEpisodeMatcherGroup { group: &'static str },
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
            backtrace: Backtrace::new(),
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

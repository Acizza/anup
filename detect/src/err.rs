use snafu::{Backtrace, Snafu};
use std::io;
use std::path;
use std::result;

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

    #[snafu(display("no series found with name: {}", name))]
    NoMatchingSeries { name: String },
}

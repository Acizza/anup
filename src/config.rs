use crate::file::{FileType, SaveDir, SaveFile};
use serde_derive::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub series_dir: PathBuf,
    pub reset_dates_on_rewatch: bool,
}

impl Config {
    pub fn new<P>(series_dir: P) -> Config
    where
        P: Into<PathBuf>,
    {
        Config {
            series_dir: series_dir.into(),
            reset_dates_on_rewatch: false,
        }
    }
}

impl SaveFile for Config {
    fn filename() -> &'static str {
        "config.toml"
    }

    fn save_dir() -> SaveDir {
        SaveDir::Config
    }

    fn file_type() -> FileType {
        FileType::Toml
    }
}

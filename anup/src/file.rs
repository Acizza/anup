use crate::err::{self, Result};
use lazy_static::lazy_static;
use serde::de::DeserializeOwned;
use serde::Serialize;
use snafu::ResultExt;
use std::fs::{self};
use std::path::{Path, PathBuf};

pub trait TomlSaveFile
where
    Self: DeserializeOwned + Serialize,
{
    fn filename() -> &'static str;
    fn save_dir() -> SaveDir;

    fn save_path() -> PathBuf {
        let mut path = PathBuf::from(Self::save_dir().path());
        path.push(Self::filename());
        path.set_extension("toml");
        path
    }

    fn load() -> Result<Self> {
        let path = Self::save_path();
        let content = fs::read_to_string(&path).context(err::FileIO { path: &path })?;
        toml::from_str(&content).context(err::TomlDecode { path })
    }

    fn save(&self) -> Result<()> {
        let path = Self::save_path();

        if let Some(dir) = path.parent() {
            if !dir.exists() {
                fs::create_dir_all(dir).context(err::FileIO { path: &path })?;
            }
        }

        let value = toml::to_string_pretty(self).context(err::TomlEncode { path: &path })?;
        fs::write(&path, value).context(err::FileIO { path })
    }
}

pub enum SaveDir {
    Config,
    LocalData,
}

impl SaveDir {
    pub fn path(&self) -> &Path {
        lazy_static! {
            static ref CONFIG_PATH: PathBuf = {
                let mut dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("~/.config/"));
                dir.push(env!("CARGO_PKG_NAME"));
                dir
            };
            static ref LOCALDATA_PATH: PathBuf = {
                let mut dir =
                    dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("~/.local/share/"));
                dir.push(env!("CARGO_PKG_NAME"));
                dir
            };
        }

        match self {
            SaveDir::Config => CONFIG_PATH.as_ref(),
            SaveDir::LocalData => LOCALDATA_PATH.as_ref(),
        }
    }
}

impl TomlSaveFile for anime::remote::anilist::AccessToken {
    fn filename() -> &'static str {
        "anilist"
    }

    fn save_dir() -> SaveDir {
        SaveDir::Config
    }
}

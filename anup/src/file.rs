use crate::err::{self, Result};
use once_cell::sync::Lazy;
use serde::de::DeserializeOwned;
use serde::Serialize;
use snafu::ResultExt;
use std::fs;
use std::path::{Path, PathBuf};

pub trait TomlFile: DeserializeOwned + Serialize {
    fn filename() -> &'static str;
    fn save_dir() -> SaveDir;

    fn validated_save_path() -> Result<PathBuf> {
        let mut path = Self::save_dir().validated_dir_path()?.to_path_buf();
        path.push(Self::filename());
        path.set_extension("toml");
        Ok(path)
    }

    fn load() -> Result<Self> {
        let path = Self::validated_save_path()?;
        let contents = fs::read_to_string(&path).context(err::FileIO { path: &path })?;
        toml::from_str(&contents).context(err::TomlDecode { path })
    }

    fn save(&self) -> Result<()> {
        let path = Self::validated_save_path()?;
        let serialized = toml::to_string_pretty(self).context(err::TomlEncode { path: &path })?;
        fs::write(&path, serialized).context(err::FileIO { path })
    }
}

pub enum SaveDir {
    Config,
    LocalData,
}

impl SaveDir {
    pub fn dir_path(&self) -> &Path {
        static CONFIG_PATH: Lazy<PathBuf> = Lazy::new(|| {
            let mut dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("~/.config/"));
            dir.push(env!("CARGO_PKG_NAME"));
            dir
        });

        static LOCAL_DATA_PATH: Lazy<PathBuf> = Lazy::new(|| {
            let mut dir =
                dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("~/.local/share/"));
            dir.push(env!("CARGO_PKG_NAME"));
            dir
        });

        match self {
            SaveDir::Config => CONFIG_PATH.as_ref(),
            SaveDir::LocalData => LOCAL_DATA_PATH.as_ref(),
        }
    }

    pub fn validated_dir_path(&self) -> Result<&Path> {
        let dir = self.dir_path();

        if !dir.exists() {
            fs::create_dir_all(dir).context(err::FileIO { path: dir })?;
        }

        Ok(dir)
    }
}

impl TomlFile for anime::remote::AccessToken {
    fn filename() -> &'static str {
        "token"
    }

    fn save_dir() -> SaveDir {
        SaveDir::Config
    }
}

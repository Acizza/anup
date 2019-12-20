use crate::err::{self, Result};
use once_cell::sync::Lazy;
use serde::de::DeserializeOwned;
use serde::Serialize;
use snafu::ResultExt;
use std::fs;
use std::path::{Path, PathBuf};

pub trait SaveFile
where
    Self: DeserializeOwned + Serialize,
{
    fn filename() -> &'static str;
    fn file_type() -> FileType;
    fn save_dir() -> SaveDir;

    fn save_path() -> Result<PathBuf> {
        let mut path = Self::save_dir().validated_path()?.to_path_buf();
        path.push(Self::filename());
        path.set_extension(Self::file_type().extension());
        Ok(path)
    }

    fn load() -> Result<Self> {
        let path = Self::save_path()?;
        Self::file_type().deserialize_from_file(path)
    }

    fn save(&self) -> Result<()> {
        let path = Self::save_path()?;
        Self::file_type().serialize_to_file(path, self)
    }
}

pub enum FileType {
    Toml,
}

impl FileType {
    pub fn extension(&self) -> &'static str {
        match self {
            FileType::Toml => "toml",
        }
    }

    pub fn serialize_to_file<P, T>(&self, path: P, item: &T) -> Result<()>
    where
        P: AsRef<Path>,
        T: Serialize,
    {
        let path = path.as_ref();

        match self {
            FileType::Toml => {
                let serialized = toml::to_string_pretty(item).context(err::TomlEncode { path })?;
                fs::write(path, serialized).context(err::FileIO { path })
            }
        }
    }

    pub fn deserialize_from_file<P, T>(&self, path: P) -> Result<T>
    where
        P: AsRef<Path>,
        T: DeserializeOwned,
    {
        let path = path.as_ref();

        match self {
            FileType::Toml => {
                let contents = fs::read_to_string(path).context(err::FileIO { path })?;
                toml::from_str(&contents).context(err::TomlDecode { path })
            }
        }
    }
}

pub enum SaveDir {
    Config,
    LocalData,
}

impl SaveDir {
    pub fn path(&self) -> &Path {
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

    pub fn validated_path(&self) -> Result<&Path> {
        let path = self.path();

        if let Some(dir) = path.parent() {
            if !dir.exists() {
                fs::create_dir_all(dir).context(err::FileIO { path })?;
            }
        }

        Ok(path)
    }
}

impl SaveFile for anime::remote::AccessToken {
    fn filename() -> &'static str {
        "token"
    }

    fn file_type() -> FileType {
        FileType::Toml
    }

    fn save_dir() -> SaveDir {
        SaveDir::Config
    }
}

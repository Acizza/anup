use crate::err::{self, Result};
use lazy_static::lazy_static;
use serde::de::DeserializeOwned;
use serde::Serialize;
use snafu::ResultExt;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

pub trait SaveFile
where
    Self: DeserializeOwned + Serialize,
{
    fn filename() -> &'static str;
    fn file_type() -> FileType;
    fn save_dir() -> SaveDir;

    fn save_path() -> PathBuf {
        let mut path = PathBuf::from(Self::save_dir().path());
        path.push(Self::filename());
        path.set_extension(Self::file_type().extension());
        path
    }

    fn load() -> Result<Self> {
        let path = Self::save_path();
        Self::file_type().deserialize_from_file(path)
    }

    fn save(&self) -> Result<()> {
        let path = Self::save_path();
        Self::file_type().serialize_to_file(path, self)
    }
}

pub enum FileType {
    Toml,
    MessagePack,
}

impl FileType {
    pub fn extension(&self) -> &'static str {
        match self {
            FileType::Toml => "toml",
            FileType::MessagePack => "mpack",
        }
    }

    pub fn serialize_to_file<P, T>(&self, path: P, item: &T) -> Result<()>
    where
        P: AsRef<Path>,
        T: Serialize,
    {
        let path = path.as_ref();

        if let Some(dir) = path.parent() {
            if !dir.exists() {
                fs::create_dir_all(dir).context(err::FileIO { path })?;
            }
        }

        match self {
            FileType::Toml => {
                let serialized = toml::to_string_pretty(item).context(err::TomlEncode { path })?;
                fs::write(path, serialized).context(err::FileIO { path })
            }
            FileType::MessagePack => {
                let serialized = rmp_serde::to_vec(item).context(err::RMPEncode { path })?;
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
            FileType::MessagePack => {
                let file = File::open(path).context(err::FileIO { path })?;
                rmp_serde::from_read(file).context(err::RMPDecode { path })
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

impl SaveFile for anime::remote::anilist::AccessToken {
    fn filename() -> &'static str {
        "anilist"
    }

    fn file_type() -> FileType {
        FileType::Toml
    }

    fn save_dir() -> SaveDir {
        SaveDir::Config
    }
}

use crate::err::{self, Result};
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
    fn save_dir() -> SaveDir;
    fn file_type() -> FileType;

    fn save_path() -> PathBuf {
        let mut dir = Self::save_dir().path();
        dir.push(Self::filename());
        dir
    }

    fn load() -> Result<Self> {
        let path = Self::save_path();
        let ftype = Self::file_type();
        ftype.deserialize_from_file(path)
    }

    fn save(&self) -> Result<()> {
        let path = Self::save_path();
        let ftype = Self::file_type();
        ftype.serialize_to_file(self, path)
    }
}

pub trait SaveFileInDir
where
    Self: DeserializeOwned + Serialize,
{
    fn filename() -> &'static str;
    fn save_dir() -> SaveDir;
    fn file_type() -> FileType;

    fn save_path<S>(dir: S) -> PathBuf
    where
        S: AsRef<str>,
    {
        let mut path = Self::save_dir().path();
        path.push(dir.as_ref());
        path.push(Self::filename());
        path
    }

    fn load<S>(dir: S) -> Result<Self>
    where
        S: AsRef<str>,
    {
        let path = Self::save_path(dir);
        let ftype = Self::file_type();
        ftype.deserialize_from_file(path)
    }

    fn save<S>(&self, dir: S) -> Result<()>
    where
        S: AsRef<str>,
    {
        let path = Self::save_path(dir);
        let ftype = Self::file_type();
        ftype.serialize_to_file(self, path)
    }
}

pub enum FileType {
    Toml,
    MessagePack,
}

impl FileType {
    fn serialize_to_file<T, P>(&self, item: &T, path: P) -> Result<()>
    where
        T: Serialize,
        P: AsRef<Path>,
    {
        let path = path.as_ref();

        if let Some(dir) = path.parent() {
            if !dir.exists() {
                fs::create_dir_all(dir).context(err::FileIO { path })?;
            }
        }

        match self {
            FileType::Toml => {
                let value = toml::to_string_pretty(item).context(err::TomlEncode { path })?;
                fs::write(&path, value).context(err::FileIO { path })
            }
            FileType::MessagePack => {
                let bytes = rmp_serde::to_vec(item).context(err::RMPEncode { path })?;
                fs::write(&path, bytes).context(err::FileIO { path })
            }
        }
    }

    fn deserialize_from_file<T, P>(&self, path: P) -> Result<T>
    where
        T: DeserializeOwned,
        P: AsRef<Path>,
    {
        let path = path.as_ref();

        match self {
            FileType::Toml => {
                let content = fs::read_to_string(&path).context(err::FileIO { path })?;
                toml::from_str(&content).context(err::TomlDecode { path })
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
    pub fn path(&self) -> PathBuf {
        let mut dir = match self {
            SaveDir::Config => dirs::config_dir().unwrap_or_else(|| PathBuf::from("~/.config/")),
            SaveDir::LocalData => {
                dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("~/.local/share/"))
            }
        };

        dir.push(env!("CARGO_PKG_NAME"));
        dir
    }
}

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
    fn save_dir() -> SaveDir;
    fn file_type() -> FileType;

    fn save_path<'a, S, D>(prefix: S, subdir: D) -> PathBuf
    where
        S: Into<Option<String>>,
        D: Into<Option<&'a str>>,
    {
        let mut path = PathBuf::from(Self::save_dir().path());

        if let Some(subdir) = subdir.into() {
            path.push(subdir);
        }

        if let Some(prefix) = prefix.into() {
            path.push(format!("{}_{}", prefix, Self::filename()));
        } else {
            path.push(Self::filename());
        }

        path
    }

    fn load<'a, S>(subdir: S) -> Result<Self>
    where
        S: Into<Option<&'a str>>,
    {
        let path = Self::save_path(None, subdir);
        let ftype = Self::file_type();
        ftype.deserialize_from_file(path)
    }

    fn load_with_id<'a, S>(id: u32, subdir: S) -> Result<Self>
    where
        S: Into<Option<&'a str>>,
    {
        let id = id.to_string();
        let path = Self::save_path(id, subdir);
        let ftype = Self::file_type();
        ftype.deserialize_from_file(path)
    }

    fn save<'a, S>(&self, subdir: S) -> Result<()>
    where
        S: Into<Option<&'a str>>,
    {
        let path = Self::save_path(None, subdir.into());
        let ftype = Self::file_type();
        ftype.serialize_to_file(self, path)
    }

    fn save_with_id<'a, S>(&self, id: u32, subdir: S) -> Result<()>
    where
        S: Into<Option<&'a str>>,
    {
        let id = id.to_string();
        let path = Self::save_path(id, subdir);
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

    pub fn get_subdirs(&self) -> Result<Vec<String>> {
        let path = self.path();
        let entries = fs::read_dir(&path).context(err::FileIO { path: &path })?;

        let mut paths = Vec::new();

        for entry in entries {
            let entry = entry.context(err::EntryIO { dir: &path })?;
            let ftype = entry.file_type().context(err::EntryIO { dir: &path })?;

            if !ftype.is_dir() {
                continue;
            }

            let fname = entry.file_name().to_string_lossy().into_owned();
            paths.push(fname);
        }

        Ok(paths)
    }

    pub fn remove_subdir<S>(&self, name: S) -> Result<()>
    where
        S: AsRef<str>,
    {
        let base_path = self.path();

        let mut path = PathBuf::from(base_path);
        path.push(name.as_ref());
        path.canonicalize().context(err::IO)?;

        if !path.starts_with(base_path) {
            return Ok(());
        }

        fs::remove_dir_all(&path).context(err::FileIO { path })
    }
}

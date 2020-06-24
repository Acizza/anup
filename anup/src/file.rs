use crate::err;
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs::{self, DirEntry, File};
use std::path::{Path, PathBuf};

pub trait SerializedFile: DeserializeOwned + Serialize + Default {
    fn filename() -> &'static str;
    fn save_dir() -> SaveDir;
    fn format() -> FileFormat;

    fn validated_save_path() -> Result<PathBuf> {
        let mut path = Self::save_dir().validated_dir_path()?.to_path_buf();
        path.push(Self::filename());
        path.set_extension(Self::format().extension());
        Ok(path)
    }

    fn load() -> Result<Self> {
        let path = Self::validated_save_path().context("getting path")?;

        Self::format()
            .deserialize(path)
            .context("deserializing file")
    }

    fn load_or_create() -> Result<Self> {
        match Self::load() {
            Ok(data) => Ok(data),
            Err(err) if err::is_file_nonexistant(&err) => {
                let data = Self::default();
                data.save()?;
                Ok(data)
            }
            err @ Err(_) => err,
        }
    }

    fn save(&self) -> Result<()> {
        let path = Self::validated_save_path()?;
        Self::format().serialize(self, path)
    }
}

#[derive(Copy, Clone)]
pub enum FileFormat {
    Toml,
    MessagePack,
}

impl FileFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Toml => "toml",
            Self::MessagePack => "mpack",
        }
    }

    pub fn deserialize<P, T>(self, path: P) -> Result<T>
    where
        P: AsRef<Path>,
        T: DeserializeOwned,
    {
        let path = path.as_ref();

        match self {
            Self::Toml => {
                let contents = fs::read_to_string(&path).context("reading file")?;
                toml::from_str(&contents).context("decoding TOML")
            }
            Self::MessagePack => {
                let file = File::open(path).context("opening file")?;
                rmp_serde::from_read(file).context("decoding MessagePack")
            }
        }
    }

    pub fn serialize<T, P>(self, data: &T, path: P) -> Result<()>
    where
        T: Serialize,
        P: AsRef<Path>,
    {
        let path = path.as_ref();

        match self {
            Self::Toml => {
                let serialized = toml::to_string_pretty(data).context("encoding TOML")?;
                fs::write(&path, serialized).context("writing file")
            }
            Self::MessagePack => {
                let mut file = File::create(path).context("creating / opening file")?;
                rmp_serde::encode::write(&mut file, data).context("encoding MessagePack")
            }
        }
    }
}

#[derive(Copy, Clone)]
pub enum SaveDir {
    Config,
    LocalData,
}

impl SaveDir {
    pub fn dir_path(self) -> &'static Path {
        static CONFIG_PATH: Lazy<PathBuf> = Lazy::new(|| {
            let mut dir = dirs_next::config_dir().unwrap_or_else(|| PathBuf::from("~/.config/"));
            dir.push(env!("CARGO_PKG_NAME"));
            dir
        });

        static LOCAL_DATA_PATH: Lazy<PathBuf> = Lazy::new(|| {
            let mut dir =
                dirs_next::data_local_dir().unwrap_or_else(|| PathBuf::from("~/.local/share/"));
            dir.push(env!("CARGO_PKG_NAME"));
            dir
        });

        match self {
            SaveDir::Config => CONFIG_PATH.as_ref(),
            SaveDir::LocalData => LOCAL_DATA_PATH.as_ref(),
        }
    }

    pub fn validated_dir_path(self) -> Result<&'static Path> {
        let dir = self.dir_path();

        if !dir.exists() {
            fs::create_dir_all(dir).context("creating directory")?;
        }

        Ok(dir)
    }
}

pub fn read_dir<D>(dir: D) -> Result<Vec<DirEntry>>
where
    D: AsRef<Path>,
{
    let dir = dir.as_ref();
    let entries = fs::read_dir(dir).context("reading directory")?;

    let mut dirs = Vec::new();

    for entry in entries {
        let entry = entry.context("getting dir entry")?;
        let etype = entry.file_type().context("getting dir entry file type")?;

        if !etype.is_dir() {
            continue;
        }

        dirs.push(entry);
    }

    Ok(dirs)
}

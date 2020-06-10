use crate::err::{self, Result};
use once_cell::sync::Lazy;
use serde::de::DeserializeOwned;
use serde::Serialize;
use snafu::ResultExt;
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
        let path = Self::validated_save_path()?;
        Self::format().deserialize(path)
    }

    fn load_or_create() -> Result<Self> {
        match Self::load() {
            Ok(data) => Ok(data),
            Err(ref err) if err.is_file_nonexistant() => {
                let data = Self::default();
                data.save()?;
                Ok(data)
            }
            Err(err) => Err(err),
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
                let contents = fs::read_to_string(&path).context(err::FileIO { path })?;
                toml::from_str(&contents).context(err::TomlDecode { path })
            }
            Self::MessagePack => {
                let file = File::open(path).context(err::FileIO { path })?;
                rmp_serde::from_read(file).context(err::RMPDecode { path })
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
                let serialized = toml::to_string_pretty(data).context(err::TomlEncode { path })?;
                fs::write(&path, serialized).context(err::FileIO { path })
            }
            Self::MessagePack => {
                let mut file = File::create(path).context(err::FileIO { path })?;
                rmp_serde::encode::write(&mut file, data).context(err::RMPEncode { path })
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
            fs::create_dir_all(dir).context(err::FileIO { path: dir })?;
        }

        Ok(dir)
    }
}

pub fn read_dir<D>(dir: D) -> Result<Vec<DirEntry>>
where
    D: AsRef<Path>,
{
    let dir = dir.as_ref();
    let entries = fs::read_dir(dir).context(err::FileIO { path: dir })?;

    let mut dirs = Vec::new();

    for entry in entries {
        let entry = entry.context(err::EntryIO { dir })?;
        let etype = entry.file_type().context(err::EntryIO { dir })?;

        if !etype.is_dir() {
            continue;
        }

        dirs.push(entry);
    }

    Ok(dirs)
}

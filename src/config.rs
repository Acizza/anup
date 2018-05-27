use base64;
use directories::ProjectDirs;
use error::ConfigError;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, ErrorKind, Write};
use std::path::PathBuf;
use toml;

pub const DEFAULT_CONFIG_NAME: &str = "config.toml";

lazy_static! {
    static ref PROJECT_DIRS: ProjectDirs = ProjectDirs::from("", "", env!("CARGO_PKG_NAME"));
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub user: User,
    pub series: HashMap<String, PathBuf>,
}

impl Config {
    pub fn new(user: User) -> Config {
        Config {
            user,
            series: HashMap::new(),
        }
    }

    pub fn load() -> Result<Config, ConfigError> {
        let path = get_config_file_path()?;

        let file_contents = match fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(err) => match err.kind() {
                ErrorKind::NotFound => return Ok(Config::new(User::new(None))),
                _ => return Err(err.into()),
            },
        };

        let config = toml::from_str(&file_contents)?;
        Ok(config)
    }

    pub fn save(&mut self, save_access_token: bool) -> Result<(), ConfigError> {
        let access_token = self.user.access_token.clone();

        if !save_access_token {
            self.user.access_token = None;
        }

        let path = get_config_file_path()?;
        let mut file = File::create(path)?;
        let toml = toml::to_string_pretty(self)?;

        write!(file, "{}", toml)?;

        self.user.access_token = access_token;
        Ok(())
    }

    pub fn remove_invalid_series(&mut self) {
        self.series.retain(|_, path: &mut PathBuf| path.exists());
    }
}

fn get_config_file_path() -> io::Result<PathBuf> {
    let path = PROJECT_DIRS.config_dir();

    if !path.exists() {
        fs::create_dir(path)?;
    }

    let mut path = PathBuf::from(path);
    path.push(DEFAULT_CONFIG_NAME);

    Ok(path)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct User {
    pub access_token: Option<String>,
}

impl User {
    pub fn new(access_token: Option<&str>) -> User {
        User {
            access_token: access_token.map(|t| base64::encode(t)),
        }
    }

    pub fn encode_access_token(&mut self, access_token: &str) {
        self.access_token = Some(base64::encode(access_token));
    }

    pub fn decode_access_token(&self) -> Result<String, ConfigError> {
        let access_token = self.access_token.as_ref().ok_or(ConfigError::TokenNotSet)?;
        let bytes = base64::decode(access_token).map_err(ConfigError::FailedTokenDecode)?;
        let string = String::from_utf8(bytes)?;

        Ok(string)
    }
}

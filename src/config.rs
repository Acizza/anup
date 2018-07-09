use base64;
use error::ConfigError;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{ErrorKind, Write};
use std::path::PathBuf;
use toml;
use util;

pub const DEFAULT_CONFIG_NAME: &str = "config.toml";

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub series: HashMap<String, PathBuf>,
    pub anilist: AniList,
}

impl Config {
    pub fn new() -> Config {
        Config {
            series: HashMap::new(),
            anilist: AniList::new(),
        }
    }

    pub fn load() -> Result<Config, ConfigError> {
        let path = util::get_valid_config_path(DEFAULT_CONFIG_NAME)?;

        let file_contents = match fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(err) => match err.kind() {
                ErrorKind::NotFound => return Ok(Config::new()),
                _ => return Err(err.into()),
            },
        };

        let config = toml::from_str(&file_contents)?;
        Ok(config)
    }

    pub fn save(&self) -> Result<(), ConfigError> {
        let path = util::get_valid_config_path(DEFAULT_CONFIG_NAME)?;
        let mut file = File::create(path)?;
        let toml = toml::to_string_pretty(self)?;

        write!(file, "{}", toml)?;
        Ok(())
    }

    pub fn remove_invalid_series(&mut self) {
        self.series.retain(|_, path: &mut PathBuf| path.exists());
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AccessToken {
    pub token: Option<String>,
}

impl AccessToken {
    pub fn new(token: Option<String>) -> AccessToken {
        AccessToken { token }
    }

    pub fn encode(&mut self, token: &str) {
        self.token = Some(base64::encode(token));
    }

    pub fn decode(&self) -> Result<String, ConfigError> {
        let token = self.token.as_ref().ok_or(ConfigError::TokenNotSet)?;
        let bytes = base64::decode(token).map_err(ConfigError::FailedTokenDecode)?;
        let string = String::from_utf8(bytes)?;

        Ok(string)
    }

    pub fn is_set(&self) -> bool {
        self.token.is_some()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AniList {
    #[serde(flatten)]
    pub token: AccessToken,
}

impl AniList {
    pub fn new() -> AniList {
        AniList {
            token: AccessToken::new(None),
        }
    }
}

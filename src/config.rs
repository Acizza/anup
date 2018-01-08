use base64;
use failure::Error;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use toml;

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub user: User,
    #[serde(skip)] pub path: PathBuf,
}

impl Config {
    pub fn new(user: User, path: PathBuf) -> Config {
        Config { user, path }
    }

    pub fn from_path(path: &Path) -> Result<Config, Error> {
        let mut file = File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        let mut config: Config = toml::from_str(&contents)?;
        config.path = path.into();

        Ok(config)
    }

    pub fn save(&self) -> Result<(), Error> {
        let mut file = File::create(&self.path)?;
        let toml = toml::to_string_pretty(self)?;

        write!(file, "{}", toml)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct User {
    pub name: String,
    password: String,
}

impl User {
    pub fn new<S: Into<String>>(username: S, password: &str) -> User {
        User {
            name: username.into(),
            password: base64::encode(password),
        }
    }

    pub fn encode_password(&mut self, password: &str) {
        self.password = base64::encode(password);
    }

    pub fn decode_password(&self) -> Result<String, Error> {
        let bytes = base64::decode(&self.password)?;
        let string = String::from_utf8(bytes)?;

        Ok(string)
    }
}

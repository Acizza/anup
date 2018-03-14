use base64;
use error::ConfigError;
use input;
use std::env;
use std::fs::File;
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use toml;

pub const DEFAULT_CONFIG_NAME: &str = "config.toml";

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub user: User,
    #[serde(skip)] pub path: PathBuf,
}

impl Config {
    pub fn new(user: User, path: PathBuf) -> Config {
        Config { user, path }
    }

    pub fn from_path(path: &Path) -> Result<Config, ConfigError> {
        let mut file = File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        let mut config: Config = toml::from_str(&contents)?;
        config.path = path.into();

        Ok(config)
    }

    pub fn save(&self) -> Result<(), ConfigError> {
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

    pub fn decode_password(&self) -> Result<String, ConfigError> {
        let bytes = base64::decode(&self.password).map_err(ConfigError::FailedPasswordDecode)?;
        let string = String::from_utf8(bytes)?;

        Ok(string)
    }
}

pub fn load(path: Option<&Path>) -> Result<Config, ConfigError> {
    let path = match path {
        Some(path) => PathBuf::from(path),
        None => {
            let mut current = env::current_exe().map_err(ConfigError::FailedToGetExePath)?;

            current.pop();
            current.push(DEFAULT_CONFIG_NAME);
            current
        }
    };

    match Config::from_path(&path) {
        Ok(config) => Ok(config),
        Err(ConfigError::Io(e)) => match e.kind() {
            ErrorKind::NotFound => {
                println!("please enter your MAL username:");
                let name = input::read_line()?;

                println!("please enter your MAL password:");
                let pass = input::read_line()?;

                let user = User::new(name, &pass);
                let config = Config::new(user, path);

                Ok(config)
            }
            _ => Err(ConfigError::Io(e)),
        },
        Err(e) => Err(e),
    }
}

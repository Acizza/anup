use base64;
use directories::ProjectDirs;
use error::ConfigError;
use input;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};
use toml;

pub const DEFAULT_CONFIG_NAME: &str = "config.toml";

lazy_static! {
    static ref PROJECT_DIRS: ProjectDirs = ProjectDirs::from("", "", env!("CARGO_PKG_NAME"));
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub user: User,
    pub series: HashMap<String, PathBuf>,
    #[serde(skip)]
    pub path: PathBuf,
}

impl Config {
    pub fn new(user: User, path: PathBuf) -> Config {
        Config {
            user,
            series: HashMap::new(),
            path,
        }
    }

    pub fn from_path(path: &Path) -> Result<Config, ConfigError> {
        let file_contents = fs::read_to_string(path)?;

        let mut config: Config = toml::from_str(&file_contents)?;
        config.path = path.into();

        Ok(config)
    }

    pub fn save(&mut self, save_password: bool) -> Result<(), ConfigError> {
        let password = self.user.password.clone();

        if !save_password {
            self.user.password = None;
        }

        let mut file = File::create(&self.path)?;
        let toml = toml::to_string_pretty(self)?;

        write!(file, "{}", toml)?;

        self.user.password = password;
        Ok(())
    }

    pub fn remove_invalid_series(&mut self) {
        self.series.retain(|_, path: &mut PathBuf| path.exists());
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct User {
    pub name: String,
    password: Option<String>,
}

impl User {
    pub fn new<S: Into<String>>(username: S, password: &str) -> User {
        User {
            name: username.into(),
            password: Some(base64::encode(password)),
        }
    }

    pub fn encode_password(&mut self, password: &str) {
        self.password = Some(base64::encode(password));
    }

    pub fn decode_password(&self) -> Result<String, ConfigError> {
        let password = self.password.as_ref().ok_or(ConfigError::PasswordNotSet)?;
        let bytes = base64::decode(password).map_err(ConfigError::FailedPasswordDecode)?;
        let string = String::from_utf8(bytes)?;

        Ok(string)
    }
}

pub fn load() -> Result<Config, ConfigError> {
    let path = get_config_file_path()?;

    match Config::from_path(&path) {
        Ok(mut config) => {
            if config.user.password.is_none() {
                println!("please enter your MAL password:");
                let pass = input::read_line()?;

                config.user.encode_password(&pass);
            }

            Ok(config)
        }
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

fn get_config_file_path() -> io::Result<PathBuf> {
    let path = PROJECT_DIRS.config_dir();

    if !path.exists() {
        fs::create_dir(path)?;
    }

    let mut path = PathBuf::from(path);
    path.push(DEFAULT_CONFIG_NAME);

    Ok(path)
}

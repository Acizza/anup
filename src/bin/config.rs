use base64;
use failure::Error;
use input;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use toml;

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub user: Option<User>,
    #[serde(skip)] pub path: PathBuf,
}

impl Config {
    pub fn new(path: PathBuf) -> Config {
        Config { user: None, path }
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

    // TODO: return Cow<User> instead to avoid cloning so much
    pub fn load_user_prompt(&mut self, username: Option<String>) -> Result<User, Error> {
        match self.user {
            Some(ref mut user) => {
                if let Some(ref name) = username {
                    if &user.name != name {
                        *user = User::from_input(username.clone())?;
                    }
                }

                Ok(user.clone())
            }
            None => {
                let user = User::from_input(username)?;
                self.user = Some(user.clone());

                Ok(user)
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct User {
    pub name: String,
    pub password: String,
}

impl User {
    pub fn new<S: Into<String>>(username: S, password: &str) -> User {
        User {
            name: username.into(),
            password: base64::encode(password),
        }
    }

    pub fn from_input(name: Option<String>) -> Result<User, Error> {
        let username = match name {
            Some(name) => name,
            None => {
                println!("please enter your username:");
                input::read_line()?
            }
        };

        println!("please enter the password for [{}]:", username);
        let password = input::read_line()?;

        Ok(User::new(username, &password))
    }

    pub fn decode_password(&self) -> Result<String, Error> {
        let bytes = base64::decode(&self.password)?;
        let string = String::from_utf8(bytes)?;

        Ok(string)
    }
}

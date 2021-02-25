use anime::remote::{AccessToken, Remote};
use anyhow::{anyhow, Result};

pub type Username = String;

pub enum RemoteLogin {
    AniList(Username, AccessToken),
}

pub enum RemoteStatus {
    LoggingIn(Username),
    LoggedIn(Remote),
}

impl RemoteStatus {
    pub fn get_logged_in(&self) -> Result<&Remote> {
        match self {
            Self::LoggingIn(name) => Err(anyhow!("currently logging in as {}", name)),
            Self::LoggedIn(remote) => Ok(remote),
        }
    }
}

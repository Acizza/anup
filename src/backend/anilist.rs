use super::{AnimeInfo, SyncBackend};
use config::Config;
use error::BackendError;
use input;
use reqwest::Client;
use std::io;
use std::process::{Command, ExitStatus};

const LOGIN_URL: &str =
    "https://anilist.co/api/v2/oauth/authorize?client_id=427&response_type=token";

const API_URL: &str = "https://graphql.anilist.co";

pub struct Anilist {
    client: Client,
    access_token: String,
}

impl SyncBackend for Anilist {
    fn init(config: &mut Config) -> Result<Anilist, BackendError> {
        let access_token = match config.user.access_token {
            Some(_) => config.user.decode_access_token()?,
            None => {
                // TODO: add better error reporting
                open_url(LOGIN_URL)?;

                println!("please authorize your account in the opened browser tab and paste the code below:");
                let token = input::read_line()?;
                config.user.encode_access_token(&token);

                token
            }
        };

        let instance = Anilist {
            client: Client::new(),
            access_token,
        };

        Ok(instance)
    }

    fn find_series_by_name(&self, name: &str) -> Result<Vec<AnimeInfo>, BackendError> {
        Ok(Vec::new())
    }
}

fn open_url(url: &str) -> io::Result<ExitStatus> {
    // TODO: add support for Windows / macOS
    Command::new("xdg-open").arg(url).status()
}

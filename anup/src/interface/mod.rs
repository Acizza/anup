use crate::err::{self, Result};
use crate::file::SaveFile;
use anime::remote::RemoteService;
use clap::ArgMatches;
use snafu::{ensure, ResultExt};
use std::io;

pub mod cli;
pub mod tui;

fn get_remote(args: &ArgMatches, can_use_offline: bool) -> Result<Box<dyn RemoteService>> {
    use anime::remote::anilist::{self, AccessToken, AniList};
    use anime::remote::offline::Offline;

    if args.is_present("offline") {
        ensure!(can_use_offline, err::MustRunOnline);
        Ok(Box::new(Offline::new()))
    } else {
        let token = match AccessToken::load() {
            Ok(config) => config,
            Err(ref err) if err.is_file_nonexistant() => {
                ensure!(!args.is_present("interactive"), err::GetAniListTokenFromCLI);

                println!(
                    "need AniList login token\ngo to {}\n\npaste your token:",
                    anilist::auth_url(super::ANILIST_CLIENT_ID)
                );

                let token = {
                    let mut buffer = String::new();
                    io::stdin().read_line(&mut buffer).context(err::IO)?;
                    let buffer = buffer.trim_end();

                    AccessToken::encode(buffer)
                };

                token.save()?;
                token
            }
            Err(err) => return Err(err),
        };

        let anilist = AniList::login(token)?;
        Ok(Box::new(anilist))
    }
}

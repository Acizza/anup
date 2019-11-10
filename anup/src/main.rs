mod config;
mod detect;
mod err;
mod file;
mod interface;
mod series;
mod util;

use crate::err::Result;
use crate::file::SaveFile;
use anime::remote::RemoteService;
use clap::clap_app;
use clap::ArgMatches;
use interface::{cli, tui};
use snafu::ensure;

const ANILIST_CLIENT_ID: u32 = 427;

fn main() {
    let args = clap_app!(anup =>
        (version: env!("CARGO_PKG_VERSION"))
        (author: env!("CARGO_PKG_AUTHORS"))
        (@arg series: +takes_value "The name of the series to watch")
        (@arg matcher: -m --matcher +takes_value "The custom pattern to match episode files with")
        (@arg offline: -o --offline "Run in offline mode")
        (@arg prefetch: --prefetch "Fetch series info from AniList (for use with offline mode)")
        (@arg sync: --sync "Syncronize changes made while offline to AniList")
        (@arg path: -p --path +takes_value "Manually specify a path to a series")
        (@arg interactive: -i --interactive "Launch the terminal user interface")
        (@arg token: -t --token +takes_value "Your account access token")
        (@setting AllowLeadingHyphen)
    )
    .get_matches();

    if let Err(err) = run(&args) {
        err::display_error(err);
        std::process::exit(1);
    }
}

fn run(args: &ArgMatches) -> Result<()> {
    if args.is_present("interactive") {
        tui::run(args)
    } else {
        cli::run(args)
    }
}

fn init_remote(args: &ArgMatches, can_use_offline: bool) -> Result<Box<dyn RemoteService>> {
    use anime::remote::anilist::AniList;
    use anime::remote::offline::Offline;
    use anime::remote::AccessToken;

    if args.is_present("offline") {
        ensure!(can_use_offline, err::MustRunOnline);
        Ok(Box::new(Offline::new()))
    } else {
        let token = match args.value_of("token") {
            Some(token) => {
                let token = AccessToken::encode(token);
                token.save()?;
                token
            }
            None => match AccessToken::load() {
                Ok(token) => token,
                Err(ref err) if err.is_file_nonexistant() => {
                    return Err(err::Error::NeedAniListToken);
                }
                Err(err) => return Err(err),
            },
        };

        let anilist = AniList::login(token)?;
        Ok(Box::new(anilist))
    }
}

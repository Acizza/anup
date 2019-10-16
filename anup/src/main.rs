mod config;
mod detect;
mod err;
mod file;
mod interface;
mod track;
mod util;

use crate::err::Result;
use clap::clap_app;
use clap::ArgMatches;
use interface::{cli, tui};

const ANILIST_CLIENT_ID: u32 = 427;

fn main() {
    let args = clap_app!(anup =>
        (version: env!("CARGO_PKG_VERSION"))
        (author: env!("CARGO_PKG_AUTHORS"))
        (@arg series: +takes_value "The name of the series to watch")
        (@arg season: -s --season +takes_value "The season to watch. Meant to be used when playing from a folder that has multiple seasons merged together under one name")
        (@arg matcher: -m --matcher +takes_value "The custom pattern to match episode files with")
        (@arg offline: -o --offline "Run in offline mode")
        (@arg prefetch: --prefetch "Fetch series info from AniList (for use with offline mode)")
        (@arg sync: --sync "Syncronize changes made while offline to AniList")
        (@arg rate: -r --rate +takes_value "Rate a series")
        (@arg drop: -d --drop "Drop a series")
        (@arg hold: -h --hold "Put a series on hold")
        (@arg path: -p --path +takes_value "Manually specify a path to a series")
        (@arg series_player_args: -a --args +takes_value "Additional arguments to pass to the video player for the current series")
        (@arg clean: -c --clean "Remove series data that is no longer needed")
        (@arg interactive: -i --interactive "Launch the terminal user interface")
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

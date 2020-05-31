#[macro_use]
extern crate diesel;

mod config;
mod database;
mod err;
mod file;
mod series;
mod tui;
mod util;

use crate::config::Config;
use crate::database::Database;
use crate::err::{Error, Result};
use crate::file::TomlFile;
use crate::series::config::SeriesConfig;
use crate::series::entry::SeriesEntry;
use crate::series::info::SeriesInfo;
use crate::series::{LastWatched, Series};
use anime::remote::RemoteService;
use chrono::Utc;
use gumdrop::Options;
use snafu::{ensure, ResultExt};
use std::path::PathBuf;
use std::str;

const ANILIST_CLIENT_ID: u32 = 427;
const SERIES_TITLE_REP: &str = "{title}";
const SERIES_EPISODE_REP: &str = "{episode}";

#[derive(Options)]
pub struct CmdOptions {
    #[options(help = "print help message")]
    help: bool,
    #[options(free, help = "the nickname of the series to watch")]
    pub series: Option<String>,
    #[options(help = "run in offline mode")]
    pub offline: bool,
    #[options(help = "play a single episode from the specified or last watched series")]
    pub single: bool,
    #[options(help = "your account access token")]
    pub token: Option<String>,
    #[options(no_short, help = "the ID to use for the series")]
    pub series_id: Option<i32>,
    #[options(
        no_short,
        help = "the custom regex pattern to match episode files with"
    )]
    pub matcher: Option<String>,
    #[options(no_short, help = "the path to the series")]
    pub path: Option<PathBuf>,
    #[options(no_short, help = "syncronize changes made while offline to AniList")]
    pub sync: bool,
}

fn main() {
    let args = CmdOptions::parse_args_default_or_exit();

    if let Err(err) = run(args) {
        err::display_error(err);
        std::process::exit(1);
    }
}

fn run(args: CmdOptions) -> Result<()> {
    if args.single {
        play_episode(args)
    } else if args.sync {
        sync(args)
    } else {
        tui::run(args)
    }
}

fn init_remote(args: &CmdOptions, can_use_offline: bool) -> Result<Box<dyn RemoteService>> {
    use anime::remote::anilist::AniList;
    use anime::remote::offline::Offline;
    use anime::remote::AccessToken;

    if args.offline {
        ensure!(can_use_offline, err::MustRunOnline);
        Ok(Box::new(Offline::new()))
    } else {
        let token = match &args.token {
            Some(token) => {
                let token = AccessToken::encode(token);
                token.save()?;
                token
            }
            None => match AccessToken::load() {
                Ok(token) => token,
                Err(ref err) if err.is_file_nonexistant() => {
                    return Err(Error::NeedAniListToken);
                }
                Err(err) => return Err(err),
            },
        };

        let anilist = AniList::authenticated(token)?;
        Ok(Box::new(anilist))
    }
}

fn sync(args: CmdOptions) -> Result<()> {
    let db = Database::open()?;
    let mut list_entries = SeriesEntry::entries_that_need_sync(&db)?;

    if list_entries.is_empty() {
        return Ok(());
    }

    let remote = init_remote(&args, false)?;

    for entry in &mut list_entries {
        match SeriesInfo::load(&db, entry.id()) {
            Ok(info) => println!("{} is being synced..", info.title_preferred),
            Err(err) => eprintln!(
                "warning: failed to get info for anime with ID {}: {}",
                entry.id(),
                err
            ),
        }

        entry.sync_to_remote(remote.as_ref())?;
        entry.save(&db)?;
    }

    Ok(())
}

fn play_episode(args: CmdOptions) -> Result<()> {
    use anime::remote::Status;

    let config = Config::load_or_create()?;
    let db = Database::open()?;
    let mut last_watched = LastWatched::load()?;

    let remote = init_remote(&args, true)?;
    let remote = remote.as_ref();

    let desired_series = args
        .series
        .as_ref()
        .or_else(|| last_watched.get())
        .ok_or(Error::MustSpecifySeriesName)?;

    let mut series = {
        let cfg =
            SeriesConfig::load_by_name(&db, desired_series).map_err(|_| Error::MustAddSeries {
                name: desired_series.clone(),
            })?;

        Series::load_from_config(cfg, &config, &db)?
    };

    if last_watched.set(&series.data.config.nickname) {
        last_watched.save()?;
    }

    series.begin_watching(remote, &config, &db)?;

    let progress_time = series.data.next_watch_progress_time(&config);
    let next_episode_num = series.data.entry.watched_episodes() + 1;

    series
        .play_episode_cmd(next_episode_num as u32, &config)?
        .status()
        .context(err::FailedToPlayEpisode {
            episode: next_episode_num as u32,
        })?;

    if Utc::now() >= progress_time {
        series.episode_completed(remote, &config, &db)?;

        if series.data.entry.status() == Status::Completed {
            println!("{} completed!", series.data.info.title_preferred);
        } else {
            println!(
                "{}/{} of {} completed",
                series.data.entry.watched_episodes(),
                series.data.info.episodes,
                series.data.info.title_preferred
            );
        }
    } else {
        println!("did not watch long enough to count episode as completed");
    }

    Ok(())
}

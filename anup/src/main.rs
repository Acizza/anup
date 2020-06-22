#![deny(trivial_casts)]
#![deny(trivial_numeric_casts)]
#![deny(unused_import_braces)]
#![deny(variant_size_differences)]
#![warn(warnings, rust_2018_idioms)]
#![warn(clippy::all, clippy::pedantic, clippy::restriction)]
#![allow(elided_lifetimes_in_paths)]

#[macro_use]
extern crate diesel;

mod config;
mod database;
mod err;
mod file;
mod series;
mod tui;
mod user;
mod util;

use crate::config::Config;
use crate::database::Database;
use crate::err::{Error, Result};
use crate::file::SerializedFile;
use crate::series::config::SeriesConfig;
use crate::series::entry::SeriesEntry;
use crate::series::info::SeriesInfo;
use crate::series::{LastWatched, LoadedSeries, Series};
use crate::user::Users;
use anime::remote::Remote;
use chrono::Utc;
use gumdrop::Options;
use snafu::{ensure, OptionExt, ResultExt};
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

fn init_remote(args: &CmdOptions, can_use_offline: bool) -> Result<Remote> {
    use anime::remote::anilist::{AniList, Auth};

    if args.offline {
        ensure!(can_use_offline, err::MustRunOnline);
        Ok(Remote::offline())
    } else {
        let token = match Users::load_or_create() {
            Ok(users) => users.take_last_used_token().context(err::MustAddAccount)?,
            Err(err) => return Err(err),
        };

        let auth = Auth::retrieve(token)?;
        Ok(AniList::Authenticated(auth).into())
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

        entry.sync_to_remote(&remote)?;
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

        match Series::load_from_config(cfg, &config, &db) {
            LoadedSeries::Complete(series) => series,
            LoadedSeries::Partial(_, err) | LoadedSeries::None(_, err) => return Err(err),
        }
    };

    if last_watched.set(&series.data.config.nickname) {
        last_watched.save()?;
    }

    series.begin_watching(&remote, &config, &db)?;

    let progress_time = series.data.next_watch_progress_time(&config);
    let next_episode_num = series.data.entry.watched_episodes() + 1;

    series
        .play_episode_cmd(next_episode_num as u32, &config)?
        .status()
        .context(err::FailedToPlayEpisode {
            episode: next_episode_num as u32,
        })?;

    if Utc::now() >= progress_time {
        series.episode_completed(&remote, &config, &db)?;

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

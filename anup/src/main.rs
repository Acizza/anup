#![warn(clippy::pedantic)]
#![allow(clippy::clippy::cast_possible_truncation)]
#![allow(clippy::inline_always)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::shadow_unrelated)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::map_err_ignore)]
#![allow(clippy::default_trait_access)]

#[macro_use]
extern crate diesel;

mod config;
mod database;
mod err;
mod file;
mod key;
mod remote;
mod series;
mod tui;
mod user;
mod util;

use crate::config::Config;
use crate::database::Database;
use crate::file::SerializedFile;
use crate::series::config::SeriesConfig;
use crate::series::entry::SeriesEntry;
use crate::series::info::SeriesInfo;
use crate::series::{LastWatched, LoadedSeries, Series};
use crate::user::Users;
use anime::remote::Remote;
use anyhow::{anyhow, Context, Result};
use argh::FromArgs;
use chrono::Utc;

const ANILIST_CLIENT_ID: u32 = 427;

#[derive(FromArgs)]
/// Play, manage, and sync anime from the terminal.
pub struct Args {
    /// the nickname of the series to watch
    #[argh(positional)]
    pub series: Option<String>,

    /// run in offline mode
    #[argh(switch, short = 'o')]
    pub offline: bool,

    /// play a single episode from the last played series
    #[argh(switch)]
    pub play_one: bool,

    /// syncronize changes made while offline
    #[argh(switch)]
    pub sync: bool,
}

fn main() -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .context("failed to build async runtime")?;

    rt.block_on(async { run().await })
}

async fn run() -> Result<()> {
    let args: Args = argh::from_env();

    if args.play_one {
        play_episode(&args).await
    } else if args.sync {
        sync(&args)
    } else {
        tui::run(&args).await
    }
}

/// Initialize a new remote service specified by `args`.
///
/// If there are no users, returns Ok(None).
fn init_remote(args: &Args) -> Result<Option<Remote>> {
    use anime::remote::anilist::{AniList, Auth};

    if args.offline {
        Ok(Some(Remote::offline()))
    } else {
        let token = match Users::load_or_create()?.take_last_used_token() {
            Some(token) => token,
            None => return Ok(None),
        };

        let auth = Auth::retrieve(token)?;
        Ok(Some(AniList::Authenticated(auth).into()))
    }
}

fn sync(args: &Args) -> Result<()> {
    if args.offline {
        return Err(anyhow!("must be online to run this command"));
    }

    let db = Database::open().context("failed to open database")?;
    let mut list_entries = SeriesEntry::entries_that_need_sync(&db)?;

    if list_entries.is_empty() {
        return Ok(());
    }

    let remote =
        init_remote(&args)?.ok_or_else(|| anyhow!("no users found\nadd one in the TUI"))?;

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

async fn play_episode(args: &Args) -> Result<()> {
    use anime::remote::Status;

    let config = Config::load_or_create()?;
    let db = Database::open().context("failed to open database")?;
    let mut last_watched = LastWatched::load()?;

    let remote =
        init_remote(&args)?.ok_or_else(|| anyhow!("no users found\nadd one in the TUI"))?;

    let desired_series = args
        .series
        .as_ref()
        .or_else(|| last_watched.get())
        .ok_or_else(|| anyhow!("series name must be specified"))?;

    let mut series = {
        let cfg = SeriesConfig::load_by_name(&db, desired_series).with_context(|| {
            format!(
                "{} must be added to the program in the TUI first",
                desired_series
            )
        })?;

        match Series::load_from_config(cfg, &config, &db) {
            LoadedSeries::Complete(series) => series,
            LoadedSeries::Partial(_, err) => return Err(err.into()),
            LoadedSeries::None(_, err) => return Err(err),
        }
    };

    if last_watched.set(&series.data.config.nickname) {
        last_watched.save()?;
    }

    series.begin_watching(&remote, &config, &db)?;

    let progress_time = series.data.next_watch_progress_time(&config);
    let next_episode_num = series.data.entry.watched_episodes() + 1;

    series
        .play_episode(next_episode_num as u32, &config)?
        .wait()
        .await
        .context("waiting for episode to finish failed")?;

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

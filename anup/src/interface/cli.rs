use crate::config::Config;
use crate::err::{self, Result};
use crate::file::SaveFile;
use crate::series::{SavedSeries, Series};
use chrono::{Duration, Utc};
use clap::ArgMatches;
use snafu::{ensure, ResultExt};

pub fn run(args: &ArgMatches) -> Result<()> {
    if args.is_present("prefetch") {
        prefetch(args)
    } else if args.is_present("sync") {
        sync(args)
    } else {
        play(args)
    }
}

fn prefetch(args: &ArgMatches) -> Result<()> {
    let mut saved_series = SavedSeries::load_or_default()?;

    let desired_series = match args.value_of("series") {
        Some(desired_series) => desired_series,
        None => return Err(err::Error::MustSpecifySeriesName),
    };

    let config = Config::load_or_create()?;
    let remote = super::get_remote(args, false)?;

    let series = saved_series.insert_and_save_from_args_and_remote(
        args,
        desired_series,
        &config,
        remote.as_ref(),
    )?;

    println!(
        "{} was fetched\nyou can now watch this series offline",
        series.info.title.preferred
    );

    Ok(())
}

fn sync(args: &ArgMatches) -> Result<()> {
    let mut saved_series = SavedSeries::load_or_default()?;
    let mut series_list = saved_series.load_all_series_and_validate()?;

    let remote = super::get_remote(args, false)?;

    for series in &mut series_list {
        if !series.entry.needs_sync() {
            continue;
        }

        println!("{} is being synced..", series.info.title.preferred);

        match series.force_sync_changes_to_remote(remote.as_ref()) {
            Ok(()) => series.save()?,
            Err(err) => eprintln!("{} failed to sync:\n", err),
        }
    }

    Ok(())
}

fn play(args: &ArgMatches) -> Result<()> {
    use anime::remote::Status;

    let mut saved_series = SavedSeries::load_or_default()?;

    let config = Config::load_or_create()?;
    let remote = super::get_remote(args, true)?;

    // TODO: refactor
    let mut series = match args.value_of("series") {
        Some(existing_series) if saved_series.contains(&existing_series) => {
            saved_series.load_series(existing_series)?
        }
        Some(new_series) => saved_series.insert_and_save_from_args_and_remote(
            args,
            new_series,
            &config,
            remote.as_ref(),
        )?,
        None => match saved_series.last_watched_id {
            Some(last_id) => {
                // TODO: fetch from remote if this fails
                // We won't be saving this, so we don't need to set the nickname
                Series::load(last_id, "")?
            }
            None => return Err(err::Error::MustSpecifySeriesName),
        },
    };

    if saved_series.set_last_watched(series.info.id) {
        saved_series.save()?;
    }

    series.begin_watching(remote.as_ref(), &config)?;

    let progress_time = {
        let secs_must_watch =
            (series.info.episode_length as f32 * config.episode.pcnt_must_watch) * 60.0;
        let time_must_watch = Duration::seconds(secs_must_watch as i64);

        Utc::now() + time_must_watch
    };

    let next_episode_num = series.entry.watched_eps() + 1;

    let status = series
        .play_episode_cmd(next_episode_num, &config)?
        .status()
        .context(err::FailedToPlayEpisode {
            episode: next_episode_num,
        })?;

    ensure!(status.success(), err::AbnormalPlayerExit);

    if Utc::now() >= progress_time {
        series.episode_completed(remote.as_ref(), &config)?;

        if series.entry.status() == Status::Completed {
            println!("{} completed!", series.info.title.preferred);
        } else {
            println!(
                "{}/{} of {} completed",
                series.entry.watched_eps(),
                series.info.episodes,
                series.info.title.preferred
            );
        }
    } else {
        println!("did not watch long enough to count episode as completed");
    }

    Ok(())
}

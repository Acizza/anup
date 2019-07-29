use crate::config::Config;
use crate::err::{self, Result};
use crate::file::{SaveDir, SaveFile};
use crate::track::{EntryState, SeriesTracker};
use anime::remote::RemoteService;
use anime::{SeasonInfoList, Series};
use chrono::Utc;
use clap::ArgMatches;
use snafu::OptionExt;

pub fn run(args: &ArgMatches) -> Result<()> {
    if args.is_present("prefetch") {
        prefetch(args)
    } else if args.is_present("sync") {
        sync(args)
    } else if args.is_present("rate") || args.is_present("drop") || args.is_present("hold") {
        modify_series(args)
    } else if args.is_present("clean") {
        remove_orphaned_data()
    } else {
        play(args)
    }
}

fn prefetch(args: &ArgMatches) -> Result<()> {
    let name = crate::get_series_name(args)?;
    let config = crate::get_config()?;
    let episodes = crate::get_episodes(args, &name, &config)?;
    let remote = crate::get_remote(args, false)?;
    let info = crate::get_best_info_from_remote(&remote, &episodes.title)?;

    let seasons = SeasonInfoList::from_info_and_remote(info, &remote, None)?;
    seasons.save(name.as_ref())?;

    for (season_num, season) in seasons.inner().iter().enumerate() {
        if let Some(entry) = remote.get_list_entry(season.id)? {
            let state = EntryState::new(entry);
            state.save_with_id(season.id, name.as_ref())?;
        }

        println!("season {} -> {}", 1 + season_num, season.title);
    }

    println!("\nprefetch complete\nyou can now fully watch this series offline");
    Ok(())
}

fn sync(args: &ArgMatches) -> Result<()> {
    let name = crate::get_series_name(args)?;
    let remote = crate::get_remote(args, false)?;
    let seasons = SeasonInfoList::load(name.as_ref())?;

    for (season_num, season) in seasons.inner().iter().enumerate() {
        let mut state = match EntryState::load_with_id(season.id, name.as_ref()) {
            Ok(state) => state,
            Err(ref err) if err.is_file_nonexistant() => continue,
            Err(err) => return Err(err),
        };

        if !state.needs_sync() {
            continue;
        }

        println!("syncing season {}: {}", 1 + season_num, season.title);
        state.sync_changes_to_remote(&remote, &name)?;
    }

    Ok(())
}

fn modify_series(args: &ArgMatches) -> Result<()> {
    let name = crate::get_series_name(args)?;
    let config = crate::get_config()?;
    let remote = crate::get_remote(args, true)?;
    let season_num = crate::get_season_num(args);

    let season = {
        let seasons = SeasonInfoList::load(name.as_ref())?;
        seasons.take_unchecked(season_num)
    };

    let mut state = EntryState::load_with_id(season.id, name.as_ref())?;
    state.sync_changes_from_remote(&remote, &name)?;

    if let Some(score) = args.value_of("rate") {
        let score = remote.parse_score(score).context(err::ScoreParseFailed)?;
        state.set_score(Some(score));
    }

    match (args.is_present("drop"), args.is_present("hold")) {
        (true, true) => return Err(err::Error::CantDropAndHold),
        (true, false) => state.mark_as_dropped(&config),
        (false, true) => state.mark_as_on_hold(),
        (false, false) => (),
    }

    state.sync_changes_to_remote(&remote, &name)
}

fn remove_orphaned_data() -> Result<()> {
    let config = crate::get_config()?;
    let series_data = SaveDir::LocalData.get_subdirs()?;

    for series in series_data {
        let exists = match crate::get_series_path(&series, &config) {
            Ok(dir) => dir.exists(),
            Err(err::Error::NoMatchingSeries { .. }) => false,
            Err(err) => return Err(err),
        };

        if exists {
            continue;
        }

        println!("{} will be purged", series);
        SaveDir::LocalData.remove_subdir(&series)?;
    }

    Ok(())
}

fn play(args: &ArgMatches) -> Result<()> {
    let name = crate::get_series_name(args)?;
    let config = crate::get_config()?;
    let episodes = crate::get_episodes(args, &name, &config)?;
    let remote = crate::get_remote(args, true)?;
    let season_num = crate::get_season_num(args);
    let series = crate::get_series(&name, &remote, episodes, season_num)?;

    let mut tracker = SeriesTracker::init(&remote, &series.info, &name)?;
    tracker.begin_watching(&remote, &config)?;

    if !args.is_present("quiet") {
        crate::print_info(&remote, &config, &series, &tracker.state);
    }

    play_episode(remote, &config, &series, &mut tracker)
}

fn play_episode<R>(
    remote: R,
    config: &Config,
    series: &Series,
    tracker: &mut SeriesTracker,
) -> Result<()>
where
    R: AsRef<RemoteService>,
{
    use anime::remote::Status;

    let ep_num = tracker.state.watched_eps() + 1;
    let start_time = Utc::now();

    series.play_episode(ep_num)?;

    let end_time = Utc::now();

    let mins_watched = {
        let watch_time = end_time - start_time;
        watch_time.num_seconds() as f32 / 60.0
    };

    let mins_must_watch = series.info.episode_length as f32 * config.episode.pcnt_must_watch;

    if mins_watched < mins_must_watch {
        println!("did not watch episode long enough");
        return Ok(());
    }

    tracker.episode_completed(&remote, config)?;

    if let Status::Completed = tracker.state.status() {
        println!("completed!");
    } else {
        println!("{}/{} completed", ep_num, series.info.episodes)
    }

    Ok(())
}

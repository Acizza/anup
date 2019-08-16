use crate::config::Config;
use crate::err::{self, Result};
use crate::file::{SaveDir, SaveFile};
use crate::track::{EntryState, SeriesTracker};
use anime::remote::RemoteService;
use anime::{SeasonInfoList, Series};
use chrono::Utc;
use clap::ArgMatches;
use snafu::{ensure, OptionExt, ResultExt};

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
    let watch_info = crate::get_watch_info(args)?;
    let name = &watch_info.name;

    let config = crate::get_config()?;
    let episodes = crate::get_episodes(args, name, &config, true)?;

    let remote = crate::get_remote(args, false)?;
    let remote = remote.as_ref();

    let info = crate::get_best_info_from_remote(remote, &episodes.title)?;

    let seasons = SeasonInfoList::from_info_and_remote(info, remote)?;
    seasons.save(name.as_ref())?;

    for (season_num, season) in seasons.inner().iter().enumerate() {
        if let Some(entry) = remote.get_list_entry(season.id)? {
            let state = EntryState::new(entry);
            state.save_with_id(season.id, name.as_ref())?;
        }

        println!("season {} -> {}", 1 + season_num, season.title.preferred);
    }

    println!("\nprefetch complete\nyou can now fully watch this series offline");
    Ok(())
}

fn sync(args: &ArgMatches) -> Result<()> {
    let watch_info = crate::get_watch_info(args)?;
    let name = &watch_info.name;

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

        println!(
            "syncing season {}: {}",
            1 + season_num,
            season.title.preferred
        );

        state.sync_changes_to_remote(remote.as_ref(), name)?;
    }

    Ok(())
}

fn modify_series(args: &ArgMatches) -> Result<()> {
    let watch_info = crate::get_watch_info(args)?;
    let name = &watch_info.name;

    let config = crate::get_config()?;
    let remote = crate::get_remote(args, true)?;
    let remote = remote.as_ref();

    let season = {
        let seasons = SeasonInfoList::load(name.as_ref())?;
        seasons.take_unchecked(watch_info.season)
    };

    let mut state = EntryState::load_with_id(season.id, name.as_ref())?;
    state.sync_changes_from_remote(remote, name)?;

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

    state.sync_changes_to_remote(remote, name)
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
    let watch_info = crate::get_watch_info(args)?;
    let name = &watch_info.name;

    let config = crate::get_config()?;
    let episodes = crate::get_episodes(args, name, &config, true)?;

    let remote = crate::get_remote(args, true)?;
    let remote = remote.as_ref();

    let seasons = crate::get_season_list(name, remote, &episodes)?;
    let series = Series::from_season_list(&seasons, watch_info.season, episodes)?;

    let mut tracker = SeriesTracker::init(&series.info, name)?;
    tracker.begin_watching(remote, &config)?;

    play_episode(remote, &config, &series, &mut tracker)
}

fn play_episode<R>(
    remote: &R,
    config: &Config,
    series: &Series,
    tracker: &mut SeriesTracker,
) -> Result<()>
where
    R: RemoteService + ?Sized,
{
    use anime::remote::Status;

    let ep_num = tracker.entry.watched_eps() + 1;
    let start_time = Utc::now();

    let episode = series
        .get_episode(ep_num)
        .context(err::EpisodeNotFound { episode: ep_num })?;

    let status = crate::prepare_episode_cmd(config, episode)
        .status()
        .context(err::FailedToPlayEpisode { episode: ep_num })?;

    ensure!(status.success(), err::AbnormalPlayerExit);

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

    tracker.episode_completed(remote, config)?;

    if let Status::Completed = tracker.entry.status() {
        println!("completed!");
    } else {
        println!("{}/{} completed", ep_num, series.info.episodes)
    }

    Ok(())
}

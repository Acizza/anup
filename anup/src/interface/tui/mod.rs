mod ui;

use crate::config::Config;
use crate::err::{self, Result};
use crate::file::{SaveDir, SaveFile};
use crate::track::{EntryState, SeriesTracker};
use crate::util;
use crate::LastWatched;
use anime::remote::{RemoteService, SeriesInfo};
use anime::{SeasonInfoList, Series};
use chrono::{DateTime, Duration, Utc};
use clap::ArgMatches;
use snafu::{OptionExt, ResultExt};
use std::borrow::Cow;
use std::ops::Add;
use std::process;
use termion::event::Key;
use ui::{Event, Events, UI};

pub fn run(args: &ArgMatches) -> Result<()> {
    let config = crate::get_config()?;

    let remote = crate::get_remote(args, true)?;
    let remote = remote.as_ref();

    let mut state = {
        let name = crate::get_series_name(args)?;
        let series_names = SaveDir::LocalData.get_subdirs()?;
        let selected_series = series_names.iter().position(|s| *s == name).unwrap_or(0);

        UIState {
            series: SeriesState::new(args, &name, remote, &config, true)?,
            series_names,
            selected_series,
            selection: Selection::Series,
        }
    };

    let mut ui = UI::init()?;
    let events = Events::new(Duration::seconds(1));

    loop {
        ui.draw(&state)?;

        let series = &mut state.series;
        let season = &mut series.season;

        match events.next()? {
            Event::Input(key) => match key {
                // Exit
                Key::Char('q') => {
                    // Prevent ruining the user's terminal
                    ui.clear().ok();
                    break Ok(());
                }
                // Sync list entry from / to remote
                Key::Char(ch @ 'r') | Key::Char(ch @ 's') => {
                    let name = &series.name;
                    let entry = &mut season.tracker.state;

                    if ch == 'r' {
                        ui.log_capture("Syncing entry from remote", || {
                            entry.force_sync_changes_from_remote(remote, name)
                        });
                    } else if ch == 's' {
                        ui.log_capture("Syncing entry to remote", || {
                            entry.force_sync_changes_to_remote(remote, name)
                        });
                    }

                    season.update_value_cache(remote);
                }
                // Play next episode
                Key::Char('\n') => {
                    // Update the last watched series
                    if !series.is_last_watched {
                        let name = &series.name;

                        ui.log_capture("Marking as the last watched series", || {
                            let last_watched = LastWatched::new(name);
                            last_watched.save(None)
                        });

                        series.is_last_watched = true;
                    }

                    ui.log_capture("Playing next episode", || {
                        season.play_next_episode_async(remote, &config)
                    });
                }
                // Switch between series and season selection
                Key::Left | Key::Right if state.is_idle() => {
                    state.selection.set_opposite();
                }
                // Select series
                Key::Up | Key::Down if state.can_select_series() => {
                    let next_index = if key == Key::Up {
                        state.selected_series.saturating_sub(1)
                    } else {
                        state.selected_series + 1
                    };

                    let new_name = match state.series_names.get(next_index) {
                        Some(new_name) => new_name,
                        None => continue,
                    };

                    state.series = SeriesState::new(args, new_name, remote, &config, false)?;
                    state.selected_series = next_index;
                }
                // Select season
                Key::Up | Key::Down if state.can_select_season() => {
                    let series = &mut state.series;

                    let next_season = if key == Key::Up {
                        series.season.season_num.saturating_sub(1)
                    } else {
                        series.season.season_num + 1
                    };

                    if series.seasons.has(next_season) {
                        // This is a bit funky. Here we avoid cloning the episodes by moving them
                        // out of the season so they can be moved into the new Series struct.
                        // Due to some aspect(s) of the borrowing rules, we cannot use a pointer alias
                        // to make this look a bit cleaner.
                        let eps = state.series.season.series.episodes;
                        let series =
                            Series::from_season_list(&state.series.seasons, next_season, eps)?;
                        state.series.season =
                            SeasonState::new(remote, &state.series.name, series, next_season)?;
                    }
                }
                _ => (),
            },
            Event::Tick => match &mut season.watch_state {
                WatchState::Idle => (),
                WatchState::Watching(start_time, _, child) => {
                    let status = match child.try_wait().context(err::IO)? {
                        Some(status) => status,
                        None => continue,
                    };

                    if !status.success() {
                        ui.push_log_status("Player did not exit properly");
                        continue;
                    }

                    let mins_watched = {
                        let watch_time = Utc::now() - *start_time;
                        watch_time.num_seconds() as f32 / 60.0
                    };

                    let mins_must_watch =
                        season.series.info.episode_length as f32 * config.episode.pcnt_must_watch;

                    if mins_watched >= mins_must_watch {
                        ui.log_capture("Marking episode as completed", || {
                            season.tracker.episode_completed(remote, &config)
                        });
                    } else {
                        ui.push_log_status("Not marking episode as completed");
                    }

                    season.watch_state = WatchState::Idle;
                    season.update_value_cache(remote);
                }
            },
        }
    }
}

pub struct UIState<'a> {
    pub series: SeriesState<'a>,
    pub series_names: Vec<String>,
    pub selected_series: usize,
    pub selection: Selection,
}

impl<'a> UIState<'a> {
    fn is_idle(&self) -> bool {
        self.series.season.watch_state == WatchState::Idle
    }

    fn can_select_season(&self) -> bool {
        self.selection == Selection::Season && self.is_idle()
    }

    fn can_select_series(&self) -> bool {
        self.selection == Selection::Series && self.is_idle()
    }
}

#[derive(PartialEq, Copy, Clone)]
pub enum Selection {
    Series,
    Season,
}

impl Selection {
    fn opposite(self) -> Selection {
        match self {
            Selection::Series => Selection::Season,
            Selection::Season => Selection::Series,
        }
    }

    fn set_opposite(&mut self) {
        *self = self.opposite();
    }
}

pub type ProgressTime = DateTime<Utc>;
pub type StartTime = DateTime<Utc>;

pub enum WatchState {
    Idle,
    Watching(StartTime, ProgressTime, process::Child),
}

impl PartialEq for WatchState {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (WatchState::Idle, WatchState::Idle) => true,
            (WatchState::Watching(_, _, _), WatchState::Watching(_, _, _)) => true,
            _ => false,
        }
    }
}

pub struct SeriesState<'a> {
    pub name: String,
    pub season: SeasonState<'a>,
    pub seasons: SeasonInfoList,
    pub num_seasons: usize,
    pub is_last_watched: bool,
}

impl<'a> SeriesState<'a> {
    pub fn new<S, R>(
        args: &clap::ArgMatches,
        name: S,
        remote: &'a R,
        config: &Config,
        is_last_watched: bool,
    ) -> Result<SeriesState<'a>>
    where
        S: Into<String>,
        R: RemoteService + ?Sized,
    {
        let name = name.into();

        let episodes = crate::get_episodes(args, &name, &config)?;
        let seasons = crate::get_season_list(&name, remote, &episodes)?;
        let num_seasons = seasons.len();
        let season_num = crate::get_season_num(args);
        let series = Series::from_season_list(&seasons, season_num, episodes)?;
        let season = SeasonState::new(remote, &name, series, season_num)?;

        Ok(SeriesState {
            name,
            season,
            seasons,
            num_seasons,
            is_last_watched,
        })
    }
}

pub struct SeasonState<'a> {
    pub series: Series<'a>,
    pub tracker: SeriesTracker<'a>,
    pub value_cache: SeasonValueCache<'a>,
    pub season_num: usize,
    pub watch_state: WatchState,
}

impl<'a> SeasonState<'a> {
    fn new<R, S>(
        remote: &'a R,
        name: S,
        series: Series<'a>,
        season_num: usize,
    ) -> Result<SeasonState<'a>>
    where
        R: RemoteService + ?Sized,
        S: Into<String>,
    {
        let tracker = SeriesTracker::init(series.info.clone(), name)?;
        let value_cache = SeasonValueCache::new(remote, &tracker);

        Ok(SeasonState {
            series,
            tracker,
            value_cache,
            season_num,
            watch_state: WatchState::Idle,
        })
    }

    fn play_next_episode_async<R>(&mut self, remote: &R, config: &Config) -> Result<()>
    where
        R: RemoteService + ?Sized,
    {
        self.tracker.begin_watching(remote, config)?;
        let next_ep = self.tracker.state.watched_eps() + 1;

        let episode = self
            .series
            .get_episode(next_ep)
            .context(err::EpisodeNotFound { episode: next_ep })?;

        let start_time = Utc::now();

        let child = crate::process::open_with_default(episode)
            .context(err::FailedToPlayEpisode { episode: next_ep })?;

        let progress_time = {
            let secs_must_watch =
                (self.series.info.episode_length as f32 * config.episode.pcnt_must_watch) * 60.0;
            let time_must_watch = Duration::seconds(secs_must_watch as i64);

            start_time.add(time_must_watch)
        };

        self.watch_state = WatchState::Watching(start_time, progress_time, child);

        Ok(())
    }

    fn update_value_cache<R>(&mut self, remote: &'a R)
    where
        R: RemoteService + ?Sized,
    {
        self.value_cache.update(remote, &self.tracker);
    }
}

pub struct SeasonValueCache<'a> {
    pub progress: String,
    pub score: Cow<'a, str>,
    pub start_date: Cow<'a, str>,
    pub end_date: Cow<'a, str>,
    pub watch_time_left: String,
    // The following fields will not change
    pub watch_time: String,
    pub episode_length: String,
}

impl<'a> SeasonValueCache<'a> {
    pub fn new<R>(remote: &'a R, tracker: &SeriesTracker<'a>) -> SeasonValueCache<'a>
    where
        R: RemoteService + ?Sized,
    {
        let info = &tracker.info;
        let entry = &tracker.state;

        let watch_time = {
            let watch_time_mins = info.episodes * info.episode_length;
            util::hm_from_mins(watch_time_mins as f32)
        };

        let episode_length = format!("{}M", info.episode_length);

        SeasonValueCache {
            progress: SeasonValueCache::progress(info, entry),
            score: SeasonValueCache::score(remote, entry),
            start_date: SeasonValueCache::start_date(entry),
            end_date: SeasonValueCache::end_date(entry),
            watch_time_left: SeasonValueCache::watch_time_left(info, entry),
            watch_time,
            episode_length,
        }
    }

    pub fn update<R>(&mut self, remote: &'a R, tracker: &SeriesTracker<'a>)
    where
        R: RemoteService + ?Sized,
    {
        let info = &tracker.info;
        let entry = &tracker.state;

        self.progress = SeasonValueCache::progress(info, entry);
        self.score = SeasonValueCache::score(remote, entry);
        self.start_date = SeasonValueCache::start_date(entry);
        self.end_date = SeasonValueCache::end_date(entry);
        self.watch_time_left = SeasonValueCache::watch_time_left(info, entry);
    }

    fn progress(info: &SeriesInfo, entry: &EntryState) -> String {
        format!("{}|{}", entry.watched_eps(), info.episodes)
    }

    fn score<R>(remote: &'a R, entry: &EntryState) -> Cow<'a, str>
    where
        R: RemoteService + ?Sized,
    {
        match entry.score() {
            Some(score) => remote.score_to_str(score),
            None => "??".into(),
        }
    }

    fn start_date(entry: &EntryState) -> Cow<'a, str> {
        match entry.start_date() {
            Some(date) => format!("{}", date.format("%D")).into(),
            None => "??".into(),
        }
    }

    fn end_date(entry: &EntryState) -> Cow<'a, str> {
        match entry.end_date() {
            Some(date) => format!("{}", date.format("%D")).into(),
            None => "??".into(),
        }
    }

    fn watch_time_left(info: &SeriesInfo, entry: &EntryState) -> String {
        let time_left_mins = (info.episodes - entry.watched_eps()) * info.episode_length;
        util::hm_from_mins(time_left_mins as f32)
    }
}

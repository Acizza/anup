mod ui;

use crate::config::Config;
use crate::err::{self, Result};
use crate::file::{SaveDir, SaveFile};
use crate::track::{EntryState, SeriesTracker};
use crate::util;
use crate::CurrentWatchInfo;
use anime::remote::{RemoteService, SeriesInfo};
use anime::{SeasonInfoList, Series};
use chrono::{DateTime, Duration, Utc};
use clap::ArgMatches;
use snafu::{OptionExt, ResultExt};
use std::borrow::Cow;
use std::ops::Add;
use std::process;
use termion::event::Key;
use ui::{Event, Events, LogItem, UI};

pub fn run(args: &ArgMatches) -> Result<()> {
    let cstate = {
        let config = crate::get_config()?;
        let remote = crate::get_remote(args, true)?;

        CommonState {
            args,
            config,
            remote,
        }
    };

    let mut ui_state = {
        let watch_info = crate::get_watch_info(args)?;
        let series = SeriesState::new(&cstate, watch_info, true)?;
        let series_names = SaveDir::LocalData.get_subdirs()?;
        let selected_series = series_names
            .iter()
            .position(|s| *s == series.watch_info.name)
            .unwrap_or(0);

        UIState {
            series,
            series_names,
            selected_series,
            selection: Selection::Series,
        }
    };

    let mut ui = UI::init()?;
    let events = Events::new(Duration::seconds(1));

    loop {
        ui.draw(&ui_state)?;

        match events.next()? {
            Event::Input(key) => match key {
                // Exit
                Key::Char('q') => {
                    // Prevent ruining the user's terminal
                    ui.clear().ok();
                    break Ok(());
                }
                key => match ui_state.process_key(&cstate, &mut ui, key) {
                    Ok(_) => (),
                    Err(err) => {
                        ui.push_log_status(LogItem::failed("Processing key", err));
                    }
                },
            },
            Event::Tick => match ui_state.process_tick(&cstate, &mut ui) {
                Ok(_) => (),
                Err(err) => ui.push_log_status(LogItem::failed("Processing tick", err)),
            },
        }
    }
}

/// Items that are not tied to the UI and are commonly used together.
struct CommonState<'a> {
    args: &'a ArgMatches<'a>,
    config: Config,
    remote: Box<RemoteService>,
}

/// Current state of the UI.
pub struct UIState<'a> {
    series: SeriesState<'a>,
    series_names: Vec<String>,
    selected_series: usize,
    selection: Selection,
}

impl<'a> UIState<'a> {
    fn process_key<B>(&mut self, state: &'a CommonState, ui: &mut UI<B>, key: Key) -> Result<()>
    where
        B: tui::backend::Backend,
    {
        match key {
            // Sync list entry from / to remote
            Key::Char(ch @ 'r') | Key::Char(ch @ 's') => {
                let remote = state.remote.as_ref();

                let name = &self.series.watch_info.name;
                let season = &mut self.series.season;
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
                self.series.set_last_watched(ui);

                ui.log_capture("Playing next episode", || {
                    self.series.season.play_next_episode_async(&state)
                });
            }
            // Switch between series and season selection
            Key::Left | Key::Right if self.is_idle() => {
                self.selection.set_opposite();
            }
            // Select series
            Key::Up | Key::Down if self.can_select_series() => {
                let index = UIState::next_arrow_key_value(key, self.selected_series);

                let new_name = match self.series_names.get(index) {
                    Some(new_name) => new_name,
                    None => return Ok(()),
                };

                let watch_info = CurrentWatchInfo::new(new_name, 0);

                self.series = SeriesState::new(state, watch_info, false)?;
                self.selected_series = index;
            }
            // Select season
            Key::Up | Key::Down if self.can_select_season() => {
                let remote = state.remote.as_ref();
                let new_season = UIState::next_arrow_key_value(key, self.series.watch_info.season);
                self.series.set_season(new_season, remote)?;
            }
            _ => (),
        }

        Ok(())
    }

    fn process_tick<B>(&mut self, state: &'a CommonState, ui: &mut UI<B>) -> Result<()>
    where
        B: tui::backend::Backend,
    {
        self.series.season.process_tick(state, ui)
    }

    fn next_arrow_key_value(key: Key, value: usize) -> usize {
        match key {
            Key::Up => value.saturating_sub(1),
            Key::Down => value + 1,
            _ => value,
        }
    }

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

struct SeriesState<'a> {
    watch_info: CurrentWatchInfo,
    season: SeasonState<'a>,
    seasons: SeasonInfoList,
    num_seasons: usize,
    is_last_watched: bool,
}

impl<'a> SeriesState<'a> {
    fn new(
        state: &'a CommonState,
        watch_info: CurrentWatchInfo,
        is_last_watched: bool,
    ) -> Result<SeriesState<'a>> {
        let remote = state.remote.as_ref();
        let name = &watch_info.name;

        let episodes = crate::get_episodes(&state.args, name, &state.config)?;
        let seasons = crate::get_season_list(name, remote, &episodes)?;
        let num_seasons = seasons.len();
        let series = Series::from_season_list(&seasons, watch_info.season, episodes)?;
        let season = SeasonState::new(remote, name, series)?;

        Ok(SeriesState {
            watch_info,
            season,
            seasons,
            num_seasons,
            is_last_watched,
        })
    }

    /// Loads the season specified by `season_num` and points `season` to it.
    fn set_season<R>(&mut self, season_num: usize, remote: &'a R) -> Result<()>
    where
        R: RemoteService + ?Sized,
    {
        if !self.seasons.has(season_num) {
            return Ok(());
        }

        let episodes = self.season.series.episodes.clone();
        let series = Series::from_season_list(&self.seasons, season_num, episodes)?;

        self.season = SeasonState::new(remote, &self.watch_info.name, series)?;
        self.watch_info.season = season_num;

        Ok(())
    }

    /// Sets the current series as the last watched one if it isn't already.
    fn set_last_watched<B>(&mut self, ui: &mut UI<B>)
    where
        B: tui::backend::Backend,
    {
        if self.is_last_watched {
            return;
        }

        ui.log_capture("Marking as the last watched series", || {
            self.watch_info.save(None)
        });

        self.is_last_watched = true;
    }
}

pub struct SeasonState<'a> {
    pub series: Series<'a>,
    pub tracker: SeriesTracker<'a>,
    pub value_cache: SeasonValueCache<'a>,
    pub watch_state: WatchState,
}

impl<'a> SeasonState<'a> {
    fn new<R, S>(remote: &'a R, name: S, series: Series<'a>) -> Result<SeasonState<'a>>
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
            watch_state: WatchState::Idle,
        })
    }

    fn play_next_episode_async(&mut self, state: &CommonState) -> Result<()> {
        let remote = state.remote.as_ref();
        let config = &state.config;

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

    fn process_tick<B>(&mut self, state: &'a CommonState, ui: &mut UI<B>) -> Result<()>
    where
        B: tui::backend::Backend,
    {
        match &mut self.watch_state {
            WatchState::Idle => (),
            WatchState::Watching(start_time, _, child) => {
                let status = match child.try_wait().context(err::IO)? {
                    Some(status) => status,
                    None => return Ok(()),
                };

                if !status.success() {
                    ui.push_log_status("Player did not exit properly");
                    return Ok(());
                }

                let mins_watched = {
                    let watch_time = Utc::now() - *start_time;
                    watch_time.num_seconds() as f32 / 60.0
                };

                let remote = state.remote.as_ref();
                let config = &state.config;

                let mins_must_watch =
                    self.series.info.episode_length as f32 * config.episode.pcnt_must_watch;

                if mins_watched >= mins_must_watch {
                    ui.log_capture("Marking episode as completed", || {
                        self.tracker.episode_completed(remote, config)
                    });
                } else {
                    ui.push_log_status("Not marking episode as completed");
                }

                self.watch_state = WatchState::Idle;
                self.update_value_cache(remote);
            }
        }

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

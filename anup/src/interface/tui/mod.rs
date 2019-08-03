mod ui;

use crate::config::Config;
use crate::err::{self, Result};
use crate::file::SaveDir;
use crate::track::SeriesTracker;
use crate::util;
use anime::remote::RemoteService;
use anime::Series;
use chrono::{DateTime, Duration, Utc};
use clap::ArgMatches;
use snafu::{ensure, OptionExt, ResultExt};
use std::borrow::Cow;
use std::ops::Add;
use std::process;
use termion::event::Key;
use ui::{Event, Events, UI};

pub fn run(args: &ArgMatches) -> Result<()> {
    let name = crate::get_series_name(args)?;
    let config = crate::get_config()?;

    let remote = crate::get_remote(args, true)?;
    let remote = remote.as_ref();

    let episodes = crate::get_episodes(args, &name, &config)?;
    let seasons = crate::get_season_list(&name, remote, &episodes)?;

    let mut state = {
        let series_names = SaveDir::LocalData.get_subdirs()?;
        let selected_series = series_names.iter().position(|s| *s == name).unwrap_or(0);
        let season_num = crate::get_season_num(args);

        let series = Series::from_season_list(&seasons, season_num, &episodes)?;
        let season_state = SeasonState::new(remote, &name, series, season_num)?;

        SeriesState {
            season: season_state,
            series_names,
            selected_series,
            num_seasons: seasons.len(),
        }
    };

    let mut ui = UI::init()?;
    let events = Events::new(Duration::seconds(1));

    loop {
        ui.draw(&state)?;

        match events.next()? {
            Event::Input(key) => match key {
                // Exit
                Key::Char('q') => {
                    // Prevent ruining the user's terminal
                    ui.clear().ok();
                    break Ok(());
                }
                // Play next episode
                Key::Char('\n') => state.season.play_next_episode_async(remote, &config)?,
                // Select season
                Key::Up | Key::Down if state.season.watch_state == WatchState::Idle => {
                    let season = &mut state.season;

                    let next_season = if key == Key::Up {
                        season.season_num.saturating_sub(1)
                    } else {
                        season.season_num + 1
                    };

                    if seasons.has(next_season) {
                        let series = Series::from_season_list(&seasons, next_season, &episodes)?;
                        *season = SeasonState::new(remote, &name, series, next_season)?;
                    }
                }
                _ => (),
            },
            Event::Tick => match &mut state.season.watch_state {
                WatchState::Idle => (),
                WatchState::Watching(start_time, _, child) => {
                    let status = match child.try_wait().context(err::IO)? {
                        Some(status) => status,
                        None => continue,
                    };

                    ensure!(status.success(), err::AbnormalPlayerExit);

                    let mins_watched = {
                        let watch_time = Utc::now() - *start_time;
                        watch_time.num_seconds() as f32 / 60.0
                    };

                    let season = &mut state.season;

                    let mins_must_watch =
                        season.series.info.episode_length as f32 * config.episode.pcnt_must_watch;

                    if mins_watched >= mins_must_watch {
                        season.tracker.episode_completed(remote, &config)?;
                    }

                    season.watch_state = WatchState::Idle;
                }
            },
        }
    }
}

pub struct SeriesState<'a> {
    pub season: SeasonState<'a>,
    pub series_names: Vec<String>,
    pub selected_series: usize,
    pub num_seasons: usize,
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

pub struct SeasonState<'a> {
    pub series: Series<'a>,
    pub tracker: SeriesTracker<'a>,
    pub season_num: usize,
    pub watch_time: String,
    pub score: Cow<'a, str>,
    pub watch_state: WatchState,
}

impl<'a> SeasonState<'a> {
    fn new<R, S>(
        remote: &R,
        name: S,
        series: Series<'a>,
        season_num: usize,
    ) -> Result<SeasonState<'a>>
    where
        R: RemoteService + ?Sized,
        S: Into<String>,
    {
        let watch_time = {
            let watch_time_mins = series.info.episodes * series.info.episode_length;
            util::hm_from_mins(watch_time_mins as f32)
        };

        let tracker = SeriesTracker::init(series.info.clone(), name)?;

        let score = match tracker.state.score() {
            Some(score) => remote.score_to_str(score).into_owned().into(),
            None => "??".into(),
        };

        Ok(SeasonState {
            series,
            tracker,
            season_num,
            watch_time,
            score,
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
}

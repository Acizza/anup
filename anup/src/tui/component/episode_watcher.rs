use super::{Component, ShouldReset};
use crate::err::{self, Result};
use crate::series::LastWatched;
use crate::tui::UIState;
use chrono::{DateTime, Utc};
use snafu::ResultExt;
use std::mem;
use std::process;

pub type ProgressTime = DateTime<Utc>;

pub struct EpisodeWatcher {
    last_watched: LastWatched,
    watch_state: WatchState,
}

impl EpisodeWatcher {
    pub fn new(last_watched: LastWatched) -> Self {
        Self {
            last_watched,
            watch_state: WatchState::default(),
        }
    }

    pub fn begin_watching_episode(&mut self, state: &mut UIState) -> Result<Option<ProgressTime>> {
        let series = match state.series.valid_selection_mut() {
            Some(series) => series,
            None => return Ok(None),
        };

        let is_diff_series = self.last_watched.set(&series.data.config.nickname);

        if is_diff_series {
            self.last_watched.save()?;
        }

        series.begin_watching(state.remote.as_ref(), &state.config, &state.db)?;

        let next_ep = series.data.entry.watched_episodes() + 1;

        let child = series
            .play_episode_cmd(next_ep as u32, &state.config)?
            .spawn()
            .context(err::FailedToPlayEpisode {
                episode: next_ep as u32,
            })?;

        let progress_time = series.data.next_watch_progress_time(&state.config);
        self.watch_state = WatchState::WatchingEpisode(progress_time, child);

        Ok(Some(progress_time))
    }
}

impl Component for EpisodeWatcher {
    type TickResult = ShouldReset;
    type KeyResult = ();

    fn tick(&mut self, state: &mut UIState) -> Result<Self::TickResult> {
        match &mut self.watch_state {
            WatchState::Idle => Ok(ShouldReset::No),
            WatchState::WatchingEpisode(_, child) => {
                match child.try_wait().context(err::IO) {
                    Ok(Some(_)) => (),
                    Ok(None) => return Ok(ShouldReset::No),
                    Err(err) => return Err(err),
                }

                // We should reset the current action immediately so we can't end up in a loop if an error occurs ahead
                let progress_time = match mem::take(&mut self.watch_state) {
                    WatchState::WatchingEpisode(progress_time, _) => progress_time,
                    _ => unreachable!(),
                };

                let series = match state.series.valid_selection_mut() {
                    Some(series) => series,
                    None => return Ok(ShouldReset::Yes),
                };

                if Utc::now() < progress_time {
                    return Ok(ShouldReset::Yes);
                }

                let remote = state.remote.as_ref();
                series.episode_completed(remote, &state.config, &state.db)?;

                Ok(ShouldReset::Yes)
            }
        }
    }
}

enum WatchState {
    Idle,
    WatchingEpisode(ProgressTime, process::Child),
}

impl Default for WatchState {
    fn default() -> Self {
        Self::Idle
    }
}

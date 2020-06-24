use super::Component;
use crate::series::LastWatched;
use crate::try_opt_r;
use crate::tui::{CurrentAction, UIState};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::mem;
use termion::event::Key;

pub type ProgressTime = DateTime<Utc>;

pub struct EpisodeWatcher {
    last_watched: LastWatched,
}

impl EpisodeWatcher {
    #[inline(always)]
    pub fn new(last_watched: LastWatched) -> Self {
        Self { last_watched }
    }

    pub fn begin_watching_episode(&mut self, state: &mut UIState) -> Result<()> {
        let series = try_opt_r!(state.series.valid_selection_mut());
        let is_diff_series = self.last_watched.set(&series.data.config.nickname);

        if is_diff_series {
            self.last_watched.save()?;
        }

        series.begin_watching(&state.remote, &state.config, &state.db)?;

        let next_ep = series.data.entry.watched_episodes() + 1;
        let child = series.play_episode(next_ep as u32, &state.config)?;
        let progress_time = series.data.next_watch_progress_time(&state.config);

        state.current_action = CurrentAction::WatchingEpisode(progress_time, child);

        Ok(())
    }
}

impl Component for EpisodeWatcher {
    type State = UIState;
    type KeyResult = ();

    fn tick(&mut self, state: &mut Self::State) -> Result<()> {
        match &mut state.current_action {
            CurrentAction::WatchingEpisode(_, child) => {
                match child.try_wait().context("waiting for episode to finish") {
                    Ok(Some(_)) => (),
                    Ok(None) => return Ok(()),
                    Err(err) => return Err(err),
                }

                // We should reset the current action immediately so we can't end up in a loop if an error occurs ahead
                let progress_time = match mem::take(&mut state.current_action) {
                    CurrentAction::WatchingEpisode(progress_time, _) => progress_time,
                    _ => unreachable!(),
                };

                let series = match state.series.valid_selection_mut() {
                    Some(series) => series,
                    None => {
                        state.current_action.reset();
                        return Ok(());
                    }
                };

                if Utc::now() >= progress_time {
                    series
                        .episode_completed(&state.remote, &state.config, &state.db)
                        .context("marking episode as completed")?;
                }

                state.current_action.reset();
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn process_key(&mut self, _: Key, _: &mut UIState) -> Self::KeyResult {}
}

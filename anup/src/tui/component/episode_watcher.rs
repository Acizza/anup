use super::Component;
use crate::err::{self, Error};
use crate::series::LastWatched;
use crate::tui::{CurrentAction, LogResult, UIState};
use chrono::{Duration, Utc};
use snafu::ResultExt;
use std::mem;
use termion::event::Key;

pub struct EpisodeWatcher {
    last_watched: LastWatched,
}

impl EpisodeWatcher {
    pub fn new(last_watched: LastWatched) -> Self {
        Self { last_watched }
    }
}

impl Component for EpisodeWatcher {
    fn tick(&mut self, state: &mut UIState) -> LogResult {
        LogResult::capture("processing episode", || {
            match &mut state.current_action {
                CurrentAction::WatchingEpisode(_, child) => {
                    let status = match child.try_wait().context(err::IO) {
                        Ok(Some(status)) => status,
                        Ok(None) => return Ok(()),
                        Err(err) => return Err(err),
                    };

                    // We should reset the current action immediately so we can't end up in a loop if an error occurs ahead
                    let progress_time = match mem::take(&mut state.current_action) {
                        CurrentAction::WatchingEpisode(progress_time, _) => progress_time,
                        _ => unreachable!(),
                    };

                    let series = match state.series.valid_selection_mut() {
                        Some(series) => series,
                        None => return Ok(()),
                    };

                    if !status.success() {
                        return Err(Error::AbnormalPlayerExit);
                    }

                    if Utc::now() >= progress_time {
                        let remote = state.remote.as_ref();
                        series.episode_completed(remote, &state.config, &state.db)?;
                    }

                    Ok(())
                }
                _ => Ok(()),
            }
        })
    }

    fn process_key(&mut self, key: Key, state: &mut UIState) -> LogResult {
        match key {
            Key::Char(ch) if ch == state.config.tui.keys.play_next_episode => {
                LogResult::capture("watching episode", || {
                    let series = match state.series.valid_selection_mut() {
                        Some(series) => series,
                        None => return Ok(()),
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

                    let progress_time = {
                        let secs_must_watch = (series.data.info.episode_length_mins as f32
                            * state.config.episode.pcnt_must_watch)
                            * 60.0;

                        let time_must_watch = Duration::seconds(secs_must_watch as i64);

                        Utc::now() + time_must_watch
                    };

                    state.current_action = CurrentAction::WatchingEpisode(progress_time, child);
                    Ok(())
                })
            }
            _ => LogResult::Ok,
        }
    }
}

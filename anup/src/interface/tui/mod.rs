mod component;
mod ui;

use super::{CurrentWatchInfo, SeriesPlayerArgs};
use crate::config::Config;
use crate::err::{self, Result};
use crate::file::{SaveDir, SaveFile};
use crate::track::SeriesTracker;
use anime::remote::RemoteService;
use anime::{SeasonInfoList, Series};
use chrono::{DateTime, Duration, Utc};
use clap::ArgMatches;
use component::command_prompt::{Command, CommandPrompt};
use component::log::{LogItem, StatusLog};
use snafu::{OptionExt, ResultExt};
use std::mem;
use std::process;
use termion::event::Key;
use ui::{Event, Events, UI};

pub fn run(args: &ArgMatches) -> Result<()> {
    let cstate = {
        let config = super::get_config()?;
        let remote = super::get_remote(args, true)?;

        CommonState {
            args,
            config,
            remote,
        }
    };

    let mut ui = UI::init()?;

    // Due to the way series selection works, we can't select a saved series that no longer
    // has matching episodes on disk, so we might as well just remove the series data.
    //
    // Series data could be deleted simply by renaming the folder episodes are in to something
    // the program can't recognize; however, the risk is small enough for this to be worth it.
    super::remove_orphaned_data(&cstate.config, |removed| {
        ui.status_log.push(format!("Removing {}", removed))
    })?;

    let mut ui_state = {
        let watch_info = super::get_watch_info(args)?;
        let series = SeriesState::new(&cstate, watch_info, true)?;

        let series_names = {
            let mut names = SaveDir::LocalData.get_subdirs()?;
            names.sort_unstable();
            names
        };

        let selected_series = series_names
            .iter()
            .position(|s| *s == series.watch_info.name)
            .unwrap_or(0);

        UIState {
            series,
            series_names,
            selected_series,
            selection: Selection::Series,
            status_bar_state: StatusBarState::default(),
        }
    };

    let events = Events::new(Duration::seconds(1));

    loop {
        ui.draw(&ui_state, cstate.remote.as_ref())?;

        match events.next()? {
            Event::Input(key) => match key {
                // Exit
                Key::Char('q') if !ui_state.status_bar_state.in_input_dialog() => {
                    // Prevent ruining the user's terminal
                    ui.clear().ok();
                    break Ok(());
                }
                key => match ui_state.process_key(&cstate, &mut ui, key) {
                    Ok(_) => (),
                    Err(err) => {
                        ui.status_log.push(LogItem::failed("Processing key", err));
                    }
                },
            },
            Event::Tick => match ui_state.process_tick(&cstate, &mut ui) {
                Ok(_) => (),
                Err(err) => ui.status_log.push(LogItem::failed("Processing tick", err)),
            },
        }
    }
}

/// Items that are not tied to the UI and are commonly used together.
struct CommonState<'a> {
    args: &'a ArgMatches<'a>,
    config: Config,
    remote: Box<dyn RemoteService>,
}

/// Current state of the UI.
pub struct UIState<'a> {
    series: SeriesState<'a>,
    series_names: Vec<String>,
    selected_series: usize,
    selection: Selection,
    status_bar_state: StatusBarState<'a>,
}

impl<'a> UIState<'a> {
    fn process_key<B>(&mut self, state: &'a CommonState, ui: &mut UI<B>, key: Key) -> Result<()>
    where
        B: tui::backend::Backend,
    {
        if !self.is_idle() {
            return Ok(());
        }

        if self.status_bar_state.in_input_dialog() {
            return self.process_input_dialog_key(state, ui, key);
        }

        match key {
            // Play next episode
            Key::Char(ch) if ch == state.config.tui.keys.play_next_episode => {
                self.series.set_as_last_watched(ui);

                ui.status_log.capture_status("Playing next episode", || {
                    self.series.season.play_next_episode_async(&state)
                });
            }
            // Command prompt
            Key::Char(':') => {
                self.status_bar_state.set_to_command_prompt();
            }
            // Switch between series and season selection
            Key::Left | Key::Right => {
                self.selection.set_opposite();
            }
            // Select series / season
            Key::Up | Key::Down => {
                let next_value = |value: usize| match key {
                    Key::Up => value.saturating_sub(1),
                    Key::Down => value + 1,
                    _ => value,
                };

                match self.selection {
                    Selection::Series => {
                        let series_index = next_value(self.selected_series);
                        let new_name = match self.series_names.get(series_index) {
                            Some(new_name) => new_name,
                            None => return Ok(()),
                        };

                        let watch_info = CurrentWatchInfo::new(new_name, 0);

                        self.series = SeriesState::new(state, watch_info, false)?;
                        self.selected_series = series_index;
                    }
                    Selection::Season => {
                        let season_index = next_value(self.series.watch_info.season);
                        self.series.set_season(season_index)?;
                    }
                }
            }
            _ => (),
        }

        Ok(())
    }

    fn process_input_dialog_key<B>(
        &mut self,
        state: &'a CommonState,
        ui: &mut UI<B>,
        key: Key,
    ) -> Result<()>
    where
        B: tui::backend::Backend,
    {
        match &mut self.status_bar_state {
            StatusBarState::Log => Ok(()),
            StatusBarState::CommandPrompt(prompt) => {
                use component::command_prompt::PromptResult;

                match prompt.process_key(key) {
                    Ok(PromptResult::Command(command)) => {
                        self.status_bar_state.reset();
                        self.process_command(command, state, &mut ui.status_log)
                    }
                    Ok(PromptResult::Done) => {
                        self.status_bar_state.reset();
                        Ok(())
                    }
                    Ok(PromptResult::NotDone) => Ok(()),
                    // We need to set the status bar state back before propagating errors,
                    // otherwise we'll be stuck in the prompt
                    Err(err) => {
                        self.status_bar_state.reset();
                        Err(err)
                    }
                }
            }
        }
    }

    fn process_command(
        &mut self,
        command: Command,
        cstate: &CommonState,
        log: &mut StatusLog,
    ) -> Result<()> {
        let remote = cstate.remote.as_ref();

        match command {
            Command::SyncFromRemote => {
                let season = &mut self.series.season;

                log.capture_status("Syncing entry from remote", || {
                    season.tracker.force_sync_changes_from_remote(remote)
                });

                Ok(())
            }
            Command::SyncToRemote => {
                let season = &mut self.series.season;

                log.capture_status("Syncing entry to remote", || {
                    season.tracker.force_sync_changes_to_remote(remote)
                });

                Ok(())
            }
            Command::Status(status) => {
                let season = &mut self.series.season;
                let entry = &mut season.tracker.entry;

                entry.set_status(status);

                log.capture_status(format!("Setting series status to \"{}\"", status), || {
                    season.tracker.sync_changes_to_remote(remote)
                });

                Ok(())
            }
            Command::Progress(direction) => {
                use component::command_prompt::ProgressDirection;

                let tracker = &mut self.series.season.tracker;

                match direction {
                    ProgressDirection::Forwards => {
                        log.capture_status("Forcing forward watch progress", || {
                            tracker.episode_completed(remote, &cstate.config)
                        });
                    }
                    ProgressDirection::Backwards => {
                        log.capture_status("Forcing backwards watch progress", || {
                            tracker.episode_regressed(remote)
                        });
                    }
                }

                Ok(())
            }
            Command::Score(raw_score) => {
                let score = match cstate.remote.parse_score(&raw_score) {
                    Some(score) if score == 0 => None,
                    Some(score) => Some(score),
                    None => {
                        log.push(LogItem::failed("Parsing score", None));
                        return Ok(());
                    }
                };

                let tracker = &mut self.series.season.tracker;
                tracker.entry.set_score(score);

                log.capture_status("Setting score", || {
                    tracker.sync_changes_to_remote(cstate.remote.as_ref())
                });

                Ok(())
            }
            Command::PlayerArgs(args) => {
                let name = self.series.watch_info.name.as_ref();

                log.capture_status("Saving player args for series", || {
                    SeriesPlayerArgs::new(args).save(name)
                });

                Ok(())
            }
        }
    }

    fn process_tick<B>(&mut self, state: &'a CommonState, ui: &mut UI<B>) -> Result<()>
    where
        B: tui::backend::Backend,
    {
        self.series.season.process_tick(state, ui)
    }

    fn is_idle(&self) -> bool {
        self.series.season.watch_state == WatchState::Idle
    }
}

#[derive(Copy, Clone)]
enum Selection {
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

enum StatusBarState<'a> {
    Log,
    CommandPrompt(CommandPrompt<'a>),
}

impl<'a> StatusBarState<'a> {
    fn set_to_command_prompt(&mut self) {
        *self = StatusBarState::CommandPrompt(CommandPrompt::new());
    }

    fn reset(&mut self) {
        *self = StatusBarState::default();
    }

    fn in_input_dialog(&self) -> bool {
        match self {
            StatusBarState::Log => false,
            StatusBarState::CommandPrompt(_) => true,
        }
    }
}

impl<'a> Default for StatusBarState<'a> {
    fn default() -> StatusBarState<'a> {
        StatusBarState::Log
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

        let episodes = super::get_episodes(&state.args, name, &state.config, is_last_watched)?;
        let seasons = super::get_season_list(name, remote, &episodes)?;
        let num_seasons = seasons.len();
        let series = Series::from_season_list(&seasons, watch_info.season, episodes)?;
        let season = SeasonState::new(name, series)?;

        Ok(SeriesState {
            watch_info,
            season,
            seasons,
            num_seasons,
            is_last_watched,
        })
    }

    /// Loads the season specified by `season_num` and points `season` to it.
    fn set_season(&mut self, season_num: usize) -> Result<()> {
        if !self.seasons.has(season_num) {
            return Ok(());
        }

        let episodes = self.season.series.episodes.clone();
        let series = Series::from_season_list(&self.seasons, season_num, episodes)?;

        self.season = SeasonState::new(&self.watch_info.name, series)?;
        self.watch_info.season = season_num;

        Ok(())
    }

    /// Sets the current series as the last watched one if it isn't already.
    fn set_as_last_watched<B>(&mut self, ui: &mut UI<B>)
    where
        B: tui::backend::Backend,
    {
        if self.is_last_watched {
            return;
        }

        ui.status_log
            .capture_status("Marking as the last watched series", || {
                self.watch_info.save(None)
            });

        self.is_last_watched = true;
    }
}

struct SeasonState<'a> {
    series: Series<'a>,
    tracker: SeriesTracker<'a>,
    watch_state: WatchState,
}

impl<'a> SeasonState<'a> {
    fn new<S>(name: S, series: Series<'a>) -> Result<SeasonState<'a>>
    where
        S: Into<String>,
    {
        let tracker = SeriesTracker::init(series.info.clone(), name)?;

        Ok(SeasonState {
            series,
            tracker,
            watch_state: WatchState::Idle,
        })
    }

    fn play_next_episode_async(&mut self, state: &CommonState) -> Result<()> {
        let remote = state.remote.as_ref();
        let config = &state.config;

        self.tracker.begin_watching(remote, config)?;
        let next_ep = self.tracker.entry.watched_eps() + 1;

        let episode = self
            .series
            .get_episode(next_ep)
            .context(err::EpisodeNotFound { episode: next_ep })?;

        let child = super::prepare_episode_cmd(&self.tracker.name, &state.config, episode)?
            .spawn()
            .context(err::FailedToPlayEpisode { episode: next_ep })?;

        let progress_time = {
            let secs_must_watch =
                (self.series.info.episode_length as f32 * config.episode.pcnt_must_watch) * 60.0;
            let time_must_watch = Duration::seconds(secs_must_watch as i64);

            Utc::now() + time_must_watch
        };

        self.watch_state = WatchState::Watching(progress_time, child);

        Ok(())
    }

    fn process_tick<B>(&mut self, state: &'a CommonState, ui: &mut UI<B>) -> Result<()>
    where
        B: tui::backend::Backend,
    {
        match &mut self.watch_state {
            WatchState::Idle => (),
            WatchState::Watching(_, child) => {
                let status = match child.try_wait().context(err::IO)? {
                    Some(status) => status,
                    None => return Ok(()),
                };

                // The watch state should be set to idle immediately to avoid a potential infinite loop.
                let progress_time = match mem::replace(&mut self.watch_state, WatchState::Idle) {
                    WatchState::Watching(progress_time, _) => progress_time,
                    WatchState::Idle => unreachable!(),
                };

                if !status.success() {
                    ui.status_log.push("Player did not exit properly");
                    return Ok(());
                }

                if Utc::now() >= progress_time {
                    ui.status_log
                        .capture_status("Marking episode as completed", || {
                            self.tracker
                                .episode_completed(state.remote.as_ref(), &state.config)
                        });
                } else {
                    ui.status_log.push("Not marking episode as completed");
                }
            }
        }

        Ok(())
    }
}

type ProgressTime = DateTime<Utc>;

enum WatchState {
    Idle,
    Watching(ProgressTime, process::Child),
}

impl PartialEq for WatchState {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (WatchState::Idle, WatchState::Idle) => true,
            (WatchState::Watching(_, _), WatchState::Watching(_, _)) => true,
            _ => false,
        }
    }
}

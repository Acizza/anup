mod component;
mod ui;

use crate::config::Config;
use crate::err::{self, Result};
use crate::file::SaveFile;
use crate::series::{SavedSeries, Series};
use anime::remote::RemoteService;
use chrono::{DateTime, Duration, Utc};
use clap::ArgMatches;
use component::command_prompt::{Command, CommandPrompt};
use component::log::{LogItem, StatusLog};
use snafu::ResultExt;
use std::mem;
use std::process;
use termion::event::Key;
use ui::{Event, Events, UI};

pub fn run(args: &ArgMatches) -> Result<()> {
    let mut ui = UI::init()?;

    let mut cstate = {
        let config = Config::load_or_create()?;
        let remote = init_remote(args, &mut ui.status_log);

        CommonState { config, remote }
    };

    let mut ui_state = init_ui_state(&cstate, args)?;
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
                key => match ui_state.process_key(&mut cstate, &mut ui.status_log, key) {
                    Ok(_) => (),
                    Err(err) => {
                        ui.status_log.push(LogItem::failed("Processing key", err));
                    }
                },
            },
            Event::Tick => match ui_state.process_tick(&cstate, &mut ui.status_log) {
                Ok(_) => (),
                Err(err) => ui.status_log.push(LogItem::failed("Processing tick", err)),
            },
        }
    }
}

fn init_remote(args: &ArgMatches, log: &mut StatusLog) -> Box<dyn RemoteService> {
    use anime::remote::anilist;
    use anime::remote::offline::Offline;

    match crate::init_remote(args, true) {
        Ok(remote) => remote,
        Err(err) => {
            match err {
                err::Error::NeedAniListToken => {
                    log.push(format!(
                        "No access token found. Go to {} \
                         and set your token with the 'token' command",
                        anilist::auth_url(crate::ANILIST_CLIENT_ID)
                    ));
                }
                _ => {
                    log.push(LogItem::failed("Logging in", err));
                    log.push(format!(
                        "If you need a new token, go to {} \
                         and set it with the 'token' command",
                        anilist::auth_url(crate::ANILIST_CLIENT_ID)
                    ));
                }
            }

            log.push("Continuing in offline mode");

            Box::new(Offline::new())
        }
    }
}

/// Items that are not tied to the UI and are commonly used together.
struct CommonState {
    config: Config,
    remote: Box<dyn RemoteService>,
}

fn init_ui_state<'a>(cstate: &CommonState, args: &ArgMatches) -> Result<UIState<'a>> {
    let mut saved_series = SavedSeries::load_or_default()?;
    let series = init_series_list(&cstate, args, &mut saved_series)?;

    let selected_series = match saved_series.last_watched_id {
        Some(id) => series
            .iter()
            .position(|series| series.info.id == id)
            .unwrap_or(0),
        None => 0,
    };

    let series = series.into_iter().map(SeriesState::new).collect();

    Ok(UIState {
        series,
        selected_series,
        saved_series,
        status_bar_state: StatusBarState::default(),
        last_used_command: None,
    })
}

fn init_series_list(
    cstate: &CommonState,
    args: &ArgMatches,
    saved_series: &mut SavedSeries,
) -> Result<Vec<Series>> {
    let mut series = saved_series.load_all_series_and_validate()?;

    // If the user specified a series, we'll need to check to see if we
    // already have it or fetch & save it otherwise.
    let desired_series = match args.value_of("series") {
        Some(desired_series) => desired_series,
        None => return Ok(series),
    };

    if saved_series.contains(desired_series) {
        return Ok(series);
    }

    let new_series = saved_series.insert_and_save_from_args_and_remote(
        args,
        desired_series,
        &cstate.config,
        cstate.remote.as_ref(),
    )?;

    series.push(new_series);
    Ok(series)
}

/// Current state of the UI.
pub struct UIState<'a> {
    series: Vec<SeriesState>,
    selected_series: usize,
    saved_series: SavedSeries,
    status_bar_state: StatusBarState<'a>,
    last_used_command: Option<Command>,
}

macro_rules! cur_series {
    ($struct:ident) => {
        match $struct.cur_series() {
            Some(value) => value,
            None => return Ok(()),
        }
    };
}

macro_rules! cur_series_mut {
    ($struct:ident) => {
        match $struct.cur_series_mut() {
            Some(value) => value,
            None => return Ok(()),
        }
    };
}

impl<'a> UIState<'a> {
    fn cur_series(&self) -> Option<&SeriesState> {
        self.series.get(self.selected_series)
    }

    fn cur_series_mut(&mut self) -> Option<&mut SeriesState> {
        self.series.get_mut(self.selected_series)
    }

    fn process_key(
        &mut self,
        state: &mut CommonState,
        log: &mut StatusLog,
        key: Key,
    ) -> Result<()> {
        if !self.is_idle() {
            return Ok(());
        }

        if self.status_bar_state.in_input_dialog() {
            return self.process_input_dialog_key(state, log, key);
        }

        match key {
            // Play next episode
            Key::Char(ch) if ch == state.config.tui.keys.play_next_episode => {
                let last_watched_changed = {
                    let id = cur_series!(self).inner.info.id;
                    self.saved_series.set_last_watched(id)
                };

                if last_watched_changed {
                    log.capture_status("Marking series as the last watched one", || {
                        self.saved_series.save()
                    });
                }

                log.capture_status("Playing next episode", || {
                    cur_series_mut!(self).play_next_episode_async(&state)
                });
            }
            // Command prompt
            Key::Char(':') => {
                self.status_bar_state.set_to_command_prompt();
            }
            // Run last used command
            Key::Char(ch) if ch == state.config.tui.keys.run_last_command => {
                let cmd = match &self.last_used_command {
                    Some(cmd) => cmd.clone(),
                    None => return Ok(()),
                };

                self.process_command(cmd, state, log)?;
            }
            // Select series
            Key::Up | Key::Down => {
                self.selected_series = match key {
                    Key::Up => self.selected_series.saturating_sub(1),
                    Key::Down if self.selected_series < self.series.len().saturating_sub(1) => {
                        self.selected_series + 1
                    }
                    _ => self.selected_series,
                };
            }
            _ => (),
        }

        Ok(())
    }

    fn process_input_dialog_key(
        &mut self,
        state: &mut CommonState,
        log: &mut StatusLog,
        key: Key,
    ) -> Result<()> {
        match &mut self.status_bar_state {
            StatusBarState::Log => Ok(()),
            StatusBarState::CommandPrompt(prompt) => {
                use component::command_prompt::PromptResult;

                match prompt.process_key(key) {
                    Ok(PromptResult::Command(command)) => {
                        self.status_bar_state.reset();
                        self.last_used_command = Some(command.clone());
                        self.process_command(command, state, log)
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
        cstate: &mut CommonState,
        log: &mut StatusLog,
    ) -> Result<()> {
        match command {
            Command::SyncFromRemote => {
                let series = &mut cur_series_mut!(self).inner;
                let remote = cstate.remote.as_ref();

                log.capture_status("Syncing entry from remote", || {
                    series.force_sync_changes_from_remote(remote)
                });

                Ok(())
            }
            Command::SyncToRemote => {
                let series = &mut cur_series_mut!(self).inner;
                let remote = cstate.remote.as_ref();

                log.capture_status("Syncing entry to remote", || {
                    series.force_sync_changes_to_remote(remote)
                });

                Ok(())
            }
            Command::Status(status) => {
                let series = &mut cur_series_mut!(self).inner;
                let remote = cstate.remote.as_ref();

                series.entry.set_status(status, &cstate.config);

                log.capture_status(format!("Setting series status to \"{}\"", status), || {
                    series.sync_changes_to_remote(remote)
                });

                Ok(())
            }
            Command::Progress(direction) => {
                use component::command_prompt::ProgressDirection;

                let series = &mut cur_series_mut!(self).inner;
                let remote = cstate.remote.as_ref();

                match direction {
                    ProgressDirection::Forwards => {
                        log.capture_status("Forcing forward watch progress", || {
                            series.episode_completed(remote, &cstate.config)
                        });
                    }
                    ProgressDirection::Backwards => {
                        log.capture_status("Forcing backwards watch progress", || {
                            series.episode_regressed(remote, &cstate.config)
                        });
                    }
                }

                Ok(())
            }
            Command::Score(raw_score) => {
                let series = &mut cur_series_mut!(self).inner;
                let remote = cstate.remote.as_ref();

                let score = match cstate.remote.parse_score(&raw_score) {
                    Some(score) if score == 0 => None,
                    Some(score) => Some(score),
                    None => {
                        log.push(LogItem::failed("Parsing score", None));
                        return Ok(());
                    }
                };

                log.capture_status("Setting score", || {
                    series.entry.set_score(score);
                    series.sync_changes_to_remote(remote)
                });

                Ok(())
            }
            Command::PlayerArgs(args) => {
                let series = &mut cur_series_mut!(self).inner;

                log.capture_status("Saving player args for series", || {
                    series.player_args = args;
                    series.save()
                });

                Ok(())
            }
            Command::LoginToken(token) => {
                use anime::remote::anilist::AniList;
                use anime::remote::AccessToken;

                log.capture_status("Setting user access token", || {
                    let token = AccessToken::encode(token);
                    token.save()?;
                    cstate.remote = Box::new(AniList::login(token)?);
                    Ok(())
                });

                Ok(())
            }
        }
    }

    fn process_tick(&mut self, state: &CommonState, log: &mut StatusLog) -> Result<()> {
        cur_series_mut!(self).process_tick(state, log)
    }

    fn is_idle(&self) -> bool {
        match self.cur_series() {
            Some(cur_series) => cur_series.watch_state == WatchState::Idle,
            None => true,
        }
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

#[derive(Debug)]
struct SeriesState {
    inner: Series,
    watch_state: WatchState,
}

impl SeriesState {
    fn new(inner: Series) -> SeriesState {
        SeriesState {
            inner,
            watch_state: WatchState::Idle,
        }
    }

    fn play_next_episode_async(&mut self, state: &CommonState) -> Result<()> {
        let remote = state.remote.as_ref();
        let config = &state.config;

        self.inner.begin_watching(remote, config)?;
        let next_ep = self.inner.entry.watched_eps() + 1;

        let child = self
            .inner
            .play_episode_cmd(next_ep, &state.config)?
            .spawn()
            .context(err::FailedToPlayEpisode { episode: next_ep })?;

        let progress_time = {
            let secs_must_watch =
                (self.inner.info.episode_length as f32 * config.episode.pcnt_must_watch) * 60.0;
            let time_must_watch = Duration::seconds(secs_must_watch as i64);

            Utc::now() + time_must_watch
        };

        self.watch_state = WatchState::Watching(progress_time, child);

        Ok(())
    }

    fn process_tick(&mut self, state: &CommonState, log: &mut StatusLog) -> Result<()> {
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
                    log.push("Player did not exit properly");
                    return Ok(());
                }

                if Utc::now() >= progress_time {
                    log.capture_status("Marking episode as completed", || {
                        self.inner
                            .episode_completed(state.remote.as_ref(), &state.config)
                    });
                } else {
                    log.push("Not marking episode as completed");
                }
            }
        }

        Ok(())
    }
}

type ProgressTime = DateTime<Utc>;

#[derive(Debug)]
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

mod backend;
mod component;
mod selection;
mod state;
mod widget_util;

use self::{
    backend::Events,
    selection::Selection,
    state::{InputState, Reactive, ReactiveState, UIState},
};
use crate::series::LastWatched;
use crate::try_opt_r;
use crate::Args;
use crate::{key::Key, util::arc_mutex};
use anime::remote::{Remote, ScoreParser};
use anyhow::{anyhow, Context, Result};
use backend::{EventKind, UIBackend};
use component::prompt::command::InputResult;
use component::prompt::COMMAND_KEY;
use component::prompt::{command::Command, log::LogKind};
use component::series_list::SeriesList;
use component::{main_panel::MainPanel, prompt::command::CommandPrompt};
use component::{Component, Draw};
use crossterm::{event::KeyCode, terminal};
use state::ThreadedState;
use std::sync::Arc;
use tui::layout::{Constraint, Direction, Layout};

pub async fn run(args: &Args) -> Result<()> {
    let backend = UIBackend::init().context("failed to init backend")?;

    let mut ui = UI::init(&args, backend)
        .await
        .context("failed to init UI")?;

    let result = ui.run().await;
    ui.exit()?;

    result
}

struct UI {
    events: Events,
    backend: UIBackend,
    state: ThreadedState,
    panels: Panels,
}

impl UI {
    async fn init(args: &Args, backend: UIBackend) -> Result<UI> {
        let (remote, remote_error) = match crate::init_remote(args) {
            Ok(Some(remote)) => (remote, None),
            Ok(None) => (Remote::offline(), None),
            Err(err) => (Remote::offline(), Some(err)),
        };

        let mut state = UIState::init(remote).context("UI state init")?;

        if let Some(err) = remote_error {
            let log = &mut state.log;
            log.push_error(&err);
            log.push(LogKind::Context, "enter user management with 'u' and add your account again if a new token is needed");
            log.push(LogKind::Info, "continuing in offline mode");
        }

        let threaded_state = arc_mutex(Reactive::new(state));

        let panels = Panels::init(args, &threaded_state)
            .await
            .context("panel init")?;

        Ok(Self {
            events: Events::new(),
            backend,
            state: threaded_state,
            panels,
        })
    }

    async fn run(&mut self) -> Result<()> {
        {
            let mut state = self.state.lock();

            if let Err(err) = self.panels.draw(state.get_mut(), &mut self.backend) {
                return Err(err);
            }
        }

        loop {
            match self.next_cycle().await {
                CycleResult::Ok => (),
                CycleResult::Exit => break Ok(()),
                CycleResult::Error(err) => return Err(err),
            }
        }
    }

    async fn next_cycle(&mut self) -> CycleResult {
        let event = match self.events.next().await {
            Ok(Some(event)) => event,
            Ok(None) => return CycleResult::Ok,
            Err(backend::ErrorKind::ExitRequest) => return CycleResult::Exit,
            Err(backend::ErrorKind::Other(err)) => return CycleResult::Error(err),
        };

        let mut state = self.state.lock();

        let result = match event {
            EventKind::Key(key) => self.panels.process_key(key, &mut state).await,
            EventKind::Tick => CycleResult::Ok,
        };

        match self.backend.update_term_size() {
            Ok(true) => state.mark_dirty(),
            Ok(false) => (),
            Err(err) => return CycleResult::Error(err.into()),
        }

        if !state.dirty() {
            return result;
        }

        if let Err(err) = self.panels.draw(state.get_mut(), &mut self.backend) {
            return CycleResult::Error(err);
        }

        state.reset_dirty();
        result
    }

    pub fn exit(mut self) -> Result<()> {
        self.backend.clear().ok();
        terminal::disable_raw_mode().map_err(Into::into)
    }
}

pub enum CycleResult {
    Ok,
    Exit,
    Error(anyhow::Error),
}

struct Panels {
    command_prompt: CommandPrompt,
    series_list: SeriesList,
    main_panel: MainPanel,
    state: ThreadedState,
}

impl Panels {
    async fn init(args: &Args, state: &ThreadedState) -> Result<Panels> {
        let last_watched = LastWatched::load().context("last watched series")?;

        let series_list = {
            let mut state = state.lock();
            SeriesList::init(args, state.get_mut(), &last_watched)
        };

        Ok(Self {
            command_prompt: CommandPrompt::new(),
            series_list,
            main_panel: MainPanel::new(Arc::clone(state)),
            state: Arc::clone(state),
        })
    }

    async fn process_key(&mut self, key: Key, state: &mut ReactiveState) -> CycleResult {
        macro_rules! capture {
            ($result:expr) => {
                match $result {
                    Ok(value) => value,
                    Err(err) => {
                        state.get_mut().log.push_error(&err);
                        return CycleResult::Ok;
                    }
                }
            };
        }

        macro_rules! process_key {
            ($component:ident) => {
                capture!(self.$component.process_key(key, state.get_mut()))
            };
        }

        match state.input_state {
            InputState::Idle => match *key {
                KeyCode::Char('q') => return CycleResult::Exit,
                _ if key == state.config.tui.keys.play_next_episode => {
                    capture!(state.get_mut().play_next_series_episode(&self.state).await)
                }
                KeyCode::Char('a') => {
                    capture!(self.main_panel.switch_to_add_series(state.get_mut()))
                }
                KeyCode::Char('e') => {
                    capture!(self.main_panel.switch_to_update_series(state.get_mut()))
                }
                KeyCode::Char('D') => {
                    capture!(self.main_panel.switch_to_delete_series(state.get_mut()))
                }
                KeyCode::Char('u') => self.main_panel.switch_to_user_panel(state.get_mut()),
                KeyCode::Char('s') => {
                    capture!(self.main_panel.switch_to_split_series(state.get_mut()))
                }
                KeyCode::Char(COMMAND_KEY) => {
                    state.get_mut().input_state = InputState::EnteringCommand
                }
                _ => process_key!(series_list),
            },
            InputState::Locked => (),
            InputState::FocusedOnMainPanel => process_key!(main_panel),
            InputState::EnteringCommand => {
                let state = state.get_mut();
                let result = self.command_prompt.process_key(key, state);

                if !matches!(result, Ok(InputResult::Continue)) {
                    self.command_prompt.reset();
                    state.input_state.reset();
                }

                match capture!(result) {
                    InputResult::Command(cmd) => {
                        capture!(Self::process_command(cmd, state))
                    }
                    InputResult::Done | InputResult::Continue => (),
                }
            }
        }

        CycleResult::Ok
    }

    fn draw(&mut self, state: &mut UIState, backend: &mut UIBackend) -> Result<()> {
        // We need to remove the mutable borrow on self so we can call other mutable methods on it during our draw call.
        // This *should* be completely safe as none of the methods we need to call can mutate our backend.
        let term: *mut _ = &mut backend.terminal;
        let term: &mut _ = unsafe { &mut *term };

        term.draw(|mut frame| {
            let horiz_splitter = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(20), Constraint::Percentage(70)].as_ref())
                .split(frame.size());

            self.series_list.draw(state, horiz_splitter[0], &mut frame);

            // Series info panel vertical splitter
            let info_panel_splitter = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(80), Constraint::Percentage(20)].as_ref())
                .split(horiz_splitter[1]);

            self.main_panel
                .draw(state, info_panel_splitter[0], &mut frame);

            match state.input_state {
                InputState::EnteringCommand => {
                    self.command_prompt.draw(&(), info_panel_splitter[1], frame)
                }
                _ => state.log.draw(&(), info_panel_splitter[1], frame),
            }
        })
        .map_err(Into::into)
    }

    fn process_command(command: Command, state: &mut UIState) -> Result<()> {
        let remote = &mut state.remote;
        let config = &state.config;
        let db = &state.db;

        match command {
            Command::PlayerArgs(args) => {
                let series = try_opt_r!(state.series.valid_selection_mut());

                series.data.config.player_args = args.into();
                series.save(db)?;
                Ok(())
            }
            Command::Progress(direction) => {
                use component::prompt::command::ProgressDirection;

                let series = try_opt_r!(state.series.valid_selection_mut());

                match direction {
                    ProgressDirection::Forwards => series.episode_completed(remote, config, db),
                    ProgressDirection::Backwards => series.episode_regressed(remote, config, db),
                }
            }
            cmd @ Command::SyncFromRemote | cmd @ Command::SyncToRemote => {
                let series = try_opt_r!(state.series.valid_selection_mut());

                match cmd {
                    Command::SyncFromRemote => series.data.force_sync_from_remote(remote)?,
                    Command::SyncToRemote => series.data.entry.force_sync_to_remote(remote)?,
                    _ => unreachable!(),
                }

                series.save(db)?;
                Ok(())
            }
            Command::Score(raw_score) => {
                let series = try_opt_r!(state.series.valid_selection_mut());

                let score = match remote.parse_score(&raw_score) {
                    Some(score) if score == 0 => None,
                    Some(score) => Some(score),
                    None => return Err(anyhow!("invalid score")),
                };

                series.data.entry.set_score(score.map(i16::from));
                series.data.entry.sync_to_remote(remote)?;
                series.save(db)?;

                Ok(())
            }
            Command::Status(status) => {
                let series = try_opt_r!(state.series.valid_selection_mut());

                series.data.entry.set_status(status, config);
                series.data.entry.sync_to_remote(remote)?;
                series.save(db)?;

                Ok(())
            }
        }
    }
}

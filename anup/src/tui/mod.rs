mod backend;
mod component;
mod selection;
mod widget_util;

use crate::database::Database;
use crate::file::SerializedFile;
use crate::series::config::SeriesConfig;
use crate::series::info::SeriesInfo;
use crate::series::{LastWatched, LoadedSeries, Series, SeriesData};
use crate::user::Users;
use crate::Args;
use crate::{config::Config, key::Key};
use crate::{try_opt_r, try_opt_ret};
use anime::local::SortedEpisodes;
use anime::remote::{Remote, ScoreParser};
use anyhow::{anyhow, Context, Result};
use backend::{EventKind, UIBackend};
use component::episode_watcher::{EpisodeWatcher, ProgressTime};
use component::main_panel::MainPanel;
use component::prompt::command::Command;
use component::prompt::log::Log;
use component::prompt::{Prompt, PromptResult, COMMAND_KEY};
use component::series_list::SeriesList;
use component::{Component, Draw};
use crossterm::{event::KeyCode, terminal};
use std::mem;
use std::process;
use tui::layout::{Constraint, Direction, Layout};

use self::{backend::Events, selection::Selection};

pub async fn run(args: &Args) -> Result<()> {
    let backend = UIBackend::init().context("failed to init backend")?;
    let mut ui = UI::init(&args, backend).context("failed to init UI")?;

    loop {
        match ui.next_cycle().await {
            CycleResult::Ok => (),
            CycleResult::Exit => break,
            CycleResult::Error(err) => {
                ui.exit().ok();
                return Err(err);
            }
        }
    }

    ui.exit()
}

pub struct UIState {
    series: Selection<LoadedSeries>,
    current_action: CurrentAction,
    config: Config,
    users: Users,
    remote: Remote,
    db: Database,
}

impl UIState {
    fn init(remote: Remote) -> Result<Self> {
        let config = Config::load_or_create().context("failed to load / create config")?;
        let users = Users::load_or_create().context("failed to load / create users")?;
        let db = Database::open().context("failed to open database")?;

        let mut series = SeriesConfig::load_all(&db)
            .context("failed to load series configs")?
            .into_iter()
            .map(|sconfig| Series::load_from_config(sconfig, &config, &db))
            .collect::<Vec<_>>();

        series.sort_unstable();

        Ok(Self {
            series: Selection::new(series),
            current_action: CurrentAction::default(),
            config,
            users,
            remote,
            db,
        })
    }

    fn add_series<E>(&mut self, config: SeriesConfig, info: SeriesInfo, episodes: E) -> Result<()>
    where
        E: Into<Option<SortedEpisodes>>,
    {
        let data = SeriesData::from_remote(config, info, &self.remote)?;

        let series = match episodes.into() {
            Some(episodes) => LoadedSeries::Complete(Series::with_episodes(data, episodes)),
            None => Series::init(data, &self.config),
        };

        series.save(&self.db)?;

        let nickname = series.nickname().to_string();

        self.series.push(series);
        self.series.items_mut().sort_unstable();

        let selected = self
            .series
            .iter()
            .position(|s| s.nickname() == nickname)
            .unwrap_or(0);

        self.series.set_selected(selected);
        Ok(())
    }

    fn init_selected_series(&mut self) {
        let selected = try_opt_ret!(self.series.selected_mut());
        selected.try_load(&self.config, &self.db)
    }

    fn delete_selected_series(&mut self) -> Result<LoadedSeries> {
        let series = match self.series.remove_selected() {
            Some(series) => series,
            None => return Err(anyhow!("must select series to delete")),
        };

        // Since we changed our selected series, we need to make sure the new one is initialized
        self.init_selected_series();

        series.config().delete(&self.db)?;
        Ok(series)
    }
}

pub enum CurrentAction {
    Idle,
    WatchingEpisode(ProgressTime, process::Child),
    FocusedOnMainPanel,
    EnteringCommand,
}

impl CurrentAction {
    #[inline(always)]
    fn reset(&mut self) {
        *self = Self::default();
    }
}

impl Default for CurrentAction {
    fn default() -> Self {
        Self::Idle
    }
}

impl PartialEq for CurrentAction {
    fn eq(&self, other: &Self) -> bool {
        mem::discriminant(self) == mem::discriminant(other)
    }
}

struct UI<'a> {
    events: Events,
    backend: UIBackend,
    state: UIState,
    prompt: Prompt<'a>,
    series_list: SeriesList,
    main_panel: MainPanel,
    episode_watcher: EpisodeWatcher,
}

macro_rules! capture_err {
    ($self:ident, $result:expr) => {
        match $result {
            value @ Ok(_) => value,
            Err(err) => {
                $self.prompt.log.push_error(&err);
                Err(err)
            }
        }
    };
}

impl<'a> UI<'a> {
    fn init(args: &Args, backend: UIBackend) -> Result<Self> {
        let mut prompt = Prompt::new();
        let remote = Self::init_remote(args, &mut prompt.log);

        let mut state = UIState::init(remote).context("UI state init")?;

        let last_watched = LastWatched::load().context("last watched series")?;
        let series_list = SeriesList::init(args, &mut state, &last_watched);

        Ok(Self {
            events: Events::new(),
            backend,
            state,
            prompt,
            series_list,
            main_panel: MainPanel::new(),
            episode_watcher: EpisodeWatcher::new(last_watched),
        })
    }

    fn init_remote(args: &Args, log: &mut Log) -> Remote {
        match crate::init_remote(args) {
            Ok(Some(remote)) => remote,
            Ok(None) => Remote::offline(),
            Err(err) => {
                log.push_error(&err);
                log.push_context(
                    "enter user management with 'u' and add your account again if a new token is needed",
                );

                log.push_info("continuing in offline mode");
                Remote::offline()
            }
        }
    }

    async fn next_cycle(&mut self) -> CycleResult {
        if let Err(err) = self.draw() {
            return CycleResult::Error(err);
        }

        let event = match self.events.next().await {
            Ok(Some(event)) => event,
            Ok(None) => return CycleResult::Ok,
            Err(backend::ErrorKind::ExitRequest) => return CycleResult::Exit,
            Err(backend::ErrorKind::Other(err)) => return CycleResult::Error(err),
        };

        match event {
            EventKind::Key(key) => self.process_key(key),
            EventKind::Tick => self.tick(),
        }
    }

    fn tick(&mut self) -> CycleResult {
        macro_rules! capture {
            ($result:expr) => {
                capture_err!(self, $result)
            };
        }

        macro_rules! tick {
            ($($component:ident),+) => {
                $(capture!(self.$component.tick(&mut self.state)).ok();)+
            };
        }

        tick!(prompt, series_list, main_panel, episode_watcher);

        CycleResult::Ok
    }

    fn draw(&mut self) -> Result<()> {
        // We need to remove the mutable borrow on self so we can call other mutable methods on it during our draw call.
        // This *should* be completely safe as none of the methods we need to call can mutate our backend.
        let term: *mut _ = &mut self.backend.terminal;
        let term: &mut _ = unsafe { &mut *term };

        term.draw(|mut frame| {
            let horiz_splitter = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(20), Constraint::Percentage(70)].as_ref())
                .split(frame.size());

            self.series_list
                .draw(&self.state, horiz_splitter[0], &mut frame);

            // Series info panel vertical splitter
            let info_panel_splitter = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(80), Constraint::Percentage(20)].as_ref())
                .split(horiz_splitter[1]);

            self.main_panel
                .draw(&self.state, info_panel_splitter[0], &mut frame);

            self.prompt
                .draw(&self.state, info_panel_splitter[1], &mut frame);
        })
        .map_err(Into::into)
    }

    /// Process a key input for all UI components.
    ///
    /// Returns true if the program should exit.
    fn process_key(&mut self, key: Key) -> CycleResult {
        macro_rules! capture {
            ($result:expr) => {
                match capture_err!(self, $result) {
                    Ok(value) => value,
                    Err(_) => return CycleResult::Ok,
                }
            };
        }

        macro_rules! process_key {
            ($component:ident) => {
                capture!(self.$component.process_key(key, &mut self.state))
            };
        }

        match &self.state.current_action {
            CurrentAction::Idle => match *key {
                KeyCode::Char('q') => return CycleResult::Exit,
                _ if key == self.state.config.tui.keys.play_next_episode => {
                    capture!(self.episode_watcher.begin_watching_episode(&mut self.state))
                }
                KeyCode::Char('a') => {
                    capture!(self.main_panel.switch_to_add_series(&mut self.state))
                }
                KeyCode::Char('e') => {
                    capture!(self.main_panel.switch_to_update_series(&mut self.state))
                }
                KeyCode::Char('D') => {
                    capture!(self.main_panel.switch_to_delete_series(&mut self.state))
                }
                KeyCode::Char('u') => self.main_panel.switch_to_user_panel(&mut self.state),
                KeyCode::Char('s') => {
                    capture!(self.main_panel.switch_to_split_series(&mut self.state))
                }
                KeyCode::Char(COMMAND_KEY) => {
                    self.state.current_action = CurrentAction::EnteringCommand
                }
                _ => process_key!(series_list),
            },
            CurrentAction::WatchingEpisode(_, _) => (),
            CurrentAction::FocusedOnMainPanel => process_key!(main_panel),
            CurrentAction::EnteringCommand => {
                match capture!(self.prompt.process_key(key, &mut self.state)) {
                    PromptResult::Ok => (),
                    PromptResult::HasCommand(cmd) => capture!(self.process_command(cmd)),
                }
            }
        }

        CycleResult::Ok
    }

    fn process_command(&mut self, command: Command) -> Result<()> {
        let remote = &mut self.state.remote;
        let config = &self.state.config;
        let db = &self.state.db;

        match command {
            Command::PlayerArgs(args) => {
                let series = try_opt_r!(self.state.series.valid_selection_mut());

                series.data.config.player_args = args.into();
                series.save(db)?;
                Ok(())
            }
            Command::Progress(direction) => {
                use component::prompt::command::ProgressDirection;

                let series = try_opt_r!(self.state.series.valid_selection_mut());

                match direction {
                    ProgressDirection::Forwards => series.episode_completed(remote, config, db),
                    ProgressDirection::Backwards => series.episode_regressed(remote, config, db),
                }
            }
            cmd @ Command::SyncFromRemote | cmd @ Command::SyncToRemote => {
                let series = try_opt_r!(self.state.series.valid_selection_mut());

                match cmd {
                    Command::SyncFromRemote => series.data.force_sync_from_remote(remote)?,
                    Command::SyncToRemote => series.data.entry.force_sync_to_remote(remote)?,
                    _ => unreachable!(),
                }

                series.save(db)?;
                Ok(())
            }
            Command::Score(raw_score) => {
                let series = try_opt_r!(self.state.series.valid_selection_mut());

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
                let series = try_opt_r!(self.state.series.valid_selection_mut());

                series.data.entry.set_status(status, config);
                series.data.entry.sync_to_remote(remote)?;
                series.save(db)?;

                Ok(())
            }
        }
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

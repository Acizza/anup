mod backend;
mod component;
mod widget_util;

use crate::config::Config;
use crate::database::Database;
use crate::err::{self, Error, Result};
use crate::file::SerializedFile;
use crate::series::config::SeriesConfig;
use crate::series::info::SeriesInfo;
use crate::series::{LastWatched, Series, SeriesData};
use crate::user::Users;
use crate::CmdOptions;
use crate::{try_opt_r, try_opt_ret};
use anime::local::SortedEpisodes;
use anime::remote::{Remote, ScoreParser};
use backend::{TermionBackend, UIBackend, UIEvent, UIEvents};
use chrono::Duration;
use component::episode_watcher::{EpisodeWatcher, ProgressTime};
use component::main_panel::MainPanel;
use component::prompt::command::Command;
use component::prompt::log::Log;
use component::prompt::{Prompt, PromptResult, COMMAND_KEY};
use component::series_list::SeriesList;
use component::{Component, Draw};
use snafu::ResultExt;
use std::mem;
use std::ops::{Index, IndexMut};
use std::process;
use termion::event::Key;
use tui::backend::Backend;
use tui::layout::{Constraint, Direction, Layout};

pub fn run(args: CmdOptions) -> Result<()> {
    let backend = UIBackend::init()?;
    let mut ui = UIWorld::<TermionBackend>::init(&args, backend)?;
    let events = UIEvents::new(Duration::seconds(1));

    loop {
        ui.draw()?;

        match events.next()? {
            UIEvent::Input(key) => {
                if ui.process_key(key) {
                    ui.exit();
                    break Ok(());
                }
            }
            UIEvent::Tick => ui.tick(),
        }
    }
}

pub struct UIState {
    series: Selection<SeriesStatus>,
    current_action: CurrentAction,
    config: Config,
    users: Users,
    remote: Remote,
    db: Database,
}

impl UIState {
    fn init(remote: Remote) -> Result<Self> {
        let config = Config::load_or_create()?;
        let users = Users::load_or_create()?;
        let db = Database::open()?;

        let series = SeriesConfig::load_all(&db)?
            .into_iter()
            .map(|sconfig| SeriesStatus::init(sconfig, &config, &db))
            .collect::<Vec<_>>();

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
            Some(episodes) => Series::with_episodes(data, episodes),
            None => Series::new(data, &self.config)?,
        };

        series.save(&self.db)?;

        let nickname = series.data.config.nickname.clone();

        self.series.push(SeriesStatus::Loaded(series));
        self.series
            .sort_unstable_by(|x, y| x.nickname().cmp(y.nickname()));

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
        selected.load(&self.config, &self.db)
    }

    fn delete_selected_series(&mut self) -> Result<()> {
        let series = try_opt_r!(self.series.remove_selected());

        // Since we changed our selected series, we need to make sure the new one is initialized
        self.init_selected_series();

        series.config().delete(&self.db)?;
        Ok(())
    }
}

#[derive(Debug)]
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

struct UIWorld<'a, B: Backend> {
    backend: UIBackend<B>,
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
                $self.prompt.log.push(&err);
                Err(err)
            }
        }
    };
}

impl<'a, B> UIWorld<'a, B>
where
    B: Backend,
{
    fn init(args: &CmdOptions, backend: UIBackend<B>) -> Result<Self> {
        let mut prompt = Prompt::new();
        let remote = Self::init_remote(args, &mut prompt.log);

        let mut state = UIState::init(remote)?;

        let last_watched = LastWatched::load()?;
        let series_list = SeriesList::init(args, &mut state, &last_watched);

        Ok(Self {
            backend,
            state,
            prompt,
            series_list,
            main_panel: MainPanel::new(),
            episode_watcher: EpisodeWatcher::new(last_watched),
        })
    }

    fn init_remote(args: &CmdOptions, log: &mut Log) -> Remote {
        match crate::init_remote(args, true) {
            Ok(remote) => remote,
            Err(Error::MustAddAccount) => Remote::offline(),
            Err(err) => {
                log.push(err);
                log.push_context(
                    "enter user management with 'u' and add your account again if a new token is needed",
                );

                log.push_info("continuing in offline mode");
                Remote::offline()
            }
        }
    }

    fn exit(mut self) {
        self.backend.clear().ok();
    }

    fn tick(&mut self) {
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
        .context(err::IO)
    }

    /// Process a key input for all UI components.
    ///
    /// Returns true if the program should exit.
    fn process_key(&mut self, key: Key) -> bool {
        macro_rules! capture {
            ($result:expr) => {
                match capture_err!(self, $result) {
                    Ok(value) => value,
                    Err(_) => return false,
                }
            };
        }

        macro_rules! process_key {
            ($component:ident) => {
                capture!(self.$component.process_key(key, &mut self.state))
            };
        }

        match &self.state.current_action {
            CurrentAction::Idle => match key {
                Key::Char('q') => return true,
                Key::Char(key) if key == self.state.config.tui.keys.play_next_episode => {
                    capture!(self.episode_watcher.begin_watching_episode(&mut self.state))
                }
                Key::Char('a') => capture!(self.main_panel.switch_to_add_series(&mut self.state)),
                Key::Char('u') => self.main_panel.switch_to_user_panel(&mut self.state),
                Key::Char(COMMAND_KEY) => {
                    self.state.current_action = CurrentAction::EnteringCommand
                }
                _ => process_key!(series_list),
            },
            CurrentAction::WatchingEpisode(_, _) => (),
            CurrentAction::FocusedOnMainPanel => process_key!(main_panel),
            CurrentAction::EnteringCommand => match self.prompt.process_key(key, &mut self.state) {
                PromptResult::Ok => (),
                PromptResult::HasCommand(cmd) => capture!(self.process_command(cmd)),
                PromptResult::Error(err) => {
                    self.prompt.log.push(err);
                    return false;
                }
            },
        }

        false
    }

    fn process_command(&mut self, command: Command) -> Result<()> {
        let remote = &mut self.state.remote;
        let config = &self.state.config;
        let db = &self.state.db;

        match command {
            Command::Delete => self.state.delete_selected_series(),
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
            Command::Set(params) => {
                let status = try_opt_r!(self.state.series.selected_mut());

                match status {
                    SeriesStatus::Loaded(series) => {
                        series.update(params, config, db, remote)?;
                        series.save(db)?;
                        Ok(())
                    }
                    SeriesStatus::Error(_, _) => Ok(()),
                }
            }
            cmd @ Command::SyncFromRemote | cmd @ Command::SyncToRemote => {
                let series = try_opt_r!(self.state.series.valid_selection_mut());

                match cmd {
                    Command::SyncFromRemote => series.data.entry.force_sync_from_remote(remote)?,
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
                    None => return Err(Error::InvalidScore),
                };

                series.data.entry.set_score(score.map(|s| s as i16));
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
}

#[derive(Debug)]
pub struct Selection<T> {
    items: Vec<T>,
    index: WrappingIndex,
}

impl<T> Selection<T> {
    #[inline(always)]
    fn new(items: Vec<T>) -> Self {
        Self {
            items,
            index: WrappingIndex::new(0),
        }
    }

    #[inline(always)]
    fn index(&self) -> usize {
        self.index.get()
    }

    #[inline(always)]
    fn selected(&self) -> Option<&T> {
        if self.items.is_empty() {
            return None;
        }

        Some(&self.items[self.index])
    }

    #[inline(always)]
    fn selected_mut(&mut self) -> Option<&mut T> {
        if self.items.is_empty() {
            return None;
        }

        Some(&mut self.items[self.index])
    }

    #[inline(always)]
    fn inc_selected(&mut self) {
        self.index.increment(self.items.len())
    }

    #[inline(always)]
    fn dec_selected(&mut self) {
        self.index.decrement(self.items.len())
    }

    #[inline(always)]
    fn set_selected(&mut self, selected: usize) {
        if selected >= self.items.len() {
            return;
        }

        *self.index.get_mut() = selected;
    }

    #[inline(always)]
    fn push(&mut self, item: T) {
        self.items.push(item);
    }

    #[inline(always)]
    fn remove_selected(&mut self) -> Option<T> {
        self.remove_selected_with(|items, index| items.remove(index))
    }

    #[inline(always)]
    fn swap_remove_selected(&mut self) -> Option<T> {
        self.remove_selected_with(|items, index| items.swap_remove(index))
    }

    fn remove_selected_with<F>(&mut self, func: F) -> Option<T>
    where
        F: Fn(&mut Vec<T>, usize) -> T,
    {
        if self.items.is_empty() {
            return None;
        }

        let item = func(&mut self.items, self.index.get());

        if self.index == self.items.len() {
            self.index.decrement(self.items.len());
        }

        Some(item)
    }

    #[inline(always)]
    fn sort_unstable_by<F>(&mut self, compare: F)
    where
        F: FnMut(&T, &T) -> std::cmp::Ordering,
    {
        self.items.sort_unstable_by(compare)
    }

    #[inline(always)]
    fn iter(&self) -> impl Iterator<Item = &T> {
        self.items.iter()
    }
}

impl Selection<SeriesStatus> {
    #[inline(always)]
    fn valid_selection_mut(&mut self) -> Option<&mut Series> {
        self.selected_mut().and_then(SeriesStatus::loaded_mut)
    }
}

impl<T> Index<usize> for Selection<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        &self.items[index]
    }
}

impl<T> IndexMut<usize> for Selection<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.items[index]
    }
}

impl<T> From<Vec<T>> for Selection<T> {
    fn from(value: Vec<T>) -> Self {
        Self::new(value)
    }
}

#[derive(Copy, Clone, Debug)]
pub struct WrappingIndex(usize);

impl WrappingIndex {
    #[inline(always)]
    pub fn new(index: usize) -> Self {
        Self(index)
    }

    #[inline(always)]
    pub fn get(&self) -> usize {
        self.0
    }

    #[inline(always)]
    pub fn get_mut(&mut self) -> &mut usize {
        &mut self.0
    }

    #[inline(always)]
    fn increment(&mut self, max: usize) {
        self.0 = if max > 0 { (self.0 + 1) % max } else { max };
    }

    #[inline(always)]
    fn decrement(&mut self, max: usize) {
        self.0 = if self.0 == 0 {
            max.saturating_sub(1)
        } else {
            self.0 - 1
        }
    }
}

impl PartialEq<usize> for WrappingIndex {
    fn eq(&self, other: &usize) -> bool {
        self.get() == *other
    }
}

impl<T> Index<WrappingIndex> for Vec<T> {
    type Output = T;

    fn index(&self, index: WrappingIndex) -> &Self::Output {
        &self[index.get()]
    }
}

impl<T> IndexMut<WrappingIndex> for Vec<T> {
    fn index_mut(&mut self, index: WrappingIndex) -> &mut Self::Output {
        &mut self[index.get()]
    }
}

impl Into<usize> for WrappingIndex {
    fn into(self) -> usize {
        self.0
    }
}

pub enum SeriesStatus {
    Loaded(Series),
    Error(SeriesConfig, Error),
}

impl SeriesStatus {
    fn init(sconfig: SeriesConfig, config: &Config, db: &Database) -> Self {
        match Series::load_from_config(sconfig.clone(), config, db) {
            Ok(series) => Self::Loaded(series),
            Err(err) => Self::Error(sconfig, err),
        }
    }

    fn load(&mut self, config: &Config, db: &Database) {
        match self {
            Self::Loaded(_) => (),
            Self::Error(cfg, cur_err) => match Series::load_from_config(cfg.clone(), config, db) {
                Ok(series) => *self = Self::Loaded(series),
                Err(err) => *cur_err = err,
            },
        }
    }

    fn config(&self) -> &SeriesConfig {
        match self {
            Self::Loaded(series) => &series.data.config,
            Self::Error(cfg, _) => cfg,
        }
    }

    fn loaded_mut(&mut self) -> Option<&mut Series> {
        match self {
            Self::Loaded(series) => Some(series),
            Self::Error(_, _) => None,
        }
    }

    fn nickname(&self) -> &str {
        match self {
            Self::Loaded(series) => series.data.config.nickname.as_ref(),
            Self::Error(cfg, _) => cfg.nickname.as_ref(),
        }
    }
}

mod backend;
mod component;

use crate::config::Config;
use crate::database::Database;
use crate::err::{self, Error, Result};
use crate::file::TomlFile;
use crate::series::config::SeriesConfig;
use crate::series::info::{InfoResult, InfoSelector, SeriesInfo};
use crate::series::{LastWatched, Series, SeriesParams};
use crate::try_opt_r;
use crate::CmdOptions;
use anime::remote::RemoteService;
use backend::{TermionBackend, UIBackend, UIEvent, UIEvents};
use chrono::{DateTime, Duration, Utc};
use component::episode_watcher::EpisodeWatcher;
use component::info_panel::InfoPanel;
use component::prompt::command::Command;
use component::prompt::log::{LogItem, StatusLog};
use component::prompt::Prompt;
use component::series_list::SeriesList;
use component::{Component, Draw};
use snafu::ResultExt;
use std::borrow::Cow;
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
    remote: Box<dyn RemoteService>,
    db: Database,
}

impl UIState {
    fn init(remote: Box<dyn RemoteService>) -> Result<Self> {
        let config = Config::load_or_create()?;
        let db = Database::open()?;

        let series = SeriesConfig::load_all(&db)?
            .into_iter()
            .map(Into::into)
            .map(SeriesStatus::Unloaded)
            .collect::<Vec<_>>();

        Ok(Self {
            series: Selection::new(series),
            current_action: CurrentAction::default(),
            config,
            remote,
            db,
        })
    }

    fn add_series(&mut self, config: SeriesConfig, info: SeriesInfo) {
        let config: Cow<SeriesConfig> = Cow::Owned(config);

        let series = Series::from_remote(config.clone(), info, &self.config, self.remote.as_ref())
            .and_then(|series| {
                series.save(&self.db)?;
                Ok(series)
            });

        let status = SeriesStatus::from_series(series, config);
        let nickname = status.nickname().to_string();

        let existing_position = self.series.iter().position(|s| s.nickname() == nickname);

        if let Some(pos) = existing_position {
            self.series[pos] = status;
        } else {
            self.series.push(status);
            self.series
                .sort_unstable_by(|x, y| x.nickname().cmp(y.nickname()));
        }

        let selected = self
            .series
            .iter()
            .position(|s| s.nickname() == nickname)
            .unwrap_or(0);

        self.series.set_selected(selected);
    }

    fn init_selected_series(&mut self) {
        let selected = match self.series.selected_mut() {
            Some(selected) => selected,
            None => return,
        };

        selected.ensure_valid(&self.config, &self.db);
    }

    fn delete_selected_series(&mut self) -> Result<()> {
        let series = try_opt_r!(self.series.remove_selected());

        // Since we changed our selected series, we need to make sure the new one is initialized
        self.init_selected_series();

        Series::delete_by_name(&self.db, series.nickname())?;
        Ok(())
    }

    fn process_command(&mut self, command: Command) -> LogResult {
        match command {
            Command::Add(nickname, params) => LogResult::capture("Adding series", || {
                if self.remote.is_offline() {
                    return Err(Error::MustRunOnline);
                }

                let info = {
                    let sel = InfoSelector::from_params_or_name(&params, &nickname);
                    SeriesInfo::from_remote(sel, self.remote.as_ref())?
                };

                match info {
                    InfoResult::Confident(info) => {
                        let config =
                            SeriesConfig::from_params(nickname, &info, params, &self.config)?;

                        self.add_series(config, info);
                    }
                    InfoResult::Unconfident(info_list) => {
                        self.current_action =
                            CurrentAction::select_series(info_list, params, nickname);
                    }
                }

                Ok(())
            }),
            Command::AniList(token) => LogResult::capture("logging in to AniList", || {
                use anime::remote::anilist::AniList;
                use anime::remote::AccessToken;

                let token = match token {
                    Some(token) => {
                        let token = AccessToken::encode(token);
                        token.save()?;
                        token
                    }
                    None => match AccessToken::load() {
                        Ok(token) => token,
                        Err(err) if err.is_file_nonexistant() => {
                            return Err(Error::NeedAniListToken)
                        }
                        Err(err) => return Err(err),
                    },
                };

                self.remote = Box::new(AniList::authenticated(token)?);
                Ok(())
            }),
            Command::Delete => {
                LogResult::capture("deleting series", || self.delete_selected_series())
            }
            Command::Offline => {
                use anime::remote::offline::Offline;
                self.remote = Box::new(Offline::new());
                LogResult::Ok
            }
            Command::PlayerArgs(args) => LogResult::capture("setting series args", || {
                let series = try_opt_r!(self.series.valid_selection_mut());

                series.config.player_args = args.into();
                series.save(&self.db)
            }),
            Command::Progress(direction) => LogResult::capture("forcing watch progress", || {
                use component::prompt::command::ProgressDirection;

                let series = try_opt_r!(self.series.valid_selection_mut());
                let remote = self.remote.as_ref();

                match direction {
                    ProgressDirection::Forwards => {
                        series.episode_completed(remote, &self.config, &self.db)
                    }
                    ProgressDirection::Backwards => {
                        series.episode_regressed(remote, &self.config, &self.db)
                    }
                }
            }),
            Command::Set(params) => LogResult::capture("applying series parameters", || {
                // Note: we can't place this after getting the current series status due to borrow issues
                if let Some(id) = params.id {
                    if let Some(found) = self.series.iter().find(|&s| s.eq(&id)) {
                        return Err(Error::SeriesAlreadyExists {
                            name: found.nickname().into(),
                        });
                    }
                }

                let status = try_opt_r!(self.series.selected_mut());
                let remote = self.remote.as_ref();

                if params.id.is_some() && remote.is_offline() {
                    return Err(Error::MustBeOnlineTo {
                        reason: "set a new series id".into(),
                    });
                }

                match status {
                    SeriesStatus::Valid(series) => {
                        series.apply_parameters(params, &self.config, remote)?;
                        series.save(&self.db)?;
                    }
                    SeriesStatus::Invalid(cfg, _) => {
                        cfg.apply_params(&params, &self.config)?;
                        let cfg = Cow::Borrowed(cfg.as_ref());

                        let series = if params.id.is_some() {
                            let info = SeriesInfo::from_remote_by_id(cfg.id, remote)?;
                            Series::from_remote(cfg, info, &self.config, remote)
                        } else {
                            Series::load(cfg.into_owned(), &self.config, &self.db)
                        };

                        if let Ok(series) = &series {
                            series.save(&self.db)?;
                        }

                        status.update(series);
                    }
                    SeriesStatus::Unloaded(_) => (),
                }

                Ok(())
            }),
            cmd @ Command::SyncFromRemote | cmd @ Command::SyncToRemote => {
                LogResult::capture("syncing entry to/from remote", || {
                    let series = try_opt_r!(self.series.valid_selection_mut());
                    let remote = self.remote.as_ref();

                    match cmd {
                        Command::SyncFromRemote => series.entry.force_sync_from_remote(remote)?,
                        Command::SyncToRemote => series.entry.force_sync_to_remote(remote)?,
                        _ => unreachable!(),
                    }

                    series.save(&self.db)
                })
            }
            Command::Score(raw_score) => LogResult::capture("setting score", || {
                let series = try_opt_r!(self.series.valid_selection_mut());

                let score = match self.remote.parse_score(&raw_score) {
                    Some(score) if score == 0 => None,
                    Some(score) => Some(score),
                    None => return Err(Error::InvalidScore),
                };

                let remote = self.remote.as_ref();

                series.entry.set_score(score.map(|s| s as i16));
                series.entry.sync_to_remote(remote)?;
                series.save(&self.db)
            }),
            Command::Status(status) => LogResult::capture("setting series status", || {
                let series = try_opt_r!(self.series.valid_selection_mut());
                let remote = self.remote.as_ref();

                series.entry.set_status(status, &self.config);
                series.entry.sync_to_remote(remote)?;
                series.save(&self.db)
            }),
        }
    }
}

struct UIWorld<'a, B: Backend> {
    backend: UIBackend<B>,
    state: UIState,
    prompt: Prompt<'a>,
    series_list: SeriesList,
    info_panel: InfoPanel,
    episode_watcher: EpisodeWatcher,
}

macro_rules! impl_ui_component_fns {
    ($($comp_var:ident),+) => {
        impl<'a, B> UIWorld<'a, B> where B: Backend {
            fn tick_components(&mut self) -> LogResult {
                $(
                    if let err @ LogResult::Err(_, _) = self.$comp_var.tick(&mut self.state) {
                        return err;
                    }
                )+

                LogResult::Ok
            }

            fn process_key_for_components(&mut self, key: Key) -> LogResult {
                $(
                    if let err @ LogResult::Err(_, _) = self.$comp_var.process_key(key, &mut self.state) {
                        return err;
                    }
                )+

                LogResult::Ok
            }
        }
    };
}

impl_ui_component_fns!(prompt, series_list, info_panel, episode_watcher);

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
            info_panel: InfoPanel::new(),
            episode_watcher: EpisodeWatcher::new(last_watched),
        })
    }

    fn init_remote(args: &CmdOptions, log: &mut StatusLog) -> Box<dyn RemoteService> {
        use anime::remote::anilist;
        use anime::remote::offline::Offline;

        match crate::init_remote(args, true) {
            Ok(remote) => remote,
            Err(err) => {
                match err {
                    Error::NeedAniListToken => {
                        log.push(format!(
                            "No access token found. Go to {} \
                             and set your token with the 'anilist' command",
                            anilist::auth_url(crate::ANILIST_CLIENT_ID)
                        ));
                    }
                    _ => {
                        log.push(LogItem::failed("Logging in", err));
                        log.push(format!(
                            "If you need a new token, go to {} \
                             and set it with the 'anilist' command",
                            anilist::auth_url(crate::ANILIST_CLIENT_ID)
                        ));
                    }
                }

                log.push("Continuing in offline mode");

                Box::new(Offline::new())
            }
        }
    }

    fn exit(mut self) {
        self.backend.clear().ok();
    }

    fn tick(&mut self) {
        match self.tick_components() {
            LogResult::Ok => (),
            LogResult::Err(desc, err) => {
                self.prompt.log.push(LogItem::failed(desc, Some(err)));
            }
        }
    }

    fn draw_internal(&mut self) -> Result<()> {
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

            self.info_panel
                .draw(&self.state, info_panel_splitter[0], &mut frame);

            self.prompt
                .draw(&self.state, info_panel_splitter[1], &mut frame);
        })
        .context(err::IO)
    }

    fn draw(&mut self) -> Result<()> {
        self.draw_internal()?;

        self.prompt.after_draw(&mut self.backend);
        self.info_panel.after_draw(&mut self.backend);

        Ok(())
    }

    /// Process a key input for all UI components.
    ///
    /// Returns true if the program should exit.
    fn process_key(&mut self, key: Key) -> bool {
        if let Key::Char('q') = key {
            return true;
        }

        match &self.state.current_action {
            CurrentAction::Idle => (),
            CurrentAction::WatchingEpisode(_, _) => return false,
            action @ CurrentAction::SelectingSeries(_)
            | action @ CurrentAction::EnteringCommand => {
                let component: &mut dyn Component = match action {
                    CurrentAction::SelectingSeries(_) => &mut self.info_panel,
                    CurrentAction::EnteringCommand => &mut self.prompt,
                    _ => unreachable!(),
                };

                if let LogResult::Err(desc, err) = component.process_key(key, &mut self.state) {
                    self.prompt.log.push(LogItem::failed(desc, err));
                }

                return false;
            }
        }

        if let LogResult::Err(desc, err) = self.process_key_for_components(key) {
            self.prompt.log.push(LogItem::failed(desc, err));
        }

        false
    }
}

type ProgressTime = DateTime<Utc>;

#[derive(Debug)]
pub enum CurrentAction {
    Idle,
    WatchingEpisode(ProgressTime, process::Child),
    SelectingSeries(SelectingSeriesState),
    EnteringCommand,
}

impl CurrentAction {
    #[inline(always)]
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn select_series<I, S>(series_list: I, params: SeriesParams, nickname: S) -> Self
    where
        I: Into<Selection<SeriesInfo>>,
        S: Into<String>,
    {
        let state = SelectingSeriesState::new(series_list, params, nickname);
        Self::SelectingSeries(state)
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

#[derive(Debug)]
pub struct SelectingSeriesState {
    pub series_list: Selection<SeriesInfo>,
    pub params: SeriesParams,
    pub nickname: String,
}

impl SelectingSeriesState {
    fn new<I, S>(series_list: I, params: SeriesParams, nickname: S) -> Self
    where
        I: Into<Selection<SeriesInfo>>,
        S: Into<String>,
    {
        Self {
            series_list: series_list.into(),
            params,
            nickname: nickname.into(),
        }
    }
}

#[derive(Debug)]
pub struct Selection<T> {
    items: Vec<T>,
    selected: usize,
}

impl<T> Selection<T> {
    #[inline(always)]
    fn new(items: Vec<T>) -> Self {
        Self { items, selected: 0 }
    }

    #[inline(always)]
    fn index(&self) -> usize {
        self.selected
    }

    #[inline(always)]
    fn is_valid_index(&self, index: usize) -> bool {
        index < self.items.len()
    }

    #[inline(always)]
    fn selected(&self) -> Option<&T> {
        if self.items.is_empty() {
            return None;
        }

        Some(&self.items[self.selected])
    }

    #[inline(always)]
    fn selected_mut(&mut self) -> Option<&mut T> {
        if self.items.is_empty() {
            return None;
        }

        Some(&mut self.items[self.selected])
    }

    #[inline(always)]
    fn inc_selected(&mut self) {
        let new_index = self.selected + 1;

        if !self.is_valid_index(new_index) {
            return;
        }

        self.selected = new_index;
    }

    #[inline(always)]
    fn dec_selected(&mut self) {
        if self.selected == 0 {
            return;
        }

        self.selected -= 1;
    }

    #[inline(always)]
    fn set_selected(&mut self, selected: usize) {
        if !self.is_valid_index(selected) {
            return;
        }

        self.selected = selected;
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

        let item = func(&mut self.items, self.selected);

        if self.selected == self.items.len() {
            self.selected = self.selected.saturating_sub(1);
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
        self.selected_mut().and_then(SeriesStatus::get_valid_mut)
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

pub enum LogResult {
    Ok,
    Err(String, Error),
}

impl LogResult {
    fn err<S>(desc: S, err: Error) -> Self
    where
        S: Into<String>,
    {
        Self::Err(desc.into(), err)
    }

    fn capture<S, F>(context: S, func: F) -> Self
    where
        S: Into<String>,
        F: FnOnce() -> Result<()>,
    {
        match func() {
            Ok(_) => Self::Ok,
            Err(err) => Self::Err(context.into(), err),
        }
    }
}

type Reason = String;

pub enum SeriesStatus {
    Valid(Box<Series>),
    Invalid(Box<SeriesConfig>, Reason),
    Unloaded(Box<SeriesConfig>),
}

impl SeriesStatus {
    fn from_series(series: Result<Series>, config: Cow<SeriesConfig>) -> Self {
        match series {
            Ok(series) => Self::Valid(Box::new(series)),
            Err(err) => Self::Invalid(Box::new(config.into_owned()), Self::error_reason(err)),
        }
    }

    fn ensure_valid(&mut self, config: &Config, db: &Database) {
        match self {
            Self::Valid(_) | Self::Invalid(_, _) => (),
            Self::Unloaded(cfg) => {
                let series = Series::load(*cfg.clone(), config, db);
                self.update(series);
            }
        }
    }

    fn update(&mut self, series: Result<Series>) {
        use replace_with::replace_with_or_abort;

        replace_with_or_abort(self, |self_| match (self_, series) {
            (_, Ok(new_series)) => Self::Valid(Box::new(new_series)),
            (Self::Invalid(cfg, _), Err(err)) | (Self::Unloaded(cfg), Err(err)) => {
                Self::Invalid(cfg, Self::error_reason(err))
            }
            (Self::Valid(series), Err(err)) => {
                Self::Invalid(Box::new(series.config), Self::error_reason(err))
            }
        });
    }

    /// Get a concise reason for an error message.
    ///
    /// This is useful to avoid having error messages that are too verbose when the `SeriesStatus` is `Invalid`.
    fn error_reason(err: Error) -> String {
        match err {
            Error::Anime { source, .. } => format!("{}", source),
            err => format!("{}", err),
        }
    }

    fn get_valid_mut(&mut self) -> Option<&mut Series> {
        match self {
            Self::Valid(series) => Some(series),
            Self::Invalid(_, _) => None,
            Self::Unloaded(_) => None,
        }
    }

    fn nickname(&self) -> &str {
        match self {
            Self::Valid(series) => series.config.nickname.as_ref(),
            Self::Invalid(config, _) => config.nickname.as_ref(),
            Self::Unloaded(config) => config.nickname.as_ref(),
        }
    }
}

impl PartialEq<i32> for SeriesStatus {
    fn eq(&self, id: &i32) -> bool {
        match self {
            Self::Valid(series) => series.config.id == *id,
            Self::Invalid(_, _) | Self::Unloaded(_) => false,
        }
    }
}

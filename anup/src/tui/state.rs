use super::{component::episode_watcher::ProgressTime, selection::Selection};
use crate::config::Config;
use crate::database::Database;
use crate::file::SerializedFile;
use crate::series::config::SeriesConfig;
use crate::series::info::SeriesInfo;
use crate::series::{LoadedSeries, Series, SeriesData};
use crate::try_opt_ret;
use crate::user::Users;
use anime::local::SortedEpisodes;
use anime::remote::Remote;
use anyhow::{anyhow, Context, Result};
use std::process;
use std::{mem, ops::Deref};

pub struct UIState {
    pub series: Selection<LoadedSeries>,
    pub current_action: CurrentAction,
    pub config: Config,
    pub users: Users,
    pub remote: Remote,
    pub db: Database,
}

impl UIState {
    pub fn init(remote: Remote) -> Result<Self> {
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

    pub fn add_series<E>(
        &mut self,
        config: SeriesConfig,
        info: SeriesInfo,
        episodes: E,
    ) -> Result<()>
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

    pub fn init_selected_series(&mut self) {
        let selected = try_opt_ret!(self.series.selected_mut());
        selected.try_load(&self.config, &self.db)
    }

    pub fn delete_selected_series(&mut self) -> Result<LoadedSeries> {
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

pub type ReactiveState = Reactive<UIState>;

pub enum CurrentAction {
    Idle,
    WatchingEpisode(ProgressTime, process::Child),
    FocusedOnMainPanel,
    EnteringCommand,
}

impl CurrentAction {
    #[inline(always)]
    pub fn reset(&mut self) {
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

pub struct Reactive<T> {
    state: T,
    dirty: bool,
}

impl<T> Reactive<T> {
    pub fn new(state: T) -> Self {
        Self { state, dirty: true }
    }

    #[inline(always)]
    pub fn dirty(&self) -> bool {
        self.dirty
    }

    #[inline(always)]
    pub fn get(&self) -> &T {
        &self.state
    }

    pub fn get_mut(&mut self) -> &mut T {
        self.mark_dirty();
        &mut self.state
    }

    #[inline(always)]
    pub fn get_mut_unchanged(&mut self) -> &mut T {
        &mut self.state
    }

    #[inline(always)]
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    #[inline(always)]
    pub fn reset_dirty(&mut self) {
        self.dirty = false;
    }
}

impl<T> Deref for Reactive<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.get()
    }
}

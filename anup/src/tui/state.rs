use super::{component::prompt::log::Log, selection::Selection};
use crate::series::config::SeriesConfig;
use crate::series::info::SeriesInfo;
use crate::series::{LoadedSeries, Series, SeriesData};
use crate::try_opt_ret;
use crate::user::Users;
use crate::{config::Config, util::ArcMutex};
use crate::{database::Database, series::LastWatched};
use crate::{file::SerializedFile, key::Key};
use anime::local::SortedEpisodes;
use anime::remote::Remote;
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use crossterm::event::{Event, EventStream};
use futures::{select, FutureExt, StreamExt};
use std::{mem, ops::Deref, sync::Arc};
use tokio::{
    process::Child,
    signal::unix::{signal, Signal, SignalKind},
    sync::{broadcast, Notify},
    task,
};

pub struct UIState {
    pub series: Selection<LoadedSeries>,
    pub last_watched: LastWatched,
    pub input_state: InputState,
    pub events: broadcast::Sender<StateEvent>,
    pub log: Log<'static>,
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
        let last_watched = LastWatched::load().context("last watched series")?;

        let mut series = SeriesConfig::load_all(&db)
            .context("failed to load series configs")?
            .into_iter()
            .map(|sconfig| Series::load_from_config(sconfig, &config, &db))
            .collect::<Vec<_>>();

        series.sort_unstable();

        let (events_tx, _) = broadcast::channel(8);

        Ok(Self {
            series: Selection::new(series),
            last_watched,
            input_state: InputState::default(),
            events: events_tx,
            log: Log::new(15),
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

    async fn start_next_series_episode(state: &mut UIState) -> Result<(Child, ProgressTime)> {
        let series = match state.series.valid_selection_mut() {
            Some(series) => series,
            None => return Err(anyhow!("no series selected")),
        };

        let is_diff_series = state.last_watched.set(&series.data.config.nickname);

        if is_diff_series {
            state
                .last_watched
                .save()
                .context("setting last watched series")?;
        }

        series
            .begin_watching(&state.remote, &state.config, &state.db)
            .context("updating series status")?;

        let next_ep = series.data.entry.watched_episodes() + 1;

        let child = series
            .play_episode(next_ep as u32, &state.config)
            .context("playing episode")?;

        let progress_time = series.data.next_watch_progress_time(&state.config);

        Ok((child, progress_time))
    }

    async fn track_episode_finish(
        mut ep_process: Child,
        progress_time: ProgressTime,
        state: &ThreadedState,
    ) -> Result<()> {
        ep_process
            .wait()
            .await
            .context("waiting for episode to finish")?;

        let mut state = state.lock();
        let state = state.get_mut();

        state.input_state.reset();

        if Utc::now() < progress_time {
            return Ok(());
        }

        let series = if let Some(series) = state.series.valid_selection_mut() {
            series
        } else {
            return Ok(());
        };

        series
            .episode_completed(&state.remote, &state.config, &state.db)
            .context("marking episode as completed")
    }

    pub async fn play_next_series_episode(&mut self, threaded_state: &ThreadedState) -> Result<()> {
        let (ep_process, progress_time) = Self::start_next_series_episode(self).await?;

        self.events
            .send(StateEvent::StartedEpisode(progress_time))
            .ok();

        self.input_state = InputState::Locked;

        let threaded_state = Arc::clone(&threaded_state);

        task::spawn(async move {
            let result =
                Self::track_episode_finish(ep_process, progress_time, &threaded_state).await;

            let mut state = threaded_state.lock();
            let state = state.get_mut();

            if let Err(err) = result {
                state.log.push_error(&err);
            }

            state.input_state.reset();
            state.events.send(StateEvent::FinishedEpisode).ok();
        });

        Ok(())
    }
}

pub type ReactiveState = Reactive<UIState>;
pub type ThreadedState = ArcMutex<ReactiveState>;

#[derive(Clone, Copy)]
pub enum InputState {
    Idle,
    Locked,
    FocusedOnMainPanel,
    EnteringCommand,
}

impl InputState {
    #[inline(always)]
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

impl Default for InputState {
    fn default() -> Self {
        Self::Idle
    }
}

impl PartialEq for InputState {
    fn eq(&self, other: &Self) -> bool {
        mem::discriminant(self) == mem::discriminant(other)
    }
}

pub type ProgressTime = DateTime<Utc>;

#[derive(Debug, Clone)]
pub enum StateEvent {
    StartedEpisode(ProgressTime),
    FinishedEpisode,
}

pub struct Reactive<T> {
    state: T,
    pub dirty: Arc<Notify>,
}

impl<T> Reactive<T> {
    pub const fn new(state: T, dirty: Arc<Notify>) -> Self {
        Self { state, dirty }
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
    pub fn mark_dirty(&mut self) {
        self.dirty.notify_waiters()
    }
}

impl<T> Deref for Reactive<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.get()
    }
}

#[derive(Debug)]
pub enum UIEvent {
    Key(Key),
    StateChange,
    Resize,
}

pub enum UIErrorKind {
    ExitRequest,
    Other(anyhow::Error),
}

pub type UIEventError<T> = std::result::Result<T, UIErrorKind>;

pub struct UIEvents {
    reader: EventStream,
    resize_event_stream: Signal,
}

impl UIEvents {
    pub fn new() -> Result<Self> {
        let resize_event_stream =
            signal(SignalKind::window_change()).context("SIGWINCH signal capture failed")?;

        Ok(Self {
            reader: EventStream::new(),
            resize_event_stream,
        })
    }

    #[allow(clippy::mut_mut)]
    pub async fn next(&mut self, state_change: &Notify) -> UIEventError<Option<UIEvent>> {
        let state_change = state_change.notified().fuse();
        tokio::pin!(state_change);

        let window_resize = self.resize_event_stream.recv().fuse();
        tokio::pin!(window_resize);

        let mut next_event = self.reader.next().fuse();

        select! {
            _ = state_change => Ok(Some(UIEvent::StateChange)),
            _ = window_resize => Ok(Some(UIEvent::Resize)),
            event = next_event => match event {
                Some(Ok(Event::Key(key))) => Ok(Some(UIEvent::Key(Key::new(key)))),
                Some(Ok(_)) => Ok(None),
                Some(Err(err)) => Err(UIErrorKind::Other(err.into())),
                None => Err(UIErrorKind::ExitRequest),
            }
        }
    }
}

mod add_series;
mod delete_series;
mod info;
mod select_series;
mod split_series;
mod user_panel;

use super::Component;
use crate::series::info::InfoResult;
use crate::try_opt_r;
use crate::tui::state::{InputState, UIState};
use crate::{key::Key, series::config::SeriesConfig};
use crate::{series::SeriesParams, tui::state::SharedState};
use add_series::{AddSeriesPanel, AddSeriesResult};
use anime::local::SortedEpisodes;
use anime::remote::RemoteService;
use anyhow::{anyhow, Result};
use delete_series::DeleteSeriesPanel;
use info::InfoPanel;
use select_series::{SelectSeriesPanel, SelectSeriesResult, SelectState};
use split_series::{SplitPanelResult, SplitSeriesPanel};
use std::mem;
use tui::backend::Backend;
use tui::layout::Rect;
use tui::terminal::Frame;
use user_panel::UserPanel;

pub struct MainPanel {
    current: Panel,
    state: SharedState,
}

impl MainPanel {
    pub fn new(state: SharedState) -> Self {
        Self {
            current: Panel::info(&state),
            state,
        }
    }

    fn default_panel(&self) -> Panel {
        Panel::info(&self.state)
    }

    pub fn switch_to_add_series(&mut self, state: &mut UIState) -> Result<()> {
        let remote = state.remote.get_logged_in()?;

        if remote.is_offline() {
            return Err(anyhow!("must be online to add a series"));
        }

        self.current = Panel::add_series(state, &self.state)?;
        state.input_state = InputState::FocusedOnMainPanel;

        Ok(())
    }

    pub fn switch_to_update_series(&mut self, state: &mut UIState) -> Result<()> {
        self.current = Panel::update_series(state, &self.state)?;
        state.input_state = InputState::FocusedOnMainPanel;
        Ok(())
    }

    pub fn switch_to_delete_series(&mut self, state: &mut UIState) -> Result<()> {
        self.current = Panel::delete_series(state)?;
        state.input_state = InputState::FocusedOnMainPanel;
        Ok(())
    }

    fn switch_to_select_series(&mut self, select: SelectState, state: &mut UIState) {
        self.current = Panel::select_series(select);
        state.input_state = InputState::FocusedOnMainPanel;
    }

    pub fn switch_to_user_panel(&mut self, state: &mut UIState) {
        self.current = Panel::user(self.state.clone());
        state.input_state = InputState::FocusedOnMainPanel;
    }

    pub fn switch_to_split_series(&mut self, state: &mut UIState) -> Result<()> {
        let remote = state.remote.get_logged_in()?;

        if remote.is_offline() {
            return Err(anyhow!("must be online to split a series"));
        }

        let panel = Panel::split_series(&self.state);

        self.current = panel;
        state.input_state = InputState::FocusedOnMainPanel;
        Ok(())
    }

    fn add_partial_series(&mut self, series: PartialSeries, state: &mut UIState) -> Result<()> {
        match series.info {
            InfoResult::Confident(info) => {
                self.reset(state);

                let config = SeriesConfig::new(info.id, series.params, &state.db)?;
                state.add_series(config, info, series.episodes)?;

                Ok(())
            }
            InfoResult::Unconfident(info_list) => {
                let select = SelectState::new(info_list, series.params);
                self.switch_to_select_series(select, state);
                Ok(())
            }
        }
    }

    fn reset(&mut self, state: &mut UIState) {
        self.current = self.default_panel();
        state.input_state.reset();
    }

    pub fn draw<B: Backend>(&mut self, state: &UIState, rect: Rect, frame: &mut Frame<B>) {
        match &mut self.current {
            Panel::Info(info) => info.draw(state, rect, frame),
            Panel::AddSeries(add) => add.draw(rect, frame),
            Panel::SelectSeries(panel) => panel.draw(rect, frame),
            Panel::DeleteSeries(panel) => panel.draw(rect, frame),
            Panel::User(user) => user.draw(state, rect, frame),
            Panel::SplitSeries(split) => split.draw(rect, frame),
        }
    }
}

impl Component for MainPanel {
    type State = UIState;
    type KeyResult = Result<()>;

    fn process_key(&mut self, key: Key, state: &mut Self::State) -> Self::KeyResult {
        match &mut self.current {
            Panel::Info(_) => Ok(()),
            Panel::AddSeries(add) => match add.process_key(key, state) {
                Ok(AddSeriesResult::Ok) => Ok(()),
                Ok(AddSeriesResult::Reset) => {
                    self.reset(state);
                    Ok(())
                }
                Ok(AddSeriesResult::AddSeries(partial)) => self.add_partial_series(*partial, state),
                Ok(AddSeriesResult::UpdateSeries(params)) => {
                    let selected = try_opt_r!(state.series.selected_mut());
                    let remote = state.remote.get_logged_in()?;

                    selected.update(*params, &state.config, &state.db, remote)?;

                    self.reset(state);
                    Ok(())
                }
                Err(err) => Err(err),
            },
            Panel::SelectSeries(panel) => match panel.process_key(key, &mut ()) {
                SelectSeriesResult::Ok => Ok(()),
                SelectSeriesResult::AddSeries(info) => {
                    let default_panel = self.default_panel();

                    let params = match mem::replace(&mut self.current, default_panel) {
                        Panel::SelectSeries(panel) => panel.take_params(),
                        _ => unreachable!(),
                    };

                    let series = PartialSeries::new(InfoResult::Confident(info), params, None);
                    self.add_partial_series(series, state)?;

                    Ok(())
                }
                SelectSeriesResult::Reset => {
                    self.reset(state);
                    Ok(())
                }
            },
            Panel::DeleteSeries(panel) => match panel.process_key(key, state) {
                Ok(ShouldReset::Yes) => {
                    self.reset(state);
                    Ok(())
                }
                Ok(ShouldReset::No) => Ok(()),
                Err(err) => Err(err),
            },
            Panel::User(user) => match user.process_key(key, state) {
                Ok(ShouldReset::Yes) => {
                    self.reset(state);
                    Ok(())
                }
                Ok(ShouldReset::No) => Ok(()),
                Err(err) => Err(err),
            },
            Panel::SplitSeries(split) => match split.process_key(key, state) {
                Ok(SplitPanelResult::Ok) => Ok(()),
                Ok(SplitPanelResult::Reset) => {
                    self.reset(state);
                    Ok(())
                }
                Ok(SplitPanelResult::AddSeries(info, cfg)) => {
                    state.add_series(*cfg, (*info).into(), None)
                }
                Err(err) => Err(err),
            },
        }
    }
}

enum Panel {
    Info(InfoPanel),
    AddSeries(Box<AddSeriesPanel>),
    SelectSeries(SelectSeriesPanel),
    DeleteSeries(DeleteSeriesPanel),
    User(UserPanel),
    SplitSeries(SplitSeriesPanel),
}

impl Panel {
    #[inline(always)]
    fn info(state: &SharedState) -> Self {
        Self::Info(InfoPanel::new(state))
    }

    fn add_series(state: &UIState, shared_state: &SharedState) -> Result<Self> {
        use add_series::Mode;
        let panel = AddSeriesPanel::init(state, shared_state, Mode::AddSeries)?;
        Ok(Self::AddSeries(panel.into()))
    }

    fn update_series(state: &UIState, shared_state: &SharedState) -> Result<Self> {
        use add_series::Mode;
        let panel = AddSeriesPanel::init(state, shared_state, Mode::UpdateSeries)?;
        Ok(Self::AddSeries(panel.into()))
    }

    fn delete_series(state: &UIState) -> Result<Self> {
        let panel = DeleteSeriesPanel::init(state)?;
        Ok(Self::DeleteSeries(panel))
    }

    fn select_series(select: SelectState) -> Self {
        Self::SelectSeries(SelectSeriesPanel::new(select))
    }

    fn user(state: SharedState) -> Self {
        Self::User(UserPanel::new(state))
    }

    fn split_series(state: &SharedState) -> Self {
        let panel = SplitSeriesPanel::new(state);
        Self::SplitSeries(panel)
    }
}

#[derive(Copy, Clone)]
pub enum ShouldReset {
    Yes,
    No,
}

pub struct PartialSeries {
    info: InfoResult,
    params: SeriesParams,
    episodes: Option<SortedEpisodes>,
}

impl PartialSeries {
    #[inline(always)]
    fn new<E>(info: InfoResult, params: SeriesParams, episodes: E) -> Self
    where
        E: Into<Option<SortedEpisodes>>,
    {
        Self {
            info,
            params,
            episodes: episodes.into(),
        }
    }
}

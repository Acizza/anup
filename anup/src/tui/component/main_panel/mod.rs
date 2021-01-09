mod add_series;
mod delete_series;
mod info;
mod select_series;
mod split_series;
mod user_panel;

use super::{Component, Draw};
use crate::series::info::InfoResult;
use crate::series::SeriesParams;
use crate::try_opt_r;
use crate::tui::{CurrentAction, UIState};
use crate::{series::config::SeriesConfig, tui::backend::Key};
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
}

impl MainPanel {
    pub fn new() -> Self {
        Self {
            current: Panel::default(),
        }
    }

    pub fn switch_to_add_series(&mut self, state: &mut UIState) -> Result<()> {
        if state.remote.is_offline() {
            return Err(anyhow!("must be online to add a series"));
        }

        self.current = Panel::add_series(state)?;
        state.current_action = CurrentAction::FocusedOnMainPanel;

        Ok(())
    }

    pub fn switch_to_update_series(&mut self, state: &mut UIState) -> Result<()> {
        self.current = Panel::update_series(state)?;
        state.current_action = CurrentAction::FocusedOnMainPanel;
        Ok(())
    }

    pub fn switch_to_delete_series(&mut self, state: &mut UIState) -> Result<()> {
        self.current = Panel::delete_series(state)?;
        state.current_action = CurrentAction::FocusedOnMainPanel;
        Ok(())
    }

    fn switch_to_select_series(&mut self, select: SelectState, state: &mut UIState) {
        self.current = Panel::select_series(select);
        state.current_action = CurrentAction::FocusedOnMainPanel;
    }

    pub fn switch_to_user_panel(&mut self, state: &mut UIState) {
        self.current = Panel::user();
        state.current_action = CurrentAction::FocusedOnMainPanel;
    }

    pub fn switch_to_split_series(&mut self, state: &mut UIState) -> Result<()> {
        if state.remote.is_offline() {
            return Err(anyhow!("must be online to split a series"));
        }

        let panel = Panel::split_series();

        self.current = panel;
        state.current_action = CurrentAction::FocusedOnMainPanel;
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
        self.current = Panel::default();
        state.current_action.reset();
    }
}

impl Component for MainPanel {
    type State = UIState;
    type KeyResult = Result<()>;

    fn tick(&mut self, state: &mut UIState) -> Result<()> {
        macro_rules! capture {
            ($panel:expr) => {
                match $panel.tick(state) {
                    ok @ Ok(_) => ok,
                    err @ Err(_) => {
                        self.reset(state);
                        err
                    }
                }
            };
        }

        match &mut self.current {
            Panel::Info(_) => Ok(()),
            Panel::AddSeries(panel) => capture!(panel),
            Panel::SelectSeries(panel) => capture!(panel),
            Panel::DeleteSeries(panel) => capture!(panel),
            Panel::User(panel) => capture!(panel),
            Panel::SplitSeries(panel) => capture!(panel),
        }
    }

    fn process_key(&mut self, key: Key, state: &mut Self::State) -> Self::KeyResult {
        match &mut self.current {
            Panel::Info(_) => Ok(()),
            Panel::AddSeries(add) => match add.process_key(key, state) {
                Ok(AddSeriesResult::Ok) => Ok(()),
                Ok(AddSeriesResult::Reset) => {
                    self.reset(state);
                    Ok(())
                }
                Ok(AddSeriesResult::AddSeries(partial)) => {
                    self.add_partial_series(*partial, state)?;
                    Ok(())
                }
                Ok(AddSeriesResult::UpdateSeries(params)) => {
                    let selected = try_opt_r!(state.series.selected_mut());
                    selected.update(*params, &state.config, &state.db, &state.remote)?;
                    self.reset(state);
                    Ok(())
                }
                Err(err) => Err(err),
            },
            Panel::SelectSeries(panel) => match panel.process_key(key, &mut ()) {
                SelectSeriesResult::Ok => Ok(()),
                SelectSeriesResult::AddSeries(info) => {
                    let params = match mem::take(&mut self.current) {
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

impl<B> Draw<B> for MainPanel
where
    B: Backend,
{
    type State = UIState;

    fn draw(&mut self, state: &Self::State, rect: Rect, frame: &mut Frame<B>) {
        match &mut self.current {
            Panel::Info(info) => info.draw(state, rect, frame),
            Panel::AddSeries(add) => add.draw(&(), rect, frame),
            Panel::SelectSeries(panel) => panel.draw(&(), rect, frame),
            Panel::DeleteSeries(panel) => panel.draw(&(), rect, frame),
            Panel::User(user) => user.draw(state, rect, frame),
            Panel::SplitSeries(split) => split.draw(&(), rect, frame),
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
    fn info() -> Self {
        Self::Info(InfoPanel::new())
    }

    fn add_series(state: &UIState) -> Result<Self> {
        use add_series::Mode;
        let panel = AddSeriesPanel::init(state, Mode::AddSeries)?;
        Ok(Self::AddSeries(panel.into()))
    }

    fn update_series(state: &UIState) -> Result<Self> {
        use add_series::Mode;
        let panel = AddSeriesPanel::init(state, Mode::UpdateSeries)?;
        Ok(Self::AddSeries(panel.into()))
    }

    fn delete_series(state: &UIState) -> Result<Self> {
        let panel = DeleteSeriesPanel::init(state)?;
        Ok(Self::DeleteSeries(panel))
    }

    fn select_series(select: SelectState) -> Self {
        Self::SelectSeries(SelectSeriesPanel::new(select))
    }

    fn user() -> Self {
        Self::User(UserPanel::new())
    }

    fn split_series() -> Self {
        let panel = SplitSeriesPanel::new();
        Self::SplitSeries(panel)
    }
}

impl Default for Panel {
    fn default() -> Self {
        Self::info()
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

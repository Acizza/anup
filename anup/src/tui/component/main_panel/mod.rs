mod add_series;
mod info;
mod input;
pub mod select_series;

use super::{Component, Draw};
use crate::err::{Error, Result};
use crate::series::config::SeriesConfig;
use crate::series::info::InfoResult;
use crate::series::SeriesParams;
use crate::tui::{CurrentAction, UIBackend, UIState};
use add_series::{AddSeriesPanel, AddSeriesResult};
use anime::local::Episodes;
use info::InfoPanel;
use select_series::KeyResult;
use select_series::SelectSeriesPanel;
use select_series::SelectState;
use std::mem;
use termion::event::Key;
use tui::backend::Backend;
use tui::layout::Rect;
use tui::terminal::Frame;

pub struct MainPanel {
    current: Panel,
    cursor_needs_hiding: bool,
}

impl MainPanel {
    pub fn new() -> Self {
        Self {
            current: Panel::default(),
            cursor_needs_hiding: false,
        }
    }

    pub fn switch_to_add_series(&mut self, state: &mut UIState) -> Result<()> {
        if state.remote.is_offline() {
            return Err(Error::MustBeOnlineTo {
                reason: "add a series",
            });
        }

        self.current = Panel::add_series();
        state.current_action = CurrentAction::FocusedOnMainPanel;

        Ok(())
    }

    pub fn switch_to_select_series(&mut self, select: SelectState, state: &mut UIState) {
        self.current = Panel::select_series(select);
        state.current_action = CurrentAction::FocusedOnMainPanel;
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
        match &mut self.current {
            Panel::AddSeries(add) => add.tick(state),
            _ => Ok(()),
        }
    }

    fn process_key(&mut self, key: Key, state: &mut Self::State) -> Self::KeyResult {
        match &mut self.current {
            Panel::Info(_) => Ok(()),
            Panel::AddSeries(add) => match add.process_key(key, state) {
                AddSeriesResult::Ok => Ok(()),
                AddSeriesResult::Reset => {
                    self.cursor_needs_hiding = true;
                    self.reset(state);
                    Ok(())
                }
                AddSeriesResult::AddSeries(partial) => {
                    self.add_partial_series(*partial, state)?;
                    Ok(())
                }
                AddSeriesResult::Error(err) => Err(err),
            },
            Panel::SelectSeries(panel) => match panel.process_key(key, &mut ()) {
                KeyResult::Ok => Ok(()),
                KeyResult::AddSeries(info) => {
                    let params = match mem::take(&mut self.current) {
                        Panel::SelectSeries(panel) => panel.take_params(),
                        _ => unreachable!(),
                    };

                    let series = PartialSeries::new(InfoResult::Confident(info), params, None);
                    self.add_partial_series(series, state)?;

                    Ok(())
                }
                KeyResult::Reset => {
                    self.reset(state);
                    Ok(())
                }
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
        }
    }

    fn after_draw(&mut self, backend: &mut UIBackend<B>, _: &Self::State) {
        if let Panel::AddSeries(add) = &mut self.current {
            add.after_draw(backend, &());
        }

        if self.cursor_needs_hiding {
            backend.hide_cursor().ok();
            self.cursor_needs_hiding = false;
        }
    }
}

enum Panel {
    Info(InfoPanel),
    AddSeries(Box<AddSeriesPanel>),
    SelectSeries(SelectSeriesPanel),
}

impl Panel {
    #[inline(always)]
    fn info() -> Self {
        Self::Info(InfoPanel::new())
    }

    #[inline(always)]
    fn add_series() -> Self {
        Self::AddSeries(Box::new(AddSeriesPanel::new()))
    }

    #[inline(always)]
    fn select_series(select: SelectState) -> Self {
        Self::SelectSeries(SelectSeriesPanel::new(select))
    }
}

impl Default for Panel {
    fn default() -> Self {
        Self::info()
    }
}

#[derive(Debug)]
pub struct PartialSeries {
    info: InfoResult,
    params: SeriesParams,
    episodes: Option<Episodes>,
}

impl PartialSeries {
    #[inline(always)]
    fn new<E>(info: InfoResult, params: SeriesParams, episodes: E) -> Self
    where
        E: Into<Option<Episodes>>,
    {
        Self {
            info,
            params,
            episodes: episodes.into(),
        }
    }
}
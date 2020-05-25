mod info;
mod select_series;

use super::{Component, Draw, ShouldReset};
use crate::err::Result;
use crate::series::config::SeriesConfig;
use crate::series::info::SeriesInfo;
use crate::series::SeriesParams;
use crate::tui::{Selection, UIState};
use info::SeriesInfoPanel;
use select_series::{SelectInputResult, SelectSeriesPanel, SelectState};
use std::mem;
use std::path::PathBuf;
use termion::event::Key;
use tui::backend::Backend;
use tui::layout::Rect;
use tui::terminal::Frame;

impl Default for Panel {
    fn default() -> Self {
        Self::info()
    }
}

pub struct MainPanel {
    selected: Panel,
}

impl MainPanel {
    pub fn new() -> Self {
        Self {
            selected: Panel::Info(SeriesInfoPanel::new()),
        }
    }

    pub fn switch_to_select_series<I, S>(
        &mut self,
        series_list: I,
        params: SeriesParams,
        path: PathBuf,
        nickname: S,
    ) where
        I: Into<Selection<SeriesInfo>>,
        S: Into<String>,
    {
        let state = SelectState::new(series_list, params, path, nickname);
        self.selected = Panel::select_series(state);
    }
}

impl Component for MainPanel {
    type TickResult = ();
    type KeyResult = ShouldReset;

    fn process_key(&mut self, key: Key, state: &mut UIState) -> Result<Self::KeyResult> {
        match &mut self.selected {
            Panel::Info(_) => Ok(ShouldReset::No),
            Panel::SelectSeries(select, select_state) => {
                match select.process_key(key, select_state) {
                    SelectInputResult::Continue => Ok(ShouldReset::No),
                    SelectInputResult::Finish => Ok(ShouldReset::Yes),
                    SelectInputResult::AddSeries(info) => {
                        let select = match mem::take(&mut self.selected) {
                            Panel::SelectSeries(_, state) => state,
                            _ => unreachable!(),
                        };

                        let config = SeriesConfig::from_params(
                            select.nickname,
                            info.id,
                            select.path,
                            select.params,
                            &state.config,
                            &state.db,
                        )?;

                        state.add_series(config, info)?;
                        Ok(ShouldReset::Yes)
                    }
                }
            }
        }
    }
}

impl<B> Draw<B> for MainPanel
where
    B: Backend,
{
    fn draw(&mut self, state: &UIState, rect: Rect, frame: &mut Frame<B>) {
        match &mut self.selected {
            Panel::Info(info) => info.draw(state, rect, frame),
            Panel::SelectSeries(select, select_state) => select.draw(select_state, rect, frame),
        }
    }
}

enum Panel {
    Info(SeriesInfoPanel),
    SelectSeries(SelectSeriesPanel, SelectState),
}

impl Panel {
    fn info() -> Self {
        Self::Info(SeriesInfoPanel::new())
    }

    fn select_series(state: SelectState) -> Self {
        Self::SelectSeries(SelectSeriesPanel::new(), state)
    }
}

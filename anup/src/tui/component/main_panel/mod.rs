mod info;
pub mod select_series;

use super::{Component, Draw};
use crate::err::Result;
use crate::series::config::SeriesConfig;
use crate::series::info::SeriesInfo;
use crate::tui::{CurrentAction, UIBackend, UIState};
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

    pub fn switch_to_select_series(&mut self, select: SelectState, state: &mut UIState) {
        self.current = Panel::select_series(select);
        state.current_action = CurrentAction::FocusedOnMainPanel;
    }

    fn add_series(&mut self, info: SeriesInfo, state: &mut UIState) -> Result<()> {
        let select = match mem::take(&mut self.current) {
            Panel::SelectSeries(_, select) => select,
            _ => return Ok(()),
        };

        state.current_action.reset();

        let config = SeriesConfig::from_params(
            select.nickname,
            info.id,
            select.path,
            select.params,
            &state.config,
            &state.db,
        )?;

        state.add_series(config, info)
    }

    fn reset(&mut self, state: &mut UIState) {
        self.current = Panel::default();
        state.current_action.reset();
    }
}

impl Component for MainPanel {
    type State = UIState;
    type KeyResult = Result<()>;

    fn process_key(&mut self, key: Key, state: &mut Self::State) -> Self::KeyResult {
        match &mut self.current {
            Panel::Info(_) => Ok(()),
            Panel::SelectSeries(panel, select) => match panel.process_key(key, select) {
                KeyResult::Ok => Ok(()),
                KeyResult::AddSeries(info) => {
                    let result = self.add_series(info, state);
                    self.reset(state);
                    result
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
            Panel::SelectSeries(panel, select) => panel.draw(select, rect, frame),
        }
    }

    fn after_draw(&mut self, backend: &mut UIBackend<B>, _: &Self::State) {
        if self.cursor_needs_hiding {
            backend.hide_cursor().ok();
            self.cursor_needs_hiding = false;
        }
    }
}

enum Panel {
    Info(InfoPanel),
    SelectSeries(SelectSeriesPanel, SelectState),
}

impl Panel {
    #[inline(always)]
    fn info() -> Self {
        Self::Info(InfoPanel::new())
    }

    #[inline(always)]
    fn select_series(select: SelectState) -> Self {
        Self::SelectSeries(SelectSeriesPanel::new(), select)
    }
}

impl Default for Panel {
    fn default() -> Self {
        Self::info()
    }
}

pub mod episode_watcher;
pub mod main_panel;
pub mod prompt;
pub mod series_list;

use super::{UIBackend, UIState};
use crate::err::Result;
use termion::event::Key;
use tui::backend::Backend;
use tui::layout::Rect;
use tui::terminal::Frame;

pub trait Component {
    type TickResult: Default;
    type KeyResult: Default;

    fn tick(&mut self, _: &mut UIState) -> Result<Self::TickResult> {
        Ok(Self::TickResult::default())
    }

    fn process_key(&mut self, _: Key, _: &mut UIState) -> Result<Self::KeyResult> {
        Ok(Self::KeyResult::default())
    }
}

pub trait Draw<B>
where
    B: Backend,
{
    fn draw(&mut self, state: &UIState, rect: Rect, frame: &mut Frame<B>);
    fn after_draw(&mut self, _: &mut UIBackend<B>) {}
}

pub enum ShouldReset {
    No,
    Yes,
}

impl Default for ShouldReset {
    fn default() -> Self {
        Self::No
    }
}

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
    type State;
    type KeyResult;

    fn tick(&mut self, _: &mut UIState) -> Result<()> {
        Ok(())
    }

    fn process_key(&mut self, _: Key, _: &mut Self::State) -> Self::KeyResult;
}

pub trait Draw<B>
where
    B: Backend,
{
    type State;

    fn draw(&mut self, state: &Self::State, rect: Rect, frame: &mut Frame<B>);
    fn after_draw(&mut self, _: &mut UIBackend<B>, _: &Self::State) {}
}

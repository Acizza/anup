pub mod episode_watcher;
pub mod info_panel;
pub mod prompt;
pub mod series_list;

use super::{LogResult, UIBackend, UIState};
use termion::event::Key;
use tui::backend::Backend;
use tui::layout::Rect;
use tui::terminal::Frame;

pub trait Component {
    fn tick(&mut self, _: &mut UIState) -> LogResult {
        LogResult::Ok
    }

    fn process_key(&mut self, _: Key, _: &mut UIState) -> LogResult {
        LogResult::Ok
    }
}

pub trait Draw<B>
where
    B: Backend,
{
    fn draw(&mut self, state: &UIState, rect: Rect, frame: &mut Frame<B>);
    fn after_draw(&mut self, _: &mut UIBackend<B>) {}
}

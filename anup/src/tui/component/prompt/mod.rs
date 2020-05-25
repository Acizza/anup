pub mod command;
pub mod log;

use super::{Component, Draw};
use crate::err::Error;
use crate::tui::{CurrentAction, UIBackend, UIState};
use command::{Command, CommandPrompt, InputResult};
use log::Log;
use termion::event::Key;
use tui::backend::Backend;
use tui::layout::Rect;
use tui::terminal::Frame;

pub const COMMAND_KEY: char = ':';

pub struct Prompt<'a> {
    pub log: Log<'a>,
    command: CommandPrompt,
    draw_rect: Rect,
}

impl<'a> Prompt<'a> {
    pub fn new() -> Self {
        Self {
            log: Log::new(),
            command: CommandPrompt::new(),
            draw_rect: Rect::default(),
        }
    }

    fn reset(&mut self, state: &mut UIState) {
        self.command.reset();
        state.current_action.reset();
    }
}

impl<'a> Component for Prompt<'a> {
    type State = UIState;
    type KeyResult = PromptResult;

    fn process_key(&mut self, key: Key, state: &mut Self::State) -> Self::KeyResult {
        match &mut state.current_action {
            CurrentAction::EnteringCommand => match self.command.process_key(key, &mut ()) {
                InputResult::Done => {
                    self.reset(state);
                    PromptResult::Ok
                }
                InputResult::Command(cmd) => {
                    self.reset(state);
                    PromptResult::HasCommand(cmd)
                }
                InputResult::Continue => PromptResult::Ok,
                InputResult::Error(err) => {
                    self.reset(state);
                    PromptResult::Error(err)
                }
            },
            _ => PromptResult::Ok,
        }
    }
}

impl<'a, B> Draw<B> for Prompt<'a>
where
    B: Backend,
{
    type State = UIState;

    fn draw(&mut self, state: &Self::State, rect: Rect, frame: &mut Frame<B>) {
        self.draw_rect = rect;

        match &state.current_action {
            CurrentAction::EnteringCommand => self.command.draw(&(), rect, frame),
            _ => self.log.draw(&(), rect, frame),
        }
    }

    fn after_draw(&mut self, backend: &mut UIBackend<B>, state: &UIState) {
        match &state.current_action {
            CurrentAction::EnteringCommand if backend.will_cursor_fit(self.draw_rect) => {
                if !backend.cursor_visible {
                    backend.show_cursor().ok();
                }

                backend
                    .set_cursor_inside(self.command.width() as u16, self.draw_rect)
                    .ok();
            }
            _ => {
                if backend.cursor_visible {
                    backend.hide_cursor().ok();
                }
            }
        }
    }
}

pub enum PromptResult {
    Ok,
    HasCommand(Command),
    Error(Error),
}

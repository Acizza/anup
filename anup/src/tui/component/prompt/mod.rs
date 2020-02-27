pub mod command;
pub mod log;

use super::{Component, Draw};
use crate::tui::{CurrentAction, LogResult, UIBackend, UIState};
use command::{CommandPrompt, InputResult};
use log::StatusLog;
use termion::event::Key;
use tui::backend::Backend;
use tui::layout::Rect;
use tui::terminal::Frame;

const COMMAND_KEY: char = ':';

pub struct Prompt<'a> {
    pub log: StatusLog<'a>,
    command: CommandPrompt,
    state: PromptState,
    draw_rect: Rect,
}

impl<'a> Prompt<'a> {
    pub fn new() -> Self {
        Self {
            log: StatusLog::new(),
            command: CommandPrompt::new(),
            state: PromptState::default(),
            draw_rect: Rect::default(),
        }
    }

    fn reset(&mut self, state: &mut UIState) {
        self.state = PromptState::default();
        self.command.reset();
        state.current_action.reset();
    }
}

impl<'a> Component for Prompt<'a> {
    fn process_key(&mut self, key: Key, state: &mut UIState) -> LogResult {
        if let Key::Char(COMMAND_KEY) = key {
            self.state = PromptState::Command;
            state.current_action = CurrentAction::EnteringCommand;

            return LogResult::Ok;
        }

        match &mut self.state {
            PromptState::Log => LogResult::Ok,
            PromptState::Command => match self.command.process_key(key) {
                Ok(InputResult::Done) => {
                    self.reset(state);
                    LogResult::Ok
                }
                Ok(InputResult::Command(cmd)) => {
                    self.reset(state);
                    state.process_command(cmd)
                }
                Ok(InputResult::Continue) => LogResult::Ok,
                Err(err) => {
                    self.reset(state);
                    LogResult::err("processing command key", err)
                }
            },
        }
    }
}

impl<'a, B> Draw<B> for Prompt<'a>
where
    B: Backend,
{
    fn draw(&mut self, _: &UIState, rect: Rect, frame: &mut Frame<B>) {
        self.draw_rect = rect;

        match &mut self.state {
            PromptState::Log => self.log.draw(rect, frame),
            PromptState::Command => self.command.draw(rect, frame),
        }
    }

    fn after_draw(&mut self, backend: &mut UIBackend<B>) {
        match &self.state {
            PromptState::Command if backend.will_cursor_fit(self.draw_rect) => {
                if !backend.cursor_visible {
                    backend.show_cursor().ok();
                }

                backend
                    .set_cursor_inside(self.command.width() as u16, self.draw_rect)
                    .ok();
            }
            PromptState::Command | PromptState::Log if backend.cursor_visible => {
                backend.hide_cursor().ok();
            }
            PromptState::Command | PromptState::Log => (),
        }
    }
}

enum PromptState {
    Log,
    Command,
}

impl Default for PromptState {
    fn default() -> Self {
        Self::Log
    }
}

pub mod command;
pub mod log;

use super::{Component, Draw};
use crate::err::Result;
use crate::tui::{UIBackend, UIState};
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
    state: PromptState,
    draw_rect: Rect,
}

impl<'a> Prompt<'a> {
    pub fn new() -> Self {
        Self {
            log: Log::new(),
            command: CommandPrompt::new(),
            state: PromptState::default(),
            draw_rect: Rect::default(),
        }
    }

    fn reset(&mut self) {
        self.state = PromptState::default();
        self.command.reset();
    }

    pub fn switch_to_command_entry(&mut self) {
        self.state = PromptState::Command;
    }
}

impl<'a> Component for Prompt<'a> {
    type TickResult = ();
    type KeyResult = KeyResult;

    fn process_key(&mut self, key: Key, _: &mut UIState) -> Result<Self::KeyResult> {
        match &mut self.state {
            PromptState::Log => Ok(KeyResult::Ok),
            PromptState::Command => match self.command.process_key(key) {
                Ok(InputResult::Done) => {
                    self.reset();
                    Ok(KeyResult::Reset)
                }
                Ok(InputResult::Command(cmd)) => {
                    self.reset();
                    Ok(KeyResult::HasCommand(cmd))
                }
                Ok(InputResult::Continue) => Ok(KeyResult::Ok),
                Err(err) => {
                    self.reset();
                    Err(err)
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

pub enum KeyResult {
    Ok,
    HasCommand(Command),
    Reset,
}

impl Default for KeyResult {
    fn default() -> Self {
        Self::Ok
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

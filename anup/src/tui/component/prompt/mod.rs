pub mod command;
pub mod log;

use super::{Component, Draw};
use crate::tui::{CurrentAction, UIState};
use anyhow::Result;
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
    type KeyResult = Result<PromptResult>;

    fn process_key(&mut self, key: Key, state: &mut Self::State) -> Self::KeyResult {
        match &mut state.current_action {
            CurrentAction::EnteringCommand => match self.command.process_key(key, state) {
                Ok(InputResult::Done) => {
                    self.reset(state);
                    Ok(PromptResult::Ok)
                }
                Ok(InputResult::Command(cmd)) => {
                    self.reset(state);
                    Ok(PromptResult::HasCommand(cmd))
                }
                Ok(InputResult::Continue) => Ok(PromptResult::Ok),
                Err(err) => {
                    self.reset(state);
                    Err(err)
                }
            },
            _ => Ok(PromptResult::Ok),
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
}

pub enum PromptResult {
    Ok,
    HasCommand(Command),
}

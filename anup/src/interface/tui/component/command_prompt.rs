use crate::err::{self, Result};
use crate::util;
use smallvec::{smallvec, SmallVec};
use snafu::ensure;
use std::convert::TryFrom;
use std::result;
use termion::event::Key;
use tui::style::{Color, Style};
use tui::widgets::Text;

/// A prompt to enter commands in that provides suggestions.
pub struct CommandPrompt<'a> {
    buffer: String,
    hint_text: Option<&'a str>,
}

impl<'a> CommandPrompt<'a> {
    /// Create a new `CommandPrompt`.
    pub fn new() -> CommandPrompt<'a> {
        CommandPrompt {
            buffer: String::new(),
            hint_text: None,
        }
    }

    /// Process a key for the `CommandPrompt`.
    ///
    /// Returns a `PromptResult` representing the state of the prompt based off of `key`.
    /// Example return values are given below:
    ///
    /// | `key`             | `PromptResult`             |
    /// | ----------------- | -------------------------- |
    /// | `Key::Char('a')`  | `PromptResult::NotDone`    |
    /// | `Key::Esc`        | `PromptResult::Done`       |
    /// | `Key::Char('\n')` | `PromptResult::Command(_)` |
    pub fn process_key(&mut self, key: Key) -> Result<PromptResult> {
        match key {
            Key::Char('\n') => {
                let command = Command::try_from(self.buffer.as_ref())?;
                self.buffer.clear();
                return Ok(PromptResult::Command(command));
            }
            Key::Char('\t') => {
                if let Some(hint_text) = self.hint_text {
                    self.buffer.push_str(hint_text);
                    self.buffer.push(' ');
                    self.hint_text = None;
                }
            }
            Key::Char(ch) => {
                self.buffer.push(ch);

                if let Some(matching_cmd) = Command::best_matching_name(&self.buffer) {
                    if self.buffer.len() <= matching_cmd.len() {
                        let visible_slice = &matching_cmd[self.buffer.len()..];
                        self.hint_text = Some(visible_slice);
                    }
                } else {
                    self.hint_text = None;
                }
            }
            Key::Backspace => {
                self.buffer.pop();
                self.hint_text = None;
            }
            Key::Esc => {
                self.buffer.clear();
                return Ok(PromptResult::Done);
            }
            _ => (),
        }

        Ok(PromptResult::NotDone)
    }

    /// The items of the `CommandPrompt` in a form ready for drawing.
    pub fn draw_items<'b>(&'b self) -> SmallVec<[Text<'b>; 2]> {
        let mut text = smallvec![Text::raw(&self.buffer)];

        if let Some(hint_text) = self.hint_text {
            text.push(Text::styled(
                hint_text,
                Style::default().fg(Color::DarkGray),
            ));
        }

        text
    }
}

/// The result of processing a key in a `CommandPrompt`.
pub enum PromptResult {
    /// A successfully parsed command.
    Command(Command),
    /// Input is considered completed without a command.
    Done,
    /// More input is needed to reach a `Command(_)` or `Done` state.
    NotDone,
}

macro_rules! impl_command_matching {
    ($enum_name:ident, $num_cmds:expr, $($field:pat => { name: $name:expr, min_args: $min_args:expr, fn: $parse_fn:expr, },)+) => {
        impl $enum_name {
            const CMD_NAMES: [&'static str; $num_cmds] = [
                $($name,)+
            ];
        }

        impl TryFrom<&str> for $enum_name {
            type Error = err::Error;

            fn try_from(value: &str) -> result::Result<Self, Self::Error> {
                let fragments = value.split_whitespace().collect::<SmallVec<[&str; 3]>>();

                ensure!(!fragments.is_empty(), err::NoCommandSpecified);

                let name = fragments[0].to_ascii_lowercase();
                let args = if fragments.len() > 1 {
                    &fragments[1..]
                } else {
                    &[]
                };

                match name.as_ref() {
                    $($name => {
                        #[allow(unused_comparisons)]
                        let has_min_args = args.len() >= $min_args;

                        ensure!(
                            has_min_args,
                            err::NotEnoughArguments {
                                has: args.len(),
                                need: $min_args as usize,
                            }
                        );

                        $parse_fn(args)
                    },)+
                    _ => Err(err::Error::CommandNotFound {
                        command: value.into(),
                    }),
                }
            }
        }
    };
}

/// A parsed command with its arguments.
#[derive(Debug, Clone)]
pub enum Command {
    /// Syncronize the selected season to the remote service.
    SyncFromRemote,
    /// Syncronize the selected season from the remote service.
    SyncToRemote,
    /// Set the watch status of the selected season.
    Status(anime::remote::Status),
    /// Increment / decrement the watched episodes of the selected season.
    Progress(ProgressDirection),
    /// Rate the selected season.
    Score(String),
    /// Specify the video player arguments for the selected season.
    PlayerArgs(Vec<String>),
}

impl_command_matching!(Command, 6,
    SyncFromRemote => {
        name: "syncfromremote",
        min_args: 0,
        fn: |_| Ok(Command::SyncFromRemote),
    },
    SyncToRemote => {
        name: "synctoremote",
        min_args: 0,
        fn: |_| Ok(Command::SyncToRemote),
    },
    Status(_) => {
        name: "status",
        min_args: 1,
        fn: |args: &[&str]| {
            use anime::remote::Status;

            let status = match args[0].to_ascii_lowercase().as_ref() {
                "w" | "watching" => Status::Watching,
                "c" | "completed" => Status::Completed,
                "h" | "hold" => Status::OnHold,
                "d" | "drop" => Status::Dropped,
                "p" | "plan" => Status::PlanToWatch,
                "r" | "rewatch" => Status::Rewatching,
                _ => {
                    return Err(err::Error::UnknownCmdPromptArg {
                        value: args[0].into(),
                    })
                }
            };

            Ok(Command::Status(status))
        },
    },
    Progress(_) => {
        name: "progress",
        min_args: 1,
        fn: |args: &[&str]| {
            let dir = ProgressDirection::try_from(args[0])?;
            Ok(Command::Progress(dir))
        },
    },
    Score(_) => {
        name: "rate",
        min_args: 1,
        fn: |args: &[&str]| {
            let score = args[0].into();
            Ok(Command::Score(score))
        },
    },
    PlayerArgs(_) => {
        name: "args",
        min_args: 0,
        fn: |args: &[&str]| {
            let args = args.iter()
                .map(|frag| frag.to_string())
                .collect();

            Ok(Command::PlayerArgs(args))
        },
    },
);

impl Command {
    /// Returns the command most similar to `name`.
    ///
    /// `None` will be returned if `name` does not match a command with
    /// at least 70% similarity.
    fn best_matching_name(name: &str) -> Option<&'static str> {
        const MIN_CONFIDENCE: f32 = 0.7;

        util::closest_str_match(
            &Command::CMD_NAMES,
            name,
            MIN_CONFIDENCE,
            strsim::jaro_winkler,
        )
    }
}

/// Indicates which way to advance the episode count of a season.
#[derive(Debug, Copy, Clone)]
pub enum ProgressDirection {
    /// Increase the episode count.
    Forwards,
    /// Decrease the episode count.
    Backwards,
}

impl TryFrom<&str> for ProgressDirection {
    type Error = err::Error;

    fn try_from(value: &str) -> result::Result<Self, Self::Error> {
        match value {
            "f" | "forward" => Ok(ProgressDirection::Forwards),
            "b" | "backward" => Ok(ProgressDirection::Backwards),
            _ => Err(err::Error::UnknownCmdPromptArg {
                value: value.into(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commands() {
        use anime::remote::Status;

        let mut prompt = CommandPrompt::new();

        let mut enter_command = |name: &str| {
            for ch in name.chars() {
                match prompt.process_key(Key::Char(ch)) {
                    Ok(PromptResult::NotDone) => (),
                    Ok(PromptResult::Done) => panic!("expected {} command, got nothing", name),
                    Ok(PromptResult::Command(cmd)) => return cmd,
                    Err(err) => panic!("error processing command: {}", err),
                }
            }

            panic!("unexpected finish when entering \"{}\"", name);
        };

        macro_rules! expected {
            ($got:expr, $wanted:expr) => {
                panic!("got unexpected command: {:?}, wanted  {:?}", $got, $wanted)
            };
        }

        macro_rules! test_command {
            ($name:expr, $should_eq:pat) => {
                match enter_command($name) {
                    $should_eq => (),
                    other => expected!(other, stringify!($should_eq)),
                }
            };
        }

        test_command!("synctoremote\n", Command::SyncToRemote);
        test_command!("syncfromremote\n", Command::SyncFromRemote);
        test_command!("status watching\n", Command::Status(Status::Watching));
        test_command!(
            "progress forward\n",
            Command::Progress(ProgressDirection::Forwards)
        );

        let expected_args: Vec<String> = vec!["arg1".into(), "arg2".into()];

        match enter_command("args arg1 arg2\n") {
            Command::PlayerArgs(args) => {
                if args.len() != expected_args.len() {
                    expected!(
                        Command::PlayerArgs(args),
                        Command::PlayerArgs(expected_args)
                    );
                }

                for (parsed, expected_arg) in args.iter().zip(&expected_args) {
                    if *parsed != *expected_arg {
                        expected!(
                            Command::PlayerArgs(args),
                            Command::PlayerArgs(expected_args)
                        );
                    }
                }
            }
            other => expected!(other, Command::PlayerArgs(expected_args)),
        }
    }
}

use crate::err::{self, Result};
use smallvec::{smallvec, SmallVec};
use snafu::ensure;
use std::convert::TryFrom;
use std::result;
use termion::event::Key;
use tui::style::{Color, Style};
use tui::widgets::Text;
use unicode_width::UnicodeWidthChar;

/// A prompt to enter commands in that provides suggestions.
pub struct CommandPrompt {
    buffer: String,
    hint_cmd: Option<HintCommand<'static>>,
    width: usize,
}

impl CommandPrompt {
    /// Create a new `CommandPrompt`.
    pub fn new() -> Self {
        CommandPrompt {
            buffer: String::with_capacity(32),
            hint_cmd: None,
            width: 0,
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
                self.width = 0;
                return Ok(PromptResult::Command(command));
            }
            Key::Char('\t') => {
                if let Some(hint_cmd) = &self.hint_cmd {
                    let remaining_name = hint_cmd.remaining_name();

                    self.buffer.push_str(remaining_name);
                    self.buffer.push(' ');
                    // Our hint text should always be ASCII, so we can skip getting the unicode width in this case
                    self.width += remaining_name.len() + 1;

                    self.hint_cmd = None;
                }
            }
            Key::Char(ch) => {
                self.buffer.push(ch);
                self.width += UnicodeWidthChar::width(ch).unwrap_or(0);

                self.hint_cmd = match Command::best_matching_cmd_info(&self.buffer) {
                    // Once again, our hint text should always be ASCII, so we don't care about the unicode width here as well
                    Some(matching_cmd) if self.buffer.len() <= matching_cmd.name.len() => {
                        let cmd = HintCommand::new(matching_cmd, self.buffer.len());
                        Some(cmd)
                    }
                    _ => None,
                };
            }
            Key::Backspace => {
                if let Some(popped) = self.buffer.pop() {
                    self.width -= UnicodeWidthChar::width(popped).unwrap_or(0);
                }

                self.hint_cmd = None;
            }
            Key::Esc => {
                self.buffer.clear();
                self.width = 0;
                return Ok(PromptResult::Done);
            }
            _ => (),
        }

        Ok(PromptResult::NotDone)
    }

    #[inline(always)]
    pub fn width(&self) -> usize {
        self.width
    }

    /// The items of the `CommandPrompt` in a form ready for drawing.
    pub fn draw_items<'b>(&'b self) -> SmallVec<[Text<'b>; 2]> {
        let mut text = smallvec![Text::raw(&self.buffer)];

        if let Some(hint_cmd) = &self.hint_cmd {
            text.push(Text::styled(
                hint_cmd.remaining_name_and_usage(),
                Style::default().fg(Color::DarkGray),
            ));
        }

        text
    }
}

struct HintCommand<'a> {
    info: &'a CommandInfo,
    /// Represents the number of characters that have been "eaten" by user input.
    ///
    /// This is used so we can return a slice of the command's name and/or usage only
    /// containing the part that hasn't already been entered by the user.
    eaten: usize,
}

impl<'a> HintCommand<'a> {
    #[inline(always)]
    fn new(info: &'static CommandInfo, eaten: usize) -> Self {
        Self { info, eaten }
    }

    #[inline(always)]
    fn remaining_name(&self) -> &'a str {
        &self.info.name[self.eaten..]
    }

    #[inline(always)]
    fn remaining_name_and_usage(&self) -> &'a str {
        &self.info.name_and_usage[self.eaten..]
    }
}

struct CommandInfo {
    name: &'static str,
    name_and_usage: &'static str,
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
    ($enum_name:ident, $num_cmds:expr, $($field:pat => { name: $name:expr, usage: $usage:expr, min_args: $min_args:expr, fn: $parse_fn:expr, },)+) => {
        impl $enum_name {
            const COMMANDS: [CommandInfo; $num_cmds] = [
                $(CommandInfo {
                    name: $name,
                    name_and_usage: concat!($name, " ", $usage),
                },)+
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
    /// Add a new series with the specified nickname and optional series ID.
    Add(String, Option<anime::remote::SeriesID>),
    /// Remove the selected series from the program.
    Delete,
    /// Set the user's login token for a remote service.
    LoginToken(String),
    /// Set the episode matcher for the selected series.
    Matcher(Option<String>),
    /// Specify the video player arguments for the selected season.
    PlayerArgs(Vec<String>),
    /// Increment / decrement the watched episodes of the selected season.
    Progress(ProgressDirection),
    /// Syncronize the selected season to the remote service.
    SyncFromRemote,
    /// Syncronize the selected season from the remote service.
    SyncToRemote,
    /// Rate the selected season.
    Score(String),
    /// Set the watch status of the selected season.
    Status(anime::remote::Status),
}

impl_command_matching!(Command, 10,
    Add(_) => {
        name: "add",
        usage: "<nickname> [id]",
        min_args: 1,
        fn: |args: &[&str]| {
            let id = if args.len() > 1 {
                args[1].parse().ok()
            } else {
                None
            };

            Ok(Command::Add(args[0].into(), id))
        },
    },
    Delete => {
        name: "delete",
        usage: "",
        min_args: 0,
        fn: |_| Ok(Command::Delete),
    },
    LoginToken(_) => {
        name: "token",
        usage: "<token>",
        min_args: 1,
        fn: |args: &[&str]| {
            Ok(Command::LoginToken(args.join(" ")))
        },
    },
    Matcher(_) => {
        name: "matcher",
        usage: "[regex containing {title} and {episode}]",
        min_args: 0,
        fn: |args: &[&str]| {
            if args.is_empty() {
                Ok(Command::Matcher(None))
            } else {
                Ok(Command::Matcher(Some(args.join(" "))))
            }
        },
    },
    PlayerArgs(_) => {
        name: "args",
        usage: "<player args>",
        min_args: 0,
        fn: |args: &[&str]| {
            let args = args.iter()
                .map(|frag| frag.to_string())
                .collect();

            Ok(Command::PlayerArgs(args))
        },
    },
    Progress(_) => {
        name: "progress",
        usage: "<f, forward | b, backward>",
        min_args: 1,
        fn: |args: &[&str]| {
            let dir = ProgressDirection::try_from(args[0])?;
            Ok(Command::Progress(dir))
        },
    },
    SyncFromRemote => {
        name: "syncfromremote",
        usage: "",
        min_args: 0,
        fn: |_| Ok(Command::SyncFromRemote),
    },
    SyncToRemote => {
        name: "synctoremote",
        usage: "",
        min_args: 0,
        fn: |_| Ok(Command::SyncToRemote),
    },
    Score(_) => {
        name: "rate",
        usage: "<0-100>",
        min_args: 1,
        fn: |args: &[&str]| {
            let score = args[0].into();
            Ok(Command::Score(score))
        },
    },
    Status(_) => {
        name: "status",
        usage: "<w, watching | c, completed | h, hold | d, drop | p, plan | r, rewatch>",
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
);

impl Command {
    /// Returns the `CommandInfo` that has a name most similar to `name`.
    ///
    /// `None` will be returned if `name` does not match a command name with
    /// at least 70% similarity.
    fn best_matching_cmd_info(name: &str) -> Option<&'static CommandInfo> {
        const MIN_CONFIDENCE: f32 = 0.7;

        detect::closest_match(&Command::COMMANDS, MIN_CONFIDENCE, |cmd| {
            Some(strsim::jaro_winkler(&cmd.name, name) as f32)
        })
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

        test_command!("delete\n", Command::Delete);

        match enter_command("token inserttokenhere\n") {
            Command::LoginToken(token) if token == "inserttokenhere" => (),
            other => expected!(other, Command::LoginToken(String::from("inserttokenhere"))),
        }

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

        test_command!(
            "progress forward\n",
            Command::Progress(ProgressDirection::Forwards)
        );

        test_command!("status watching\n", Command::Status(Status::Watching));
    }
}

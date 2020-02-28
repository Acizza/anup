use crate::err::{self, Result};
use crate::series::SeriesParams;
use smallvec::{smallvec, SmallVec};
use snafu::ensure;
use std::convert::TryFrom;
use std::result;
use termion::event::Key;
use tui::backend::Backend;
use tui::layout::Rect;
use tui::style::{Color, Style};
use tui::widgets::{Block, Borders, Paragraph, Text, Widget};
use tui::Frame;
use unicode_width::UnicodeWidthChar;

/// A prompt to enter commands in that provides suggestions.
pub struct CommandPrompt {
    buffer: String,
    hint_cmd: Option<HintCommand<'static>>,
    width: usize,
}

impl CommandPrompt {
    pub fn new() -> Self {
        Self {
            buffer: String::with_capacity(32),
            hint_cmd: None,
            width: 0,
        }
    }

    /// Process a key for the `CommandPrompt`.
    ///
    /// Returns a `InputResult` representing the state of the prompt based off of `key`.
    /// Example return values are given below:
    ///
    /// | `key`             | `InputResult`                   |
    /// | ----------------- | ------------------------------- |
    /// | `Key::Char('a')`  | `InputResult::Continue`    |
    /// | `Key::Esc`        | `InputResult::Done`       |
    /// | `Key::Char('\n')` | `InputResult::Command(_)` |
    pub fn process_key(&mut self, key: Key) -> Result<InputResult> {
        match key {
            Key::Char('\n') => {
                let command = Command::try_from(self.buffer.as_ref())?;
                self.reset();
                return Ok(InputResult::Command(command));
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
                self.reset();
                return Ok(InputResult::Done);
            }
            _ => (),
        }

        Ok(InputResult::Continue)
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
        self.hint_cmd = None;
        self.width = 0;
    }

    #[inline(always)]
    pub fn width(&self) -> usize {
        self.width
    }

    /// The items of the `CommandPrompt` in a form ready for drawing.
    fn draw_items<'b>(&'b self) -> SmallVec<[Text<'b>; 2]> {
        let mut text = smallvec![Text::raw(&self.buffer)];

        if let Some(hint_cmd) = &self.hint_cmd {
            text.push(Text::styled(
                hint_cmd.remaining_name_and_usage(),
                Style::default().fg(Color::DarkGray),
            ));
        }

        text
    }

    pub fn draw<B>(&mut self, rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
    {
        Paragraph::new(self.draw_items().iter())
            .block(
                Block::default()
                    .title("Enter Command")
                    .borders(Borders::ALL),
            )
            .wrap(true)
            .render(frame, rect);
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
pub enum InputResult {
    /// A successfully parsed command.
    Command(Command),
    /// Input is considered completed without a command.
    Done,
    /// More input is needed.
    Continue,
}

/// Split `string` into shell words.
///
/// This implementation only groups (non-nested) quotes into one argument.
fn split_shell_words<'a>(string: &'a str) -> SmallVec<[&'a str; 3]> {
    if string.is_empty() {
        return SmallVec::new();
    }

    let mut slices = SmallVec::new();
    let mut start = 0;
    let mut in_quote = false;

    let is_surrounded_by_quotes = |slice: &str| {
        if slice.len() < 2 {
            return false;
        }

        let char_is_quote = |c| c == '\"' || c == '\'';

        slice.starts_with(char_is_quote) && slice.ends_with(char_is_quote)
    };

    let mut push_slice = |start, end| {
        let mut slice = &string[start..end];

        if is_surrounded_by_quotes(slice) {
            slice = &slice[1..slice.len() - 1];
        }

        if slice.is_empty() {
            return;
        }

        slices.push(slice);
    };

    for (i, ch) in string.chars().enumerate() {
        match ch {
            ' ' => {
                if in_quote {
                    continue;
                }

                push_slice(start, i);
                start = i + 1;
            }
            '\"' | '\'' => in_quote = !in_quote,
            _ => (),
        }
    }

    push_slice(start, string.len());
    slices
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
                let fragments = split_shell_words(value);

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
    /// Add a new series with the specified nickname and series parameters.
    Add(String, SeriesParams),
    /// Set the current remote to AniList with an optional login token.
    AniList(Option<String>),
    /// Remove the selected series from the program.
    Delete,
    // Set the current remote to offline.
    Offline,
    /// Specify the video player arguments for the selected season.
    PlayerArgs(SmallVec<[String; 3]>),
    /// Increment / decrement the watched episodes of the selected season.
    Progress(ProgressDirection),
    /// Set the parameters for the selected series.
    Set(SeriesParams),
    /// Syncronize the selected season to the remote service.
    SyncFromRemote,
    /// Syncronize the selected season from the remote service.
    SyncToRemote,
    /// Rate the selected season.
    Score(String),
    /// Set the watch status of the selected season.
    Status(anime::remote::Status),
}

impl_command_matching!(Command, 11,
    Add(_) => {
        name: "add",
        usage: "<nickname> [id=value] [path=\"value\"] [matcher=\"regex with {title} and {episode}\"]",
        min_args: 1,
        fn: |args: &[&str]| {
            let params = if args.len() > 1 {
                SeriesParams::from_name_value_list(&args[1..])?
            } else {
                SeriesParams::default()
            };

            Ok(Command::Add(args[0].into(), params))
        },
    },
    AniList(_) => {
        name: "anilist",
        usage: "[login token]",
        min_args: 0,
        fn: |args: &[&str]| {
            let token = if !args.is_empty() {
                Some(args.join(" "))
            } else {
                None
            };

            Ok(Command::AniList(token))
        },
    },
    Delete => {
        name: "delete",
        usage: "",
        min_args: 0,
        fn: |_| Ok(Command::Delete),
    },
    Offline => {
        name: "offline",
        usage: "",
        min_args: 0,
        fn: |_| Ok(Command::Offline),
    },
    PlayerArgs(_) => {
        name: "args",
        usage: "<player args>",
        min_args: 0,
        fn: |args: &[&str]| {
            let args = args.iter()
                .map(|&frag| frag.to_string())
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
    Set(_) => {
        name: "set",
        usage: "[id=value] [path=\"value\"] [matcher=\"regex with {title} and {episode}\"]",
        min_args: 1,
        fn: |args: &[&str]| {
            let params = SeriesParams::from_name_value_list(args)?;
            Ok(Command::Set(params))
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
        .map(|(_, cmd)| cmd)
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
            "f" | "forward" => Ok(Self::Forwards),
            "b" | "backward" => Ok(Self::Backwards),
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
                    Ok(InputResult::Continue) => (),
                    Ok(InputResult::Done) => panic!("expected {} command, got nothing", name),
                    Ok(InputResult::Command(cmd)) => return cmd,
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

        match enter_command("anilist inserttokenhere\n") {
            Command::AniList(token) if token == Some("inserttokenhere".into()) => (),
            other => expected!(other, Command::AniList(Some("inserttokenhere".into()))),
        }

        let expected_args: SmallVec<[_; 3]> = smallvec!["arg1".into(), "arg2".into()];

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

    #[test]
    fn test_shell_words() {
        use smallvec::smallvec;

        let expected: SmallVec<[_; 4]> = smallvec!["this", "is", "a", "test"];
        assert_eq!(split_shell_words("this is a test"), expected);

        let expected: SmallVec<[_; 3]> = smallvec!["this", "is a harder", "test"];
        assert_eq!(split_shell_words("this \"is a harder\" test"), expected);

        let expected: SmallVec<[_; 4]> =
            smallvec!["this", "tests=\"quotes inside of\"", "one", "string"];
        assert_eq!(
            split_shell_words("this tests=\"quotes inside of\" one string"),
            expected
        );

        let expected: SmallVec<[_; 3]> = smallvec!["tests", "empty", "quotes"];
        assert_eq!(split_shell_words("tests \"\" empty quotes"), expected);

        let expected: SmallVec<[_; 1]> = smallvec!["tests single quote with spaces"];
        assert_eq!(
            split_shell_words("\"tests single quote with spaces\""),
            expected
        );

        let expected: SmallVec<[_; 3]> = smallvec!["tests", "alternative quote", "matcher"];
        assert_eq!(
            split_shell_words("tests \'alternative quote\' matcher"),
            expected
        );

        let expected: SmallVec<[_; 3]> = smallvec!["tests", "having mixed", "quotes"];
        assert_eq!(split_shell_words("tests \"having mixed\' quotes"), expected);

        // Only one quote
        let expected: SmallVec<[_; 1]> = smallvec!["\""];
        assert_eq!(split_shell_words("\""), expected);

        // Only one space
        let expected: SmallVec<[&str; 0]> = smallvec![];
        assert_eq!(split_shell_words(" "), expected);

        // Empty quotes without any other arguments
        assert_eq!(split_shell_words("\"\""), expected);
    }
}
use crate::config::Config;
use crate::series::SeriesPath;
use crate::tui::component::input::Input;
use crate::tui::component::Draw;
use crate::{SERIES_EPISODE_REP, SERIES_TITLE_REP};
use anime::local::EpisodeParser;
use std::borrow::Cow;
use std::path::PathBuf;
use tui::backend::Backend;
use tui::layout::Rect;
use tui::terminal::Frame;

pub struct InputSet {
    pub name: NameInput,
    pub id: IDInput,
    pub path: PathInput,
    pub parser: ParserInput,
}

impl InputSet {
    const TOTAL: usize = 4;

    pub fn new(config: &Config) -> Self {
        Self {
            name: NameInput::new(true),
            id: IDInput::new(false),
            path: PathInput::new(false, config),
            parser: ParserInput::new(false),
        }
    }

    #[inline(always)]
    pub const fn len(&self) -> usize {
        Self::TOTAL
    }

    #[inline(always)]
    pub fn all_mut(&mut self) -> [&mut dyn ValidatedInput; Self::TOTAL] {
        [
            &mut self.name,
            &mut self.id,
            &mut self.path,
            &mut self.parser,
        ]
    }

    #[inline(always)]
    pub fn index_mut(&mut self, index: usize) -> &mut dyn ValidatedInput {
        self.all_mut()[index]
    }
}

pub trait ValidatedInput {
    fn label(&self) -> &'static str;
    fn input_mut(&mut self) -> &mut Input;

    fn validate(&mut self);

    fn has_error(&self) -> bool;
    fn error_message(&self) -> Cow<'static, str>;

    fn error(&self) -> Option<Cow<'static, str>> {
        if self.has_error() {
            Some(self.error_message())
        } else {
            None
        }
    }
}

pub trait ParsedValue {
    type Value: ?Sized;

    fn parsed_value(&self) -> &Self::Value;
}

macro_rules! impl_draw_for_input {
    ($struct:ident) => {
        impl<B> Draw<B> for $struct
        where
            B: Backend,
        {
            type State = ();

            fn draw(&mut self, _: &Self::State, rect: Rect, frame: &mut Frame<B>) {
                self.input_mut().draw(&(), rect, frame)
            }
        }
    };
}

pub struct NameInput(Input);

impl NameInput {
    #[inline(always)]
    fn new(selected: bool) -> Self {
        Self(Input::new(selected))
    }
}

impl ValidatedInput for NameInput {
    fn label(&self) -> &'static str {
        "Name"
    }

    fn input_mut(&mut self) -> &mut Input {
        &mut self.0
    }

    fn validate(&mut self) {
        let empty = self.0.text().is_empty();
        self.0.error = empty;
    }

    fn has_error(&self) -> bool {
        self.0.error
    }

    fn error_message(&self) -> Cow<'static, str> {
        "Name must not be empty".into()
    }
}

impl ParsedValue for NameInput {
    type Value = str;

    fn parsed_value(&self) -> &Self::Value {
        self.0.text()
    }
}

impl_draw_for_input!(NameInput);

pub struct IDInput {
    input: Input,
    id: Option<i32>,
}

impl IDInput {
    fn new(selected: bool) -> Self {
        Self {
            input: Input::new(selected),
            id: None,
        }
    }
}

impl ValidatedInput for IDInput {
    fn label(&self) -> &'static str {
        "ID"
    }

    fn input_mut(&mut self) -> &mut Input {
        &mut self.input
    }

    fn validate(&mut self) {
        let text = self.input.text();

        if text.is_empty() {
            self.id = None;
            self.input.error = false;
            return;
        }

        let (result, error) = match text.parse() {
            Ok(num) if num >= 0 => (Some(num), false),
            _ => (None, true),
        };

        self.id = result;
        self.input.error = error;
    }

    fn has_error(&self) -> bool {
        self.input.error
    }

    fn error_message(&self) -> Cow<'static, str> {
        "ID must be a positive number".into()
    }
}

impl ParsedValue for IDInput {
    type Value = Option<i32>;

    fn parsed_value(&self) -> &Self::Value {
        &self.id
    }
}

impl_draw_for_input!(IDInput);

pub struct PathInput {
    input: Input,
    base_path: PathBuf,
    path: Option<SeriesPath>,
}

impl PathInput {
    fn new(selected: bool, config: &Config) -> Self {
        Self {
            input: Input::new(selected),
            base_path: config.series_dir.clone(),
            path: None,
        }
    }
}

impl ValidatedInput for PathInput {
    fn label(&self) -> &'static str {
        "Path"
    }

    fn input_mut(&mut self) -> &mut Input {
        &mut self.input
    }

    fn validate(&mut self) {
        let text = self.input.text();

        if text.is_empty() {
            self.path = None;
            self.input.error = false;
            return;
        }

        let path = SeriesPath::with_base(&self.base_path, Cow::Owned(PathBuf::from(text)));
        let exists = path.exists_base(&self.base_path);

        self.path = if exists { Some(path) } else { None };
        self.input.error = !exists;
    }

    fn has_error(&self) -> bool {
        self.input.error
    }

    fn error_message(&self) -> Cow<'static, str> {
        "Path must exist".into()
    }
}

impl ParsedValue for PathInput {
    type Value = Option<SeriesPath>;

    fn parsed_value(&self) -> &Self::Value {
        &self.path
    }
}

impl_draw_for_input!(PathInput);

pub struct ParserInput {
    input: Input,
    parser: EpisodeParser,
}

impl ParserInput {
    fn new(selected: bool) -> Self {
        Self {
            input: Input::new(selected),
            parser: EpisodeParser::default(),
        }
    }
}

impl ValidatedInput for ParserInput {
    fn label(&self) -> &'static str {
        "Episode Regex"
    }

    fn input_mut(&mut self) -> &mut Input {
        &mut self.input
    }

    fn validate(&mut self) {
        let text = self.input.text();

        if text.is_empty() {
            self.parser = EpisodeParser::default();
            self.input.error = false;
            return;
        }

        let parser =
            EpisodeParser::custom_with_replacements(text, SERIES_TITLE_REP, SERIES_EPISODE_REP)
                .ok();

        self.input.error = parser.is_none();
        self.parser = parser.unwrap_or_else(Default::default);
    }

    fn has_error(&self) -> bool {
        self.input.error
    }

    fn error_message(&self) -> Cow<'static, str> {
        // TODO: use concat! macro if/when it can accept constants, or when a similiar crate doesn't require nightly
        format!(
            "Regex must contain \"{}\" and be valid",
            crate::SERIES_EPISODE_REP
        )
        .into()
    }
}

impl ParsedValue for ParserInput {
    type Value = EpisodeParser;

    fn parsed_value(&self) -> &Self::Value {
        &self.parser
    }
}

impl_draw_for_input!(ParserInput);

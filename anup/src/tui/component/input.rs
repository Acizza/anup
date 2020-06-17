use crate::config::Config;
use crate::series::SeriesPath;
use crate::try_opt_ret;
use crate::tui::component::Draw;
use crate::tui::widget_util::{block, style};
use crate::{SERIES_EPISODE_REP, SERIES_TITLE_REP};
use anime::local::EpisodeParser;
use std::borrow::Cow;
use std::path::PathBuf;
use termion::event::Key;
use tui::backend::Backend;
use tui::layout::Rect;
use tui::style::Color;
use tui::terminal::Frame;
use tui::widgets::{Paragraph, Text};
use unicode_width::UnicodeWidthChar;

pub struct Input {
    caret: Caret,
    visible_width: u16,
    offset: usize,
    pub selected: bool,
    pub error: bool,
}

impl Input {
    const BORDER_SIZE: u16 = 2;

    pub fn new(selected: bool) -> Self {
        Self {
            caret: Caret::new(),
            visible_width: 0,
            offset: 0,
            selected,
            error: false,
        }
    }

    pub fn process_key(&mut self, key: Key) {
        match key {
            Key::Char(ch) => self.caret.push(ch),
            Key::Backspace => self.caret.pop(),
            Key::Left => self.caret.move_left(),
            Key::Right => self.caret.move_right(),
            Key::Home => self.caret.move_front(),
            Key::End => self.caret.move_end(),
            _ => (),
        }

        self.update_visible_slice();
    }

    fn update_visible_slice(&mut self) {
        let max_width = self.visible_width - Self::BORDER_SIZE - 1;

        if self.caret.pos < max_width as usize {
            self.offset = 0;
            return;
        }

        self.offset = self.caret.pos - max_width as usize;
    }

    pub fn calculate_cursor_pos(column: u16, rect: Rect) -> (u16, u16) {
        let rect_width = rect.width.saturating_sub(Self::BORDER_SIZE);

        let (len, line_num) = if rect_width > 0 {
            let line_num = column / rect_width;
            let max_line = rect.height - Self::BORDER_SIZE - 1;

            // We want to cap the position of the cursor to the last character of the last line
            if line_num > max_line {
                (rect_width - 1, max_line)
            } else {
                (column % rect_width, line_num)
            }
        } else {
            (column, 0)
        };

        let x = 1 + rect.left() + len;
        let y = 1 + rect.top() + line_num;

        (x, y)
    }

    #[inline(always)]
    pub fn will_cursor_fit(rect: Rect) -> bool {
        rect.height > Self::BORDER_SIZE && rect.width > Self::BORDER_SIZE
    }

    fn set_cursor_pos<B>(&self, rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
    {
        if !self.selected || !Self::will_cursor_fit(rect) {
            return;
        }

        let width = (self.caret.cur_width as u16).min(rect.width);
        let (x, y) = Self::calculate_cursor_pos(width, rect);

        frame.set_cursor(x, y);
    }

    pub fn clear(&mut self) {
        self.caret.clear();
        self.offset = 0;
    }

    #[inline(always)]
    pub fn text(&self) -> &str {
        &self.caret.buffer
    }

    #[inline(always)]
    pub fn visible(&self) -> &str {
        &self.caret.buffer[self.offset..]
    }
}

impl<B> Draw<B> for Input
where
    B: Backend,
{
    type State = ();

    fn draw(&mut self, _: &Self::State, rect: Rect, frame: &mut Frame<B>) {
        self.visible_width = rect.width;

        let block_color = match (self.selected, self.error) {
            (true, true) => Some(Color::LightRed),
            (true, false) => Some(Color::Blue),
            (false, true) => Some(Color::Red),
            (false, false) => None,
        };

        let mut block = block::with_borders(None);

        if let Some(color) = block_color {
            block = block.border_style(style::fg(color));
        }

        let text = [Text::raw(self.visible())];
        let widget = Paragraph::new(text.iter()).block(block).wrap(false);

        frame.render_widget(widget, rect);
        self.set_cursor_pos(rect, frame);
    }
}

struct Caret {
    buffer: String,
    cur_width: usize,
    total_width: usize,
    pos: usize,
}

impl Caret {
    fn new() -> Self {
        Self {
            buffer: String::new(),
            cur_width: 0,
            total_width: 0,
            pos: 0,
        }
    }

    fn push(&mut self, ch: char) {
        self.buffer.insert(self.cur_width, ch);

        let width = UnicodeWidthChar::width(ch).unwrap_or(0);

        self.cur_width += width;
        self.total_width += width;
        self.pos += 1;
    }

    fn pop(&mut self) {
        if self.buffer.is_empty() || self.pos == 0 {
            return;
        }

        let ch = if self.pos == self.buffer.len() {
            try_opt_ret!(self.buffer.pop())
        } else {
            self.buffer.remove(self.pos - 1)
        };

        let width = UnicodeWidthChar::width(ch).unwrap_or(0);

        self.cur_width -= width;
        self.total_width -= width;
        self.pos -= 1;
    }

    fn move_left(&mut self) {
        if self.pos == 0 {
            return;
        }

        let left_char = try_opt_ret!(self.buffer.chars().nth(self.pos - 1));

        self.cur_width -= UnicodeWidthChar::width(left_char).unwrap_or(0);
        self.pos -= 1;
    }

    fn move_right(&mut self) {
        if self.pos > self.buffer.len() {
            return;
        }

        let right_char = try_opt_ret!(self.buffer.chars().nth(self.pos));

        self.cur_width += UnicodeWidthChar::width(right_char).unwrap_or(0);
        self.pos += 1;
    }

    fn move_front(&mut self) {
        self.cur_width = 0;
        self.pos = 0;
    }

    fn move_end(&mut self) {
        self.cur_width = self.total_width;
        self.pos = self.buffer.len();
    }

    fn clear(&mut self) {
        self.buffer.clear();
        self.cur_width = 0;
        self.total_width = 0;
        self.pos = 0;
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
    pub fn new(selected: bool) -> Self {
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
    pub fn new(selected: bool) -> Self {
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
    pub fn new(selected: bool, config: &Config) -> Self {
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
    pub fn new(selected: bool) -> Self {
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

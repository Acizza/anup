use crate::series::SeriesPath;
use crate::tui::component::Draw;
use crate::tui::widget_util::{block, style, text};
use crate::{config::Config, key::Key};
use anime::local::detect::CustomPattern;
use anime::local::EpisodeParser;
use anime::remote::SeriesID;
use bitflags::bitflags;
use crossterm::event::KeyCode;
use std::borrow::Cow;
use std::path::PathBuf;
use tui::backend::Backend;
use tui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use tui::style::Color;
use tui::terminal::Frame;
use tui::text::Text;
use tui::widgets::Paragraph;
use unicode_segmentation::GraphemeCursor;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

bitflags! {
    pub struct InputFlags: u8 {
        /// Indicates that the input should is selected and can take input.
        const SELECTED = 0b0001;
        /// Indicates that the input has an error.
        const HAS_ERROR = 0b0010;
        /// Indicates whether the placeholder should be ignored if it is Some(_).
        /// This is used to toggle it on and off without having to store the placeholder somewhere else in between.
        const IGNORE_PLACEHOLDER = 0b0100;
        /// Indicates that the input is disabled and will not accept input, even if selected.
        const DISABLED = 0b1000;
    }
}

pub struct Input {
    caret: Caret,
    visible_width: u16,
    visible_offset: usize,
    pub flags: InputFlags,
    pub draw_mode: DrawMode,
    /// A string to display in the input when there is no input.
    pub placeholder: Option<String>,
}

impl Input {
    pub const DRAW_WITH_LABEL_CONSTRAINT: Constraint = Constraint::Length(4);

    const BORDER_SIZE: u16 = 2;

    fn with_caret(
        flags: InputFlags,
        draw_mode: DrawMode,
        placeholder: Option<String>,
        caret: Caret,
    ) -> Self {
        Self {
            caret,
            visible_width: 0,
            visible_offset: 0,
            flags,
            draw_mode,
            placeholder,
        }
    }

    #[inline(always)]
    pub fn new(flags: InputFlags, draw_mode: DrawMode) -> Self {
        Self::with_caret(flags, draw_mode, None, Caret::new())
    }

    #[inline(always)]
    pub fn with_label(flags: InputFlags, label: &'static str) -> Self {
        Self::new(flags, DrawMode::Label(label))
    }

    pub fn with_label_and_placeholder<S>(
        flags: InputFlags,
        label: &'static str,
        placeholder: S,
    ) -> Self
    where
        S: Into<String>,
    {
        let caret = Caret::new();

        Self::with_caret(
            flags,
            DrawMode::Label(label),
            Some(placeholder.into()),
            caret,
        )
    }

    pub fn with_label_and_text<S>(flags: InputFlags, label: &'static str, text: S) -> Self
    where
        S: AsRef<str>,
    {
        let caret = Caret::new();
        let mut input = Self::with_caret(flags, DrawMode::Label(label), None, caret);
        input.caret.push_str(text.as_ref());
        input
    }

    pub fn process_key(&mut self, key: Key) {
        if self.flags.contains(InputFlags::DISABLED) {
            return;
        }

        match *key {
            KeyCode::Char(ch) => self.caret.push(ch),
            KeyCode::Backspace => self.caret.pop(),
            KeyCode::Left => self.caret.move_left(),
            KeyCode::Right => match (self.caret.is_empty(), self.placeholder.as_ref()) {
                // Fill our input with the placeholder if present and we don't currently have user input
                (true, Some(placeholder)) => self.caret.push_str(&placeholder[self.caret.pos()..]),
                _ => self.caret.move_right(),
            },
            KeyCode::Home => self.caret.move_front(),
            KeyCode::End => self.caret.move_end(),
            _ => (),
        }

        self.update_visible_slice();
    }

    fn update_visible_slice(&mut self) {
        let max_width = self.visible_width - Self::BORDER_SIZE - 1;

        if self.caret.display_offset < max_width as usize {
            self.visible_offset = 0;
            return;
        }

        let desired_offset = self.caret.display_offset - max_width as usize;
        let mut cursor = GraphemeCursor::new(0, self.caret.buffer.len(), true);

        // TODO: this can probably be optimized
        for _ in 0..desired_offset {
            match cursor.next_boundary(&self.caret.buffer, 0) {
                Ok(Some(_)) => (),
                Ok(None) => break,
                Err(_) => return,
            }
        }

        self.visible_offset = cursor.cur_cursor();
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
        if !self.is_selected() || !Self::will_cursor_fit(rect) {
            return;
        }

        let width = (self.caret.display_offset as u16).min(rect.width);
        let (x, y) = Self::calculate_cursor_pos(width, rect);

        frame.set_cursor(x, y);
    }

    pub fn clear(&mut self) {
        self.caret.clear();
        self.visible_offset = 0;
    }

    pub fn text(&self) -> &str {
        if !self.caret.is_empty() || self.flags.contains(InputFlags::IGNORE_PLACEHOLDER) {
            return &self.caret.buffer;
        }

        match self.placeholder.as_ref() {
            Some(placeholder) => placeholder,
            None => &self.caret.buffer,
        }
    }

    #[inline(always)]
    pub fn visible(&self) -> &str {
        &self.caret.buffer[self.visible_offset..]
    }

    #[inline(always)]
    pub fn has_input(&self) -> bool {
        self.caret.is_empty()
    }

    #[inline(always)]
    pub fn has_error(&self) -> bool {
        self.flags.contains(InputFlags::HAS_ERROR)
    }

    #[inline(always)]
    pub fn set_error(&mut self, error: bool) {
        self.flags.set(InputFlags::HAS_ERROR, error);
    }

    #[inline(always)]
    pub fn is_selected(&self) -> bool {
        self.flags.contains(InputFlags::SELECTED)
    }

    #[inline(always)]
    pub fn set_selected(&mut self, selected: bool) {
        self.flags.set(InputFlags::SELECTED, selected)
    }
}

impl<B> Draw<B> for Input
where
    B: Backend,
{
    type State = ();

    fn draw(&mut self, _: &Self::State, rect: Rect, frame: &mut Frame<B>) {
        self.visible_width = rect.width;

        let is_disabled = self.flags.contains(InputFlags::DISABLED);

        let block_color = if is_disabled {
            Some(Color::DarkGray)
        } else {
            match (self.is_selected(), self.has_error()) {
                (true, true) => Some(Color::LightRed),
                (true, false) => Some(Color::Blue),
                (false, true) => Some(Color::Red),
                (false, false) => None,
            }
        };

        let mut block = block::with_borders(None);

        if let Some(color) = block_color {
            block = block.border_style(style::fg(color));
        }

        let input_pos = match &self.draw_mode {
            DrawMode::Label(label) => {
                let layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(1), Constraint::Length(3)].as_ref())
                    .split(rect);

                let text = if is_disabled {
                    text::bold_with(*label, |s| s.fg(Color::DarkGray))
                } else {
                    text::bold(*label)
                };

                let widget = Paragraph::new(text).alignment(Alignment::Center);
                frame.render_widget(widget, layout[0]);

                layout[1]
            }
            DrawMode::Blank => rect,
        };

        let text: Text = match (self.caret.is_empty(), &self.placeholder) {
            (true, Some(placeholder)) if !self.flags.contains(InputFlags::IGNORE_PLACEHOLDER) => {
                let slice = &placeholder[self.caret.pos()..];
                text::with_color(slice, Color::DarkGray).into()
            }
            _ => self.visible().into(),
        };

        let widget = Paragraph::new(text).block(block);
        frame.render_widget(widget, input_pos);

        self.set_cursor_pos(input_pos, frame);
    }
}

pub enum DrawMode {
    Label(&'static str),
    Blank,
}

impl Default for DrawMode {
    fn default() -> Self {
        Self::Blank
    }
}

struct Caret {
    buffer: String,
    cursor: GraphemeCursor,
    display_offset: usize,
}

impl Caret {
    fn new() -> Self {
        Self {
            buffer: String::new(),
            cursor: GraphemeCursor::new(0, 0, true),
            display_offset: 0,
        }
    }

    fn push(&mut self, ch: char) {
        let pos = self.pos();

        self.buffer.insert(pos, ch);
        self.cursor = GraphemeCursor::new(pos + ch.len_utf8(), self.buffer.len(), true);

        self.display_offset += UnicodeWidthChar::width(ch).unwrap_or(0);
    }

    fn push_str(&mut self, value: &str) {
        self.buffer.push_str(value);
        self.cursor = GraphemeCursor::new(self.pos() + value.len(), self.buffer.len(), true);

        self.display_offset += UnicodeWidthStr::width(value);
    }

    fn pop(&mut self) {
        if self.pos() == 0 {
            return;
        }

        let pos = match self.cursor.prev_boundary(&self.buffer, 0).ok().flatten() {
            Some(pos) => pos,
            None => return,
        };

        let ch = self.buffer.remove(pos);
        let width = UnicodeWidthChar::width(ch).unwrap_or(0);

        self.display_offset = self.display_offset.saturating_sub(width);
        self.cursor = GraphemeCursor::new(pos, self.buffer.len(), true);
    }

    fn move_left(&mut self) {
        if self.pos() == 0 {
            return;
        }

        let old_pos = self.pos();

        if let Some(new_pos) = self.cursor.prev_boundary(&self.buffer, 0).ok().flatten() {
            let slice = &self.buffer[new_pos..old_pos];
            let width = UnicodeWidthStr::width(slice);

            self.display_offset = self.display_offset.saturating_sub(width);
        }
    }

    fn move_right(&mut self) {
        if self.pos() >= self.buffer.len() {
            return;
        }

        let old_pos = self.pos();

        if let Some(new_pos) = self.cursor.next_boundary(&self.buffer, 0).ok().flatten() {
            let slice = &self.buffer[old_pos..new_pos];
            let width = UnicodeWidthStr::width(slice);

            self.display_offset += width;
        }
    }

    fn move_front(&mut self) {
        self.cursor.set_cursor(0);
        self.display_offset = 0;
    }

    fn move_end(&mut self) {
        self.cursor.set_cursor(self.buffer.len());
        self.display_offset = UnicodeWidthStr::width(self.buffer.as_str());
    }

    fn clear(&mut self) {
        self.buffer.clear();
        self.cursor = GraphemeCursor::new(0, 0, true);
        self.display_offset = 0;
    }

    #[inline(always)]
    fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    #[inline(always)]
    fn pos(&self) -> usize {
        self.cursor.cur_cursor()
    }
}

pub trait ValidatedInput {
    fn label(&self) -> &'static str;

    fn input(&self) -> &Input;
    fn input_mut(&mut self) -> &mut Input;

    fn validate(&mut self);

    fn has_error(&self) -> bool {
        self.input().has_error()
    }

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
    const LABEL: &'static str = "Name";

    pub fn new(flags: InputFlags) -> Self {
        Self(Input::with_label(flags, Self::LABEL))
    }

    pub fn with_placeholder<S>(flags: InputFlags, name: S) -> Self
    where
        S: Into<String>,
    {
        let input = Input::with_label_and_placeholder(flags, Self::LABEL, name);
        Self(input)
    }
}

impl ValidatedInput for NameInput {
    fn label(&self) -> &'static str {
        Self::LABEL
    }

    fn input(&self) -> &Input {
        &self.0
    }

    fn input_mut(&mut self) -> &mut Input {
        &mut self.0
    }

    fn validate(&mut self) {
        let empty = !self.0.flags.contains(InputFlags::DISABLED) && self.0.text().is_empty();
        self.0.set_error(empty);
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
    id: Option<SeriesID>,
}

impl IDInput {
    const LABEL: &'static str = "ID";

    pub fn new(flags: InputFlags) -> Self {
        Self {
            input: Input::with_label(flags, Self::LABEL),
            id: None,
        }
    }

    pub fn with_id(flags: InputFlags, id: SeriesID) -> Self {
        Self {
            input: Input::with_label_and_text(flags, Self::LABEL, id.to_string()),
            id: Some(id),
        }
    }
}

impl ValidatedInput for IDInput {
    fn label(&self) -> &'static str {
        Self::LABEL
    }

    fn input(&self) -> &Input {
        &self.input
    }

    fn input_mut(&mut self) -> &mut Input {
        &mut self.input
    }

    fn validate(&mut self) {
        let text = self.input.text();

        if text.is_empty() {
            self.id = None;
            self.input.set_error(false);
            return;
        }

        let (result, error) = match text.parse() {
            Ok(num) => (Some(num), false),
            Err(_) => (None, true),
        };

        self.id = result;
        self.input.set_error(error);
    }

    fn error_message(&self) -> Cow<'static, str> {
        "ID must be a positive number".into()
    }
}

impl ParsedValue for IDInput {
    type Value = Option<SeriesID>;

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
    const LABEL: &'static str = "Path";

    pub fn new(flags: InputFlags, config: &Config) -> Self {
        Self {
            input: Input::with_label(flags, Self::LABEL),
            base_path: config.series_dir.clone(),
            path: None,
        }
    }

    pub fn with_placeholder<P>(flags: InputFlags, config: &Config, path: P) -> Self
    where
        P: Into<PathBuf>,
    {
        let path = SeriesPath::new(path.into(), config);
        let path_display = path.inner().to_string_lossy();

        Self {
            input: Input::with_label_and_placeholder(flags, Self::LABEL, path_display),
            base_path: config.series_dir.clone(),
            path: None,
        }
    }

    pub fn with_path(flags: InputFlags, config: &Config, path: SeriesPath) -> Self {
        Self {
            input: Input::with_label_and_text(flags, Self::LABEL, format!("{}", path.display())),
            base_path: config.series_dir.clone(),
            path: Some(path),
        }
    }
}

impl ValidatedInput for PathInput {
    fn label(&self) -> &'static str {
        Self::LABEL
    }

    fn input(&self) -> &Input {
        &self.input
    }

    fn input_mut(&mut self) -> &mut Input {
        &mut self.input
    }

    fn validate(&mut self) {
        let text = self.input.text();

        if text.is_empty() {
            self.path = None;
            self.input.set_error(false);
            return;
        }

        let path = SeriesPath::with_base(&self.base_path, Cow::Owned(PathBuf::from(text)));
        let exists = path.exists_base(&self.base_path);

        self.path = if exists { Some(path) } else { None };
        self.input.set_error(!exists);
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
    const LABEL: &'static str = "Episode Pattern";

    pub fn new(flags: InputFlags) -> Self {
        Self {
            input: Input::with_label(flags, Self::LABEL),
            parser: EpisodeParser::default(),
        }
    }

    pub fn with_text<S>(flags: InputFlags, pattern: S) -> Self
    where
        S: AsRef<str>,
    {
        Self {
            input: Input::with_label_and_text(flags, Self::LABEL, pattern),
            parser: EpisodeParser::default(),
        }
    }

    fn reset(&mut self, with_error: bool) {
        self.parser = EpisodeParser::default();
        self.input.set_error(with_error);
    }
}

impl ValidatedInput for ParserInput {
    fn label(&self) -> &'static str {
        Self::LABEL
    }

    fn input(&self) -> &Input {
        &self.input
    }

    fn input_mut(&mut self) -> &mut Input {
        &mut self.input
    }

    fn validate(&mut self) {
        let text = self.input.text();

        if text.is_empty() {
            self.reset(false);
            return;
        }

        let pattern = CustomPattern::new(text);

        if !pattern.has_episode_marker() {
            self.reset(true);
            return;
        }

        self.parser = EpisodeParser::Custom(pattern);
        self.input.set_error(false);
    }

    fn error_message(&self) -> Cow<'static, str> {
        // TODO: use concat! macro if/when it can accept constants, or when a similiar crate doesn't require nightly
        format!(
            "Must mark episode location with {}",
            CustomPattern::EPISODE_MARKER,
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

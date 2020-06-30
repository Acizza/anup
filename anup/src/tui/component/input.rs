use crate::config::Config;
use crate::series::SeriesPath;
use crate::tui::component::Draw;
use crate::tui::widget_util::{block, style, text};
use anime::local::detect::CustomPattern;
use anime::local::EpisodeParser;
use smallvec::{smallvec, SmallVec};
use std::borrow::Cow;
use std::path::PathBuf;
use termion::event::Key;
use tui::backend::Backend;
use tui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use tui::style::Color;
use tui::terminal::Frame;
use tui::widgets::{Paragraph, Text};
use unicode_segmentation::GraphemeCursor;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub struct Input {
    caret: Caret,
    visible_width: u16,
    visible_offset: usize,
    pub selected: bool,
    pub error: bool,
    pub draw_mode: DrawMode,
    /// A string to display in the input when there is no input.
    pub placeholder: Option<String>,
    /// Indicates whether the placeholder should be used if present.
    /// Mainly used for toggling.
    pub use_placeholder: bool,
}

impl Input {
    pub const DRAW_WITH_LABEL_CONSTRAINT: Constraint = Constraint::Length(4);

    const BORDER_SIZE: u16 = 2;

    fn with_caret(
        selected: bool,
        draw_mode: DrawMode,
        placeholder: Option<String>,
        caret: Caret,
    ) -> Self {
        let use_placeholder = placeholder.is_some();

        Self {
            caret,
            visible_width: 0,
            visible_offset: 0,
            selected,
            error: false,
            draw_mode,
            placeholder,
            use_placeholder,
        }
    }

    #[inline(always)]
    pub fn new(selected: bool, draw_mode: DrawMode) -> Self {
        Self::with_caret(selected, draw_mode, None, Caret::new())
    }

    #[inline(always)]
    pub fn with_label(selected: bool, label: &'static str) -> Self {
        Self::new(selected, DrawMode::Label(label))
    }

    #[inline(always)]
    pub fn with_label_and_hint<S>(selected: bool, label: &'static str, hint: S) -> Self
    where
        S: Into<String>,
    {
        let caret = Caret::new();
        Self::with_caret(selected, DrawMode::Label(label), Some(hint.into()), caret)
    }

    pub fn process_key(&mut self, key: Key) {
        match key {
            Key::Char(ch) => self.caret.push(ch),
            Key::Backspace => self.caret.pop(),
            Key::Left => self.caret.move_left(),
            Key::Right => match (self.caret.is_empty(), self.placeholder.as_ref()) {
                // Fill our input with the placeholder if present and we don't currently have user input
                (true, Some(placeholder)) => self.caret.push_str(&placeholder[self.caret.pos()..]),
                _ => self.caret.move_right(),
            },
            Key::Home => self.caret.move_front(),
            Key::End => self.caret.move_end(),
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
        if !self.selected || !Self::will_cursor_fit(rect) {
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
        if !self.caret.is_empty() || !self.use_placeholder {
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

        let input_pos = match &self.draw_mode {
            DrawMode::Label(label) => {
                let layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(1), Constraint::Length(3)].as_ref())
                    .split(rect);

                let text = [text::bold(*label)];
                let widget = Paragraph::new(text.iter())
                    .wrap(false)
                    .alignment(Alignment::Center);

                frame.render_widget(widget, layout[0]);

                layout[1]
            }
            DrawMode::Blank => rect,
        };

        let mut text: SmallVec<[_; 2]> = smallvec![Text::raw(self.visible())];

        if self.caret.is_empty() && self.use_placeholder {
            if let Some(placeholder) = &self.placeholder {
                let slice = &placeholder[self.caret.pos()..];
                text.push(text::with_color(slice, Color::DarkGray));
            }
        }

        let widget = Paragraph::new(text.iter()).block(block).wrap(false);

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

        self.cursor.prev_boundary(&self.buffer, 0).ok();
        self.display_offset = self.display_offset.saturating_sub(1);
    }

    fn move_right(&mut self) {
        if self.pos() >= self.buffer.len() {
            return;
        }

        self.cursor.next_boundary(&self.buffer, 0).ok();
        self.display_offset += 1;
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
    const LABEL: &'static str = "Name";

    #[inline(always)]
    pub fn new(selected: bool) -> Self {
        Self(Input::with_label(selected, Self::LABEL))
    }

    #[inline(always)]
    pub fn with_placeholder<S>(selected: bool, name: S) -> Self
    where
        S: Into<String>,
    {
        Self(Input::with_label_and_hint(selected, Self::LABEL, name))
    }
}

impl ValidatedInput for NameInput {
    fn label(&self) -> &'static str {
        Self::LABEL
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
    const LABEL: &'static str = "ID";

    pub fn new(selected: bool) -> Self {
        Self {
            input: Input::with_label(selected, Self::LABEL),
            id: None,
        }
    }
}

impl ValidatedInput for IDInput {
    fn label(&self) -> &'static str {
        Self::LABEL
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
    const LABEL: &'static str = "Path";

    pub fn new(selected: bool, config: &Config) -> Self {
        Self {
            input: Input::with_label(selected, Self::LABEL),
            base_path: config.series_dir.clone(),
            path: None,
        }
    }

    pub fn with_placeholder<P>(selected: bool, config: &Config, path: P) -> Self
    where
        P: Into<PathBuf>,
    {
        let path = SeriesPath::new(path.into(), config);
        let path_display = path.inner().to_string_lossy();

        Self {
            input: Input::with_label_and_hint(selected, Self::LABEL, path_display),
            base_path: config.series_dir.clone(),
            path: None,
        }
    }
}

impl ValidatedInput for PathInput {
    fn label(&self) -> &'static str {
        Self::LABEL
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
    const LABEL: &'static str = "Episode Pattern";

    pub fn new(selected: bool) -> Self {
        Self {
            input: Input::with_label(selected, Self::LABEL),
            parser: EpisodeParser::default(),
        }
    }

    fn reset(&mut self, with_error: bool) {
        self.parser = EpisodeParser::default();
        self.input.error = with_error;
    }
}

impl ValidatedInput for ParserInput {
    fn label(&self) -> &'static str {
        Self::LABEL
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
        self.input.error = false;
    }

    fn has_error(&self) -> bool {
        self.input.error
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

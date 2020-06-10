use crate::try_opt_ret;
use crate::tui::component::Draw;
use crate::tui::widget_util::{block, style};
use crate::tui::UIBackend;
use termion::event::Key;
use tui::backend::Backend;
use tui::layout::Rect;
use tui::style::Color;
use tui::terminal::Frame;
use tui::widgets::{Paragraph, Text};
use unicode_width::UnicodeWidthChar;

pub struct Input {
    caret: Caret,
    visible_rect: Rect,
    offset: usize,
    pub selected: bool,
    pub error: bool,
}

impl Input {
    const BORDER_SIZE: usize = 2;

    pub fn new(selected: bool) -> Self {
        Self {
            caret: Caret::new(),
            visible_rect: Rect::default(),
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
        let max_width = self.visible_rect.width as usize - Self::BORDER_SIZE - 1;

        if self.caret.pos < max_width {
            self.offset = 0;
            return;
        }

        self.offset = self.caret.pos - max_width;
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
        self.visible_rect = rect;

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
    }

    fn after_draw(&mut self, backend: &mut UIBackend<B>, _: &Self::State) {
        if !self.selected || !backend.will_cursor_fit(self.visible_rect) {
            return;
        }

        if !backend.cursor_visible {
            backend.show_cursor().ok();
        }

        let width = (self.caret.cur_width as u16).min(self.visible_rect.width);
        backend.set_cursor_inside(width, self.visible_rect).ok();
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

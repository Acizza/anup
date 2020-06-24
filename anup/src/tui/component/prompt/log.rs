use crate::tui::component::Draw;
use crate::tui::widget_util::{block, text};
use anyhow::Error;
use std::collections::VecDeque;
use tui::backend::Backend;
use tui::layout::Rect;
use tui::style::Color;
use tui::widgets::{Paragraph, Text};
use tui::Frame;

/// A scrolling status log.
pub struct Log<'a> {
    items: VecDeque<LogItem<'a>>,
    draw_items: VecDeque<Text<'a>>,
    max_items: u16,
}

impl<'a> Log<'a> {
    pub fn new() -> Self {
        Self {
            items: VecDeque::new(),
            draw_items: VecDeque::new(),
            max_items: 1,
        }
    }

    /// Trims the `Log` so all items fit within `size`.
    ///
    /// Assumes there is both a top and bottom border if `with_border` is true.
    pub fn adjust_to_size(&mut self, size: Rect, with_border: bool) {
        self.max_items = if with_border {
            // One border edge is 1 character tall
            size.height.saturating_sub(2)
        } else {
            size.height
        };

        while self.items.len() > self.max_items as usize {
            self.pop_front();
        }
    }

    /// Pushes a new `LogItem` to the end of the log.
    pub fn push<I>(&mut self, item: I)
    where
        I: Into<LogItem<'a>>,
    {
        let item = item.into();
        self.draw_items.extend(item.text_items().iter().cloned());
        self.items.push_back(item);
    }

    pub fn push_err(&mut self, err: &Error) {
        self.push(LogItem::error(format!("{}", err)));

        for cause in err.chain().skip(1) {
            self.push_context(format!("{}", cause));
        }
    }

    pub fn push_context<S>(&mut self, context: S)
    where
        S: AsRef<str>,
    {
        self.push(LogItem::context(context))
    }

    pub fn push_info<S>(&mut self, info: S)
    where
        S: AsRef<str>,
    {
        self.push(LogItem::info(info))
    }

    /// Removes the first `LogItem` from the `StatusLog` if it exists.
    pub fn pop_front(&mut self) {
        let item = match self.items.pop_front() {
            Some(item) => item,
            None => return,
        };

        // Since we only allow pushing items to the back of the log, we can safely
        // pop all of the item's internal elements from the front of the draw list.
        for _ in 0..item.text_items().len() {
            self.draw_items.pop_front();
        }
    }
}

impl<'a, B> Draw<B> for Log<'a>
where
    B: Backend,
{
    type State = ();

    fn draw(&mut self, _: &Self::State, rect: Rect, frame: &mut Frame<B>) {
        self.adjust_to_size(rect, true);

        // TODO: use concat! macro if/when it can accept constants, or when a similiar crate doesn't require nightly
        let title = format!(
            "Error Log [press '{}' for command entry]",
            super::COMMAND_KEY
        );

        let draw_item = Paragraph::new(self.draw_items.iter())
            .block(block::with_borders(title.as_ref()))
            .wrap(true);

        frame.render_widget(draw_item, rect);
    }
}

/// A log entry meant to be used with `StatusLog`.
pub struct LogItem<'a>([Text<'a>; 2]);

impl<'a> LogItem<'a> {
    pub fn error<S>(desc: S) -> Self
    where
        S: AsRef<str>,
    {
        let header = text::with_color("error: ", Color::Red);
        let desc = Text::raw(format!("{}\n", desc.as_ref()));

        Self([header, desc])
    }

    pub fn context<S>(context: S) -> Self
    where
        S: AsRef<str>,
    {
        let header = text::with_color("^ ", Color::Yellow);
        let context = Text::raw(format!("{}\n", context.as_ref()));

        Self([header, context])
    }

    pub fn info<S>(info: S) -> Self
    where
        S: AsRef<str>,
    {
        let header = text::with_color("info: ", Color::Green);
        let info = Text::raw(format!("{}\n", info.as_ref()));

        Self([header, info])
    }

    /// Returns a reference to all of the internal text elements.
    ///
    /// This method is useful for drawing the `LogItem`.
    pub fn text_items(&self) -> &[Text<'a>; 2] {
        &self.0
    }
}

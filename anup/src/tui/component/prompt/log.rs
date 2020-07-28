use crate::tui::component::Draw;
use crate::tui::widget_util::widget::WrapHelper;
use crate::tui::widget_util::{block, text};
use anyhow::Error;
use tui::backend::Backend;
use tui::layout::Rect;
use tui::style::Color;
use tui::text::{Span, Spans};
use tui::widgets::Paragraph;
use tui::Frame;

/// A scrolling status log.
pub struct Log<'a> {
    items: Vec<Spans<'a>>,
    max_items: u16,
}

impl<'a> Log<'a> {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
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

        // TODO: optimize
        while self.items.len() > self.max_items as usize {
            self.pop_front();
        }
    }

    fn push<S, D>(&mut self, header: S, desc: D)
    where
        S: Into<Span<'a>>,
        D: Into<String>,
    {
        let mut desc = desc.into();
        desc.push('\n');

        let items = vec![header.into(), desc.into()];
        let spans = Spans::from(items);

        self.items.push(spans);
    }

    pub fn push_error(&mut self, err: &Error) {
        self.push_error_msg(format!("{}", err));

        for cause in err.chain().skip(1) {
            self.push_context(format!("{}", cause));
        }
    }

    pub fn push_error_msg<S>(&mut self, desc: S)
    where
        S: Into<String>,
    {
        self.push(text::with_color("error: ", Color::Red), desc)
    }

    pub fn push_context<S>(&mut self, context: S)
    where
        S: Into<String>,
    {
        self.push(text::with_color("^ ", Color::Yellow), context)
    }

    pub fn push_info<S>(&mut self, info: S)
    where
        S: Into<String>,
    {
        self.push(text::with_color("info: ", Color::Green), info)
    }

    /// Removes the first `LogItem` from the `StatusLog` if it exists.
    pub fn pop_front(&mut self) {
        if self.items.is_empty() {
            return;
        }

        self.items.remove(0);
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

        let draw_item = Paragraph::new(self.items.clone())
            .block(block::with_borders(title.as_ref()))
            .wrapped();

        frame.render_widget(draw_item, rect);
    }
}

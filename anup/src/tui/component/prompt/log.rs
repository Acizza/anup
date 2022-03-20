use std::collections::VecDeque;

use anyhow::Error;
use tui::backend::Backend;
use tui::layout::Rect;
use tui::style::Color;
use tui::text::Span;
use tui::Frame;
use tui_utils::{
    helpers::{block, style},
    widgets::Fragment,
    wrap,
};

#[derive(Copy, Clone)]
pub enum LogKind {
    Error,
    Context,
}

impl<'a> Into<Span<'a>> for LogKind {
    fn into(self) -> Span<'a> {
        match self {
            Self::Error => Span::styled("error: ", style::fg(Color::Red)),
            Self::Context => Span::styled("^ ", style::fg(Color::Yellow)),
        }
    }
}

pub struct LogEntry<'a> {
    kind: LogKind,
    message: Span<'a>,
}

impl<'a> LogEntry<'a> {
    fn new<S>(kind: LogKind, message: S) -> Self
    where
        S: Into<Span<'a>>,
    {
        Self {
            kind,
            message: message.into(),
        }
    }

    fn as_fragments(&self) -> [Fragment<'a>; 2] {
        [
            Fragment::span(self.kind),
            Fragment::span(self.message.clone()),
        ]
    }
}

impl<'a> Into<[Fragment<'a>; 2]> for &'a LogEntry<'a> {
    fn into(self) -> [Fragment<'a>; 2] {
        self.as_fragments()
    }
}

/// A scrolling status log.
pub struct Log<'a> {
    items: VecDeque<LogEntry<'a>>,
    max_items: u8,
    title: String,
}

impl<'a> Log<'a> {
    pub fn new(max_items: u8) -> Self {
        let title = format!(
            "Error Log [press '{}' for command entry]",
            super::COMMAND_KEY
        );

        Self {
            items: VecDeque::with_capacity(max_items as usize),
            max_items,
            title,
        }
    }

    pub fn push<S>(&mut self, kind: LogKind, msg: S)
    where
        S: Into<Span<'a>>,
    {
        while self.items.len() >= self.max_items as usize {
            self.items.pop_front();
        }

        let entry = LogEntry::new(kind, msg);
        self.items.push_back(entry);
    }

    pub fn push_error(&mut self, err: &Error) {
        self.push(LogKind::Error, format!("{}", err));

        for cause in err.chain().skip(1) {
            self.push(LogKind::Context, format!("{}", cause));
        }
    }

    pub fn draw<B: Backend>(&self, rect: Rect, frame: &mut Frame<B>) {
        let block = block::with_borders(self.title.as_str());
        let block_area = block.inner(rect);

        frame.render_widget(block, rect);

        let items = self
            .items
            .iter()
            .map(LogEntry::as_fragments)
            .map(wrap::by_newlines)
            .map(|fragments| wrap::by_letters(fragments, block_area.width));

        let log = tui_utils::widgets::Log::new(items);

        frame.render_widget(log, block_area);
    }
}

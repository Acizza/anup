use super::style;
use tui::style::Color;
use tui::widgets::{Block, Borders};

pub fn with_borders<'a, S>(title: S) -> Block<'a>
where
    S: Into<Option<&'a str>>,
{
    let mut block = Block::default().borders(Borders::ALL);

    if let Some(title) = title.into() {
        block = block.title(title);
    }

    block
}

pub fn selectable<'a, S>(title: S, selected: bool) -> Block<'a>
where
    S: Into<Option<&'a str>>,
{
    let mut block = with_borders(title);

    if selected {
        block = block.border_style(style::fg(Color::Blue));
    }

    block
}

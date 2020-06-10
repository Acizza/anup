use tui::style::{Color, Style};
use tui::widgets::{Block, Borders};

pub fn selectable<'a, S>(title: S, selected: bool) -> Block<'a>
where
    S: Into<Option<&'a str>>,
{
    let mut block = Block::default().borders(Borders::ALL);

    if let Some(title) = title.into() {
        block = block.title(title);
    }

    if selected {
        block = block.border_style(Style::default().fg(Color::Blue));
    }

    block
}

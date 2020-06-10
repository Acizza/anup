use tui::style::{Color, Modifier, Style};

#[inline(always)]
pub fn bold() -> Style {
    Style::default().modifier(Modifier::BOLD)
}

#[inline(always)]
pub fn italic() -> Style {
    Style::default().modifier(Modifier::ITALIC)
}

#[inline(always)]
pub fn fg(color: Color) -> Style {
    Style::default().fg(color)
}

pub fn list_selector(can_select: bool) -> Style {
    let color = if can_select {
        Color::Green
    } else {
        Color::DarkGray
    };

    fg(color)
}

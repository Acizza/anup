use tui::style::{Color, Style};

pub fn list_selector(can_select: bool) -> Style {
    let color = if can_select {
        Color::Green
    } else {
        Color::DarkGray
    };

    Style::default().fg(color)
}

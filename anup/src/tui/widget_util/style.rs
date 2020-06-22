use super::color;
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

#[inline(always)]
pub fn list_selector(can_select: bool) -> Style {
    list_selector_with(can_select, Color::Green)
}

#[inline(always)]
pub fn list_selector_with(can_select: bool, selected_color: Color) -> Style {
    fg_either(can_select, selected_color, Color::DarkGray)
}

pub fn fg_either(value: bool, true_color: Color, false_color: Color) -> Style {
    let color = color::either(value, true_color, false_color);
    fg(color)
}

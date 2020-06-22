use tui::style::Color;

#[inline(always)]
pub fn either(value: bool, true_color: Color, false_color: Color) -> Color {
    if value {
        true_color
    } else {
        false_color
    }
}

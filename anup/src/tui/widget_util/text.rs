use std::borrow::Cow;
use tui::style::{Color, Modifier, Style};
use tui::widgets::Text;

#[inline(always)]
pub fn bold<'a, S>(text: S) -> Text<'a>
where
    S: Into<Cow<'a, str>>,
{
    Text::styled(text, Style::default().modifier(Modifier::BOLD))
}

#[inline(always)]
pub fn italic<'a, S>(text: S) -> Text<'a>
where
    S: Into<Cow<'a, str>>,
{
    Text::styled(text, Style::default().modifier(Modifier::ITALIC))
}

#[inline(always)]
pub fn hint<'a, S>(text: S) -> Text<'a>
where
    S: Into<Cow<'a, str>>,
{
    with_color(text, Color::DarkGray)
}

#[inline(always)]
pub fn with_color<'a, S>(text: S, color: Color) -> Text<'a>
where
    S: Into<Cow<'a, str>>,
{
    Text::styled(text, Style::default().fg(color))
}

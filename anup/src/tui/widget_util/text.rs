use super::style;
use std::borrow::Cow;
use tui::style::{Color, Style};
use tui::text::Span;

#[inline(always)]
pub fn bold_with<'a, S, F>(text: S, extra_style: F) -> Span<'a>
where
    S: Into<Cow<'a, str>>,
    F: FnOnce(Style) -> Style,
{
    Span::styled(text, extra_style(style::bold()))
}

#[inline(always)]
pub fn bold<'a, S>(text: S) -> Span<'a>
where
    S: Into<Cow<'a, str>>,
{
    bold_with(text, |s| s)
}

#[inline(always)]
pub fn italic_with<'a, S, F>(text: S, extra_style: F) -> Span<'a>
where
    S: Into<Cow<'a, str>>,
    F: FnOnce(Style) -> Style,
{
    Span::styled(text, extra_style(style::italic()))
}

#[inline(always)]
pub fn italic<'a, S>(text: S) -> Span<'a>
where
    S: Into<Cow<'a, str>>,
{
    italic_with(text, |s| s)
}

#[inline(always)]
pub fn hint<'a, S>(text: S) -> Span<'a>
where
    S: Into<Cow<'a, str>>,
{
    with_color(text, Color::DarkGray)
}

#[inline(always)]
pub fn with_color<'a, S>(text: S, color: Color) -> Span<'a>
where
    S: Into<Cow<'a, str>>,
{
    Span::styled(text, style::fg(color))
}

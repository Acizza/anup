use super::ShouldReset;
use crate::tui::state::UIState;
use crate::{key::Key, tui::component::Component};
use anyhow::{anyhow, Context, Result};
use crossterm::event::KeyCode;
use std::fs;
use tui::layout::{Alignment, Direction, Rect};
use tui::style::Color;
use tui::terminal::Frame;
use tui::{backend::Backend, text::Span};
use tui_utils::{
    helpers::{block, text},
    layout::{RectExt, SimpleLayout},
    widgets::{Fragment, OverflowMode, SimpleText, SpanOptions, TextFragments},
    wrap,
};

pub struct DeleteSeriesPanel {
    remove_files: RemoveFiles,
    removal_warning_text: String,
    series_path_text: String,
}

impl DeleteSeriesPanel {
    pub fn init(state: &UIState) -> Result<Self> {
        let series = match state.series.selected() {
            Some(series) => series,
            None => return Err(anyhow!("must select a series to delete")),
        };

        let removal_warning_text = format!("{} will be removed", series.nickname());
        let series_path_text = series.path().inner().to_string_lossy().into_owned();

        Ok(Self {
            remove_files: RemoveFiles::default(),
            removal_warning_text,
            series_path_text,
        })
    }

    fn delete_selected_series(&self, state: &mut UIState) -> Result<()> {
        let series = state.delete_selected_series()?;

        if let RemoveFiles::Yes = self.remove_files {
            let path = series.config().path.absolute(&state.config);
            fs::remove_dir_all(path).context("failed to remove directory")?;
        }

        Ok(())
    }

    fn draw_remove_files_warning<B: Backend>(
        &self,
        path_rect: Rect,
        status_rect: Rect,
        frame: &mut Frame<B>,
    ) {
        let path_fragments = [
            Fragment::span(text::bold("Series Path:")),
            Fragment::Line,
            Fragment::Span(
                text::italic(&self.series_path_text),
                SpanOptions::new().overflow(OverflowMode::Truncate),
            ),
        ];

        // TODO: use std::array::IntoIter in Rust 1.51.0
        let wrapped_path_frags = wrap::by_letters(path_fragments.iter().cloned(), path_rect.width);
        let path_widget = TextFragments::new(&wrapped_path_frags).alignment(Alignment::Center);

        frame.render_widget(path_widget, path_rect);

        let delete_status_text = match self.remove_files {
            RemoveFiles::Yes => text::bold_with("will be deleted.", |s| s.fg(Color::Red)),
            RemoveFiles::No => text::bold("will not be deleted."),
        };

        let full_status_frags = {
            let frags = [
                Fragment::span(Span::raw("The series path on disk ")),
                Fragment::Span(
                    delete_status_text,
                    SpanOptions::new().overflow(OverflowMode::Truncate),
                ),
            ];

            // TODO: use std::array::IntoIter in Rust 1.51.0
            wrap::by_letters(frags.iter().cloned(), status_rect.width)
        };

        let status_widget = TextFragments::new(&full_status_frags).alignment(Alignment::Center);
        frame.render_widget(status_widget, status_rect);
    }

    fn draw_hints<B: Backend>(rect: Rect, frame: &mut Frame<B>) {
        let horiz_layout =
            SimpleLayout::new(Direction::Horizontal).split_evenly(rect.lines_from_bottom(1));

        let hint_text = text::hint("D - Toggle path deletion");
        let hint_widget = SimpleText::new(hint_text).alignment(Alignment::Center);
        frame.render_widget(hint_widget, horiz_layout.left);

        let hint_text = text::hint("Enter - Confirm");
        let hint_widget = SimpleText::new(hint_text).alignment(Alignment::Center);
        frame.render_widget(hint_widget, horiz_layout.right);
    }

    pub fn draw<B: Backend>(&mut self, rect: Rect, frame: &mut Frame<B>) {
        let block = block::with_borders("Delete Series");
        let block_area = block.inner(rect);

        frame.render_widget(block, rect);

        let vert_fields = SimpleLayout::new(Direction::Vertical)
            .horizontal_margin(2)
            .vertical_margin(1)
            .split_quarters(block_area);

        let warning_text = text::bold_with(&self.removal_warning_text, |s| s.fg(Color::Red));
        let warning_widget = SimpleText::new(warning_text)
            .alignment(Alignment::Center)
            .overflow(OverflowMode::Truncate);

        frame.render_widget(warning_widget, vert_fields.first);

        // Center of third vertical field
        let status_pos = SimpleLayout::new(Direction::Vertical)
            .split_evenly(vert_fields.third)
            .right;

        self.draw_remove_files_warning(vert_fields.second, status_pos, frame);
        Self::draw_hints(vert_fields.fourth, frame);
    }
}

impl Component for DeleteSeriesPanel {
    type State = UIState;
    type KeyResult = Result<ShouldReset>;

    fn process_key(&mut self, key: Key, state: &mut Self::State) -> Self::KeyResult {
        match *key {
            KeyCode::Esc => Ok(ShouldReset::Yes),
            KeyCode::Char('d') => {
                self.remove_files.toggle();
                Ok(ShouldReset::No)
            }
            KeyCode::Enter => {
                self.delete_selected_series(state)?;
                Ok(ShouldReset::Yes)
            }
            _ => Ok(ShouldReset::No),
        }
    }
}

#[derive(Copy, Clone)]
enum RemoveFiles {
    Yes,
    No,
}

impl RemoveFiles {
    fn next(self) -> Self {
        match self {
            Self::Yes => Self::No,
            Self::No => Self::Yes,
        }
    }

    #[inline(always)]
    fn toggle(&mut self) {
        *self = self.next();
    }
}

impl Default for RemoveFiles {
    fn default() -> Self {
        Self::No
    }
}

use super::MergedSeries;
use crate::tui::component::Component;
use crate::tui::widget_util::{block, color, style, text};
use crate::tui::UIState;
use crate::{key::Key, series::SeriesPath};
use anime::remote::SeriesInfo as RemoteInfo;
use anyhow::Result;
use crossterm::event::KeyCode;
use tui::layout::{Alignment, Direction, Rect};
use tui::style::Color;
use tui::terminal::Frame;
use tui::{backend::Backend, text::Span};
use tui_utils::{
    layout::{BasicConstraint, SimpleLayout},
    list::WrappingIndex,
    widgets::{SimpleTable, SimpleText},
};

#[derive(Default)]
pub struct SplitPanel {
    selected_series: WrappingIndex,
    merged_series: Vec<MergedSeries>,
    has_split_series: bool,
}

impl SplitPanel {
    pub(super) fn new(merged_series: Vec<MergedSeries>) -> Self {
        Self {
            selected_series: WrappingIndex::new(0),
            merged_series,
            has_split_series: false,
        }
    }

    fn draw_merged_series_table<B>(&self, rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
    {
        let row_color = color::either(self.has_split_series, Color::Blue, Color::Yellow);

        let rows = self.merged_series.iter().map(|merged| match merged {
            &MergedSeries::Failed(kind) => {
                let kind: &'static str = kind.into();

                [
                    text::with_color(kind, Color::Red),
                    text::with_color("Failed..", Color::Red),
                ]
            }
            MergedSeries::Resolved(series) => {
                let kind: &'static str = series.info.kind.into();

                [
                    text::with_color(kind, row_color),
                    text::with_color(series.info.title.preferred.as_str(), row_color),
                ]
            }
        });

        let header = [Span::raw("Type"), Span::raw("Series")];
        let layout = [BasicConstraint::Length(8), BasicConstraint::Percentage(100)];

        let table = SimpleTable::new(rows, &layout)
            .header(&header)
            .highlight_symbol(Span::styled(
                ">",
                style::list_selector(self.has_split_series),
            ));

        frame.render_widget(table, rect);
    }

    fn draw_no_series_msg<B: Backend>(area: Rect, frame: &mut Frame<B>) {
        let center = SimpleLayout::new(Direction::Vertical)
            .split_evenly(area)
            .right;

        let msg = SimpleText::new(text::bold("No Series To Split")).alignment(Alignment::Center);
        frame.render_widget(msg, center);
    }

    pub fn draw<B: Backend>(&mut self, area: Rect, frame: &mut Frame<B>) {
        let block = block::with_borders(None);
        let block_area = block.inner(area);

        frame.render_widget(block, area);

        if self.merged_series.is_empty() {
            Self::draw_no_series_msg(area, frame);
            return;
        }

        let vert_split = SimpleLayout::new(Direction::Vertical).split(
            block_area,
            &[
                BasicConstraint::MinLenRemaining(4, 2),
                BasicConstraint::Length(2),
            ],
        );

        self.draw_merged_series_table(vert_split[0], frame);

        let hint_layout = SimpleLayout::new(Direction::Horizontal).split_evenly(vert_split[1]);

        let hint = SimpleText::new(text::hint("S - Split All")).alignment(Alignment::Center);
        frame.render_widget(hint, hint_layout.left);

        let hint = SimpleText::new(text::hint("Enter - Add Series")).alignment(Alignment::Center);
        frame.render_widget(hint, hint_layout.right);
    }
}

impl Component for SplitPanel {
    type State = UIState;
    type KeyResult = Result<SplitResult>;

    fn process_key(&mut self, key: Key, state: &mut Self::State) -> Self::KeyResult {
        match *key {
            KeyCode::Esc => Ok(SplitResult::Reset),
            KeyCode::Char('s') => {
                MergedSeries::split_all(&self.merged_series, &state.config)?;

                self.has_split_series = true;
                *self.selected_series.get_mut() = 0;

                Ok(SplitResult::Ok)
            }
            KeyCode::Enter => {
                if !self.has_split_series {
                    return Ok(SplitResult::Ok);
                }

                let selected_idx = self.selected_series.get();
                let selected = self.merged_series.get(selected_idx);

                let series = match selected {
                    Some(MergedSeries::Resolved(series)) => series,
                    Some(MergedSeries::Failed(_)) | None => return Ok(SplitResult::Ok),
                };

                Ok(SplitResult::AddSeries(
                    series.info.clone(),
                    series.out_dir.clone(),
                ))
            }
            _ => {
                if !self.has_split_series {
                    return Ok(SplitResult::Ok);
                }

                match *key {
                    KeyCode::Up => self.selected_series.decrement(self.merged_series.len()),
                    KeyCode::Down => self.selected_series.increment(self.merged_series.len()),
                    _ => (),
                }

                Ok(SplitResult::Ok)
            }
        }
    }
}

pub enum SplitResult {
    Ok,
    Reset,
    AddSeries(RemoteInfo, SeriesPath),
}

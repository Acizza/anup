use super::MergedSeries;
use crate::tui::component::Component;
use crate::tui::widget_util::{block, color, style, text, SelectWidgetState};
use crate::tui::UIState;
use crate::{key::Key, series::SeriesPath};
use anime::remote::SeriesInfo as RemoteInfo;
use anyhow::Result;
use crossterm::event::KeyCode;
use tui::backend::Backend;
use tui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use tui::style::Color;
use tui::terminal::Frame;
use tui::widgets::{Row, Table, TableState};
use tui_utils::{layout::SimpleLayout, widgets::SimpleText};

#[derive(Default)]
pub struct SplitPanel {
    split_table: SelectWidgetState<TableState>,
    merged_series: Vec<MergedSeries>,
    has_split_series: bool,
}

impl SplitPanel {
    pub(super) fn new(merged_series: Vec<MergedSeries>) -> Self {
        Self {
            split_table: SelectWidgetState::unselected(),
            merged_series,
            has_split_series: false,
        }
    }

    fn draw_merged_series_table<B>(&mut self, rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
    {
        let row_color = color::either(self.has_split_series, Color::Blue, Color::Yellow);

        let rows = self.merged_series.iter().map(|merged| {
            let (rows, color) = match merged {
                &MergedSeries::Failed(cat) => (vec![cat.into(), "Failed.."], Color::Red),
                MergedSeries::Resolved(series) => (
                    vec![
                        series.info.kind.into(),
                        series.info.title.preferred.as_str(),
                    ],
                    row_color,
                ),
            };

            Row::new(rows).style(style::fg(color))
        });

        let header = Row::new(vec!["Type", "Series"]);

        let table = Table::new(rows)
            .header(header)
            .widths([Constraint::Length(8), Constraint::Percentage(100)].as_ref())
            .highlight_symbol(">")
            .highlight_style(style::list_selector(self.has_split_series))
            .column_spacing(2);

        frame.render_stateful_widget(table, rect, &mut self.split_table);
    }

    pub fn draw<B: Backend>(&mut self, rect: Rect, frame: &mut Frame<B>) {
        let outline = block::with_borders(None);
        frame.render_widget(outline, rect);

        let vert_split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(4), Constraint::Length(2)].as_ref())
            .horizontal_margin(1)
            .split(rect);

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
                self.split_table.select(Some(0));

                Ok(SplitResult::Ok)
            }
            KeyCode::Enter => {
                if !self.has_split_series {
                    return Ok(SplitResult::Ok);
                }

                let selected = self
                    .split_table
                    .selected()
                    .and_then(|idx| self.merged_series.get(idx));

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
                if self.has_split_series {
                    self.split_table
                        .update_selected(key, self.merged_series.len());
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

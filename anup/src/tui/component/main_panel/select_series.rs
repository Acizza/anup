use crate::series::info::SeriesInfo;
use crate::series::SeriesParams;
use crate::tui::Selection;
use std::path::PathBuf;
use termion::event::Key;
use tui::backend::Backend;
use tui::layout::Rect;
use tui::style::{Color, Modifier, Style};
use tui::terminal::Frame;
use tui::widgets::{Block, Borders, List, ListState, Text};

pub struct SelectSeriesPanel {
    list_state: ListState,
}

impl SelectSeriesPanel {
    pub fn new() -> Self {
        Self {
            list_state: ListState::default(),
        }
    }

    pub fn process_key(&mut self, key: Key, state: &mut SelectState) -> SelectInputResult {
        match key {
            Key::Up => {
                state.series_list.dec_selected();
                SelectInputResult::Continue
            }
            Key::Down => {
                state.series_list.inc_selected();
                SelectInputResult::Continue
            }
            Key::Char('\n') => {
                let info = match state.series_list.swap_remove_selected() {
                    Some(info) => info,
                    None => return SelectInputResult::Finish,
                };

                SelectInputResult::AddSeries(info)
            }
            Key::Esc => SelectInputResult::Finish,
            _ => SelectInputResult::Continue,
        }
    }

    pub fn draw<B>(&mut self, state: &SelectState, rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
    {
        let names = state
            .series_list
            .iter()
            .map(|info| Text::raw(&info.title_preferred));

        let items = List::new(names)
            .block(
                Block::default()
                    .title("Select a series from the list")
                    .borders(Borders::ALL),
            )
            .style(Style::default().fg(Color::White))
            .highlight_style(Style::default().fg(Color::Green).modifier(Modifier::ITALIC))
            .highlight_symbol(">");

        self.list_state.select(Some(state.series_list.index()));

        frame.render_stateful_widget(items, rect, &mut self.list_state);
    }
}

pub enum SelectInputResult {
    Continue,
    Finish,
    AddSeries(SeriesInfo),
}

#[derive(Debug)]
pub struct SelectState {
    pub series_list: Selection<SeriesInfo>,
    pub params: SeriesParams,
    pub path: PathBuf,
    pub nickname: String,
}

impl SelectState {
    pub fn new<I, S>(series_list: I, params: SeriesParams, path: PathBuf, nickname: S) -> Self
    where
        I: Into<Selection<SeriesInfo>>,
        S: Into<String>,
    {
        Self {
            series_list: series_list.into(),
            params,
            path,
            nickname: nickname.into(),
        }
    }
}

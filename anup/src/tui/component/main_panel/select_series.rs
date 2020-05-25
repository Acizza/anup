use crate::series::info::SeriesInfo;
use crate::series::SeriesParams;
use crate::tui::component::{Component, Draw};
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
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            list_state: ListState::default(),
        }
    }
}

impl Component for SelectSeriesPanel {
    type State = SelectState;
    type KeyResult = KeyResult;

    fn process_key(&mut self, key: Key, select: &mut Self::State) -> Self::KeyResult {
        match key {
            Key::Up => {
                select.series_list.dec_selected();
                KeyResult::Ok
            }
            Key::Down => {
                select.series_list.inc_selected();
                KeyResult::Ok
            }
            Key::Char('\n') => {
                let info = match select.series_list.swap_remove_selected() {
                    Some(info) => info,
                    None => return KeyResult::Reset,
                };

                KeyResult::AddSeries(info)
            }
            Key::Esc => KeyResult::Reset,
            _ => KeyResult::Ok,
        }
    }
}

impl<B> Draw<B> for SelectSeriesPanel
where
    B: Backend,
{
    type State = SelectState;

    fn draw(&mut self, select: &Self::State, rect: Rect, frame: &mut Frame<B>) {
        let names = select
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

        self.list_state.select(Some(select.series_list.index()));

        frame.render_stateful_widget(items, rect, &mut self.list_state);
    }
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

pub enum KeyResult {
    Ok,
    AddSeries(SeriesInfo),
    Reset,
}

use crate::series::SeriesParams;
use crate::tui::component::Component;
use crate::tui::widget_util::{block, style};
use crate::tui::Selection;
use crate::{key::Key, series::info::SeriesInfo};
use crossterm::event::KeyCode;
use tui::backend::Backend;
use tui::layout::Rect;
use tui::style::Color;
use tui::terminal::Frame;
use tui::widgets::{List, ListItem, ListState};

pub struct SelectSeriesPanel {
    list_state: ListState,
    state: SelectState,
}

impl SelectSeriesPanel {
    #[inline(always)]
    pub fn new(state: SelectState) -> Self {
        Self {
            list_state: ListState::default(),
            state,
        }
    }

    #[inline(always)]
    pub fn take_params(self) -> SeriesParams {
        self.state.params
    }

    pub fn draw<B: Backend>(&mut self, rect: Rect, frame: &mut Frame<B>) {
        let names = self
            .state
            .series_list
            .iter()
            .map(|info| ListItem::new(info.title_preferred.as_ref()))
            .collect::<Vec<_>>();

        let items = List::new(names)
            .block(block::with_borders("Select a series from the list"))
            .style(style::fg(Color::White))
            .highlight_style(style::italic().fg(Color::Green))
            .highlight_symbol(">");

        self.list_state.select(Some(self.state.series_list.index()));

        frame.render_stateful_widget(items, rect, &mut self.list_state);
    }
}

impl Component for SelectSeriesPanel {
    type State = ();
    type KeyResult = SelectSeriesResult;

    fn process_key(&mut self, key: Key, _: &mut Self::State) -> Self::KeyResult {
        match *key {
            KeyCode::Up => {
                self.state.series_list.dec_selected();
                SelectSeriesResult::Ok
            }
            KeyCode::Down => {
                self.state.series_list.inc_selected();
                SelectSeriesResult::Ok
            }
            KeyCode::Enter => {
                let info = match self.state.series_list.swap_remove_selected() {
                    Some(info) => info,
                    None => return SelectSeriesResult::Reset,
                };

                SelectSeriesResult::AddSeries(info)
            }
            KeyCode::Esc => SelectSeriesResult::Reset,
            _ => SelectSeriesResult::Ok,
        }
    }
}

pub struct SelectState {
    pub series_list: Selection<SeriesInfo>,
    pub params: SeriesParams,
}

impl SelectState {
    pub fn new<I>(series_list: I, params: SeriesParams) -> Self
    where
        I: Into<Selection<SeriesInfo>>,
    {
        Self {
            series_list: series_list.into(),
            params,
        }
    }
}

pub enum SelectSeriesResult {
    Ok,
    AddSeries(SeriesInfo),
    Reset,
}

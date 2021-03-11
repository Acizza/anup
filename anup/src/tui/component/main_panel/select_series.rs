use crate::tui::component::Component;
use crate::tui::widget_util::block;
use crate::{key::Key, series::info::SeriesInfo};
use crate::{series::SeriesParams, tui::widget_util::text};
use crossterm::event::KeyCode;
use tui::layout::Rect;
use tui::style::Color;
use tui::terminal::Frame;
use tui::{backend::Backend, text::Span};
use tui_utils::{list::WrappedSelection, widgets::SimpleList};

pub struct SelectSeriesPanel {
    state: SelectState,
}

impl SelectSeriesPanel {
    #[inline(always)]
    pub fn new(state: SelectState) -> Self {
        Self { state }
    }

    #[inline(always)]
    pub fn take_params(self) -> SeriesParams {
        self.state.params
    }

    pub fn draw<B: Backend>(&mut self, rect: Rect, frame: &mut Frame<B>) {
        let block = block::with_borders("Select a series from the list");
        let block_area = block.inner(rect);

        frame.render_widget(block, rect);

        let names = self
            .state
            .series_list
            .iter()
            .map(|info| Span::raw(info.title_preferred.as_str()));

        let items = SimpleList::new(names)
            .highlight_symbol(text::italic_with(">", |s| s.fg(Color::Green)))
            .select(Some(self.state.series_list.index() as u16));

        frame.render_widget(items, block_area);
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
                if !self.state.series_list.is_valid_index() {
                    return SelectSeriesResult::Reset;
                }

                let selected = self.state.series_list.index();
                let info = self.state.series_list.swap_remove(selected);

                SelectSeriesResult::AddSeries(info)
            }
            KeyCode::Esc => SelectSeriesResult::Reset,
            _ => SelectSeriesResult::Ok,
        }
    }
}

pub struct SelectState {
    pub series_list: WrappedSelection<Vec<SeriesInfo>, SeriesInfo>,
    pub params: SeriesParams,
}

impl SelectState {
    pub fn new(series_list: Vec<SeriesInfo>, params: SeriesParams) -> Self {
        Self {
            series_list: WrappedSelection::new(series_list),
            params,
        }
    }
}

pub enum SelectSeriesResult {
    Ok,
    AddSeries(SeriesInfo),
    Reset,
}

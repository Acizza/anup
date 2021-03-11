use crate::tui::state::{InputState, UIState};
use crate::{key::Key, series::LoadedSeries};
use anime::remote::Status;
use crossterm::event::KeyCode;
use tui::backend::Backend;
use tui::layout::Rect;
use tui::style::Color;
use tui::terminal::Frame;
use tui::text::Span;
use tui_utils::{
    helpers::{block, style, text},
    widgets::SimpleList,
};

pub struct SeriesList;

impl SeriesList {
    fn series_text(series: &LoadedSeries) -> Span {
        match series {
            LoadedSeries::Complete(series) => {
                let color = match series.data.entry.status() {
                    Status::Watching | Status::Rewatching => Color::Blue,
                    Status::Completed => Color::Green,
                    Status::OnHold => Color::Yellow,
                    Status::Dropped => Color::Red,
                    Status::PlanToWatch => Color::Gray,
                };

                text::with_color(series.data.config.nickname.as_str(), color)
            }
            LoadedSeries::Partial(data, _) => {
                text::with_color(data.config.nickname.as_str(), Color::LightRed)
            }
            LoadedSeries::None(cfg, _) => text::with_color(cfg.nickname.as_str(), Color::LightRed),
        }
    }

    pub fn process_key(key: Key, state: &mut UIState) {
        if !matches!(*key, KeyCode::Up | KeyCode::Down) {
            return;
        }

        match *key {
            KeyCode::Up => state.series.dec_selected(),
            KeyCode::Down => state.series.inc_selected(),
            _ => (),
        }

        state.init_selected_series();
    }

    pub fn draw<B: Backend>(state: &UIState, rect: Rect, frame: &mut Frame<B>) {
        let highlight_style = match state.input_state {
            InputState::Idle => style::italic().fg(Color::Green),
            _ => style::italic().fg(Color::DarkGray),
        };

        let block = block::with_borders("Series");
        let list_area = block.inner(rect);

        let series_names = state.series.iter().map(Self::series_text);

        let list = SimpleList::new(series_names)
            .select(state.series.index() as u16)
            .highlight_symbol(Span::styled(">", highlight_style));

        frame.render_widget(block, rect);
        frame.render_widget(list, list_area);
    }
}

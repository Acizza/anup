use super::{Component, Draw};
use crate::tui::widget_util::{block, style, text};
use crate::tui::{CurrentAction, UIState};
use crate::Args;
use crate::{
    key::Key,
    series::{LastWatched, LoadedSeries},
};
use anime::remote::Status;
use anyhow::Result;
use crossterm::event::KeyCode;
use tui::backend::Backend;
use tui::layout::Rect;
use tui::style::Color;
use tui::terminal::Frame;
use tui::text::Span;
use tui_utils::widgets::SimpleList;

pub struct SeriesList;

impl SeriesList {
    pub fn init(args: &Args, state: &mut UIState, last_watched: &LastWatched) -> Self {
        let selected = {
            let desired_series = args.series.as_ref().or_else(|| last_watched.get());

            match desired_series {
                Some(desired) => state
                    .series
                    .iter()
                    .position(|series| series.nickname() == desired)
                    .unwrap_or(0),
                None => 0,
            }
        };

        state.series.set_selected(selected);
        state.init_selected_series();

        Self {}
    }

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
}

impl Component for SeriesList {
    type State = UIState;
    type KeyResult = Result<()>;

    fn process_key(&mut self, key: Key, state: &mut Self::State) -> Self::KeyResult {
        match *key {
            KeyCode::Up | KeyCode::Down => {
                match *key {
                    KeyCode::Up => state.series.dec_selected(),
                    KeyCode::Down => state.series.inc_selected(),
                    _ => unreachable!(),
                }

                state.init_selected_series();
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

impl<B> Draw<B> for SeriesList
where
    B: Backend,
{
    type State = UIState;

    fn draw(&mut self, state: &Self::State, rect: Rect, frame: &mut Frame<B>) {
        let highlight_style = match &state.current_action {
            CurrentAction::Idle => style::italic().fg(Color::Green),
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

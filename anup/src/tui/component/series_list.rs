use super::{Component, Draw};
use crate::series::LastWatched;
use crate::tui::{LogResult, UIState};
use crate::CmdOptions;
use smallvec::SmallVec;
use termion::event::Key;
use tui::backend::Backend;
use tui::layout::Rect;
use tui::style::{Color, Modifier, Style};
use tui::terminal::Frame;
use tui::widgets::{Block, Borders, SelectableList, Widget};

pub struct SeriesList;

impl SeriesList {
    pub fn init(args: &CmdOptions, state: &mut UIState, last_watched: &LastWatched) -> Self {
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
}

impl Component for SeriesList {
    fn process_key(&mut self, key: Key, state: &mut UIState) -> LogResult {
        match key {
            Key::Up | Key::Down => {
                match key {
                    Key::Up => state.series.dec_selected(),
                    Key::Down => state.series.inc_selected(),
                    _ => unreachable!(),
                }

                state.init_selected_series();
                LogResult::Ok
            }
            _ => LogResult::Ok,
        }
    }
}

impl<B> Draw<B> for SeriesList
where
    B: Backend,
{
    fn draw(&mut self, state: &UIState, rect: Rect, frame: &mut Frame<B>) {
        let series_names = state
            .series
            .iter()
            .map(|series| series.nickname())
            .collect::<SmallVec<[_; 8]>>();

        let mut series_list = SelectableList::default()
            .block(Block::default().title("Series").borders(Borders::ALL))
            .items(&series_names)
            .select(Some(state.series.index()))
            .style(Style::default().fg(Color::White))
            .highlight_style(Style::default().fg(Color::Green).modifier(Modifier::ITALIC))
            .highlight_symbol(">");

        series_list.render(frame, rect);
    }
}

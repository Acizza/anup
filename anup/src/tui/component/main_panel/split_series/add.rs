use super::SplitPanelResult;
use crate::series::{self, SeriesParams, SeriesPath};
use crate::try_opt_ret;
use crate::tui::component::input::{Input, InputFlags, NameInput, ParsedValue, ValidatedInput};
use crate::tui::component::{Component, Draw};
use crate::tui::widget_util::{block, text};
use crate::tui::UIState;
use crate::{series::config::SeriesConfig, tui::backend::Key};
use anime::local::EpisodeParser;
use anime::remote::SeriesInfo as RemoteInfo;
use anyhow::Result;
use crossterm::event::KeyCode;
use tui::backend::Backend;
use tui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use tui::style::Color;
use tui::terminal::Frame;
use tui::widgets::Paragraph;

pub struct AddPanel {
    name_input: NameInput,
    data: Option<PartialData>,
}

impl AddPanel {
    pub fn new(info: RemoteInfo, path: SeriesPath) -> Self {
        let name_input = series::generate_nickname(&info.title.preferred).map_or_else(
            || NameInput::new(InputFlags::SELECTED),
            |nickname| NameInput::with_placeholder(InputFlags::SELECTED, nickname),
        );

        Self {
            name_input,
            data: Some(PartialData::new(info, path)),
        }
    }
}

impl Component for AddPanel {
    type State = UIState;
    type KeyResult = Result<SplitPanelResult>;

    #[allow(clippy::cast_possible_wrap)]
    fn process_key(&mut self, key: Key, state: &mut Self::State) -> Self::KeyResult {
        match *key {
            KeyCode::Esc => Ok(SplitPanelResult::Reset),
            KeyCode::Enter => {
                self.name_input.validate();

                if self.name_input.has_error() {
                    return Ok(SplitPanelResult::Ok);
                }

                let data = match self.data.take() {
                    Some(data) => data,
                    None => return Ok(SplitPanelResult::Reset),
                };

                let name = self.name_input.parsed_value();
                let params = SeriesParams::new(name, data.path, EpisodeParser::Default);
                let sconfig = SeriesConfig::new(data.info.id as i32, params, &state.db)?;

                Ok(SplitPanelResult::add_series(data.info, sconfig))
            }
            _ => {
                self.name_input.input_mut().process_key(key);
                self.name_input.validate();
                Ok(SplitPanelResult::Ok)
            }
        }
    }
}

impl<B> Draw<B> for AddPanel
where
    B: Backend,
{
    type State = ();

    fn draw(&mut self, _: &Self::State, rect: Rect, frame: &mut Frame<B>) {
        let data = try_opt_ret!(&self.data);

        let outline = block::with_borders("Enter Name For Series");
        frame.render_widget(outline, rect);

        let vert_split = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Ratio(1, 3),
                    Input::DRAW_WITH_LABEL_CONSTRAINT,
                    Constraint::Min(1),
                    Constraint::Length(1),
                ]
                .as_ref(),
            )
            .horizontal_margin(2)
            .vertical_margin(3)
            .split(rect);

        let title_text = text::bold(&data.info.title.preferred);
        let title_widget = Paragraph::new(title_text).alignment(Alignment::Center);
        frame.render_widget(title_widget, vert_split[0]);

        let name_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(
                [
                    Constraint::Percentage(20),
                    Constraint::Percentage(60),
                    Constraint::Percentage(20),
                ]
                .as_ref(),
            )
            .split(vert_split[1]);

        self.name_input.draw(&(), name_layout[1], frame);

        if let Some(error) = self.name_input.error() {
            let error_text = text::bold_with(error, |s| s.fg(Color::Red));
            let error_widget = Paragraph::new(error_text).alignment(Alignment::Center);
            frame.render_widget(error_widget, vert_split[3]);
        }
    }
}

struct PartialData {
    info: RemoteInfo,
    path: SeriesPath,
}

impl PartialData {
    #[inline(always)]
    fn new(info: RemoteInfo, path: SeriesPath) -> Self {
        Self { info, path }
    }
}

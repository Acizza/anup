use super::SplitPanelResult;
use crate::series::config::SeriesConfig;
use crate::series::{self, SeriesParams, SeriesPath};
use crate::try_opt_ret;
use crate::tui::component::input::{Input, NameInput, ParsedValue, ValidatedInput};
use crate::tui::component::{Component, Draw};
use crate::tui::widget_util::{block, text};
use crate::tui::UIState;
use anime::local::EpisodeParser;
use anime::remote::SeriesInfo as RemoteInfo;
use anyhow::Result;
use termion::event::Key;
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
        let name_input = series::generate_nickname(&info.title.preferred)
            .map(|nickname| NameInput::with_placeholder(true, nickname))
            .unwrap_or_else(|| NameInput::new(true));

        Self {
            name_input,
            data: Some(PartialData::new(info, path)),
        }
    }
}

impl Component for AddPanel {
    type State = UIState;
    type KeyResult = Result<SplitPanelResult>;

    fn process_key(&mut self, key: Key, state: &mut Self::State) -> Self::KeyResult {
        match key {
            Key::Esc => Ok(SplitPanelResult::Reset),
            Key::Char('\n') => {
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
            key => {
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

        let title_text = [text::bold(&data.info.title.preferred)];
        let title_widget = Paragraph::new(title_text.iter())
            .wrap(false)
            .alignment(Alignment::Center);
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
            let error_text = [text::bold_with(error, |s| s.fg(Color::Red))];
            let error_widget = Paragraph::new(error_text.iter())
                .alignment(Alignment::Center)
                .wrap(false);
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

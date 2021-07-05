use super::SplitPanelResult;
use crate::series::{self, SeriesParams, SeriesPath};
use crate::try_opt_ret;
use crate::tui::component::input::{
    DrawInput, Input, InputFlags, NameInput, ParsedValue, ValidatedInput,
};
use crate::tui::component::Component;
use crate::tui::UIState;
use crate::{key::Key, series::config::SeriesConfig};
use anime::local::EpisodeParser;
use anime::remote::SeriesInfo as RemoteInfo;
use anyhow::Result;
use crossterm::event::KeyCode;
use tui::backend::Backend;
use tui::layout::{Alignment, Direction, Rect};
use tui::style::Color;
use tui::terminal::Frame;
use tui_utils::{
    helpers::{block, text},
    layout::{BasicConstraint, SimpleLayout},
    widgets::{OverflowMode, SimpleText},
};

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

    pub fn draw<B: Backend>(&mut self, rect: Rect, frame: &mut Frame<B>) {
        let data = try_opt_ret!(&self.data);

        let block = block::with_borders("Enter Name For Series");
        let block_area = block.inner(rect);

        frame.render_widget(block, rect);

        let vert_split = SimpleLayout::new(Direction::Vertical)
            .horizontal_margin(1)
            .vertical_margin(2)
            .split(
                block_area,
                [
                    // Title
                    BasicConstraint::Percentage(33),
                    // Series name input
                    BasicConstraint::Length(Input::DRAW_LINES_REQUIRED),
                    // Spacer
                    BasicConstraint::MinLenRemaining(1, 1),
                    // Error
                    BasicConstraint::Length(1),
                ],
            );

        let title_text = text::bold(&data.info.title.preferred);
        let title_widget = SimpleText::new(title_text)
            .alignment(Alignment::Center)
            .overflow(OverflowMode::Truncate);

        frame.render_widget(title_widget, vert_split[0]);

        let name_layout = SimpleLayout::new(Direction::Horizontal).split(
            vert_split[1],
            [
                BasicConstraint::Percentage(20),
                BasicConstraint::Percentage(60),
                BasicConstraint::Percentage(20),
            ],
        );

        self.name_input.draw(name_layout[1], frame);

        if let Some(error) = self.name_input.error() {
            let error_text = text::bold_with(error, |s| s.fg(Color::Red));
            let error_widget = SimpleText::new(error_text)
                .alignment(Alignment::Center)
                .overflow(OverflowMode::Truncate);

            frame.render_widget(error_widget, vert_split[3]);
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

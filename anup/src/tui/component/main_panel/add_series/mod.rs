mod inputs;

use super::PartialSeries;
use crate::err::{Error, Result};
use crate::series::info::{InfoSelector, SeriesInfo};
use crate::series::{SeriesParams, SeriesPath};
use crate::tui::component::{Component, Draw};
use crate::tui::{UIBackend, UIState};
use crate::{try_opt_r, try_opt_ret};
use anime::local::Episodes;
use inputs::{InputSet, ParsedValue, ValidatedInput};
use std::borrow::Cow;
use std::mem;
use std::time::Instant;
use termion::event::Key;
use tui::backend::Backend;
use tui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use tui::style::{Color, Modifier, Style};
use tui::terminal::Frame;
use tui::widgets::{Block, Borders, Paragraph, Text};

const SECS_BETWEEN_SERIES_UPDATES: f32 = 0.25;

pub struct AddSeriesPanel {
    inputs: InputSet,
    selected_input: usize,
    error: Option<Cow<'static, str>>,
    last_update: Option<Instant>,
    series_builder: SeriesBuilder,
}

impl AddSeriesPanel {
    pub fn new() -> Self {
        Self {
            inputs: InputSet::new(),
            selected_input: 0,
            error: None,
            last_update: None,
            series_builder: SeriesBuilder::new(),
        }
    }

    #[inline(always)]
    fn current_input(&mut self) -> &mut dyn ValidatedInput {
        self.inputs.index_mut(self.selected_input)
    }

    fn validate_selected(&mut self) {
        self.current_input().validate();

        for input in &self.inputs.all_mut() {
            if let value @ Some(_) = input.error() {
                self.error = value;
                return;
            }
        }

        self.error = None;
    }

    fn draw_add_series_panel<B>(&mut self, rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
    {
        let vert_fields = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    // Field
                    Constraint::Length(4),
                    // Spacer
                    Constraint::Length(1),
                    // Field
                    Constraint::Length(4),
                    // Remaining space
                    Constraint::Percentage(100),
                ]
                .as_ref(),
            )
            .vertical_margin(2)
            .split(rect);

        let horiz_fields = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref());

        let horiz_fields_top = horiz_fields.clone().split(vert_fields[0]);
        let horiz_fields_bottom = horiz_fields.split(vert_fields[2]);

        let field_positions = horiz_fields_top
            .into_iter()
            .chain(horiz_fields_bottom.into_iter());

        for (input, pos) in self.inputs.all_mut().iter_mut().zip(field_positions) {
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Length(3)].as_ref())
                .horizontal_margin(3)
                .split(pos);

            let text = [bold_header(input.label())];
            let widget = Paragraph::new(text.iter())
                .wrap(false)
                .alignment(Alignment::Center);
            frame.render_widget(widget, layout[0]);

            input.input_mut().draw(&(), layout[1], frame);
        }
    }

    fn draw_detected_panel<B>(&mut self, rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
    {
        macro_rules! info_label {
            ($label:expr, $value:expr, $rect:expr) => {{
                let label = concat!($label, "\n");
                let text = [
                    bold_header(label),
                    Text::styled($value, Style::default().modifier(Modifier::ITALIC)),
                ];

                let widget = Paragraph::new(text.iter())
                    .wrap(false)
                    .alignment(Alignment::Center);
                frame.render_widget(widget, $rect);
            }};
        }

        let (header_text, has_error) = match (&self.error, &self.series_builder.params) {
            (Some(err), Some(_)) | (Some(err), None) => {
                ([bold_header_with(err.as_ref(), |s| s.fg(Color::Red))], true)
            }
            (None, Some(_)) => ([bold_header("Detected")], false),
            (None, None) => (
                [bold_header_with("Nothing Detected", |s| s.fg(Color::Red))],
                false,
            ),
        };

        let header = Paragraph::new(header_text.iter())
            .wrap(true)
            .alignment(Alignment::Center);
        frame.render_widget(header, rect);

        let fields = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
            .vertical_margin(2)
            .split(rect);

        if has_error {
            return;
        }

        let (params, episodes) = try_opt_ret!(&self.series_builder.params);

        info_label!(
            "Relative Path",
            format!("{}", params.path.display()),
            fields[0]
        );
        info_label!("Episodes", episodes.len().to_string(), fields[1]);
    }
}

impl Component for AddSeriesPanel {
    type State = UIState;
    type KeyResult = AddSeriesResult;

    fn tick(&mut self, state: &mut UIState) -> Result<()> {
        let last_update = try_opt_r!(self.last_update);

        if last_update.elapsed().as_secs_f32() < SECS_BETWEEN_SERIES_UPDATES {
            return Ok(());
        }

        self.series_builder.update(&self.inputs, state).ok();
        self.last_update = None;

        Ok(())
    }

    fn process_key(&mut self, key: Key, state: &mut Self::State) -> Self::KeyResult {
        match key {
            Key::Esc => AddSeriesResult::Reset,
            Key::Char('\n') => {
                self.validate_selected();

                if self.error.is_some() {
                    return AddSeriesResult::Ok;
                }

                match self.series_builder.build(&self.inputs, state) {
                    Ok(partial) => AddSeriesResult::AddSeries(Box::new(partial)),
                    Err(err) => AddSeriesResult::Error(err),
                }
            }
            Key::Char('\t') => {
                self.validate_selected();

                self.current_input().input_mut().selected = false;
                self.selected_input = (self.selected_input + 1) % self.inputs.len();
                self.current_input().input_mut().selected = true;

                AddSeriesResult::Ok
            }
            key => {
                self.current_input().input_mut().process_key(key);
                self.validate_selected();

                self.last_update = Some(Instant::now());
                AddSeriesResult::Ok
            }
        }
    }
}

impl<B> Draw<B> for AddSeriesPanel
where
    B: Backend,
{
    type State = ();

    fn draw(&mut self, _: &Self::State, rect: Rect, frame: &mut Frame<B>) {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(7), Constraint::Length(6)].as_ref())
            .horizontal_margin(2)
            .split(rect);

        let outline = Block::default().title("Add Series").borders(Borders::ALL);
        frame.render_widget(outline, rect);

        self.draw_add_series_panel(split[0], frame);
        self.draw_detected_panel(split[1], frame);
    }

    fn after_draw(&mut self, backend: &mut UIBackend<B>, _: &Self::State) {
        for input in &mut self.inputs.all_mut() {
            input.input_mut().after_draw(backend, &());
        }
    }
}

pub enum AddSeriesResult {
    Ok,
    Reset,
    AddSeries(Box<PartialSeries>),
    Error(Error),
}

struct SeriesBuilder {
    params: Option<(SeriesParams, Episodes)>,
}

impl SeriesBuilder {
    fn new() -> Self {
        Self { params: None }
    }

    fn path(&self, inputs: &InputSet, state: &UIState) -> Result<SeriesPath> {
        match &inputs.path.parsed_value() {
            Some(path) => Ok(SeriesPath::new(path, &state.config)),
            None => SeriesPath::closest_matching(inputs.name.parsed_value(), &state.config),
        }
    }

    fn update(&mut self, inputs: &InputSet, state: &UIState) -> Result<()> {
        match self.update_internal(inputs, state) {
            ok @ Ok(_) => ok,
            err @ Err(_) => {
                self.params = None;
                err
            }
        }
    }

    fn update_internal(&mut self, inputs: &InputSet, state: &UIState) -> Result<()> {
        let path = self.path(inputs, state)?;

        let parser = inputs.parser.parsed_value();
        let episodes = Episodes::parse(path.absolute(&state.config), parser)?;
        let name = inputs.name.parsed_value();

        match &mut self.params {
            Some((params, cur_episodes)) => {
                params.update(name, path, parser);
                *cur_episodes = episodes;
                Ok(())
            }
            None => {
                let params = SeriesParams::new(name, path, parser.clone());
                self.params = Some((params, episodes));
                Ok(())
            }
        }
    }

    fn build(&mut self, inputs: &InputSet, state: &UIState) -> Result<PartialSeries> {
        let (params, episodes) = match self.update(inputs, state) {
            Ok(_) => mem::take(&mut self.params).unwrap(),
            Err(err) => return Err(err),
        };

        let info = {
            let id = inputs.id.parsed_value();
            let sel = id.map_or_else(
                || InfoSelector::from_path_or_name(&params.path, &params.name),
                InfoSelector::ID,
            );

            SeriesInfo::from_remote(sel, state.remote.as_ref())?
        };

        let params = SeriesParams::new(params.name, params.path, params.parser);

        Ok(PartialSeries::new(info, params, episodes))
    }
}

#[inline(always)]
fn bold_header_with<F>(header: &str, extra_style: F) -> Text
where
    F: FnOnce(Style) -> Style,
{
    Text::styled(
        header,
        extra_style(Style::default().modifier(Modifier::BOLD)),
    )
}

#[inline(always)]
fn bold_header(header: &str) -> Text {
    bold_header_with(header, |s| s)
}

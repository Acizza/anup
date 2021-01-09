use super::PartialSeries;
use crate::file;
use crate::series::info::{InfoSelector, SeriesInfo};
use crate::series::{self, LoadedSeries, SeriesParams, SeriesPath, UpdateParams};
use crate::tui::component::input::{
    IDInput, Input, InputFlags, NameInput, ParsedValue, ParserInput, PathInput, ValidatedInput,
};
use crate::tui::component::{Component, Draw};
use crate::tui::widget_util::widget::WrapHelper;
use crate::tui::widget_util::{block, text};
use crate::tui::UIState;
use crate::{config::Config, key::Key};
use crate::{try_opt_r, try_opt_ret};
use anime::local::{CategorizedEpisodes, EpisodeParser, SortedEpisodes};
use anime::remote::SeriesID;
use anyhow::{Context, Result};
use crossterm::event::KeyCode;
use std::borrow::Cow;
use std::mem;
use std::time::Instant;
use tui::backend::Backend;
use tui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use tui::style::Color;
use tui::terminal::Frame;
use tui::widgets::Paragraph;

const SECS_BETWEEN_SERIES_UPDATES: f32 = 0.25;

struct PanelInputs {
    name: NameInput,
    id: IDInput,
    path: PathInput,
    parser: ParserInput,
}

impl PanelInputs {
    const TOTAL: usize = 4;

    /// Creates all panel inputs.
    ///
    /// Returns a new `PanelInputs` and a boolean indicating whether any inputs had their placeholder set.
    fn init_with_placeholders(config: &Config) -> (Self, bool) {
        use anime::local::detect::dir as anime_dir;

        let detected_path = file::last_modified_dir(&config.series_dir).ok().flatten();

        // We only set a placeholder if detected_path is some
        let placeholder_set = detected_path.is_some();

        let path = detected_path.as_ref().map_or_else(
            || PathInput::new(InputFlags::empty(), config),
            |path| PathInput::with_placeholder(InputFlags::empty(), config, path),
        );

        let name = detected_path
            .and_then(anime_dir::parse_title)
            .and_then(series::generate_nickname)
            .map_or_else(
                || NameInput::new(InputFlags::SELECTED),
                |nickname| NameInput::with_placeholder(InputFlags::SELECTED, nickname),
            );

        let result = Self {
            name,
            id: IDInput::new(InputFlags::empty()),
            path,
            parser: ParserInput::new(InputFlags::empty()),
        };

        (result, placeholder_set)
    }

    fn init_with_series(config: &Config, series: &LoadedSeries) -> Self {
        let id = series.id().map_or_else(
            || IDInput::new(InputFlags::empty()),
            |id| IDInput::with_id(InputFlags::empty(), id as SeriesID),
        );

        let parser_pattern = match series.parser() {
            EpisodeParser::Default => Cow::Borrowed(""),
            EpisodeParser::Custom(cus) => cus.inner().into(),
        };

        Self {
            name: NameInput::with_placeholder(InputFlags::DISABLED, series.nickname()),
            id,
            path: PathInput::with_path(InputFlags::empty(), config, series.path().to_owned()),
            parser: ParserInput::with_text(InputFlags::empty(), parser_pattern),
        }
    }

    #[inline(always)]
    pub fn all_mut(&mut self) -> [&mut dyn ValidatedInput; Self::TOTAL] {
        [
            &mut self.name,
            &mut self.id,
            &mut self.path,
            &mut self.parser,
        ]
    }

    #[inline(always)]
    pub fn index_mut(&mut self, index: usize) -> &mut dyn ValidatedInput {
        self.all_mut()[index]
    }
}

pub struct AddSeriesPanel {
    inputs: PanelInputs,
    selected_input: usize,
    error: Option<Cow<'static, str>>,
    last_update: Option<Instant>,
    series_builder: SeriesBuilder,
    mode: Mode,
}

impl AddSeriesPanel {
    pub fn init(state: &UIState, mode: Mode) -> Result<Self> {
        let (inputs, placeholder_set) = match mode {
            Mode::AddSeries => PanelInputs::init_with_placeholders(&state.config),
            Mode::UpdateSeries => {
                let selected = state
                    .series
                    .selected()
                    .context("must select a series in order to update it")?;

                let inputs = PanelInputs::init_with_series(&state.config, selected);
                (inputs, true)
            }
        };

        let mut result = Self {
            inputs,
            selected_input: 0,
            error: None,
            last_update: None,
            series_builder: SeriesBuilder::new(),
            mode,
        };

        // If any inputs have a placeholder, we should update our detected series now
        if placeholder_set {
            result.series_builder.update(&result.inputs, state).ok();
        }

        Ok(result)
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
                    Input::DRAW_WITH_LABEL_CONSTRAINT,
                    // Spacer
                    Constraint::Length(1),
                    // Field
                    Input::DRAW_WITH_LABEL_CONSTRAINT,
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

        let horiz_fields_top = horiz_fields.split(vert_fields[0]);
        let horiz_fields_bottom = horiz_fields.split(vert_fields[2]);

        let field_positions = horiz_fields_top
            .into_iter()
            .chain(horiz_fields_bottom.into_iter());

        for (input, pos) in self.inputs.all_mut().iter_mut().zip(field_positions) {
            let layout = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(100)].as_ref())
                .horizontal_margin(3)
                .split(pos);

            input.input_mut().draw(&(), layout[0], frame);
        }
    }

    fn draw_detected_panel<B>(&mut self, rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
    {
        macro_rules! info_label {
            ($label:expr, $value:expr, $rect:expr) => {{
                let label = concat!($label, "\n");
                let text = vec![text::bold(label).into(), $value.into()];

                let widget = Paragraph::new(text).alignment(Alignment::Center);
                frame.render_widget(widget, $rect);
            }};
        }

        let (header_text, has_error) = match (&self.error, &self.series_builder.params) {
            (Some(err), Some(_)) | (Some(err), None) => {
                (text::bold_with(err.as_ref(), |s| s.fg(Color::Red)), true)
            }
            (None, Some(_)) => (text::bold("Detected"), false),
            (None, None) => (
                text::bold_with("Nothing Detected", |s| s.fg(Color::Red)),
                false,
            ),
        };

        let header = Paragraph::new(header_text)
            .wrapped()
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

        let built = try_opt_ret!(&self.series_builder.params);

        info_label!(
            "Relative Path",
            text::italic(format!("{}", built.params.path.display())),
            fields[0]
        );

        let episodes_text = match &built.episodes {
            ParsedEpisodes::Parsed(_, range_str) => text::italic(range_str),
            ParsedEpisodes::NoneFound => text::italic_with("none", |s| s.fg(Color::Yellow)),
            ParsedEpisodes::NeedsSplitting => {
                text::italic_with("needs splitting", |s| s.fg(Color::Yellow))
            }
        };

        info_label!("Found Episodes", episodes_text, fields[1]);
    }
}

impl Component for AddSeriesPanel {
    type State = UIState;
    type KeyResult = Result<AddSeriesResult>;

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
        match *key {
            KeyCode::Esc => Ok(AddSeriesResult::Reset),
            KeyCode::Enter => {
                self.validate_selected();

                if self.error.is_some() {
                    return Ok(AddSeriesResult::Ok);
                }

                self.series_builder.build(&self.inputs, state, self.mode)
            }
            KeyCode::Tab => {
                self.validate_selected();

                self.current_input().input_mut().set_selected(false);
                self.selected_input = (self.selected_input + 1) % PanelInputs::TOTAL;
                self.current_input().input_mut().set_selected(true);

                Ok(AddSeriesResult::Ok)
            }
            _ => {
                self.current_input().input_mut().process_key(key);
                self.validate_selected();

                let path_input = self.inputs.path.input_mut();
                let name_has_input = self.inputs.name.input_mut().has_input();

                // Our path should only use a placeholder if the user hasn't changed the name input
                // This is to avoid locking the detected series in unless the user changes the path as well.
                path_input
                    .flags
                    .set(InputFlags::IGNORE_PLACEHOLDER, !name_has_input);

                self.last_update = Some(Instant::now());
                Ok(AddSeriesResult::Ok)
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

        let title = match self.mode {
            Mode::AddSeries => "Add Series",
            Mode::UpdateSeries => "Update Selected Series",
        };

        let outline = block::with_borders(title);
        frame.render_widget(outline, rect);

        self.draw_add_series_panel(split[0], frame);
        self.draw_detected_panel(split[1], frame);
    }
}

pub enum AddSeriesResult {
    Ok,
    Reset,
    AddSeries(Box<PartialSeries>),
    UpdateSeries(Box<UpdateParams>),
}

#[derive(Copy, Clone)]
pub enum Mode {
    AddSeries,
    UpdateSeries,
}

struct SeriesBuilder {
    params: Option<BuiltSeriesParams>,
}

impl SeriesBuilder {
    fn new() -> Self {
        Self { params: None }
    }

    fn path<'a>(inputs: &'a PanelInputs, state: &UIState) -> Result<Cow<'a, SeriesPath>> {
        match &inputs.path.parsed_value() {
            Some(path) => Ok(path.into()),
            None => SeriesPath::closest_matching(inputs.name.parsed_value(), &state.config)
                .map(Into::into),
        }
    }

    fn update(&mut self, inputs: &PanelInputs, state: &UIState) -> Result<()> {
        match self.update_internal(inputs, state) {
            ok @ Ok(_) => ok,
            err @ Err(_) => {
                self.params = None;
                err
            }
        }
    }

    fn update_internal(&mut self, inputs: &PanelInputs, state: &UIState) -> Result<()> {
        let path = Self::path(inputs, state)?;

        let parser = inputs.parser.parsed_value();
        let episodes = ParsedEpisodes::parse(&path, &state.config, parser)?;
        let name = inputs.name.parsed_value();

        match &mut self.params {
            Some(built) => {
                built.params.update(name, path, parser);
                built.episodes = episodes;
                Ok(())
            }
            None => {
                let params = SeriesParams::new(name, path.into_owned(), parser.clone());
                let built = BuiltSeriesParams::new(params, episodes);
                self.params = Some(built);
                Ok(())
            }
        }
    }

    fn build(
        &mut self,
        inputs: &PanelInputs,
        state: &UIState,
        mode: Mode,
    ) -> Result<AddSeriesResult> {
        let built = match self.update(inputs, state) {
            Ok(_) => mem::take(&mut self.params).unwrap(),
            Err(err) => return Err(err),
        };

        // TODO: our built series gets eaten when this fails
        let episodes = built.episodes.take_episodes()?;
        let params = built.params;

        match mode {
            Mode::AddSeries => {
                let info = {
                    let id = inputs.id.parsed_value();
                    let sel = id.map_or_else(
                        || InfoSelector::from_path_or_name(&params.path, &params.name),
                        InfoSelector::ID,
                    );

                    SeriesInfo::from_remote(sel, &state.remote)?
                };

                let partial = PartialSeries::new(info, params, episodes);

                Ok(AddSeriesResult::AddSeries(partial.into()))
            }
            Mode::UpdateSeries => {
                let params = UpdateParams {
                    id: inputs.id.parsed_value().to_owned(),
                    path: Some(params.path),
                    parser: Some(params.parser),
                    episodes: Some(episodes),
                };

                Ok(AddSeriesResult::UpdateSeries(params.into()))
            }
        }
    }
}

enum ParsedEpisodes {
    Parsed(SortedEpisodes, String),
    NoneFound,
    NeedsSplitting,
}

impl ParsedEpisodes {
    fn parse(path: &SeriesPath, config: &Config, parser: &EpisodeParser) -> Result<Self> {
        let episodes = CategorizedEpisodes::parse(path.absolute(config), parser)?;

        if episodes.is_empty() {
            return Ok(Self::NoneFound);
        }

        let result = episodes
            .take_season_episodes_or_present()
            .map(Self::from_episodes)
            .unwrap_or(Self::NeedsSplitting);

        Ok(result)
    }

    fn from_episodes(episodes: SortedEpisodes) -> Self {
        Self::episode_range_str(&episodes)
            .map(|range| Self::Parsed(episodes, range))
            .unwrap_or(Self::NoneFound)
    }

    fn take_episodes(self) -> Result<SortedEpisodes> {
        use crate::series::EpisodeScanError;

        match self {
            Self::Parsed(episodes, _) => Ok(episodes),
            Self::NoneFound => Err(EpisodeScanError::NoEpisodes.into()),
            Self::NeedsSplitting => Err(EpisodeScanError::SeriesNeedsSplitting.into()),
        }
    }

    /// Build a string that displays ranges and holes within a set of episodes.
    /// A hole is considered to be an episode that is not sequential.
    fn episode_range_str(episodes: &SortedEpisodes) -> Option<String> {
        use std::ops::Range;

        const RANGE_SEPARATOR: char = '-';
        const HOLE_SEPARATOR: char = '|';

        fn push_range(result: &mut String, range: Range<u32>) {
            result.push_str(&range.start.to_string());

            if range.end != range.start {
                result.push(RANGE_SEPARATOR);
                result.push_str(&range.end.to_string());
            }
        }

        if episodes.len() < 2 {
            return episodes.get(0).map(|ep| ep.number.to_string());
        }

        let mut result = String::new();
        let mut range = episodes[0].number..episodes[0].number;

        for episode in &episodes[1..] {
            let ep_num = episode.number;

            if ep_num - range.end > 1 {
                push_range(&mut result, range);
                result.push(HOLE_SEPARATOR);
                range = ep_num..ep_num;

                continue;
            }

            range.end = ep_num;
        }

        push_range(&mut result, range);
        Some(result)
    }
}

struct BuiltSeriesParams {
    params: SeriesParams,
    episodes: ParsedEpisodes,
}

impl BuiltSeriesParams {
    #[inline(always)]
    fn new(params: SeriesParams, episodes: ParsedEpisodes) -> Self {
        Self { params, episodes }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anime::local::{Episode, SortedEpisodes};
    use std::ops::RangeInclusive;

    fn insert_range(list: &mut Vec<Episode>, range: RangeInclusive<u32>) {
        for i in range {
            list.push(Episode::new(i, String::new()));
        }
    }

    macro_rules! episodes {
        () => {{
            SortedEpisodes::new()
        }};

        ($($range:expr),+) => {{
            let mut episodes = Vec::new();
            $(
            insert_range(&mut episodes, $range);
            )+
            SortedEpisodes::with_episodes(episodes)
        }};
    }

    #[test]
    fn found_episodes_str_detection() {
        let test_sets = vec![
            (episodes!(), None),
            (episodes!(6..=6), Some("6")),
            (episodes!(1..=6, 12..=16), Some("1-6|12-16")),
            (episodes!(1..=1), Some("1")),
            (episodes!(1..=2), Some("1-2")),
            (episodes!(1..=6, 8..=9), Some("1-6|8-9")),
            (episodes!(2..=2, 6..=12), Some("2|6-12")),
            (episodes!(1..=12, 14..=14), Some("1-12|14")),
            (
                episodes!(1..=12, 16..=24, 32..=48),
                Some("1-12|16-24|32-48"),
            ),
            (episodes!(1..=12, 16..=16, 24..=32), Some("1-12|16|24-32")),
            (episodes!(2..=2, 6..=6, 12..=12), Some("2|6|12")),
        ];

        for (episodes, expected) in test_sets {
            match ParsedEpisodes::episode_range_str(&episodes) {
                Some(result) => assert_eq!(Some(result.as_str()), expected),
                None => assert_eq!(None, expected),
            }
        }
    }
}

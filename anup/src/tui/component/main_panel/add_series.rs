use super::PartialSeries;
use crate::tui::component::input::{
    DrawInput, IDInput, Input, InputFlags, NameInput, ParsedValue, ParserInput, PathInput,
    ValidatedInput,
};
use crate::tui::component::Component;
use crate::tui::widget_util::{block, style};
use crate::tui::UIState;
use crate::{config::Config, key::Key};
use crate::{file, tui::state::SharedState};
use crate::{
    series::info::{InfoSelector, SeriesInfo},
    util::ArcMutex,
};
use crate::{
    series::{self, LoadedSeries, SeriesParams, SeriesPath, UpdateParams},
    util::arc_mutex,
};
use crate::{try_opt_ret, util::ScopedTask};
use anime::local::{CategorizedEpisodes, EpisodeParser, SortedEpisodes};
use anime::remote::SeriesID;
use anyhow::{Context, Result};
use crossterm::event::KeyCode;
use std::mem;
use std::time::Instant;
use std::{borrow::Cow, sync::Arc, time::Duration};
use tokio::task;
use tui::layout::{Alignment, Direction, Rect};
use tui::style::Color;
use tui::terminal::Frame;
use tui::{backend::Backend, text::Span};
use tui_utils::{
    layout::{BasicConstraint, RectExt, SimpleLayout},
    widgets::{Fragment, OverflowMode, SimpleText, SpanOptions, TextFragments},
};

const DURATION_BETWEEN_SERIES_UPDATES: Duration = Duration::from_millis(750);

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

struct SharedPanelState {
    inputs: PanelInputs,
    series_builder: SeriesBuilder,
    last_update: Option<Instant>,
    selected_input: usize,
    error: Option<Cow<'static, str>>,
    mode: Mode,
}

impl SharedPanelState {
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

    fn build_series(&mut self, state: &UIState) -> Result<AddSeriesResult> {
        self.series_builder.build(&self.inputs, state, self.mode)
    }

    fn update_series(&mut self, state: &UIState) -> Result<()> {
        self.series_builder.update(&self.inputs, state)
    }
}

pub struct AddSeriesPanel {
    state: ArcMutex<SharedPanelState>,
    #[allow(dead_code)]
    update_monitor_task: ScopedTask<()>,
}

impl AddSeriesPanel {
    pub fn init(state: &UIState, shared_state: &SharedState, mode: Mode) -> Result<Self> {
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

        let mut series_builder = SeriesBuilder::new();

        // If any inputs have a placeholder, we should update our detected series now
        if placeholder_set {
            series_builder.update(&inputs, state).ok();
        }

        let state = arc_mutex(SharedPanelState {
            inputs,
            series_builder,
            last_update: None,
            selected_input: 0,
            error: None,
            mode,
        });

        let update_monitor_task = Self::spawn_update_monitor(&state, shared_state).into();

        Ok(Self {
            state,
            update_monitor_task,
        })
    }

    fn spawn_update_monitor(
        panel_state: &ArcMutex<SharedPanelState>,
        state: &SharedState,
    ) -> task::JoinHandle<()> {
        let panel_state = Arc::clone(panel_state);
        let state = state.clone();

        task::spawn(async move {
            loop {
                tokio::time::sleep(DURATION_BETWEEN_SERIES_UPDATES).await;

                let mut panel_state = panel_state.lock();

                let last_update = match &panel_state.last_update {
                    Some(last_update) => last_update,
                    None => continue,
                };

                if last_update.elapsed() < DURATION_BETWEEN_SERIES_UPDATES {
                    continue;
                }

                let mut state = state.lock();

                panel_state.update_series(&state).ok();
                panel_state.last_update = None;

                state.mark_dirty();
            }
        })
    }

    fn draw_inputs<B>(panel_state: &SharedPanelState, rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
    {
        const HORIZ_PADDING: u16 = 2;

        let quadrants = SimpleLayout::default()
            .vertical_margin(1)
            .split_quadrants(rect);

        let pad = |quadrant: Rect| {
            quadrant
                .pad_horiz(HORIZ_PADDING)
                .lines_from_top(Input::DRAW_LINES_REQUIRED)
        };

        let inputs = &panel_state.inputs;

        inputs.name.draw(pad(quadrants.top_left), frame);
        inputs.id.draw(pad(quadrants.top_right), frame);
        inputs.path.draw(pad(quadrants.bottom_left), frame);
        inputs.parser.draw(pad(quadrants.bottom_right), frame);
    }

    fn draw_detected_panel<B>(panel_state: &SharedPanelState, rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
    {
        macro_rules! info_label {
            ($label:expr, $value:expr, $rect:expr) => {{
                let fragments = [
                    Fragment::span(Span::styled($label, style::bold())),
                    Fragment::Line,
                    Fragment::Span($value, SpanOptions::new().overflow(OverflowMode::Truncate)),
                ];

                let widget = TextFragments::new(&fragments).alignment(Alignment::Center);
                frame.render_widget(widget, $rect);
            }};
        }

        let (header_text, has_error) =
            match (&panel_state.error, &panel_state.series_builder.params) {
                (Some(err), Some(_)) | (Some(err), None) => (
                    Span::styled(err.as_ref(), style::bold().fg(Color::Red)),
                    true,
                ),
                (None, Some(_)) => (Span::styled("Detected", style::bold()), false),
                (None, None) => (
                    Span::styled("Nothing Detected", style::bold().fg(Color::Red)),
                    false,
                ),
            };

        let vert_layout = SimpleLayout::new(Direction::Vertical).split(
            rect,
            &[
                // Header
                BasicConstraint::Length(1),
                // Spacer
                BasicConstraint::Length(1),
                // Fields
                BasicConstraint::Length(2),
            ],
        );

        let header = SimpleText::new(header_text)
            .alignment(Alignment::Center)
            .overflow(OverflowMode::Truncate);

        frame.render_widget(header, vert_layout[0]);

        let fields = SimpleLayout::new(Direction::Horizontal).split_evenly(vert_layout[2]);

        if has_error {
            return;
        }

        let built = try_opt_ret!(&panel_state.series_builder.params);

        info_label!(
            "Relative Path",
            Span::styled(format!("{}", built.params.path.display()), style::italic()),
            fields.left
        );

        let episodes_text = match &built.episodes {
            ParsedEpisodes::Parsed(_, range_str) => Span::styled(range_str, style::italic()),
            ParsedEpisodes::NoneFound => Span::styled("none", style::italic().fg(Color::Yellow)),
            ParsedEpisodes::NeedsSplitting => {
                Span::styled("needs splitting", style::italic().fg(Color::Yellow))
            }
        };

        info_label!("Found Episodes", episodes_text, fields.right);
    }

    pub fn draw<B: Backend>(&mut self, rect: Rect, frame: &mut Frame<B>) {
        let panel_state = self.state.lock();

        let title = match panel_state.mode {
            Mode::AddSeries => "Add Series",
            Mode::UpdateSeries => "Update Selected Series",
        };

        let block = block::with_borders(title);
        let block_area = block.inner(rect);

        frame.render_widget(block, rect);

        let split = SimpleLayout::new(Direction::Vertical)
            .horizontal_margin(2)
            .split(
                block_area,
                &[
                    BasicConstraint::MinLenRemaining(11, 5),
                    BasicConstraint::Length(5),
                ],
            );

        Self::draw_inputs(&panel_state, split[0], frame);
        Self::draw_detected_panel(&panel_state, split[1], frame);
    }
}

impl Component for AddSeriesPanel {
    type State = UIState;
    type KeyResult = Result<AddSeriesResult>;

    fn process_key(&mut self, key: Key, state: &mut Self::State) -> Self::KeyResult {
        match *key {
            KeyCode::Esc => Ok(AddSeriesResult::Reset),
            KeyCode::Enter => {
                let mut panel_state = self.state.lock();

                panel_state.validate_selected();

                if panel_state.error.is_some() {
                    return Ok(AddSeriesResult::Ok);
                }

                panel_state.build_series(state)
            }
            KeyCode::Tab => {
                let mut panel_state = self.state.lock();

                panel_state.validate_selected();

                panel_state.current_input().input_mut().set_selected(false);
                panel_state.selected_input = (panel_state.selected_input + 1) % PanelInputs::TOTAL;
                panel_state.current_input().input_mut().set_selected(true);

                Ok(AddSeriesResult::Ok)
            }
            _ => {
                let mut panel_state = self.state.lock();

                panel_state.current_input().input_mut().process_key(key);
                panel_state.validate_selected();

                let name_has_input = panel_state.inputs.name.input_mut().has_input();
                let path_input = &mut panel_state.inputs.path;

                // Our path should only use a placeholder if the user hasn't changed the name input
                // This is to avoid locking the detected series in unless the user changes the path as well.
                path_input
                    .input_mut()
                    .flags
                    .set(InputFlags::IGNORE_PLACEHOLDER, !name_has_input);

                panel_state.last_update = Some(Instant::now());

                Ok(AddSeriesResult::Ok)
            }
        }
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
                let remote = state.remote.get_logged_in()?;

                let info = {
                    let id = inputs.id.parsed_value();
                    let sel = id.map_or_else(
                        || InfoSelector::from_path_or_name(&params.path, &params.name),
                        InfoSelector::ID,
                    );

                    SeriesInfo::from_remote(sel, remote)?
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

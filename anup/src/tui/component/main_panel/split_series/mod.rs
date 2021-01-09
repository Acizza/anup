mod add;
mod split;

use crate::series::config::SeriesConfig;
use crate::series::SeriesData;
use crate::series::{LoadedSeries, SeriesPath};
use crate::tui::component::{Component, Draw};
use crate::tui::widget_util::{block, text};
use crate::tui::UIState;
use crate::{config::Config, tui::backend::Key};
use add::AddPanel;
use anime::local::{CategorizedEpisodes, SortedEpisodes};
use anime::remote::{Remote, RemoteService, SeriesInfo as RemoteInfo};
use anime::SeriesKind;
use anyhow::{anyhow, Context, Result};
use split::{SplitPanel, SplitResult};
use std::borrow::Cow;
use std::mem;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use std::{fs, io};
use tui::backend::Backend;
use tui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use tui::terminal::Frame;
use tui::widgets::Paragraph;

pub struct SplitSeriesPanel {
    state: PanelState,
}

impl SplitSeriesPanel {
    pub fn new() -> Self {
        Self {
            state: PanelState::Loading,
        }
    }

    fn draw_loading_panel<B>(rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
    {
        let outline = block::with_borders("Split Series");
        frame.render_widget(outline, rect);

        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
            .margin(1)
            .split(rect);

        let text = text::bold("Loading..");
        let widget = Paragraph::new(text).alignment(Alignment::Center);
        frame.render_widget(widget, layout[1]);
    }
}

impl Component for SplitSeriesPanel {
    type State = UIState;
    type KeyResult = Result<SplitPanelResult>;

    fn tick(&mut self, state: &mut UIState) -> Result<()> {
        match &mut self.state {
            PanelState::Loading => {
                let series = match state.series.selected() {
                    Some(LoadedSeries::Complete(series)) => &series.data,
                    Some(LoadedSeries::Partial(data, _)) => data,
                    Some(LoadedSeries::None(_, _)) | None => {
                        return Err(anyhow!("cannot split a series with errors"))
                    }
                };

                let merged_series =
                    match MergedSeries::resolve(series, &state.remote, &state.config) {
                        Ok(merged) => merged,
                        Err(err) => return Err(err),
                    };

                self.state = PanelState::Splitting(SplitPanel::new(merged_series).into());
                Ok(())
            }
            PanelState::Splitting(split_panel) => split_panel.tick(state),
            PanelState::AddingSeries(add_panel, _) => add_panel.tick(state),
        }
    }

    fn process_key(&mut self, key: Key, state: &mut Self::State) -> Self::KeyResult {
        match &mut self.state {
            PanelState::Loading => Ok(SplitPanelResult::Ok),
            PanelState::Splitting(split_panel) => match split_panel.process_key(key, state) {
                Ok(SplitResult::Ok) => Ok(SplitPanelResult::Ok),
                Ok(SplitResult::Reset) => Ok(SplitPanelResult::Reset),
                Ok(SplitResult::AddSeries(info, path)) => {
                    let add_panel = AddPanel::new(info, path);
                    let split_panel = mem::take(split_panel);

                    self.state = PanelState::AddingSeries(add_panel.into(), split_panel);

                    Ok(SplitPanelResult::Ok)
                }
                Err(err) => Err(err),
            },
            PanelState::AddingSeries(add_panel, split_panel) => {
                match add_panel.process_key(key, state) {
                    Ok(result @ SplitPanelResult::Reset)
                    | Ok(result @ SplitPanelResult::AddSeries(_, _)) => {
                        let split_panel = mem::take(split_panel);
                        self.state = PanelState::Splitting(split_panel);

                        match result {
                            SplitPanelResult::Reset => Ok(SplitPanelResult::Ok),
                            other => Ok(other),
                        }
                    }
                    other => other,
                }
            }
        }
    }
}

impl<B> Draw<B> for SplitSeriesPanel
where
    B: Backend,
{
    type State = ();

    fn draw(&mut self, _: &Self::State, rect: Rect, frame: &mut Frame<B>) {
        match &mut self.state {
            PanelState::Loading => Self::draw_loading_panel(rect, frame),
            PanelState::Splitting(split_panel) => split_panel.draw(&(), rect, frame),
            PanelState::AddingSeries(add_panel, _) => add_panel.draw(&(), rect, frame),
        }
    }
}

enum PanelState {
    Loading,
    Splitting(Box<SplitPanel>),
    AddingSeries(Box<AddPanel>, Box<SplitPanel>),
}

pub enum SplitPanelResult {
    Ok,
    Reset,
    AddSeries(Box<RemoteInfo>, Box<SeriesConfig>),
}

impl SplitPanelResult {
    #[inline(always)]
    fn add_series(info: RemoteInfo, sconfig: SeriesConfig) -> Self {
        Self::AddSeries(Box::new(info), Box::new(sconfig))
    }
}

#[allow(variant_size_differences)]
enum MergedSeries {
    Resolved(Box<ResolvedSeries>),
    Failed(SeriesKind),
}

impl MergedSeries {
    #[inline(always)]
    fn resolved(resolved: ResolvedSeries) -> Self {
        Self::Resolved(Box::new(resolved))
    }

    fn resolve(data: &SeriesData, remote: &Remote, config: &Config) -> Result<Vec<Self>> {
        let episodes = CategorizedEpisodes::parse(
            data.config.path.absolute(config),
            &data.config.episode_parser,
        )?;

        let base_info = remote.search_info_by_id(data.info.id as u32)?;

        if base_info.sequels.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = Vec::with_capacity(1);

        for (cat, eps) in episodes.iter() {
            let sequel = match base_info.sequel_by_kind(*cat) {
                Some(sequel) => sequel,
                None => continue,
            };

            // Seasons need special handling as they can have several merged together
            if let SeriesKind::Season = sequel.kind {
                Self::resolve_merged_season(
                    &base_info,
                    &data.config.path,
                    remote,
                    eps,
                    config,
                    &mut results,
                );

                continue;
            }

            thread::sleep(Duration::from_millis(250));

            let sequel_info = if let Ok(info) = remote.search_info_by_id(sequel.id) {
                info
            } else {
                results.push(Self::Failed(sequel.kind));
                continue;
            };

            let resolved =
                ResolvedSeries::new(sequel_info, data.config.path.clone(), eps, 0, config);

            results.push(Self::resolved(resolved));
        }

        Ok(results)
    }

    fn resolve_merged_season(
        base_info: &RemoteInfo,
        base_path: &SeriesPath,
        remote: &Remote,
        episodes: &SortedEpisodes,
        config: &Config,
        results: &mut Vec<Self>,
    ) {
        let highest_episode = episodes.highest_episode_number();
        let mut info = Cow::Borrowed(base_info);

        // Exit early if we don't have enough episodes locally to have any merged seasons
        if info.episodes > highest_episode {
            return;
        }

        let mut episode_offset = info.episodes;

        while let Some(sequel) = info.direct_sequel() {
            info = if let Ok(info) = remote.search_info_by_id(sequel.id) {
                info.into()
            } else {
                results.push(Self::Failed(sequel.kind));
                continue;
            };

            let resolved = ResolvedSeries::new(
                info.clone().into_owned(),
                base_path.clone(),
                episodes,
                episode_offset,
                config,
            );

            results.push(Self::resolved(resolved));

            episode_offset += info.episodes;

            // We can stop if we don't have anymore sequels or if we don't have enough episodes locally to have another merged season
            if episode_offset > highest_episode || info.direct_sequel().is_none() {
                break;
            }

            thread::sleep(Duration::from_millis(250));
        }
    }

    fn split_all(merged: &[Self], config: &Config) -> Result<()> {
        for series in merged {
            let series = match series {
                Self::Resolved(series) => series,
                Self::Failed(_) => continue,
            };

            series
                .perform_split_actions(config)
                .context("performing split actions")?;
        }

        Ok(())
    }
}

pub type EpisodeOffset = u32;

struct ResolvedSeries {
    info: RemoteInfo,
    base_dir: SeriesPath,
    out_dir: SeriesPath,
    actions: Vec<SplitAction>,
}

impl ResolvedSeries {
    fn new(
        info: RemoteInfo,
        base_dir: SeriesPath,
        episodes: &SortedEpisodes,
        offset: EpisodeOffset,
        config: &Config,
    ) -> Self {
        let actions = SplitAction::from_merged_seasons(&info, episodes, offset);
        let out_dir = PathBuf::from(&info.title.preferred);
        let out_dir = SeriesPath::new(out_dir, config);

        Self {
            info,
            base_dir,
            out_dir,
            actions,
        }
    }

    fn perform_split_actions(&self, config: &Config) -> Result<()> {
        use std::os::unix::fs::symlink;

        if self.actions.is_empty() {
            return Ok(());
        }

        let base_dir = self.base_dir.absolute(config);

        if !base_dir.exists() {
            fs::create_dir_all(&base_dir).context("dir creation")?;
        }

        let out_dir = self.out_dir.absolute(config);

        if !out_dir.exists() {
            fs::create_dir_all(&out_dir).context("dir creation")?;
        }

        for action in &self.actions {
            let from_path = base_dir.join(&action.old_name);
            let to_path = out_dir.join(&action.new_name);

            if let Err(err) = symlink(&from_path, &to_path) {
                if err.kind() == io::ErrorKind::AlreadyExists {
                    continue;
                }

                return Err(anyhow!(
                    "failed to symlink files:\nfrom: {}\nto: {}\nreason: {}",
                    from_path.display(),
                    to_path.display(),
                    err
                ));
            }
        }

        Ok(())
    }
}

struct SplitAction {
    old_name: String,
    new_name: String,
}

impl SplitAction {
    fn new<S, N>(old_name: S, new_name: N) -> Self
    where
        S: Into<String>,
        N: Into<String>,
    {
        Self {
            old_name: old_name.into(),
            new_name: new_name.into(),
        }
    }

    fn from_merged_seasons(
        info: &RemoteInfo,
        episodes: &SortedEpisodes,
        offset: EpisodeOffset,
    ) -> Vec<Self> {
        let mut actions = Vec::new();

        let sequel_start = 1 + offset;
        let sequel_end = offset + info.episodes;

        for real_ep_num in sequel_start..=sequel_end {
            let episode = match episodes.find(real_ep_num) {
                Some(episode) => episode,
                None => continue,
            };

            let extension = PathBuf::from(&episode.filename).extension().map_or_else(
                || Cow::Borrowed(""),
                |e| format!(".{}", e.to_string_lossy()).into(),
            );

            let new_filename = format!(
                "{} - {:02}{}",
                info.title.preferred,
                real_ep_num - offset,
                extension
            );

            let action = Self::new(&episode.filename, new_filename);

            actions.push(action);
        }

        actions
    }
}

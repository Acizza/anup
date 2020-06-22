mod add;
mod split;

use crate::config::Config;
use crate::err;
use crate::err::{Error, Result};
use crate::series::config::SeriesConfig;
use crate::series::SeriesData;
use crate::series::{LoadedSeries, SeriesPath};
use crate::tui::component::{Component, Draw};
use crate::tui::widget_util::{block, text};
use crate::tui::UIState;
use add::AddPanel;
use anime::local::{CategorizedEpisodes, SortedEpisodes};
use anime::remote::{Remote, RemoteService, SeriesInfo as RemoteInfo};
use anime::SeriesKind;
use snafu::ResultExt;
use split::{SplitPanel, SplitResult};
use std::borrow::Cow;
use std::mem;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use std::{fs, io};
use termion::event::Key;
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

        let text = [text::bold("Loading..")];
        let widget = Paragraph::new(text.iter())
            .alignment(Alignment::Center)
            .wrap(false);
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
                        return Err(Error::CannotSplitErrorSeries)
                    }
                };

                let merged_series =
                    match MergedSeries::resolve(series, &state.remote, &state.config) {
                        Ok(merged) => merged,
                        Err(err) => return Err(err),
                    };

                self.state = PanelState::Splitting(SplitPanel::new(merged_series));
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

                    self.state = PanelState::AddingSeries(add_panel, split_panel);

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
    Splitting(SplitPanel),
    AddingSeries(AddPanel, SplitPanel),
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

#[derive(Debug)]
enum MergedSeries {
    Resolved(ResolvedSeries),
    Failed(SeriesKind),
}

impl MergedSeries {
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

            let sequel_info = match remote.search_info_by_id(sequel.id) {
                Ok(info) => info,
                Err(_) => {
                    results.push(Self::Failed(sequel.kind));
                    continue;
                }
            };

            let resolved =
                ResolvedSeries::new(sequel_info, data.config.path.clone(), eps, 0, config);

            results.push(Self::Resolved(resolved));
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
        let last_episode_num = episodes.last_episode_number();
        let mut info = Cow::Borrowed(base_info);

        // Exit early if we don't have enough episodes locally to have any merged seasons
        if info.episodes > last_episode_num {
            return;
        }

        let mut episode_offset = 0;

        while let Some(sequel) = info.direct_sequel() {
            info = match remote.search_info_by_id(sequel.id) {
                Ok(info) => info.into(),
                Err(_) => {
                    results.push(Self::Failed(sequel.kind));
                    continue;
                }
            };

            episode_offset += info.episodes;

            // Exit early if we don't have enough episodes locally to have another merged season
            if episode_offset > last_episode_num {
                break;
            }

            let resolved = ResolvedSeries::new(
                info.clone().into_owned(),
                base_path.clone(),
                episodes,
                episode_offset,
                config,
            );

            results.push(Self::Resolved(resolved));

            // We don't need to sleep if there isn't another sequel
            if info.sequels.is_empty() {
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

            series.perform_split_actions(config)?;
        }

        Ok(())
    }
}

pub type EpisodeOffset = u32;

#[derive(Debug)]
struct ResolvedSeries {
    info: RemoteInfo,
    base_dir: SeriesPath,
    out_dir: SeriesPath,
    offset: EpisodeOffset,
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
            offset,
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
            fs::create_dir_all(&base_dir).context(err::FileIO {
                path: base_dir.to_owned(),
            })?;
        }

        let out_dir = self.out_dir.absolute(config);

        if !out_dir.exists() {
            fs::create_dir_all(&out_dir).context(err::FileIO {
                path: out_dir.to_owned(),
            })?;
        }

        for action in &self.actions {
            let from_path = base_dir.join(&action.old_name);
            let to_path = out_dir.join(&action.new_name);

            if let Err(err) = symlink(&from_path, &to_path) {
                if err.kind() == io::ErrorKind::AlreadyExists {
                    continue;
                }

                return Err(Error::FileLinkFailed {
                    source: err,
                    from: from_path,
                    to: to_path,
                });
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
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
        let sequel_end = offset + info.episodes as EpisodeOffset;

        for real_ep_num in sequel_start..=sequel_end {
            let episode = match episodes.find(real_ep_num) {
                Some(episode) => episode,
                None => continue,
            };

            let extension = PathBuf::from(&episode.filename)
                .extension()
                .map(|e| format!(".{}", e.to_string_lossy()).into())
                .unwrap_or_else(|| Cow::Borrowed(""));

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

use crate::tui::state::ProgressTime;
use crate::tui::widget_util::{block, text};
use crate::tui::{state::StateEvent, UIState};
use crate::tui::{state::ThreadedState, widget_util::widget::WrapHelper};
use crate::util;
use crate::{
    series::{LoadedSeries, Series},
    tui::component::Component,
};
use anime::remote::{ScoreParser, SeriesDate};
use chrono::Utc;
use smallvec::{smallvec, SmallVec};
use std::{
    borrow::Cow,
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
use std::{fmt, sync::atomic::AtomicU32};
use tokio::{task, time};
use tui::backend::Backend;
use tui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use tui::style::Color;
use tui::terminal::Frame;
use tui::text::{Span, Text};
use tui::widgets::Paragraph;
use tui_utils::{
    grid_pos,
    widgets::{Fragment, OverflowMode, SpanOptions, TextFragments},
};
use util::ScopedTask;

pub struct InfoPanel {
    progress_remaining_secs: Arc<AtomicU32>,
    #[allow(dead_code)]
    event_monitor_task: ScopedTask<()>,
}

impl InfoPanel {
    pub fn new(state: &ThreadedState) -> Self {
        let progress_remaining_secs = Arc::new(AtomicU32::default());
        let event_monitor_task =
            Self::spawn_episode_event_monitor(state, Arc::clone(&progress_remaining_secs)).into();

        Self {
            progress_remaining_secs,
            event_monitor_task,
        }
    }

    fn spawn_episode_event_monitor(
        state: &ThreadedState,
        progress_remaining_secs: Arc<AtomicU32>,
    ) -> task::JoinHandle<()> {
        let state = Arc::clone(state);

        task::spawn(async move {
            let mut events = {
                let state = state.lock();
                state.events.subscribe()
            };

            #[allow(unused_variables)]
            let mut progress_task: Option<ScopedTask<_>> = None;

            while let Ok(event) = events.recv().await {
                #[allow(unused_assignments)]
                match event {
                    StateEvent::StartedEpisode(progress_time) => {
                        let state = Arc::clone(&state);
                        let remaining_secs = Arc::clone(&progress_remaining_secs);

                        let task =
                            Self::spawn_progress_monitor_task(state, remaining_secs, progress_time)
                                .into();

                        progress_task = Some(task);
                    }
                    StateEvent::FinishedEpisode => {
                        progress_remaining_secs.store(0, Ordering::SeqCst);
                        progress_task = None;

                        let mut state = state.lock();
                        state.mark_dirty();
                    }
                }
            }
        })
    }

    fn spawn_progress_monitor_task(
        state: ThreadedState,
        remaining_secs: Arc<AtomicU32>,
        progress_time: ProgressTime,
    ) -> task::JoinHandle<()> {
        task::spawn(async move {
            let mut first_iter = true;

            loop {
                let cur_progress = remaining_secs.load(Ordering::SeqCst);

                if cur_progress == 0 && !first_iter {
                    remaining_secs.store(0, Ordering::SeqCst);
                    state.lock().mark_dirty();
                    break;
                }

                let diff = progress_time - Utc::now();
                let secs = diff.num_seconds();

                if secs <= 0 {
                    remaining_secs.store(0, Ordering::SeqCst);
                    state.lock().mark_dirty();
                    break;
                }

                remaining_secs.store(secs as u32, Ordering::SeqCst);
                first_iter = false;

                {
                    state.lock().mark_dirty();
                }

                time::sleep(Duration::from_secs(60)).await;
            }
        })
    }

    fn text_display_layout(rect: Rect) -> Vec<Rect> {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Percentage(100)])
            .margin(2)
            .split(rect)
    }

    fn draw_text_panel<'a, B, T>(header: Span, body: T, rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
        T: Into<Text<'a>>,
    {
        let layout = Self::text_display_layout(rect);

        let header_widget = Paragraph::new(header).alignment(Alignment::Center);
        frame.render_widget(header_widget, layout[0]);

        let body_widget = Paragraph::new(body).alignment(Alignment::Center).wrapped();
        frame.render_widget(body_widget, layout[1]);
    }

    fn draw_no_users_info<B>(rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
    {
        let body = vec![
            "Add an account by pressing 'u' to open".into(),
            "user management and then by pressing tab".into(),
            "to switch to the add user panel.".into(),
            "".into(),
            "Then open the auth URL in your browser".into(),
            "by pressing Ctrl + O, and follow its instructions.".into(),
            "Once you have a token, paste it in with either".into(),
            "Ctrl + Shift + V or Ctrl + V.".into(),
            "".into(),
            "More detailed instructions here:".into(),
            "https://github.com/Acizza/anup#adding-an-account".into(),
        ];

        Self::draw_text_panel(text::bold("No Accounts Added"), body, rect, frame);
    }

    fn draw_no_series_found<B>(rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
    {
        let body = vec![
            "Add one by pressing the 'a' key.".into(),
            "".into(),
            "The opened panel will require you to specify".into(),
            "a name for the series you want to add.".into(),
            "".into(),
            "For automatic detection, the name should be".into(),
            "similar to the name of the folder the series".into(),
            "is in on disk.".into(),
        ];

        Self::draw_text_panel(text::bold("No Series Found"), body, rect, frame);
    }

    fn draw_series_error<B, E>(err: E, rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
        E: fmt::Display,
    {
        let header = text::bold_with("Error Loading Series", |s| s.fg(Color::Red));
        let body = text::with_color(err.to_string(), Color::Red);

        Self::draw_text_panel(header, body, rect, frame);
    }

    #[allow(clippy::too_many_lines)]
    fn draw_series_info<B>(
        &self,
        state: &UIState,
        series: &Series,
        rect: Rect,
        frame: &mut Frame<B>,
    ) where
        B: Backend,
    {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    Constraint::Length(4),
                    Constraint::Percentage(70),
                    Constraint::Length(4),
                ]
                .as_ref(),
            )
            .margin(2)
            .split(rect);

        let info = &series.data.info;
        let entry = &series.data.entry;

        // Series title
        {
            let mut fragments: SmallVec<[Fragment; 2]> = smallvec![Fragment::Span(
                text::bold(&info.title_preferred),
                SpanOptions::new().overflow(OverflowMode::Truncate)
            )];

            if entry.needs_sync() {
                fragments.push(Fragment::span(text::italic(" [*]")));
            }

            let title_widget = TextFragments::new(&fragments).alignment(Alignment::Center);
            frame.render_widget(title_widget, layout[0]);
        }

        // Items in panel

        macro_rules! draw_stat {
            ($x_column:expr, $y_column:expr => $header:expr, $value:expr) => {{
                let content = layout[1];

                let pos = grid_pos(
                    Rect {
                        x: $x_column,
                        y: $y_column,
                        width: content.width / 3,
                        height: content.height / 3,
                    },
                    content,
                );

                Self::draw_stat($header, $value, pos, frame);
            }};
        }

        // Left panel items

        draw_stat!(0, 0 => "Watch Time", {
            let watch_time_mins = info.episodes * info.episode_length_mins;
            util::hm_from_mins(f32::from(watch_time_mins))
        });

        draw_stat!(0, 1 => "Time Left", {
            let eps_left = info.episodes - entry.watched_episodes().min(info.episodes);
            let time_left_mins = eps_left * info.episode_length_mins;
            util::hm_from_mins(f32::from(time_left_mins))
        });

        draw_stat!(0, 2 => "Episode Length", format!("{}M", info.episode_length_mins));

        // Middle panel items

        draw_stat!(1, 0 => "Progress", format!("{}|{}", entry.watched_episodes(), info.episodes));

        draw_stat!(1, 1 => "Score", {
            match entry.score() {
                Some(score) => state.remote.score_to_str(score as u8),
                None => "??".into(),
            }
        });

        draw_stat!(1, 2 => "Status", {
            let status: &'static str = entry.status().into();
            status
        });

        // Right panel items

        // TODO: allow the format to be changed in the config
        let format_date = |date: Option<SeriesDate>| {
            date.map_or_else(
                || Cow::Borrowed("??"),
                |date| format!("{:02}/{:02}/{:02}", date.month, date.day, date.year % 100).into(),
            )
        };

        draw_stat!(2, 0 => "Start Date", format_date(entry.start_date()));
        draw_stat!(2, 1 => "Finish Date", format_date(entry.end_date()));
        draw_stat!(2, 2 => "Rewatched", entry.times_rewatched().to_string());

        let progress_remaining_secs = self.progress_remaining_secs.load(Ordering::SeqCst);

        // Watch time needed indicator at bottom
        if progress_remaining_secs > 0 {
            let mins = (progress_remaining_secs as f32 / 60.0).round() as u32;

            let fragments = [
                Fragment::span(text::bold(mins.to_string())),
                Fragment::span(text::bold(" Minutes Until Progression")),
            ];

            let widget = TextFragments::new(&fragments).alignment(Alignment::Center);
            frame.render_widget(widget, layout[2]);
        }
    }

    fn draw_stat<B, S>(header: &str, value: S, rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
        S: AsRef<str>,
    {
        let fragments = [
            Fragment::span(text::bold(header)),
            Fragment::Line,
            Fragment::span(text::italic(value.as_ref())),
        ];

        let widget = TextFragments::new(&fragments).alignment(Alignment::Center);
        frame.render_widget(widget, rect);
    }

    pub fn draw<B: Backend>(&mut self, state: &UIState, rect: Rect, frame: &mut Frame<B>) {
        let info_block = block::with_borders("Info");
        frame.render_widget(info_block, rect);

        if state.users.get().is_empty() {
            return Self::draw_no_users_info(rect, frame);
        }

        match state.series.selected() {
            Some(LoadedSeries::Complete(series)) => {
                self.draw_series_info(state, series, rect, frame)
            }
            Some(LoadedSeries::Partial(_, err)) => Self::draw_series_error(err, rect, frame),
            Some(LoadedSeries::None(_, err)) => Self::draw_series_error(err, rect, frame),
            None => Self::draw_no_series_found(rect, frame),
        }
    }
}

impl Component for InfoPanel {
    type State = ();
    type KeyResult = ();

    fn process_key(&mut self, _: crate::key::Key, _: &mut Self::State) -> Self::KeyResult {}
}

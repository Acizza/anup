use crate::remote::RemoteStatus;
use crate::tui::state::ProgressTime;
use crate::tui::state::SharedState;
use crate::tui::{state::StateEvent, UIState};
use crate::util;
use crate::{
    series::{LoadedSeries, Series},
    tui::component::Component,
};
use anime::remote::{ScoreParser, SeriesDate};
use chrono::Utc;
use smallvec::{smallvec, SmallVec};
use std::{
    array::IntoIter,
    borrow::Cow,
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
use std::{fmt, sync::atomic::AtomicU32};
use tokio::task;
use tui::backend::Backend;
use tui::layout::{Alignment, Direction, Rect};
use tui::style::Color;
use tui::terminal::Frame;
use tui::text::Span;
use tui_utils::{
    helpers::{block, text},
    layout::{BasicConstraint, RectExt, SimpleLayout},
    widgets::{Fragment, OverflowMode, SimpleText, SpanOptions, TextFragments},
    wrap,
};
use util::ScopedTask;

pub struct InfoPanel {
    progress_remaining_secs: Arc<AtomicU32>,
    #[allow(dead_code)]
    event_monitor_task: ScopedTask<()>,
}

impl InfoPanel {
    pub fn new(state: &SharedState) -> Self {
        let progress_remaining_secs = Arc::new(AtomicU32::default());
        let event_monitor_task =
            Self::spawn_episode_event_monitor(state, Arc::clone(&progress_remaining_secs)).into();

        Self {
            progress_remaining_secs,
            event_monitor_task,
        }
    }

    fn spawn_episode_event_monitor(
        state: &SharedState,
        progress_remaining_secs: Arc<AtomicU32>,
    ) -> task::JoinHandle<()> {
        let state = state.clone();

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
                        let state = state.clone();
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
        state: SharedState,
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

                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        })
    }

    fn header_body_layout(rect: Rect) -> (Rect, Rect) {
        let layout = SimpleLayout::new(Direction::Vertical).margin(2).split(
            rect,
            &[BasicConstraint::Length(2), BasicConstraint::Percentage(100)],
        );

        (layout[0], layout[1])
    }

    fn draw_text_panel<B>(
        header: Span,
        body: &[Fragment],
        header_pos: Rect,
        body_pos: Rect,
        frame: &mut Frame<B>,
    ) where
        B: Backend,
    {
        let header_widget = SimpleText::new(header)
            .alignment(Alignment::Center)
            .overflow(OverflowMode::Truncate);

        frame.render_widget(header_widget, header_pos);

        let body_widget = TextFragments::new(body).alignment(Alignment::Center);
        frame.render_widget(body_widget, body_pos);
    }

    fn draw_no_users_info<B>(rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
    {
        let span = |text| {
            Fragment::Span(
                Span::raw(text),
                SpanOptions::new().overflow(OverflowMode::Truncate),
            )
        };

        let body = [
            span("Add an account by pressing 'u' to open"),
            Fragment::Line,
            span("user management and then by pressing tab"),
            Fragment::Line,
            span("to switch to the add user panel."),
            Fragment::Line,
            Fragment::Line,
            span("Then open the auth URL in your browser"),
            Fragment::Line,
            span("by pressing Ctrl + O, and follow its instructions."),
            Fragment::Line,
            Fragment::Line,
            span("Once you have a token, paste it in with either"),
            Fragment::Line,
            span("Ctrl + Shift + V or Ctrl + V."),
            Fragment::Line,
            Fragment::Line,
            span("More detailed instructions here:"),
            Fragment::Line,
            span("https://github.com/Acizza/anup#adding-an-account"),
        ];

        let (h_pos, b_pos) = Self::header_body_layout(rect);
        Self::draw_text_panel(text::bold("No Accounts Added"), &body, h_pos, b_pos, frame);
    }

    fn draw_no_series_found<B>(rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
    {
        let span = |text| {
            Fragment::Span(
                Span::raw(text),
                SpanOptions::new().overflow(OverflowMode::Truncate),
            )
        };

        let body = [
            span("Add one by pressing the 'a' key."),
            Fragment::Line,
            Fragment::Line,
            span("The opened panel will require you to specify"),
            Fragment::Line,
            span("a name for the series you want to add."),
            Fragment::Line,
            Fragment::Line,
            span("For automatic detection, the name should be"),
            Fragment::Line,
            span("similar to the name of the folder the series"),
            Fragment::Line,
            span("is in on disk."),
        ];

        let (h_pos, b_pos) = Self::header_body_layout(rect);
        Self::draw_text_panel(text::bold("No Series Found"), &body, h_pos, b_pos, frame);
    }

    fn draw_series_error<B, E>(err: E, rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
        E: fmt::Display,
    {
        let header = text::bold_with("Error Loading Series", |s| s.fg(Color::Red));

        let body = [Fragment::Span(
            text::with_color(err.to_string(), Color::Red),
            SpanOptions::new().overflow(OverflowMode::Truncate),
        )];

        let (h_pos, b_pos) = Self::header_body_layout(rect);
        let wrapped = wrap::by_letters(IntoIter::new(body), b_pos.width);

        Self::draw_text_panel(header, &wrapped, h_pos, b_pos, frame);
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
        let layout = SimpleLayout::new(Direction::Vertical).margin(2).split(
            rect,
            &[
                BasicConstraint::Length(4),
                BasicConstraint::Percentage(70),
                BasicConstraint::Length(4),
            ],
        );

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

                let pos = content.grid_pos(Rect {
                    x: $x_column,
                    y: $y_column,
                    width: content.width / 3,
                    height: content.height / 3,
                });

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
            match (entry.score(), &state.remote) {
                (Some(score), RemoteStatus::LoggedIn(remote)) => remote.score_to_str(score as u8),
                (Some(score), RemoteStatus::LoggingIn(_)) => score.to_string().into(),
                (None, _) => "??".into(),
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

        self.draw_status_text(state, layout[2], frame);
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

    fn draw_status_text<B: Backend>(&self, state: &UIState, rect: Rect, frame: &mut Frame<B>) {
        let progress_remaining_secs = self.progress_remaining_secs.load(Ordering::SeqCst);

        // Remaining time until progression
        if progress_remaining_secs > 0 {
            let mins = (progress_remaining_secs as f32 / 60.0).round() as u32;

            let fragments = [
                Fragment::span(text::bold(mins.to_string())),
                Fragment::span(text::bold(" Minutes Until Progression")),
            ];

            let widget = TextFragments::new(&fragments).alignment(Alignment::Center);
            frame.render_widget(widget, rect);
        }
        // Login message
        else if let RemoteStatus::LoggingIn(username) = &state.remote {
            let fragments = [
                Fragment::span(text::bold("Logging In As ")),
                Fragment::Span(
                    text::bold_with(username, |s| s.fg(Color::Blue)),
                    SpanOptions::new().overflow(OverflowMode::Truncate),
                ),
            ];

            let widget = TextFragments::new(&fragments).alignment(Alignment::Center);
            frame.render_widget(widget, rect);
        }
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

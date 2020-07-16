use crate::series::{LoadedSeries, Series};
use crate::tui::component::Draw;
use crate::tui::widget_util::{block, text};
use crate::tui::{CurrentAction, UIState};
use crate::util;
use anime::remote::{ScoreParser, SeriesDate};
use chrono::Utc;
use smallvec::{smallvec, SmallVec};
use std::borrow::Cow;
use std::fmt;
use tui::backend::Backend;
use tui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use tui::style::Color;
use tui::terminal::Frame;
use tui::widgets::{Paragraph, Text};

pub struct InfoPanel;

impl InfoPanel {
    #[inline(always)]
    pub fn new() -> Self {
        Self {}
    }

    fn text_display_layout(rect: Rect) -> Vec<Rect> {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Percentage(100)])
            .margin(2)
            .split(rect)
    }

    fn draw_text_panel<B>(header: &[Text], body: &[Text], rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
    {
        let layout = Self::text_display_layout(rect);

        let header_widget = Paragraph::new(header.iter()).alignment(Alignment::Center);
        frame.render_widget(header_widget, layout[0]);

        let body_widget = Paragraph::new(body.iter())
            .alignment(Alignment::Center)
            .wrap(true);
        frame.render_widget(body_widget, layout[1]);
    }

    fn draw_no_users_info<B>(rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
    {
        let body = [Text::raw(
            "Add an account by pressing 'u' to open\
            \nuser management and then by pressing tab\
            \nto switch to the add user panel.\

            \n\nThen open the auth URL in your browser\
            \nby pressing Ctrl + O, and follow its instructions.\
            \n Once you have a token, paste it in with either\
            \nCtrl + Shift + V or Ctrl + V.\

            \n\nMore detailed instructions here:\
            \nhttps://github.com/Acizza/anup#adding-an-account",
        )];

        Self::draw_text_panel(&[text::bold("No Accounts Added")], &body, rect, frame);
    }

    fn draw_no_series_found<B>(rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
    {
        let body = [Text::raw(
            "Add one by pressing the 'a' key\

            \n\nThe opened panel will require you to specify\
            \n a name for the series you want to add.\
            \n\nFor automatic detection, the name should be\
            \nsimilar to the name of the folder the series\
            \nis in on disk.",
        )];

        Self::draw_text_panel(&[text::bold("No Series Found")], &body, rect, frame);
    }

    fn draw_series_error<B, E>(err: E, rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
        E: fmt::Display,
    {
        let header = [text::bold_with("Error Loading Series", |s| {
            s.fg(Color::Red)
        })];

        let body = [text::with_color(err.to_string(), Color::Red)];

        Self::draw_text_panel(&header, &body, rect, frame);
    }

    fn draw_series_info<B>(state: &UIState, series: &Series, rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
    {
        macro_rules! panel_items {
            ($($name:expr => $value:expr,)+) => {
                [$((concat!($name, "\n"), $value)),+]
            }
        }

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
            let text_items = {
                let mut items: SmallVec<[_; 2]> = smallvec![text::bold(&info.title_preferred)];

                if entry.needs_sync() {
                    items.push(text::italic(" [*]"));
                }

                items
            };

            let widget = Paragraph::new(text_items.iter()).alignment(Alignment::Center);
            frame.render_widget(widget, layout[0]);
        }

        // Items in panel
        let stat_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Ratio(1, 3),
                Constraint::Ratio(1, 3),
                Constraint::Ratio(1, 3),
            ])
            .split(layout[1]);

        let stat_vert_pos = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Ratio(1, 3),
                Constraint::Ratio(1, 3),
                Constraint::Ratio(1, 3),
            ]);

        let left_pane_items = panel_items! {
            "Watch Time" => {
                let watch_time_mins = info.episodes * info.episode_length_mins;
                util::hm_from_mins(watch_time_mins as f32).into()
            },
            "Time Left" => {
                let eps_left = info.episodes - entry.watched_episodes().min(info.episodes);
                let time_left_mins = eps_left * info.episode_length_mins;

                util::hm_from_mins(time_left_mins as f32).into()
            },
            "Episode Length" => format!("{}M", info.episode_length_mins).into(),
        };

        let middle_pane_items = panel_items! {
            "Progress" => format!("{}|{}", entry.watched_episodes(), info.episodes).into(),
            "Score" => match entry.score() {
                Some(score) => state.remote.score_to_str(score as u8),
                None => "??".into(),
            },
            "Status" => entry.status().to_string().into(),
        };

        let right_pane_items = {
            // TODO: allow the format to be changed in the config
            let format_date = |date: Option<SeriesDate>| {
                date.map(|date| {
                    format!("{:02}/{:02}/{:02}", date.month, date.day, date.year % 100).into()
                })
                .unwrap_or_else(|| Cow::Borrowed("??"))
            };

            panel_items! {
                "Start Date" => format_date(entry.start_date()),
                "Finish Date" => format_date(entry.end_date()),
                "Rewatched" => entry.times_rewatched().to_string().into(),
            }
        };

        let items: [&[(_, Cow<str>)]; 3] =
            [&left_pane_items, &middle_pane_items, &right_pane_items];

        for x_pos in 0..3 {
            let stat_layout = stat_vert_pos.split(stat_layout[x_pos]);
            let column_items = items[x_pos];

            for y_pos in 0..3 {
                let (header, value) = &column_items[y_pos];
                let text = [text::bold(*header), text::italic(value.as_ref())];

                let widget = Paragraph::new(text.iter()).alignment(Alignment::Center);
                frame.render_widget(widget, stat_layout[y_pos]);
            }
        }

        // Watch time needed indicator at bottom
        if let CurrentAction::WatchingEpisode(progress_time, _) = state.current_action {
            let watch_time = progress_time - Utc::now();
            let watch_secs = watch_time.num_seconds();

            if watch_secs > 0 {
                let remaining_mins = watch_secs as f32 / 60.0;

                let text_str = format!(
                    "{} Remaining Until Progression",
                    util::ms_from_mins(remaining_mins)
                );

                let text = [text::bold(text_str)];

                let widget = Paragraph::new(text.iter()).alignment(Alignment::Center);
                frame.render_widget(widget, layout[2]);
            }
        }
    }
}

impl<B> Draw<B> for InfoPanel
where
    B: Backend,
{
    type State = UIState;

    fn draw(&mut self, state: &Self::State, rect: Rect, frame: &mut Frame<B>) {
        let info_block = block::with_borders("Info");
        frame.render_widget(info_block, rect);

        if state.users.get().is_empty() {
            return Self::draw_no_users_info(rect, frame);
        }

        match state.series.selected() {
            Some(LoadedSeries::Complete(series)) => {
                Self::draw_series_info(state, series, rect, frame)
            }
            Some(LoadedSeries::Partial(_, err)) => Self::draw_series_error(err, rect, frame),
            Some(LoadedSeries::None(_, err)) => Self::draw_series_error(err, rect, frame),
            None => Self::draw_no_series_found(rect, frame),
        }
    }
}

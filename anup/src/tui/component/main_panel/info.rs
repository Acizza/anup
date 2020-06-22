use crate::err::Error;
use crate::series::{LoadedSeries, Series};
use crate::tui::component::Draw;
use crate::tui::widget_util::{block, text};
use crate::tui::{CurrentAction, UIState};
use crate::util;
use anime::remote::ScoreParser;
use chrono::Utc;
use smallvec::SmallVec;
use std::borrow::Cow;
use tui::backend::Backend;
use tui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use tui::style::Color;
use tui::terminal::Frame;
use tui::widgets::{Paragraph, Text};

pub struct InfoPanel;

macro_rules! create_stat_list {
    ($($header:expr => $value:expr),+) => {
        [$(
            create_stat_list!(h $header),
            create_stat_list!(v $value, $header.len()),
        )+]
    };

    (h $header:expr) => {
        text::bold(concat!($header, "\n"))
    };

    (v $value:expr, $len:expr) => {
        text::italic(format!("{:^width$}\n\n", $value, width = $len))
    };
}

impl InfoPanel {
    #[inline(always)]
    pub fn new() -> Self {
        Self {}
    }

    fn text_display_layout(rect: Rect) -> Vec<Rect> {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Percentage(100)].as_ref())
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

    fn draw_series_error<B>(err: &Error, rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
    {
        let header = [text::bold_with("Error Loading Series", |s| {
            s.fg(Color::Red)
        })];

        let body = [text::with_color(format!("{}", err), Color::Red)];

        Self::draw_text_panel(&header, &body, rect, frame);
    }

    fn draw_series_info<B>(state: &UIState, series: &Series, rect: Rect, frame: &mut Frame<B>)
    where
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
            let text_items = {
                let mut items = SmallVec::<[_; 2]>::new();

                items.push(text::bold(&info.title_preferred));

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
            .constraints(
                [
                    Constraint::Percentage(33),
                    Constraint::Percentage(33),
                    Constraint::Percentage(33),
                ]
                .as_ref(),
            )
            .split(layout[1]);

        {
            let left_items = create_stat_list!(
                "Watch Time" => {
                    let watch_time_mins = info.episodes * info.episode_length_mins;
                    util::hm_from_mins(watch_time_mins as f32)
                },
                "Time Left" => {
                    let eps_left = info.episodes - entry.watched_episodes().min(info.episodes);
                    let time_left_mins = eps_left * info.episode_length_mins;

                    util::hm_from_mins(time_left_mins as f32)
                },
                "Episode Length" => format!("{}M", info.episode_length_mins)
            );

            let widget = Paragraph::new(left_items.iter()).alignment(Alignment::Center);
            frame.render_widget(widget, stat_layout[0]);
        }

        {
            let center_items = create_stat_list!(
                "Progress" => format!("{}|{}", entry.watched_episodes(), info.episodes),
                "Score" => match entry.score() {
                    Some(score) => state.remote.score_to_str(score as u8),
                    None => "??".into(),
                },
                "Status" => entry.status()
            );

            let widget = Paragraph::new(center_items.iter()).alignment(Alignment::Center);
            frame.render_widget(widget, stat_layout[1]);
        }

        {
            let right_items = create_stat_list!(
                "Start Date" => match entry.start_date() {
                    Some(date) => format!("{}", date.format("%D")).into(),
                    None => Cow::Borrowed("??"),
                },
                "Finish Date" => match entry.end_date() {
                    Some(date) => format!("{}", date.format("%D")).into(),
                    None => Cow::Borrowed("??"),
                },
                "Rewatched" => entry.times_rewatched()
            );

            let widget = Paragraph::new(right_items.iter()).alignment(Alignment::Center);
            frame.render_widget(widget, stat_layout[2]);
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
            Some(LoadedSeries::Partial(_, err)) | Some(LoadedSeries::None(_, err)) => {
                Self::draw_series_error(err, rect, frame)
            }
            None => Self::draw_no_series_found(rect, frame),
        }
    }
}

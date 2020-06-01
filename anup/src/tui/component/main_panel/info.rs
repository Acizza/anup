use crate::series::Series;
use crate::tui::{CurrentAction, SeriesStatus, UIState};
use crate::util;
use chrono::Utc;
use smallvec::SmallVec;
use std::borrow::Cow;
use tui::backend::Backend;
use tui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use tui::style::{Modifier, Style};
use tui::terminal::Frame;
use tui::widgets::{Block, Borders, Paragraph, Text};

pub struct InfoPanel;

macro_rules! create_stat_list {
    ($($header:expr => $value:expr),+) => {
        [$(
            create_stat_list!(h $header),
            create_stat_list!(v $value, $header.len()),
        )+]
    };

    (h $header:expr) => {
        Text::styled(concat!($header, "\n"), Style::default().modifier(Modifier::BOLD))
    };

    (v $value:expr, $len:expr) => {
        Text::styled(format!("{:^width$}\n\n", $value, width = $len), Style::default().modifier(Modifier::ITALIC))
    };
}

impl InfoPanel {
    pub fn new() -> Self {
        Self {}
    }

    pub fn draw<B>(&mut self, state: &UIState, rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
    {
        let info_block = Block::default().title("Info").borders(Borders::ALL);
        frame.render_widget(info_block, rect);

        let info_layout = Layout::default()
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

        match state.series.selected() {
            Some(SeriesStatus::Loaded(series)) => {
                Self::draw_series_info(state, series, &info_layout, frame)
            }
            Some(SeriesStatus::Unloaded(_)) => (),
            None => {
                let header = [Text::styled(
                    "No Series Found",
                    Style::default().modifier(Modifier::BOLD),
                )];

                let header_pg = Paragraph::new(header.iter()).alignment(Alignment::Center);
                frame.render_widget(header_pg, info_layout[0]);

                let body = [Text::raw(
                    "Add one by pressing the 'a' key\

                    \n\nThe opened panel will require you to specify\
                    \n a name for the series you want to add.\
                    \n\nFor automatic detection, the name should be\
                    \nsimilar to the name of the folder the series\
                    \nis in on disk.",
                )];

                let body_pg = Paragraph::new(body.iter())
                    .alignment(Alignment::Center)
                    .wrap(true);
                frame.render_widget(body_pg, info_layout[1]);
            }
        }
    }

    fn draw_series_info<B>(state: &UIState, series: &Series, layout: &[Rect], frame: &mut Frame<B>)
    where
        B: Backend,
    {
        let info = &series.data.info;
        let entry = &series.data.entry;

        // Series title
        {
            let text_items = {
                let mut items = SmallVec::<[_; 2]>::new();

                items.push(Text::styled(
                    &info.title_preferred,
                    Style::default().modifier(Modifier::BOLD),
                ));

                if entry.needs_sync() {
                    items.push(Text::styled(
                        " [*]",
                        Style::default().modifier(Modifier::ITALIC),
                    ));
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

                let text = [Text::styled(
                    text_str,
                    Style::default().modifier(Modifier::BOLD),
                )];

                let widget = Paragraph::new(text.iter()).alignment(Alignment::Center);
                frame.render_widget(widget, layout[2]);
            }
        }
    }
}

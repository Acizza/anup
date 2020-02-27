use super::{Component, Draw};
use crate::err::Result;
use crate::series::config::SeriesConfig;
use crate::series::info::SeriesInfo;
use crate::series::Series;
use crate::tui::{CurrentAction, LogResult, SelectingSeriesState, SeriesStatus, UIState};
use crate::util;
use chrono::Utc;
use smallvec::SmallVec;
use std::borrow::Cow;
use std::mem;
use termion::event::Key;
use tui::backend::Backend;
use tui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use tui::style::{Color, Modifier, Style};
use tui::terminal::Frame;
use tui::widgets::{Block, Borders, Paragraph, SelectableList, Text, Widget};

pub struct InfoPanel {
    info_panel: SeriesInfoPanel,
    select_panel: SelectSeriesPanel,
}

impl InfoPanel {
    pub fn new() -> Self {
        Self {
            info_panel: SeriesInfoPanel::new(),
            select_panel: SelectSeriesPanel::new(),
        }
    }
}

impl Component for InfoPanel {
    fn process_key(&mut self, key: Key, state: &mut UIState) -> LogResult {
        LogResult::capture("selecting series", || match &mut state.current_action {
            CurrentAction::SelectingSeries(select) => {
                match self.select_panel.process_key(key, select)? {
                    SelectInputResult::Continue => Ok(()),
                    SelectInputResult::Finish => {
                        state.current_action.reset();
                        Ok(())
                    }
                    SelectInputResult::AddSeries(info) => {
                        let select = match mem::take(&mut state.current_action) {
                            CurrentAction::SelectingSeries(state) => state,
                            _ => unreachable!(),
                        };

                        let config = SeriesConfig::from_params(
                            select.nickname,
                            &info,
                            select.params,
                            &state.config,
                        )?;

                        state.add_series(config, info);
                        Ok(())
                    }
                }
            }
            _ => Ok(()),
        })
    }
}

impl<B> Draw<B> for InfoPanel
where
    B: Backend,
{
    fn draw(&mut self, state: &UIState, rect: Rect, frame: &mut Frame<B>) {
        match &state.current_action {
            CurrentAction::SelectingSeries(state) => self.select_panel.draw(state, rect, frame),
            _ => self.info_panel.draw(state, rect, frame),
        }
    }
}

struct SeriesInfoPanel;

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

impl SeriesInfoPanel {
    fn new() -> Self {
        Self {}
    }

    fn draw<B>(&mut self, state: &UIState, rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
    {
        Block::default()
            .title("Info")
            .borders(Borders::ALL)
            .render(frame, rect);

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
            Some(SeriesStatus::Valid(series)) => {
                Self::draw_series_info(state, series, &info_layout, frame)
            }
            Some(SeriesStatus::Invalid(_, reason)) => {
                Self::draw_invalid_series(reason, &info_layout, frame)
            }
            Some(SeriesStatus::Unloaded(_)) => (),
            None => {
                let header =
                    Text::styled("No Series Found", Style::default().modifier(Modifier::BOLD));

                Paragraph::new([header].iter())
                    .alignment(Alignment::Center)
                    .render(frame, info_layout[0]);

                let body = Text::raw(
                    "Add one by pressing ':' and using the 'add' command \

                    \n\nThe command requires a nickname for the series \
                    \nyou want to add. For automatic detection, the nickname \
                    \nshould be similar to the name of the folder the series \
                    \nis in on disk. Any tags in the series folder name \
                    \nshould not affect the detection algorithm. \

                    \n\nIf you don't care about automatic detection, \
                    \nyou can set the ID of the series by appending 'id=<value>' \
                    \nto the 'add' command and use any nickname you wish. \

                    \n\nBy default, the program will look for series in '~/anime/'. \
                    \nYou can change this in `~/.config/anup/config.toml`. \
                    \nYou can also manually specify the path to a series by appending \
                    \n'path=\"<value>\"' to the 'add' command when adding a new series, \
                    \nor the 'set' command when modifying an existing one.",
                );

                Paragraph::new([body].iter())
                    .alignment(Alignment::Center)
                    .wrap(true)
                    .render(frame, info_layout[1]);
            }
        }
    }

    fn draw_series_info<B>(state: &UIState, series: &Series, layout: &[Rect], frame: &mut Frame<B>)
    where
        B: Backend,
    {
        let info = &series.info;
        let entry = &series.entry;

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

            Paragraph::new(text_items.iter())
                .alignment(Alignment::Center)
                .render(frame, layout[0]);
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

            Paragraph::new(left_items.iter())
                .alignment(Alignment::Center)
                .render(frame, stat_layout[0]);
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

            Paragraph::new(center_items.iter())
                .alignment(Alignment::Center)
                .render(frame, stat_layout[1]);
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

            Paragraph::new(right_items.iter())
                .alignment(Alignment::Center)
                .render(frame, stat_layout[2]);
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

                let text = Text::styled(text_str, Style::default().modifier(Modifier::BOLD));

                Paragraph::new([text].iter())
                    .alignment(Alignment::Center)
                    .render(frame, layout[2]);
            }
        }
    }

    fn draw_invalid_series<B>(reason: &str, layout: &[Rect], frame: &mut Frame<B>)
    where
        B: Backend,
    {
        let header = Text::styled(
            "Error Processing Series",
            Style::default().modifier(Modifier::BOLD),
        );

        Paragraph::new([header].iter())
            .alignment(Alignment::Center)
            .render(frame, layout[0]);

        let body = Text::styled(reason, Style::default().fg(Color::Red));

        Paragraph::new([body].iter())
            .alignment(Alignment::Center)
            .wrap(true)
            .render(frame, layout[1]);
    }
}

struct SelectSeriesPanel;

impl SelectSeriesPanel {
    fn new() -> Self {
        Self {}
    }

    fn process_key(
        &mut self,
        key: Key,
        state: &mut SelectingSeriesState,
    ) -> Result<SelectInputResult> {
        match key {
            Key::Up => {
                state.series_list.dec_selected();
                Ok(SelectInputResult::Continue)
            }
            Key::Down => {
                state.series_list.inc_selected();
                Ok(SelectInputResult::Continue)
            }
            Key::Char('\n') => {
                let info = match state.series_list.swap_remove_selected() {
                    Some(info) => info,
                    None => return Ok(SelectInputResult::Finish),
                };

                Ok(SelectInputResult::AddSeries(info))
            }
            Key::Esc => Ok(SelectInputResult::Finish),
            _ => Ok(SelectInputResult::Continue),
        }
    }

    fn draw<B>(&self, state: &SelectingSeriesState, rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
    {
        let names = state
            .series_list
            .iter()
            .map(|info| &info.title_preferred)
            .collect::<Vec<_>>();

        SelectableList::default()
            .block(
                Block::default()
                    .title("Select a series from the list")
                    .borders(Borders::ALL),
            )
            .items(&names)
            .select(Some(state.series_list.index()))
            .style(Style::default().fg(Color::White))
            .highlight_style(Style::default().fg(Color::Green).modifier(Modifier::ITALIC))
            .highlight_symbol(">")
            .render(frame, rect);
    }
}

enum SelectInputResult {
    Continue,
    Finish,
    AddSeries(SeriesInfo),
}

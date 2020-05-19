use super::{Component, Draw};
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
use tui::widgets::{Block, Borders, List, ListState, Paragraph, Text};

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
        match &mut state.current_action {
            CurrentAction::SelectingSeries(select) => {
                match self.select_panel.process_key(key, select) {
                    SelectInputResult::Continue => LogResult::Ok,
                    SelectInputResult::Finish => {
                        state.current_action.reset();
                        LogResult::Ok
                    }
                    SelectInputResult::AddSeries(info) => {
                        LogResult::capture("adding series", || {
                            let select = match mem::take(&mut state.current_action) {
                                CurrentAction::SelectingSeries(state) => state,
                                _ => unreachable!(),
                            };

                            let config = SeriesConfig::from_params(
                                select.nickname,
                                info.id,
                                select.path,
                                select.params,
                                &state.config,
                                &state.db,
                            )?;

                            state.add_series(config, info)
                        })
                    }
                }
            }
            _ => LogResult::Ok,
        }
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

struct SelectSeriesPanel {
    list_state: ListState,
}

impl SelectSeriesPanel {
    fn new() -> Self {
        Self {
            list_state: ListState::default(),
        }
    }

    fn process_key(&mut self, key: Key, state: &mut SelectingSeriesState) -> SelectInputResult {
        match key {
            Key::Up => {
                state.series_list.dec_selected();
                SelectInputResult::Continue
            }
            Key::Down => {
                state.series_list.inc_selected();
                SelectInputResult::Continue
            }
            Key::Char('\n') => {
                let info = match state.series_list.swap_remove_selected() {
                    Some(info) => info,
                    None => return SelectInputResult::Finish,
                };

                SelectInputResult::AddSeries(info)
            }
            Key::Esc => SelectInputResult::Finish,
            _ => SelectInputResult::Continue,
        }
    }

    fn draw<B>(&mut self, state: &SelectingSeriesState, rect: Rect, frame: &mut Frame<B>)
    where
        B: Backend,
    {
        let names = state
            .series_list
            .iter()
            .map(|info| Text::raw(&info.title_preferred));

        let items = List::new(names)
            .block(
                Block::default()
                    .title("Select a series from the list")
                    .borders(Borders::ALL),
            )
            .style(Style::default().fg(Color::White))
            .highlight_style(Style::default().fg(Color::Green).modifier(Modifier::ITALIC))
            .highlight_symbol(">");

        self.list_state.select(Some(state.series_list.index()));

        frame.render_stateful_widget(items, rect, &mut self.list_state);
    }
}

enum SelectInputResult {
    Continue,
    Finish,
    AddSeries(SeriesInfo),
}

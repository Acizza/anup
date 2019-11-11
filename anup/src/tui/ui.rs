use super::component::log::StatusLog;
use super::{SeriesState, UIState, WatchState};
use crate::err::{self, Result};
use crate::util;
use anime::remote::ScoreParser;
use chrono::{Duration, Utc};
use smallvec::SmallVec;
use snafu::ResultExt;
use std::borrow::Cow;
use std::io;
use std::sync::mpsc;
use std::thread;
use termion::event::Key;
use termion::input::TermRead;
use termion::raw::{IntoRawMode, RawTerminal};
use tui::backend::{self, Backend};
use tui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use tui::style::{Color, Modifier, Style};
use tui::terminal::{Frame, Terminal};
use tui::widgets::{Block, Borders, Paragraph, SelectableList, Text, Widget};

pub struct UI<'a, B>
where
    B: Backend,
{
    terminal: Terminal<B>,
    pub status_log: StatusLog<'a>,
}

macro_rules! create_stat_list {
    ($($header:expr => $value:expr),+) => {
        [$(
            create_stat_list!(h $header),
            create_stat_list!(v $header.len(), $value),
        )+]
    };

    (h $header:expr) => {
        Text::styled(format!("{}\n", $header), Style::default().modifier(Modifier::BOLD))
    };

    (v $len:expr, $value:expr) => {
        Text::styled(format!("{:^width$}\n\n", $value, width = $len), Style::default().modifier(Modifier::ITALIC))
    };
}

impl<'a, B> UI<'a, B>
where
    B: Backend,
{
    pub fn clear(&mut self) -> Result<()> {
        self.terminal.clear().context(err::IO)
    }

    pub fn draw<S>(&mut self, state: &UIState, score_parser: &S) -> Result<()>
    where
        S: ScoreParser + ?Sized,
    {
        let status_log = &mut self.status_log;

        self.terminal
            .draw(|mut frame| {
                // Top panels for series information
                let horiz_splitter = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Min(20), Constraint::Percentage(70)].as_ref())
                    .split(frame.size());

                UI::draw_top_panels(state, &horiz_splitter, &mut frame);

                // Series info panel vertical splitter
                let info_panel_splitter = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(80), Constraint::Percentage(20)].as_ref())
                    .split(horiz_splitter[1]);

                UI::draw_info_panel(state, score_parser, &info_panel_splitter, &mut frame);
                UI::draw_status_bar(state, status_log, info_panel_splitter[1], &mut frame);
            })
            .context(err::IO)
    }

    fn draw_top_panels(state: &UIState, layout: &[Rect], frame: &mut Frame<B>) {
        let series_names = state
            .series
            .iter()
            .map(|series| &series.inner.nickname)
            .collect::<SmallVec<[_; 8]>>();

        let mut series_list = SelectableList::default()
            .block(Block::default().title("Series").borders(Borders::ALL))
            .items(&series_names)
            .select(Some(state.selected_series))
            .style(Style::default().fg(Color::White))
            .highlight_style(Style::default().fg(Color::Green).modifier(Modifier::ITALIC))
            .highlight_symbol(">");

        series_list.render(frame, layout[0]);
    }

    fn draw_info_panel<S>(state: &UIState, score_parser: &S, layout: &[Rect], frame: &mut Frame<B>)
    where
        S: ScoreParser + ?Sized,
    {
        Block::default()
            .title("Info")
            .borders(Borders::ALL)
            .render(frame, layout[0]);

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
            .split(layout[0]);

        match state.cur_series() {
            Some(series) => UI::draw_series_info(series, score_parser, &info_layout, frame),
            None => {
                let header =
                    Text::styled("No Series Found", Style::default().modifier(Modifier::BOLD));

                Paragraph::new([header].iter())
                    .alignment(Alignment::Center)
                    .render(frame, info_layout[0]);

                let body = Text::raw(
                    "Add one by launching the program with a \
                    \nkeyword similar to the series name on disk. \
                    
                    \n\nFor example: assuming the path \"~/anime/Ooga Booga\" \
                    \nexists, launching the program with the keyword 'booga' \
                    \nwill add it to the series list. \
                    
                    \n\nYou can also use a non-similar keyword along with \
                    \nthe -p flag to manually specify the path to the series. \
                    
                    \n\nNote that by default, the program will look for a series \
                    \nin \"~/anime/\". You can change this in the config file.",
                );

                Paragraph::new([body].iter())
                    .alignment(Alignment::Center)
                    .render(frame, info_layout[1]);
            }
        }
    }

    fn draw_series_info<S>(
        series: &SeriesState,
        score_parser: &S,
        info_layout: &[Rect],
        frame: &mut Frame<B>,
    ) where
        S: ScoreParser + ?Sized,
    {
        let info = &series.inner.info;
        let entry = &series.inner.entry;

        // Series title
        {
            let text_items = {
                let mut items = SmallVec::<[_; 2]>::new();

                items.push(Text::styled(
                    &info.title.preferred,
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
                .render(frame, info_layout[0]);
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
            .split(info_layout[1]);

        {
            let left_items = create_stat_list!(
                "Watch Time" => {
                    let watch_time_mins = info.episodes * info.episode_length;
                    util::hm_from_mins(watch_time_mins as f32)
                },
                "Time Left" => {
                    let eps_left = info.episodes - entry.watched_eps().min(info.episodes);
                    let time_left_mins = eps_left * info.episode_length;

                    util::hm_from_mins(time_left_mins as f32)
                },
                "Episode Length" => format!("{}M", info.episode_length)
            );

            Paragraph::new(left_items.iter())
                .alignment(Alignment::Center)
                .render(frame, stat_layout[0]);
        }

        {
            let center_items = create_stat_list!(
                "Progress" => format!("{}|{}", entry.watched_eps(), info.episodes),
                "Score" => match entry.score() {
                    Some(score) => score_parser.score_to_str(score),
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
        match series.watch_state {
            WatchState::Idle => (),
            WatchState::Watching(progress_time, _) => {
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
                        .render(frame, info_layout[2]);
                }
            }
        }
    }

    fn draw_status_bar(state: &UIState, log: &mut StatusLog, layout: Rect, frame: &mut Frame<B>) {
        use super::StatusBarState;

        match &state.status_bar_state {
            StatusBarState::Log => {
                log.adjust_to_size(layout, true);

                Paragraph::new(log.draw_items_iter())
                    .block(
                        Block::default()
                            .title("Status [':' for command entry]")
                            .borders(Borders::ALL),
                    )
                    .wrap(true)
                    .render(frame, layout);
            }
            StatusBarState::CommandPrompt(prompt) => {
                Paragraph::new(prompt.draw_items().iter())
                    .block(
                        Block::default()
                            .title("Enter Command")
                            .borders(Borders::ALL),
                    )
                    .wrap(true)
                    .render(frame, layout);
            }
        }
    }
}

pub type TermionBackend = backend::TermionBackend<RawTerminal<io::Stdout>>;

impl<'a> UI<'a, TermionBackend> {
    pub fn init() -> Result<UI<'a, TermionBackend>> {
        let stdout = io::stdout().into_raw_mode().context(err::IO)?;
        let backend = TermionBackend::new(stdout);
        let mut terminal = Terminal::new(backend).context(err::IO)?;

        terminal.clear().context(err::IO)?;
        terminal.hide_cursor().context(err::IO)?;

        Ok(UI {
            terminal,
            status_log: StatusLog::new(),
        })
    }
}

pub enum Event {
    Input(Key),
    Tick,
}

pub struct Events(mpsc::Receiver<Event>);

impl Events {
    pub fn new(tick_rate: Duration) -> Events {
        let (tx, rx) = mpsc::channel();

        Events::spawn_key_handler(tx.clone());
        Events::spawn_tick_handler(tick_rate, tx);

        Events(rx)
    }

    fn spawn_key_handler(tx: mpsc::Sender<Event>) -> thread::JoinHandle<()> {
        let stdin = io::stdin();

        thread::spawn(move || {
            for event in stdin.keys() {
                if let Ok(key) = event {
                    tx.send(Event::Input(key)).unwrap();
                }
            }
        })
    }

    fn spawn_tick_handler(tick_rate: Duration, tx: mpsc::Sender<Event>) -> thread::JoinHandle<()> {
        let tick_rate = tick_rate
            .to_std()
            .unwrap_or_else(|_| std::time::Duration::from_secs(1));

        thread::spawn(move || loop {
            thread::sleep(tick_rate);
            tx.send(Event::Tick).unwrap();
        })
    }

    pub fn next(&self) -> Result<Event> {
        self.0.recv().context(err::MPSCRecv)
    }
}

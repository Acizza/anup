use super::{Selection, UIState, WatchState};
use crate::err::{self, Result};
use crate::util;
use chrono::{Duration, Utc};
use smallvec::SmallVec;
use snafu::ResultExt;
use std::collections::VecDeque;
use std::fmt;
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

pub struct UI<'a, B>
where
    B: Backend,
{
    terminal: Terminal<B>,
    status_log: StatusLog<'a>,
}

impl<'a, B> UI<'a, B>
where
    B: Backend,
{
    pub fn clear(&mut self) -> Result<()> {
        self.terminal.clear().context(err::IO)
    }

    pub fn push_log_status<S>(&mut self, text: S)
    where
        S: Into<LogItem>,
    {
        self.status_log.push(text);
    }

    pub fn log_capture<S, F>(&mut self, text: S, f: F)
    where
        S: Into<String>,
        F: FnOnce() -> Result<()>,
    {
        self.status_log.capture(text, f);
    }

    pub fn draw(&mut self, state: &UIState) -> Result<()> {
        let status_log = &mut self.status_log;

        self.terminal
            .draw(|mut frame| {
                // Top panels for series information
                let horiz_splitter = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints(
                        [
                            Constraint::Min(20),
                            Constraint::Min(8),
                            Constraint::Percentage(60),
                        ]
                        .as_ref(),
                    )
                    .split(frame.size());

                UI::draw_top_panels(state, &horiz_splitter, &mut frame);

                // Series info panel vertical splitter
                let info_panel_splitter = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(80), Constraint::Percentage(20)].as_ref())
                    .split(horiz_splitter[2]);

                UI::draw_info_panel(state, &info_panel_splitter, &mut frame);
                UI::draw_status_bar(status_log, info_panel_splitter[1], &mut frame);
            })
            .context(err::IO)
    }

    fn draw_top_panels(state: &UIState, layout: &[Rect], frame: &mut Frame<B>) {
        let mut series_list = SelectableList::default()
            .block(Block::default().title("Series").borders(Borders::ALL))
            .items(state.series_names.as_ref())
            .select(Some(state.selected_series))
            .style(Style::default().fg(Color::White))
            .highlight_style(Style::default().fg(Color::Green).modifier(Modifier::ITALIC));

        if state.selection == Selection::Series {
            series_list = series_list.highlight_symbol(">");
        }

        series_list.render(frame, layout[0]);

        let season_nums = (1..=state.series.num_seasons)
            .map(|i| i.to_string())
            .collect::<SmallVec<[_; 4]>>();

        let mut season_list = SelectableList::default()
            .block(Block::default().title("Season").borders(Borders::ALL))
            .items(season_nums.as_ref())
            .select(Some(state.series.watch_info.season))
            .style(Style::default().fg(Color::White))
            .highlight_style(Style::default().fg(Color::Green).modifier(Modifier::ITALIC));

        if state.selection == Selection::Season {
            season_list = season_list.highlight_symbol(">");
        }

        season_list.render(frame, layout[1]);
    }

    fn draw_info_panel(state: &UIState, layout: &[Rect], frame: &mut Frame<B>) {
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

        let season = &state.series.season;
        let info = &season.series.info;
        let entry = &season.tracker.entry;

        // Series title
        {
            let text_items = {
                let mut items = SmallVec::<[_; 2]>::new();

                items.push(Text::styled(
                    &info.title,
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
                "Watch Time" => season.value_cache.watch_time,
                "Time Left" => season.value_cache.watch_time_left,
                "Episode Length" => season.value_cache.episode_length
            );

            Paragraph::new(left_items.iter())
                .alignment(Alignment::Center)
                .render(frame, stat_layout[0]);
        }

        {
            let center_items = create_stat_list!(
                "Progress" => season.value_cache.progress,
                "Score" => season.value_cache.score,
                "Status" => entry.status()
            );

            Paragraph::new(center_items.iter())
                .alignment(Alignment::Center)
                .render(frame, stat_layout[1]);
        }

        {
            let right_items = create_stat_list!(
                "Start Date" => season.value_cache.start_date,
                "Finish Date" => season.value_cache.end_date,
                "Rewatched" => entry.times_rewatched()
            );

            Paragraph::new(right_items.iter())
                .alignment(Alignment::Center)
                .render(frame, stat_layout[2]);
        }

        // Watch time needed indicator at bottom
        match season.watch_state {
            WatchState::Idle => (),
            WatchState::Watching(_, progress_time, _) => {
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

    fn draw_status_bar(log: &mut StatusLog, layout: Rect, frame: &mut Frame<B>) {
        log.prepare_to_draw(layout);

        Paragraph::new(log.draw_items().iter())
            .block(Block::default().title("Status").borders(Borders::ALL))
            .render(frame, layout);
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

pub enum LogItemStatus {
    Ok,
    Pending,
    Failed(err::Error),
}

impl LogItemStatus {
    pub fn is_resolved(&self) -> bool {
        match self {
            LogItemStatus::Ok | LogItemStatus::Failed(_) => true,
            LogItemStatus::Pending => false,
        }
    }
}

impl fmt::Display for LogItemStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogItemStatus::Ok => write!(f, "ok"),
            LogItemStatus::Pending => write!(f, "pending"),
            LogItemStatus::Failed(_) => write!(f, "failed"),
        }
    }
}

pub struct LogItem {
    pub text: String,
    pub status: LogItemStatus,
}

impl LogItem {
    pub fn with_status<S>(text: S, status: LogItemStatus) -> LogItem
    where
        S: Into<String>,
    {
        LogItem {
            text: text.into(),
            status,
        }
    }

    pub fn pending<S>(text: S) -> LogItem
    where
        S: Into<String>,
    {
        LogItem::with_status(text, LogItemStatus::Pending)
    }

    pub fn failed<S>(text: S, err: err::Error) -> LogItem
    where
        S: Into<String>,
    {
        LogItem::with_status(text, LogItemStatus::Failed(err))
    }
}

impl<T> From<T> for LogItem
where
    T: Into<String>,
{
    fn from(value: T) -> Self {
        LogItem::pending(value)
    }
}

pub struct StatusLog<'a> {
    items: VecDeque<LogItem>,
    formatted: Vec<Text<'a>>,
    max_items: u16,
}

impl<'a> StatusLog<'a> {
    pub fn new() -> StatusLog<'a> {
        StatusLog {
            items: VecDeque::new(),
            formatted: Vec::new(),
            max_items: 0,
        }
    }

    fn rebuild(&mut self) {
        self.formatted.clear();

        for item in &self.items {
            let value_text = if item.status.is_resolved() {
                format!("{}... ", item.text)
            } else {
                format!("{}...\n", item.text)
            };

            self.formatted.push(Text::raw(value_text));

            if !item.status.is_resolved() {
                continue;
            }

            let status_color = match item.status {
                LogItemStatus::Ok => Color::Green,
                LogItemStatus::Pending => Color::Yellow,
                LogItemStatus::Failed(_) => Color::Red,
            };

            let status = Text::styled(
                format!("{}\n", item.status),
                Style::default().fg(status_color),
            );

            self.formatted.push(status);

            if let LogItemStatus::Failed(err) = &item.status {
                let err_text =
                    Text::styled(format!(".. {}\n", err), Style::default().fg(Color::Red));

                self.formatted.push(err_text);
            }
        }
    }

    pub fn push<I>(&mut self, item: I)
    where
        I: Into<LogItem>,
    {
        self.trim();
        self.items.push_back(item.into());
        self.rebuild();
    }

    pub fn set_status_on_last(&mut self, status: LogItemStatus) {
        let last = match self.items.get_mut(self.items.len() - 1) {
            Some(last) => last,
            None => return,
        };

        last.status = status;
        self.rebuild();
    }

    pub fn capture<S, F>(&mut self, text: S, f: F)
    where
        S: Into<String>,
        F: FnOnce() -> Result<()>,
    {
        self.push(text);

        let status = match f() {
            Ok(_) => LogItemStatus::Ok,
            Err(err) => LogItemStatus::Failed(err),
        };

        self.set_status_on_last(status);
    }

    pub fn prepare_to_draw(&mut self, size: Rect) {
        // We assume a border is around the list, so subtract 2 from the height
        self.max_items = size.height.saturating_sub(2);
        self.trim();
    }

    fn trim(&mut self) {
        while self.items.len() >= self.max_items as usize && !self.items.is_empty() {
            self.items.pop_front();
        }
    }

    /// Returns the log items in a ready-to-draw form.
    ///
    /// Note that this is intended by used with the Paragraph widget type.
    pub fn draw_items(&self) -> &Vec<Text<'a>> {
        &self.formatted
    }
}

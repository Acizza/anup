use super::{Selection, UIState, WatchState};
use crate::err::{self, Result};
use crate::util;
use chrono::{Duration, Utc};
use smallvec::{smallvec, SmallVec};
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
        S: Into<LogItem<'a>>,
    {
        self.status_log.push(text);
    }

    pub fn log_capture<S, F>(&mut self, text: S, f: F)
    where
        S: Into<String>,
        F: FnOnce() -> Result<()>,
    {
        self.status_log.capture_status(text, f);
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
                UI::draw_status_bar(state, status_log, info_panel_splitter[1], &mut frame);
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

    fn draw_status_bar(state: &UIState, log: &mut StatusLog, layout: Rect, frame: &mut Frame<B>) {
        use super::{InputType, StatusBarState};

        match &state.status_bar_state {
            StatusBarState::Log => {
                log.adjust_to_size(layout, true);

                Paragraph::new(log.draw_items.iter())
                    .block(Block::default().title("Status").borders(Borders::ALL))
                    .wrap(true)
                    .render(frame, layout);
            }
            StatusBarState::Input(input_type) => {
                let (title, buffer) = match input_type {
                    InputType::Score(buffer) => ("Input Score", buffer),
                    InputType::SeriesPlayerArgs(buffer) => ("Input Player Args For Series", buffer),
                };

                let text = Text::raw(buffer);

                Paragraph::new([text].iter())
                    .block(Block::default().title(title).borders(Borders::ALL))
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

/// A scrolling log to display messages along with their status.
///
/// # Example Output
///
/// ```
/// Message with ok status... ok
/// Message with failed status... failed
/// .. error cause
/// Message with pending status...
/// ```
pub struct StatusLog<'a> {
    items: VecDeque<LogItem<'a>>,
    draw_items: VecDeque<Text<'a>>,
    max_items: u16,
}

impl<'a> StatusLog<'a> {
    /// Create a new `StatusLog`.
    pub fn new() -> StatusLog<'a> {
        StatusLog {
            items: VecDeque::new(),
            draw_items: VecDeque::new(),
            max_items: 1,
        }
    }

    /// Trim the log so all items fit within the specified `size`.
    ///
    /// Assumes there is both a top and bottom border if `with_border` is true.
    pub fn adjust_to_size(&mut self, size: Rect, with_border: bool) {
        self.max_items = if with_border {
            // One border edge is 1 character tall
            size.height.saturating_sub(2)
        } else {
            size.height
        };

        while self.items.len() > self.max_items as usize {
            let item = match self.items.pop_front() {
                Some(item) => item,
                None => continue,
            };

            for _ in 0..item.text_items.len() {
                self.draw_items.pop_front();
            }
        }
    }

    /// Push a new `LogItem` to the log.
    ///
    /// # Example
    ///
    /// ```
    /// let mut log = StatusLog::new();
    /// log.push(LogItem::pending("Explicitly defined LogItem"));
    /// log.push("Implicitly defined LogItem with a pending status");
    /// ```
    pub fn push<I>(&mut self, item: I)
    where
        I: Into<LogItem<'a>>,
    {
        let item = item.into();
        self.draw_items.extend(item.text_items.iter().cloned());
        self.items.push_back(item);
    }

    /// Execute the function defined by `f` and pushes its result
    /// as a new `LogItem` with the description specified by `desc`.
    ///
    /// # Example
    ///
    /// ```
    /// let mut log = StatusLog::new();
    /// log.capture_status("Executing function 1", || Ok(()));
    /// ```
    pub fn capture_status<S, F>(&mut self, desc: S, f: F)
    where
        S: Into<String>,
        F: FnOnce() -> Result<()>,
    {
        let status = match f() {
            Ok(_) => LogItemStatus::Ok,
            Err(err) => LogItemStatus::Failed(Some(err)),
        };

        self.push(LogItem::with_status(desc, status));
    }
}

/// A log entry meant to be used with `StatusLog`.
pub struct LogItem<'a> {
    text_items: SmallVec<[Text<'a>; 3]>,
}

impl<'a> LogItem<'a> {
    /// Create a LogItem with the specified description and status.
    pub fn with_status<S>(desc: S, status: LogItemStatus) -> LogItem<'a>
    where
        S: Into<String>,
    {
        let text_items = LogItem::create_text_items(desc, status);
        LogItem { text_items }
    }

    /// Create a LogItem with its status set to `LogItemStatus::Pending`.
    pub fn pending<S>(desc: S) -> LogItem<'a>
    where
        S: Into<String>,
    {
        LogItem::with_status(desc, LogItemStatus::Pending)
    }

    /// Create a LogItem with its status set to `LogItemStatus::Failed`.
    pub fn failed<S, O>(desc: S, err: O) -> LogItem<'a>
    where
        S: Into<String>,
        O: Into<Option<err::Error>>,
    {
        LogItem::with_status(desc, LogItemStatus::Failed(err.into()))
    }

    fn create_text_items<S>(desc: S, status: LogItemStatus) -> SmallVec<[Text<'a>; 3]>
    where
        S: Into<String>,
    {
        let desc_text = if status.is_resolved() {
            Text::raw(format!("{}... ", desc.into()))
        } else {
            Text::raw(format!("{}...\n", desc.into()))
        };

        let mut text_items = smallvec![desc_text];

        // Beyond this point, we only need to resolve the status (if we have it)
        if !status.is_resolved() {
            return text_items;
        }

        let status_text = {
            let color = match status {
                LogItemStatus::Ok => Color::Green,
                LogItemStatus::Pending => Color::Yellow,
                LogItemStatus::Failed(_) => Color::Red,
            };

            Text::styled(format!("{}\n", status), Style::default().fg(color))
        };

        text_items.push(status_text);

        if let LogItemStatus::Failed(Some(err)) = &status {
            let err_text = Text::styled(format!(".. {}\n", err), Style::default().fg(Color::Red));
            text_items.push(err_text);
        }

        text_items
    }
}

impl<'a, T> From<T> for LogItem<'a>
where
    T: Into<String>,
{
    fn from(value: T) -> Self {
        LogItem::pending(value)
    }
}

/// The result of a log event. Meant to be used with `LogItem`.
pub enum LogItemStatus {
    Ok,
    Pending,
    Failed(Option<err::Error>),
}

impl LogItemStatus {
    /// Returns true if the status indicates that it's not waiting for the result of an operation.
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

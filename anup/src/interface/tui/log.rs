use crate::err::{self, Result};
use smallvec::{smallvec, SmallVec};
use std::collections::VecDeque;
use std::fmt;
use tui::layout::Rect;
use tui::style::{Color, Style};
use tui::widgets::Text;

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

    /// Returns an iterator over all of the text items ready to be drawn.
    pub fn draw_items_iter(&self) -> impl Iterator<Item = &Text> {
        self.draw_items.iter()
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

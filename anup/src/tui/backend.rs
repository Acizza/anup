use anyhow::{Context, Result};
use crossterm::event::{Event, EventStream};
use crossterm::terminal;
use futures::{future::FutureExt, select, StreamExt};
use futures_timer::Delay;
use std::io;
use std::time::Duration;
use tui::backend::CrosstermBackend;
use tui::terminal::Terminal;

use crate::key::Key;

pub struct UIBackend {
    pub terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl UIBackend {
    pub fn init() -> Result<Self> {
        terminal::enable_raw_mode().context("failed to enable raw mode")?;

        let stdout = io::stdout();
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend).context("terminal creation failed")?;

        terminal.clear().context("failed to clear terminal")?;

        terminal
            .hide_cursor()
            .context("failed to hide mouse cursor")?;

        Ok(Self { terminal })
    }

    #[inline(always)]
    pub fn clear(&mut self) -> Result<()> {
        self.terminal.clear().map_err(Into::into)
    }
}

#[derive(Debug)]
pub enum EventKind {
    Key(Key),
    Tick,
}

pub enum ErrorKind {
    ExitRequest,
    Other(anyhow::Error),
}

type EventError<T> = std::result::Result<T, ErrorKind>;

pub struct Events {
    reader: EventStream,
}

impl Events {
    const TICK_DURATION_MS: u64 = 1_000;

    pub fn new() -> Self {
        Self {
            reader: EventStream::new(),
        }
    }

    #[allow(clippy::mut_mut)]
    pub async fn next(&mut self) -> EventError<Option<EventKind>> {
        let mut tick = Delay::new(Duration::from_millis(Self::TICK_DURATION_MS)).fuse();
        let mut next_event = self.reader.next().fuse();

        select! {
            _ = tick => Ok(Some(EventKind::Tick)),
            event = next_event => match event {
                Some(Ok(Event::Key(key))) => Ok(Some(EventKind::Key(Key::new(key)))),
                Some(Ok(_)) => Ok(None),
                Some(Err(err)) => Err(ErrorKind::Other(err.into())),
                None => Err(ErrorKind::ExitRequest),
            }
        }
    }
}

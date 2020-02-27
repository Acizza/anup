use crate::err::{self, Result};
use chrono::Duration;
use snafu::ResultExt;
use std::io::{self, Write};
use std::sync::mpsc;
use std::thread;
use termion::event::Key;
use termion::input::TermRead;
use termion::raw::{IntoRawMode, RawTerminal};
use tui::backend::{self, Backend};
use tui::layout::Rect;
use tui::terminal::Terminal;

pub struct UIBackend<B>
where
    B: Backend,
{
    pub terminal: Terminal<B>,
    pub cursor_visible: bool,
}

impl<B> UIBackend<B>
where
    B: Backend,
{
    const BORDER_SIZE: u16 = 2;

    #[inline(always)]
    pub fn clear(&mut self) -> Result<()> {
        self.terminal.clear().context(err::IO)
    }

    pub fn show_cursor(&mut self) -> Result<()> {
        self.terminal.show_cursor().context(err::IO)?;
        self.cursor_visible = true;
        Ok(())
    }

    pub fn hide_cursor(&mut self) -> Result<()> {
        self.terminal.hide_cursor().context(err::IO)?;
        self.cursor_visible = false;
        Ok(())
    }

    pub fn set_cursor_inside(&mut self, column: u16, rect: Rect) -> Result<()> {
        let rect_width = rect.width.saturating_sub(Self::BORDER_SIZE);

        let (len, line_num) = if rect_width > 0 {
            let line_num = column / rect_width;
            let max_line = rect.height - Self::BORDER_SIZE - 1;

            // We want to cap the position of the cursor to the last character of the last line
            if line_num > max_line {
                (rect_width - 1, max_line)
            } else {
                (column % rect_width, line_num)
            }
        } else {
            (column, 0)
        };

        let x = 1 + rect.left() + len;
        let y = 1 + rect.top() + line_num;

        self.terminal.set_cursor(x, y).context(err::IO)?;

        // We should flush stdout so the cursor immediately goes to our new position
        io::stdout().flush().context(err::IO)
    }

    #[inline(always)]
    pub fn will_cursor_fit(&self, rect: Rect) -> bool {
        rect.height > Self::BORDER_SIZE && rect.width > Self::BORDER_SIZE
    }
}

pub type TermionBackend = backend::TermionBackend<RawTerminal<io::Stdout>>;

impl UIBackend<TermionBackend> {
    pub fn init() -> Result<Self> {
        let stdout = io::stdout().into_raw_mode().context(err::IO)?;
        let backend = TermionBackend::new(stdout);
        let mut terminal = Terminal::new(backend).context(err::IO)?;

        terminal.clear().context(err::IO)?;
        terminal.hide_cursor().context(err::IO)?;

        Ok(Self {
            terminal,
            cursor_visible: false,
        })
    }
}

pub enum UIEvent {
    Input(Key),
    Tick,
}

pub struct UIEvents(mpsc::Receiver<UIEvent>);

impl UIEvents {
    pub fn new(tick_rate: Duration) -> Self {
        let (tx, rx) = mpsc::channel();

        Self::spawn_key_handler(tx.clone());
        Self::spawn_tick_handler(tick_rate, tx);

        Self(rx)
    }

    fn spawn_key_handler(tx: mpsc::Sender<UIEvent>) -> thread::JoinHandle<()> {
        let stdin = io::stdin();

        thread::spawn(move || {
            for event in stdin.keys() {
                if let Ok(key) = event {
                    tx.send(UIEvent::Input(key)).unwrap();
                }
            }
        })
    }

    fn spawn_tick_handler(
        tick_rate: Duration,
        tx: mpsc::Sender<UIEvent>,
    ) -> thread::JoinHandle<()> {
        let tick_rate = tick_rate
            .to_std()
            .unwrap_or_else(|_| std::time::Duration::from_secs(1));

        thread::spawn(move || loop {
            thread::sleep(tick_rate);
            tx.send(UIEvent::Tick).unwrap();
        })
    }

    #[inline(always)]
    pub fn next(&self) -> Result<UIEvent> {
        self.0.recv().context(err::MPSCRecv)
    }
}

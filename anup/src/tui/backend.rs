use anyhow::{Context, Result};
use crossterm::terminal;
use std::io;
use terminal_size::{terminal_size, Height, Width};
use tui::terminal::Terminal;
use tui::{backend::CrosstermBackend, layout::Rect};

pub struct UIBackend {
    pub terminal: Terminal<CrosstermBackend<io::Stdout>>,
    last_width: u16,
    last_height: u16,
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

        let size = terminal.size().unwrap_or_else(|_| Rect::default());
        let last_width = size.width;
        let last_height = size.height;

        Ok(Self {
            terminal,
            last_width,
            last_height,
        })
    }

    #[inline(always)]
    pub fn clear(&mut self) -> Result<()> {
        self.terminal.clear().map_err(Into::into)
    }

    pub fn update_term_size(&mut self) -> io::Result<bool> {
        // The terminal_size crate is much faster than the current backend (crossterm) for retrieving the terminal size
        let (width, height) = match terminal_size() {
            Some((Width(w), Height(h))) => (w, h),
            None => return Ok(false),
        };

        let changed = width != self.last_width || height != self.last_height;

        self.last_width = width;
        self.last_height = height;

        Ok(changed)
    }
}

use anyhow::{Context, Result};
use crossterm::terminal;
use std::io;
use tui::backend::CrosstermBackend;
use tui::terminal::Terminal;

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

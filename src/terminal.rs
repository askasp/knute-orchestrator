use std::io::{self, stdout, Stdout};

use crossterm::{
    execute,
    event::{EnableMouseCapture, DisableMouseCapture},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

pub type Tui = Terminal<CrosstermBackend<Stdout>>;

pub fn init() -> io::Result<Tui> {
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout());
    Terminal::new(backend)
}

pub fn restore() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen, DisableMouseCapture)?;
    Ok(())
}

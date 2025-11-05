// TUI module for the Crucible CLI
//
// This module implements a terminal user interface using ratatui, providing:
// - Real-time log display from worker threads
// - Status bar with kiln statistics
// - REPL for SurrealQL queries and tool execution
//
// Architecture: Actor-based with message passing via tokio channels.
// See /docs/TUI_ARCHITECTURE.md for design details.

pub mod app;
pub mod events;
pub mod log_buffer;
pub mod repl_state;
pub mod tracing_layer;
pub mod widgets;

use anyhow::Result;
use crossterm::{
    event::{self, Event as CrosstermEvent, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::Duration;
use tokio::sync::mpsc;

pub use app::{App, AppMode, RenderState, ScrollState, StatusBar};
pub use events::{LogEntry, ReplResult, StatusUpdate, UiEvent};
pub use log_buffer::LogBuffer;
pub use repl_state::{ExecutionState, ReplState};
pub use tracing_layer::TuiLayer;

/// TUI configuration
#[derive(Debug, Clone)]
pub struct TuiConfig {
    /// Log buffer capacity (number of lines)
    pub log_capacity: usize,
    /// REPL history capacity
    pub history_capacity: usize,
    /// Status update throttle (milliseconds)
    pub status_throttle_ms: u64,
    /// Log/REPL split ratio (percentage for logs)
    pub log_split_ratio: u16,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            log_capacity: 20,
            history_capacity: 100,
            status_throttle_ms: 100,
            log_split_ratio: 70,
        }
    }
}

/// Run the TUI event loop
///
/// This is the main entry point for the CLI TUI. It:
/// 1. Sets up the terminal in raw mode with alternate screen
/// 2. Creates the App state with channel receivers
/// 3. Runs the event loop (tokio::select! multiplexing)
/// 4. Cleans up terminal on exit
///
/// # Arguments
/// - `log_rx`: Channel receiving log entries from worker threads
/// - `status_rx`: Channel receiving status updates (doc count, DB size, etc.)
/// - `config`: TUI configuration
///
/// # Returns
/// Ok(()) on clean shutdown, Err on terminal/rendering errors
pub async fn run_tui(
    log_rx: mpsc::Receiver<LogEntry>,
    status_rx: mpsc::Receiver<StatusUpdate>,
    config: TuiConfig,
) -> Result<()> {
    // Setup terminal
    let mut terminal = setup_terminal()?;

    // Initialize app state
    let mut app = App::new(log_rx, status_rx, config);

    // Main event loop
    loop {
        // Render UI if dirty
        if app.render_state().is_dirty() {
            terminal.draw(|frame| {
                widgets::render(&mut app, frame);
            })?;
            app.clear_dirty();
        }

        // Poll for terminal events (non-blocking)
        if event::poll(Duration::from_millis(10))? {
            if let CrosstermEvent::Key(key) = event::read()? {
                // Quick exit on Ctrl+C
                if key.code == KeyCode::Char('c')
                    && key.modifiers.contains(event::KeyModifiers::CONTROL)
                {
                    app.shutdown();
                } else {
                    app.handle_event(UiEvent::Input(CrosstermEvent::Key(key)))
                        .await?;
                }
            }
        }

        // Process channel events (non-blocking)
        while let Ok(log_entry) = app.log_rx.try_recv() {
            app.handle_event(UiEvent::Log(log_entry)).await?;
        }

        while let Ok(status) = app.status_rx.try_recv() {
            app.handle_event(UiEvent::Status(status)).await?;
        }

        while let Ok(result) = app.repl_rx.try_recv() {
            app.handle_event(UiEvent::ReplResult(result)).await?;
        }

        // Exit on quit
        if app.is_exiting() {
            break;
        }
    }

    // Restore terminal
    restore_terminal(terminal)?;

    Ok(())
}

/// Setup terminal for TUI rendering
fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;

    Ok(terminal)
}

/// Restore terminal to normal mode
fn restore_terminal(mut terminal: Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tui_config_defaults() {
        // Test configuration defaults (doesn't require terminal)
        let config = TuiConfig::default();
        assert_eq!(config.log_capacity, 20);
        assert_eq!(config.history_capacity, 100);
        assert_eq!(config.status_throttle_ms, 100);
        assert_eq!(config.log_split_ratio, 70);
    }

    // Note: Terminal setup/teardown tests require a TTY
    // and will fail in CI environments. Manual testing required.
}

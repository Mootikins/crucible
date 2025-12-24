//! Standalone agent picker for lazy agent selection
//!
//! A minimal TUI that shows available agents and returns the user's selection.
//! Used when lazy_agent_selection is enabled and no --agent flag is provided.

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::stdout;
use std::time::Duration;

use super::splash::{SplashState, SplashWidget};

/// Poll interval for input events (milliseconds)
/// Balances responsiveness for probe updates with CPU usage
const PICKER_POLL_INTERVAL_MS: u64 = 50;

/// Result of the agent picker
#[derive(Debug, Clone)]
pub enum AgentSelection {
    /// User selected an ACP agent by name
    Acp(String),
    /// User selected the internal agent
    Internal,
    /// User cancelled (quit)
    Cancelled,
}

/// Run the agent picker and return the user's selection
///
/// This is a blocking call that shows a minimal TUI for agent selection.
/// Returns when the user confirms a selection or cancels.
pub async fn pick_agent() -> Result<AgentSelection> {
    // Get current directory for display
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| ".".to_string());

    // Create splash state
    let mut state = SplashState::new(cwd);

    // Probe for available agents in background
    let probe_handle = tokio::spawn(async { crucible_acp::probe_all_agents().await });

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_picker_loop(&mut terminal, &mut state, probe_handle).await;

    // Cleanup terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    result
}

async fn run_picker_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    state: &mut SplashState,
    probe_handle: tokio::task::JoinHandle<Vec<crucible_acp::KnownAgent>>,
) -> Result<AgentSelection> {
    let mut probe_handle = Some(probe_handle);

    loop {
        // Check if probe completed
        if let Some(handle) = probe_handle.as_ref() {
            if handle.is_finished() {
                if let Some(h) = probe_handle.take() {
                    match h.await {
                        Ok(probed) => state.update_availability(probed),
                        Err(e) => {
                            tracing::warn!("Agent probe task failed: {}", e);
                            // Mark as probed with empty results so UI shows unavailable
                            state.update_availability(vec![]);
                        }
                    }
                }
            }
        }

        // Render
        terminal.draw(|frame| {
            let widget = SplashWidget::new(state);
            frame.render_widget(widget, frame.area());
        })?;

        // Handle input with short timeout for responsive probe updates
        if event::poll(Duration::from_millis(PICKER_POLL_INTERVAL_MS))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    // Navigation
                    KeyCode::Up | KeyCode::Char('k') => {
                        state.select_prev();
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        state.select_next();
                    }
                    // Quick select by number
                    KeyCode::Char(c @ '1'..='9') => {
                        let index = (c as usize) - ('1' as usize);
                        state.select_index(index);
                    }
                    // Confirm selection
                    KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('l') => {
                        if state.can_confirm() {
                            if let Some(agent) = state.selected_agent() {
                                let selection = if agent.name == "internal" {
                                    AgentSelection::Internal
                                } else {
                                    AgentSelection::Acp(agent.name.clone())
                                };
                                return Ok(selection);
                            }
                        }
                    }
                    // Cancel
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('h') => {
                        return Ok(AgentSelection::Cancelled);
                    }
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_selection_variants() {
        let acp = AgentSelection::Acp("opencode".to_string());
        assert!(matches!(acp, AgentSelection::Acp(_)));

        let internal = AgentSelection::Internal;
        assert!(matches!(internal, AgentSelection::Internal));

        let cancelled = AgentSelection::Cancelled;
        assert!(matches!(cancelled, AgentSelection::Cancelled));
    }
}

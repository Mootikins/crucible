// Main application state for the TUI
//
// The App struct owns all UI state and coordinates event handling.
// It follows the Elm Architecture pattern:
// - Events flow in via handle_event()
// - State is mutated in response
// - Dirty flags trigger re-renders

use super::{
    events::{LogEntry, ReplResult, StatusUpdate, UiEvent},
    log_buffer::LogBuffer,
    repl_state::{ExecutionState, ReplState},
    TuiConfig,
};
use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyModifiers};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// Application mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    /// Normal operation
    Running,
    /// User is typing in REPL
    Input,
    /// Scrolling through logs
    Scrolling,
    /// Shutting down
    Exiting,
}

/// Render optimization state
///
/// Tracks which UI sections need re-rendering to avoid unnecessary draws.
#[derive(Debug, Default)]
pub struct RenderState {
    pub header_dirty: bool,
    pub logs_dirty: bool,
    pub repl_dirty: bool,
}

impl RenderState {
    pub fn is_dirty(&self) -> bool {
        self.header_dirty || self.logs_dirty || self.repl_dirty
    }

    pub fn clear(&mut self) {
        self.header_dirty = false;
        self.logs_dirty = false;
        self.repl_dirty = false;
    }

    pub fn mark_all_dirty(&mut self) {
        self.header_dirty = true;
        self.logs_dirty = true;
        self.repl_dirty = true;
    }
}

/// Log scroll state
#[derive(Debug)]
pub struct ScrollState {
    /// Current scroll offset (0 = bottom/latest)
    pub offset: usize,
    /// Whether auto-scroll is enabled
    pub auto_scroll: bool,
}

impl Default for ScrollState {
    fn default() -> Self {
        Self {
            offset: 0,
            auto_scroll: true, // Auto-scroll by default
        }
    }
}

impl ScrollState {
    pub fn scroll_up(&mut self, lines: usize) {
        self.offset = self.offset.saturating_add(lines);
    }

    pub fn scroll_down(&mut self, lines: usize) {
        self.offset = self.offset.saturating_sub(lines);
    }

    pub fn scroll_to_bottom(&mut self) {
        self.offset = 0;
        self.auto_scroll = true;
    }
}

/// Status bar state
#[derive(Debug)]
pub struct StatusBar {
    pub vault_path: PathBuf,
    pub db_type: String,
    pub doc_count: u64,
    pub db_size: u64,
    pub last_update: Instant,
}

impl Default for StatusBar {
    fn default() -> Self {
        Self {
            vault_path: PathBuf::from("~"),
            db_type: "Unknown".to_string(),
            doc_count: 0,
            db_size: 0,
            last_update: Instant::now(),
        }
    }
}

impl StatusBar {
    /// Apply a partial status update
    pub fn apply_update(&mut self, update: StatusUpdate) {
        if let Some(path) = update.vault_path {
            self.vault_path = path;
        }
        if let Some(db_type) = update.db_type {
            self.db_type = db_type;
        }
        if let Some(count) = update.doc_count {
            self.doc_count = count;
        }
        if let Some(size) = update.db_size {
            self.db_size = size;
        }
        self.last_update = Instant::now();
    }
}

/// Main application state
///
/// Owns all UI state and coordinates event handling.
/// Designed for single-threaded access in the main TUI event loop.
pub struct App {
    /// Current mode
    pub mode: AppMode,

    /// Log buffer (ring buffer)
    pub logs: LogBuffer,

    /// Status bar state
    pub status: StatusBar,

    /// REPL state
    pub repl: ReplState,

    /// Render optimization
    pub render_state: RenderState,

    /// Log scroll state
    pub log_scroll: ScrollState,

    /// Configuration
    pub config: TuiConfig,

    /// Status update throttling
    last_status_update: Instant,

    /// Last REPL result
    pub last_repl_result: Option<ReplResult>,

    /// Channel receivers (owned by App)
    pub log_rx: mpsc::Receiver<LogEntry>,
    pub status_rx: mpsc::Receiver<StatusUpdate>,
    pub repl_rx: mpsc::Receiver<ReplResult>,
}

impl App {
    /// Create a new application state
    pub fn new(
        log_rx: mpsc::Receiver<LogEntry>,
        status_rx: mpsc::Receiver<StatusUpdate>,
        config: TuiConfig,
    ) -> Self {
        // Create REPL result channel
        let (_, repl_rx) = mpsc::channel(10);

        // Initialize last_status_update to a time in the past
        // so the first status update is never throttled
        let last_status_update = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .unwrap_or_else(Instant::now);

        Self {
            mode: AppMode::Input, // Start in input mode
            logs: LogBuffer::new(config.log_capacity),
            status: StatusBar::default(),
            repl: ReplState::new(config.history_capacity),
            render_state: RenderState::default(),
            log_scroll: ScrollState::default(),
            config,
            last_status_update,
            last_repl_result: None,
            log_rx,
            status_rx,
            repl_rx,
        }
    }

    /// Get render state
    pub fn render_state(&self) -> &RenderState {
        &self.render_state
    }

    /// Clear dirty flags
    pub fn clear_dirty(&mut self) {
        self.render_state.clear();
    }

    /// Check if exiting
    pub fn is_exiting(&self) -> bool {
        self.mode == AppMode::Exiting
    }

    /// Request shutdown
    pub fn shutdown(&mut self) {
        self.mode = AppMode::Exiting;
    }

    /// Handle incoming event
    pub async fn handle_event(&mut self, event: UiEvent) -> Result<()> {
        match event {
            UiEvent::Input(input) => self.handle_input(input).await?,
            UiEvent::Log(entry) => self.handle_log(entry),
            UiEvent::Status(update) => self.handle_status(update),
            UiEvent::ReplResult(result) => self.handle_repl_result(result),
            UiEvent::Shutdown => self.shutdown(),
        }
        Ok(())
    }

    /// Handle log entry
    fn handle_log(&mut self, entry: LogEntry) {
        self.logs.push(entry);

        // Auto-scroll to latest if enabled
        if self.log_scroll.auto_scroll {
            self.log_scroll.offset = 0;
        }

        self.render_state.logs_dirty = true;
    }

    /// Handle status update
    fn handle_status(&mut self, update: StatusUpdate) {
        // Throttle status updates to avoid excessive renders
        let now = Instant::now();
        let throttle = Duration::from_millis(self.config.status_throttle_ms);

        if now.duration_since(self.last_status_update) < throttle {
            return; // Drop update
        }

        self.status.apply_update(update);
        self.render_state.header_dirty = true;
        self.last_status_update = now;
    }

    /// Handle REPL result
    fn handle_repl_result(&mut self, result: ReplResult) {
        self.last_repl_result = Some(result);
        self.repl.set_execution_state(ExecutionState::Idle);
        self.render_state.repl_dirty = true;
    }

    /// Handle keyboard input
    async fn handle_input(&mut self, event: Event) -> Result<()> {
        match event {
            Event::Key(key) => match (key.code, key.modifiers) {
                // Ctrl+C - quit
                (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                    self.shutdown();
                }

                // Ctrl+D - quit (Unix convention)
                (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                    self.shutdown();
                }

                // Enter - submit command
                (KeyCode::Enter, _) if self.mode == AppMode::Input => {
                    self.submit_command().await?;
                }

                // Up/Down - history navigation or log scrolling
                (KeyCode::Up, _) if self.mode == AppMode::Input => {
                    self.repl.history_prev();
                    self.render_state.repl_dirty = true;
                }
                (KeyCode::Down, _) if self.mode == AppMode::Input => {
                    self.repl.history_next();
                    self.render_state.repl_dirty = true;
                }

                // Page Up/Down - scroll logs
                (KeyCode::PageUp, _) => {
                    self.log_scroll.scroll_up(10);
                    self.log_scroll.auto_scroll = false;
                    self.mode = AppMode::Scrolling;
                    self.render_state.logs_dirty = true;
                }
                (KeyCode::PageDown, _) => {
                    self.log_scroll.scroll_down(10);
                    if self.log_scroll.offset == 0 {
                        self.log_scroll.auto_scroll = true;
                        self.mode = AppMode::Input;
                    }
                    self.render_state.logs_dirty = true;
                }

                // Home/End - cursor movement
                (KeyCode::Home, _) if self.mode == AppMode::Input => {
                    self.repl.move_cursor_home();
                    self.render_state.repl_dirty = true;
                }
                (KeyCode::End, _) if self.mode == AppMode::Input => {
                    self.repl.move_cursor_end();
                    self.render_state.repl_dirty = true;
                }

                // Char input
                (KeyCode::Char(c), _) if self.mode == AppMode::Input => {
                    self.repl.insert_char(c);
                    self.render_state.repl_dirty = true;
                }

                // Backspace
                (KeyCode::Backspace, _) if self.mode == AppMode::Input => {
                    self.repl.delete_char();
                    self.render_state.repl_dirty = true;
                }

                // Delete
                (KeyCode::Delete, _) if self.mode == AppMode::Input => {
                    self.repl.delete_char_forward();
                    self.render_state.repl_dirty = true;
                }

                // Left/Right - cursor movement
                (KeyCode::Left, _) if self.mode == AppMode::Input => {
                    self.repl.move_cursor_left();
                    self.render_state.repl_dirty = true;
                }
                (KeyCode::Right, _) if self.mode == AppMode::Input => {
                    self.repl.move_cursor_right();
                    self.render_state.repl_dirty = true;
                }

                _ => {}
            },

            Event::Resize(_, _) => {
                // Mark all sections dirty on resize
                self.render_state.mark_all_dirty();
            }

            _ => {}
        }

        Ok(())
    }

    /// Submit REPL command
    async fn submit_command(&mut self) -> Result<()> {
        let command = self.repl.submit();

        // Handle built-in commands
        if command.starts_with(':') {
            self.handle_builtin_command(&command).await?;
        } else {
            // TODO: Send to REPL executor
            // For now, just echo back
            let result = ReplResult::success(
                format!("Would execute: {}", command),
                Duration::from_millis(0),
            );
            self.handle_repl_result(result);
        }

        Ok(())
    }

    /// Handle built-in commands (:quit, :help, etc.)
    async fn handle_builtin_command(&mut self, command: &str) -> Result<()> {
        match command.trim() {
            ":quit" | ":q" => {
                self.shutdown();
            }

            ":help" | ":h" => {
                let help_text = r#"
Built-in Commands:
  :quit, :q       - Exit daemon
  :help, :h       - Show this help
  :clear          - Clear REPL output
  :stats          - Show vault statistics
  :tools          - List available tools
  :log <level>    - Set log level

SurrealQL Queries:
  SELECT * FROM notes WHERE tags CONTAINS '#project';
  SELECT ->links->note.title FROM notes WHERE path = 'foo.md';

Tool Execution:
  :run search_by_tags project ai
  :run semantic_search "agent orchestration"
"#;
                let result = ReplResult::success(help_text, Duration::from_millis(0));
                self.handle_repl_result(result);
            }

            ":clear" => {
                self.last_repl_result = None;
                self.render_state.repl_dirty = true;
            }

            _ => {
                let result =
                    ReplResult::error(format!("Unknown command: {}. Try :help", command));
                self.handle_repl_result(result);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_state() {
        let mut state = RenderState::default();
        assert!(!state.is_dirty());

        state.logs_dirty = true;
        assert!(state.is_dirty());

        state.clear();
        assert!(!state.is_dirty());
    }

    #[test]
    fn test_scroll_state() {
        let mut scroll = ScrollState::default();
        assert_eq!(scroll.offset, 0);
        assert!(scroll.auto_scroll);

        scroll.scroll_up(5);
        assert_eq!(scroll.offset, 5);

        scroll.scroll_down(3);
        assert_eq!(scroll.offset, 2);

        scroll.scroll_to_bottom();
        assert_eq!(scroll.offset, 0);
        assert!(scroll.auto_scroll);
    }

    #[test]
    fn test_status_bar_update() {
        let mut status = StatusBar::default();

        let update = StatusUpdate::new()
            .with_vault_path(PathBuf::from("/vault"))
            .with_doc_count(42);

        status.apply_update(update);

        assert_eq!(status.vault_path, PathBuf::from("/vault"));
        assert_eq!(status.doc_count, 42);
        assert_eq!(status.db_type, "Unknown"); // Unchanged
    }
}

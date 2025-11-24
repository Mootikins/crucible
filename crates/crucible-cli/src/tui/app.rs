// Main application state for the TUI
//
// The App struct owns all UI state and coordinates event handling.
// It follows the Elm Architecture pattern:
// - Events flow in via handle_event()
// - State is mutated in response
// - Dirty flags trigger re-renders

use super::{
    events::{LogEntry, StatusUpdate, UiEvent},
    log_buffer::LogBuffer,
    TuiConfig,
};

// Stub types for removed REPL functionality (commit 37cd887)
#[derive(Debug, Clone)]
pub struct ReplState;

impl ReplState {
    pub fn new(_capacity: usize) -> Self {
        Self
    }

    pub fn set_execution_state(&mut self, _state: ExecutionState) {}
    pub fn history_prev(&mut self) {}
    pub fn history_next(&mut self) {}
    pub fn move_cursor_home(&mut self) {}
    pub fn move_cursor_end(&mut self) {}
    pub fn insert_char(&mut self, _c: char) {}
    pub fn delete_char(&mut self) {}
    pub fn delete_char_forward(&mut self) {}
    pub fn move_cursor_left(&mut self) {}
    pub fn move_cursor_right(&mut self) {}
    pub fn submit(&mut self) -> String {
        String::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionState {
    Idle,
}
use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyModifiers};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// Application mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    /// Normal operation - viewing logs
    Running,
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
}

impl RenderState {
    pub fn is_dirty(&self) -> bool {
        self.header_dirty || self.logs_dirty
    }

    pub fn clear(&mut self) {
        self.header_dirty = false;
        self.logs_dirty = false;
    }

    pub fn mark_all_dirty(&mut self) {
        self.header_dirty = true;
        self.logs_dirty = true;
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
    pub kiln_path: PathBuf,
    pub db_type: String,
    pub doc_count: u64,
    pub db_size: u64,
    pub last_update: Instant,
}

impl Default for StatusBar {
    fn default() -> Self {
        Self {
            kiln_path: PathBuf::from("~"),
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
        if let Some(path) = update.kiln_path {
            self.kiln_path = path;
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

    /// Render optimization
    pub render_state: RenderState,

    /// Log scroll state
    pub log_scroll: ScrollState,

    /// Configuration
    pub config: TuiConfig,

    /// Status update throttling
    last_status_update: Instant,

    /// Channel receivers (owned by App)
    pub log_rx: mpsc::Receiver<LogEntry>,
    pub status_rx: mpsc::Receiver<StatusUpdate>,
}

impl App {
    /// Create a new application state
    pub fn new(
        log_rx: mpsc::Receiver<LogEntry>,
        status_rx: mpsc::Receiver<StatusUpdate>,
        config: TuiConfig,
    ) -> Self {
        // Initialize last_status_update to a time in the past
        // so the first status update is never throttled
        let last_status_update = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .unwrap_or_else(Instant::now);

        Self {
            mode: AppMode::Running,
            logs: LogBuffer::new(config.log_capacity),
            status: StatusBar::default(),
            render_state: RenderState::default(),
            log_scroll: ScrollState::default(),
            config,
            last_status_update,
            log_rx,
            status_rx,
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

                // q - quit
                (KeyCode::Char('q'), KeyModifiers::NONE) => {
                    self.shutdown();
                }

                // Page Up/Down or k/j - scroll logs
                (KeyCode::PageUp, _) | (KeyCode::Char('k'), KeyModifiers::NONE) => {
                    self.log_scroll.scroll_up(10);
                    self.log_scroll.auto_scroll = false;
                    self.mode = AppMode::Scrolling;
                    self.render_state.logs_dirty = true;
                }
                (KeyCode::PageDown, _) | (KeyCode::Char('j'), KeyModifiers::NONE) => {
                    self.log_scroll.scroll_down(10);
                    if self.log_scroll.offset == 0 {
                        self.log_scroll.auto_scroll = true;
                        self.mode = AppMode::Running;
                    }
                    self.render_state.logs_dirty = true;
                }

                // Home/End or g/G - jump to top/bottom
                (KeyCode::Home, _) | (KeyCode::Char('g'), KeyModifiers::NONE) => {
                    // Jump to oldest logs (top)
                    self.log_scroll.offset = self.logs.len().saturating_sub(1);
                    self.log_scroll.auto_scroll = false;
                    self.mode = AppMode::Scrolling;
                    self.render_state.logs_dirty = true;
                }
                (KeyCode::End, _) | (KeyCode::Char('G'), KeyModifiers::SHIFT) => {
                    // Jump to newest logs (bottom)
                    self.log_scroll.scroll_to_bottom();
                    self.mode = AppMode::Running;
                    self.render_state.logs_dirty = true;
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
            .with_kiln_path(PathBuf::from("/kiln"))
            .with_doc_count(42);

        status.apply_update(update);

        assert_eq!(status.kiln_path, PathBuf::from("/kiln"));
        assert_eq!(status.doc_count, 42);
        assert_eq!(status.db_type, "Unknown"); // Unchanged
    }
}

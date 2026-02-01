//! TUI End-to-End Test Harness
//!
//! Uses expectrl for PTY-based testing of the TUI application.

// Allow unused items - these are test utilities meant for future use
#![allow(dead_code)]
//! This enables multi-turn interaction testing with real terminal emulation.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────┐
//! │  Test Case      │
//! │  (Rust test)    │
//! └────────┬────────┘
//!          │
//!          ▼
//! ┌─────────────────┐
//! │  TuiTestSession │──► Spawns `cru chat` in PTY
//! │  (expectrl)     │──► Sends keystrokes
//! │                 │──► Captures output
//! └────────┬────────┘
//!          │
//!          ▼
//! ┌─────────────────┐
//! │  Assertions     │──► Pattern matching
//! │                 │──► Snapshot comparison
//! └─────────────────┘
//! ```
//!
//! # Future: Granular Recording
//!
//! This harness is designed to support future enhancements:
//! - Timestamped output capture for flicker detection
//! - VTE parsing for escape sequence analysis
//! - Frame-by-frame diffing
//!
//! For now, it provides multi-turn verification with pattern matching.

use expectrl::{session::OsSession, spawn, Eof, Expect, Regex};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use vt100::Parser as Vt100Parser;

/// Configuration for TUI test sessions
#[derive(Debug, Clone)]
pub struct TuiTestConfig {
    /// Path to the binary (defaults to target/debug/cru or target/release/cru)
    pub binary_path: Option<PathBuf>,
    /// Subcommand to run (e.g., "chat")
    pub subcommand: String,
    /// Additional arguments
    pub args: Vec<String>,
    /// Environment variables to set
    pub env: Vec<(String, String)>,
    /// Timeout for expect operations
    pub timeout: Duration,
    /// Terminal dimensions
    pub cols: u16,
    pub rows: u16,
}

impl Default for TuiTestConfig {
    fn default() -> Self {
        Self {
            binary_path: None,
            subcommand: "chat".to_string(),
            args: vec![],
            env: vec![],
            timeout: Duration::from_secs(10),
            cols: 80,
            rows: 24,
        }
    }
}

impl TuiTestConfig {
    pub fn new(subcommand: &str) -> Self {
        Self {
            subcommand: subcommand.to_string(),
            ..Default::default()
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env.push((key.to_string(), value.to_string()));
        self
    }

    pub fn with_args(mut self, args: &[&str]) -> Self {
        self.args = args.iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn with_dimensions(mut self, cols: u16, rows: u16) -> Self {
        self.cols = cols;
        self.rows = rows;
        self
    }

    /// Find the cru binary in target directory
    fn find_binary(&self) -> PathBuf {
        if let Some(path) = &self.binary_path {
            return path.clone();
        }

        // Try release first, then debug
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let manifest_path = PathBuf::from(manifest_dir);
        let workspace_root = manifest_path
            .parent()
            .and_then(|p| p.parent())
            .expect("Could not find workspace root");

        let release_path = workspace_root.join("target/release/cru");
        if release_path.exists() {
            return release_path;
        }

        let debug_path = workspace_root.join("target/debug/cru");
        if debug_path.exists() {
            return debug_path;
        }

        panic!(
            "Could not find cru binary. Run `cargo build` or `cargo build --release` first.\n\
             Looked in:\n  - {}\n  - {}",
            release_path.display(),
            debug_path.display()
        );
    }
}

/// A test session wrapping an expectrl PTY session with vt100 terminal emulation.
///
/// The vt100 parser accumulates ALL output from the PTY, building a queryable
/// screen buffer. Use `screen()` to inspect parsed terminal state (cells,
/// colors, cursor position) instead of raw byte matching.
pub struct TuiTestSession {
    session: OsSession,
    config: TuiTestConfig,
    output_log: Vec<OutputChunk>,
    start_time: Instant,
    recording: bool,
    /// vt100 terminal emulator — accumulates all PTY output into a queryable screen buffer.
    vt_parser: Vt100Parser,
}

/// A timestamped chunk of output for granular analysis and replay
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OutputChunk {
    /// Milliseconds since session start
    pub timestamp_ms: u64,
    /// Raw terminal output bytes
    pub data: Vec<u8>,
}

/// Input event recorded for replay
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InputEvent {
    /// Milliseconds since session start
    pub timestamp_ms: u64,
    /// The input sent (text or escape sequence)
    pub input: String,
}

/// A complete recording of a PTY session
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PtyRecording {
    /// Session configuration
    pub command: String,
    pub cols: u16,
    pub rows: u16,
    pub env: Vec<(String, String)>,
    /// Total duration in milliseconds
    pub duration_ms: u64,
    /// Output chunks (what the terminal displayed)
    pub output: Vec<OutputChunk>,
    /// Input events (what was sent to the terminal)
    pub input: Vec<InputEvent>,
}

impl TuiTestSession {
    /// Spawn a new TUI test session
    pub fn spawn(config: TuiTestConfig) -> Result<Self, expectrl::Error> {
        let binary = config.find_binary();
        let mut cmd = format!("{} {}", binary.display(), config.subcommand);

        for arg in &config.args {
            cmd.push(' ');
            cmd.push_str(arg);
        }

        // Set environment variables
        for (key, value) in &config.env {
            std::env::set_var(key, value);
        }

        let session = spawn(&cmd)?;

        let vt_parser = Vt100Parser::new(config.rows, config.cols, 0);

        Ok(Self {
            session,
            config,
            output_log: Vec::new(),
            start_time: Instant::now(),
            recording: false,
            vt_parser,
        })
    }

    /// Spawn with default config for `cru chat`
    pub fn spawn_chat() -> Result<Self, expectrl::Error> {
        Self::spawn(TuiTestConfig::default())
    }

    /// Wait for a specific text pattern to appear
    pub fn expect(&mut self, pattern: &str) -> Result<(), expectrl::Error> {
        self.session.set_expect_timeout(Some(self.config.timeout));
        self.session.expect(pattern)?;
        Ok(())
    }

    /// Wait for a regex pattern to appear
    pub fn expect_regex(&mut self, pattern: &str) -> Result<(), expectrl::Error> {
        self.session.set_expect_timeout(Some(self.config.timeout));
        self.session.expect(Regex(pattern))?;
        Ok(())
    }

    /// Send a line of text (with Enter key)
    pub fn send_line(&mut self, text: &str) -> Result<(), expectrl::Error> {
        self.session.send_line(text)?;
        Ok(())
    }

    /// Send raw text without Enter
    pub fn send(&mut self, text: &str) -> Result<(), expectrl::Error> {
        self.session.send(text)?;
        Ok(())
    }

    /// Send a control character (e.g., Ctrl+C = '\x03')
    pub fn send_control(&mut self, c: char) -> Result<(), expectrl::Error> {
        let ctrl_char = (c as u8 - b'a' + 1) as char;
        self.session.send(ctrl_char.to_string())?;
        Ok(())
    }

    /// Send special keys
    pub fn send_key(&mut self, key: Key) -> Result<(), expectrl::Error> {
        self.session.send(key.as_escape_sequence())?;
        Ok(())
    }

    /// Wait for the process to exit
    pub fn expect_eof(&mut self) -> Result<(), expectrl::Error> {
        self.session.set_expect_timeout(Some(self.config.timeout));
        self.session.expect(Eof)?;
        Ok(())
    }

    /// Capture current screen content (best effort).
    /// Also feeds raw bytes through the vt100 parser for `screen()` queries.
    pub fn capture_screen(&mut self) -> Result<String, std::io::Error> {
        let mut buffer = [0u8; 65536];
        let n = self.session.try_read(&mut buffer)?;
        if n > 0 {
            self.vt_parser.process(&buffer[..n]);
        }
        Ok(String::from_utf8_lossy(&buffer[..n]).to_string())
    }

    /// Check if output contains a pattern (non-blocking)
    pub fn output_contains(&mut self, pattern: &str) -> Result<bool, expectrl::Error> {
        self.session
            .set_expect_timeout(Some(Duration::from_millis(100)));
        match self.session.expect(pattern) {
            Ok(_) => Ok(true),
            Err(expectrl::Error::ExpectTimeout) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Wait a fixed duration (for timing-sensitive tests)
    pub fn wait(&self, duration: Duration) {
        std::thread::sleep(duration);
    }

    /// Enable recording of terminal output
    pub fn start_recording(&mut self) {
        self.recording = true;
        self.output_log.clear();
        self.start_time = Instant::now();
    }

    /// Stop recording and return collected output
    pub fn stop_recording(&mut self) -> Vec<OutputChunk> {
        self.recording = false;
        std::mem::take(&mut self.output_log)
    }

    /// Capture and record current screen content
    pub fn capture_and_record(&mut self) -> Result<String, std::io::Error> {
        let mut buffer = [0u8; 65536];
        let n = self.session.try_read(&mut buffer)?;
        if n > 0 {
            self.vt_parser.process(&buffer[..n]);
        }
        let data = buffer[..n].to_vec();
        let text = String::from_utf8_lossy(&data).to_string();

        if self.recording && n > 0 {
            self.output_log.push(OutputChunk {
                timestamp_ms: self.start_time.elapsed().as_millis() as u64,
                data,
            });
        }

        Ok(text)
    }

    /// Get all recorded output chunks
    pub fn get_recording(&self) -> &[OutputChunk] {
        &self.output_log
    }

    /// Save recording to file in JSON format
    pub fn save_recording(&self, path: &Path) -> Result<(), std::io::Error> {
        let recording = PtyRecording {
            command: format!("{} {}", self.config.subcommand, self.config.args.join(" ")),
            cols: self.config.cols,
            rows: self.config.rows,
            env: self.config.env.clone(),
            duration_ms: self.start_time.elapsed().as_millis() as u64,
            output: self.output_log.clone(),
            input: Vec::new(), // TODO: track input events too
        };

        let json = serde_json::to_string_pretty(&recording).map_err(std::io::Error::other)?;

        std::fs::write(path, json)
    }

    /// Load a recording from file
    pub fn load_recording(path: &Path) -> Result<PtyRecording, std::io::Error> {
        let json = std::fs::read_to_string(path)?;
        serde_json::from_str(&json).map_err(std::io::Error::other)
    }

    /// Concatenate all recorded output into a single string
    pub fn recorded_output_as_string(&self) -> String {
        self.output_log
            .iter()
            .map(|chunk| String::from_utf8_lossy(&chunk.data))
            .collect::<Vec<_>>()
            .join("")
    }

    // =========================================================================
    // vt100 Screen Access
    // =========================================================================

    /// Drain any available PTY output into the vt100 parser without blocking.
    pub fn refresh_screen(&mut self) {
        let mut buffer = [0u8; 65536];
        loop {
            match self.session.try_read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    self.vt_parser.process(&buffer[..n]);
                    if self.recording {
                        self.output_log.push(OutputChunk {
                            timestamp_ms: self.start_time.elapsed().as_millis() as u64,
                            data: buffer[..n].to_vec(),
                        });
                    }
                }
                Err(_) => break,
            }
        }
    }

    /// Get the current vt100 screen state.
    ///
    /// Call `refresh_screen()` first if you need the latest output after a
    /// `wait()` or `expect()` call that may not have fed the parser.
    pub fn screen(&self) -> &vt100::Screen {
        self.vt_parser.screen()
    }

    /// Get the full text contents of the parsed screen (no ANSI codes).
    pub fn screen_contents(&self) -> String {
        self.vt_parser.screen().contents()
    }

    /// Poll until a predicate on the screen becomes true, or timeout.
    ///
    /// Drains PTY output every `poll_interval` and checks the predicate.
    /// Returns `Ok(())` on success or `Err` with screen contents on timeout.
    pub fn wait_until<F>(&mut self, predicate: F, timeout: Duration) -> Result<(), String>
    where
        F: Fn(&vt100::Screen) -> bool,
    {
        let poll_interval = Duration::from_millis(80);
        let start = Instant::now();
        loop {
            self.refresh_screen();
            if predicate(self.screen()) {
                return Ok(());
            }
            if start.elapsed() >= timeout {
                return Err(format!(
                    "wait_until timed out after {:?}.\nScreen contents:\n{}",
                    timeout,
                    self.screen_contents()
                ));
            }
            std::thread::sleep(poll_interval);
        }
    }

    /// Poll until the screen contains the given text, or timeout.
    pub fn wait_for_text(&mut self, text: &str, timeout: Duration) -> Result<(), String> {
        let owned = text.to_string();
        self.wait_until(move |s| s.contents().contains(&owned), timeout)
    }

    /// Get the underlying session for advanced operations
    pub fn inner(&mut self) -> &mut OsSession {
        &mut self.session
    }
}

/// Special keys for TUI interaction
#[derive(Debug, Clone, Copy)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
}

impl Key {
    fn as_escape_sequence(&self) -> &'static str {
        match self {
            Key::Up => "\x1b[A",
            Key::Down => "\x1b[B",
            Key::Right => "\x1b[C",
            Key::Left => "\x1b[D",
            Key::Enter => "\r",
            Key::Escape => "\x1b",
            Key::Tab => "\t",
            Key::Backspace => "\x7f",
            Key::Delete => "\x1b[3~",
            Key::Home => "\x1b[H",
            Key::End => "\x1b[F",
            Key::PageUp => "\x1b[5~",
            Key::PageDown => "\x1b[6~",
            Key::F(1) => "\x1bOP",
            Key::F(2) => "\x1bOQ",
            Key::F(3) => "\x1bOR",
            Key::F(4) => "\x1bOS",
            Key::F(5) => "\x1b[15~",
            Key::F(6) => "\x1b[17~",
            Key::F(7) => "\x1b[18~",
            Key::F(8) => "\x1b[19~",
            Key::F(9) => "\x1b[20~",
            Key::F(10) => "\x1b[21~",
            Key::F(11) => "\x1b[23~",
            Key::F(12) => "\x1b[24~",
            Key::F(_) => "\x1b",
        }
    }
}

// =============================================================================
// Test Utilities
// =============================================================================

/// Builder for fluent test assertions
pub struct TuiTestBuilder {
    config: TuiTestConfig,
}

impl TuiTestBuilder {
    pub fn new() -> Self {
        Self {
            config: TuiTestConfig::default(),
        }
    }

    pub fn command(mut self, cmd: &str) -> Self {
        self.config.subcommand = cmd.to_string();
        self
    }

    pub fn timeout(mut self, secs: u64) -> Self {
        self.config.timeout = Duration::from_secs(secs);
        self
    }

    pub fn env(mut self, key: &str, value: &str) -> Self {
        self.config.env.push((key.to_string(), value.to_string()));
        self
    }

    pub fn spawn(self) -> Result<TuiTestSession, expectrl::Error> {
        TuiTestSession::spawn(self.config)
    }
}

impl Default for TuiTestBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// vt100 Screen Assertion Helpers
// =============================================================================

/// Assert that the screen contains the given text anywhere.
pub fn assert_screen_contains(screen: &vt100::Screen, text: &str) {
    let contents = screen.contents();
    assert!(
        contents.contains(text),
        "Expected screen to contain {:?}, but it was not found.\nScreen contents:\n{}",
        text,
        contents,
    );
}

/// Assert that the screen does NOT contain the given text.
pub fn assert_screen_not_contains(screen: &vt100::Screen, text: &str) {
    let contents = screen.contents();
    assert!(
        !contents.contains(text),
        "Expected screen to NOT contain {:?}, but it was found.\nScreen contents:\n{}",
        text,
        contents,
    );
}

/// Assert that a specific row contains the given text.
pub fn assert_row_contains(screen: &vt100::Screen, row: u16, text: &str) {
    let contents = screen.contents();
    let line = contents.lines().nth(row as usize).unwrap_or("");
    assert!(
        line.contains(text),
        "Expected row {} to contain {:?}, but got {:?}.\nFull screen:\n{}",
        row,
        text,
        line,
        contents,
    );
}

/// Assert that a rectangular region of the screen contains the given text.
pub fn assert_region_contains(
    screen: &vt100::Screen,
    top: u16,
    left: u16,
    bottom: u16,
    right: u16,
    text: &str,
) {
    let mut region = String::new();
    for row in top..=bottom {
        for col in left..=right {
            if let Some(cell) = screen.cell(row, col) {
                region.push_str(&cell.contents());
            }
        }
        if row < bottom {
            region.push('\n');
        }
    }
    assert!(
        region.contains(text),
        "Expected region ({},{})..({},{}) to contain {:?}, but got {:?}.\nFull screen:\n{}",
        top,
        left,
        bottom,
        right,
        text,
        region,
        screen.contents(),
    );
}

/// Assert the cursor is at the given position.
pub fn assert_cursor_at(screen: &vt100::Screen, row: u16, col: u16) {
    let (actual_row, actual_col) = screen.cursor_position();
    assert_eq!(
        (actual_row, actual_col),
        (row, col),
        "Expected cursor at ({}, {}), but it was at ({}, {}).\nScreen contents:\n{}",
        row,
        col,
        actual_row,
        actual_col,
        screen.contents(),
    );
}

/// Assert that a cell has the bold attribute set.
pub fn assert_cell_bold(screen: &vt100::Screen, row: u16, col: u16) {
    let cell = screen
        .cell(row, col)
        .unwrap_or_else(|| panic!("No cell at ({}, {})", row, col));
    assert!(
        cell.bold(),
        "Expected cell ({}, {}) to be bold, but it was not. Contents: {:?}",
        row,
        col,
        cell.contents(),
    );
}

// =============================================================================
// Example Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_binary() {
        let config = TuiTestConfig::default();
        let binary = config.find_binary();
        assert!(binary.exists(), "Binary should exist at {:?}", binary);
    }

    #[test]
    #[ignore = "requires built binary"]
    fn test_spawn_help() {
        let mut session = TuiTestBuilder::new()
            .command("--help")
            .timeout(5)
            .spawn()
            .expect("Failed to spawn");

        session.expect("Usage").expect("Should see usage");
        session.expect_eof().expect("Should exit cleanly");
    }

    // =========================================================================
    // vt100 assertion helper tests
    // =========================================================================

    fn make_screen(input: &[u8]) -> vt100::Screen {
        let mut parser = Vt100Parser::new(24, 80, 0);
        parser.process(input);
        parser.screen().clone()
    }

    #[test]
    fn vt100_assert_screen_contains_finds_text() {
        let screen = make_screen(b"Hello World");
        assert_screen_contains(&screen, "Hello");
        assert_screen_contains(&screen, "World");
    }

    #[test]
    #[should_panic(expected = "Expected screen to contain")]
    fn vt100_assert_screen_contains_panics_on_missing() {
        let screen = make_screen(b"Hello");
        assert_screen_contains(&screen, "Goodbye");
    }

    #[test]
    fn vt100_assert_screen_not_contains_passes_on_absent() {
        let screen = make_screen(b"Hello");
        assert_screen_not_contains(&screen, "Goodbye");
    }

    #[test]
    #[should_panic(expected = "Expected screen to NOT contain")]
    fn vt100_assert_screen_not_contains_panics_on_present() {
        let screen = make_screen(b"Hello World");
        assert_screen_not_contains(&screen, "Hello");
    }

    #[test]
    fn vt100_assert_row_contains_finds_text_on_row() {
        let screen = make_screen(b"Line Zero\r\nLine One\r\nLine Two");
        assert_row_contains(&screen, 0, "Zero");
        assert_row_contains(&screen, 1, "One");
        assert_row_contains(&screen, 2, "Two");
    }

    #[test]
    #[should_panic(expected = "Expected row 0 to contain")]
    fn vt100_assert_row_contains_panics_on_wrong_row() {
        let screen = make_screen(b"AAA\r\nBBB");
        assert_row_contains(&screen, 0, "BBB");
    }

    #[test]
    fn vt100_assert_region_contains_finds_text() {
        let screen = make_screen(b"ABCDE\r\nFGHIJ\r\nKLMNO");
        assert_region_contains(&screen, 1, 1, 1, 3, "GHI");
    }

    #[test]
    fn vt100_assert_cursor_at_correct_position() {
        let screen = make_screen(b"AB");
        assert_cursor_at(&screen, 0, 2);
    }

    #[test]
    #[should_panic(expected = "Expected cursor at")]
    fn vt100_assert_cursor_at_panics_on_wrong_position() {
        let screen = make_screen(b"AB");
        assert_cursor_at(&screen, 0, 0);
    }

    #[test]
    fn vt100_assert_cell_bold_with_bold_text() {
        let screen = make_screen(b"\x1b[1mBold\x1b[m");
        assert_cell_bold(&screen, 0, 0);
    }

    #[test]
    #[should_panic(expected = "Expected cell (0, 0) to be bold")]
    fn vt100_assert_cell_bold_panics_on_non_bold() {
        let screen = make_screen(b"Normal");
        assert_cell_bold(&screen, 0, 0);
    }

    #[test]
    fn vt100_screen_parses_ansi_colors() {
        let screen = make_screen(b"normal \x1b[31mRED\x1b[m normal");
        assert_screen_contains(&screen, "RED");
        let cell = screen.cell(0, 7).unwrap();
        assert_eq!(cell.fgcolor(), vt100::Color::Idx(1));
    }
}

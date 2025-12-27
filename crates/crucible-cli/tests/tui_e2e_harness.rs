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
use std::path::PathBuf;
use std::time::Duration;

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

/// A test session wrapping an expectrl PTY session
pub struct TuiTestSession {
    session: OsSession,
    config: TuiTestConfig,
    /// Captured output chunks with timestamps (for future flicker detection)
    #[allow(dead_code)]
    output_log: Vec<OutputChunk>,
}

/// A timestamped chunk of output (for future granular analysis)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct OutputChunk {
    pub timestamp_ms: u64,
    pub data: Vec<u8>,
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

        Ok(Self {
            session,
            config,
            output_log: Vec::new(),
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

    /// Capture current screen content (best effort)
    pub fn capture_screen(&mut self) -> Result<String, std::io::Error> {
        let mut buffer = [0u8; 65536];
        // Read available output without blocking
        let n = self.session.try_read(&mut buffer)?;
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
// Example Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that the binary can be found
    #[test]
    fn test_find_binary() {
        let config = TuiTestConfig::default();
        let binary = config.find_binary();
        assert!(binary.exists(), "Binary should exist at {:?}", binary);
    }

    /// Test basic session spawning with --help (doesn't require full TUI)
    #[test]
    #[ignore = "requires built binary"]
    fn test_spawn_help() {
        let mut session = TuiTestBuilder::new()
            .command("--help")
            .timeout(5)
            .spawn()
            .expect("Failed to spawn");

        // Should see usage info
        session.expect("Usage").expect("Should see usage");
        session.expect_eof().expect("Should exit cleanly");
    }
}

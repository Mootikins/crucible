//! Input mode subsystem manager
//!
//! Manages rapid input mode, paste detection, and special input modes.

use std::time::Instant;

/// Manages special input modes
#[derive(Debug, Default)]
pub struct InputModeManager {
    /// Buffer for rapid input (sequential chars without delay)
    pub rapid_input_buffer: String,
    /// Whether currently in rapid input mode
    pub in_rapid_input: bool,
    /// Timestamp of last key input (for rapid input detection)
    pub last_key_time: Option<Instant>,
}

impl InputModeManager {
    pub fn new() -> Self {
        Self {
            rapid_input_buffer: String::new(),
            in_rapid_input: false,
            last_key_time: None,
        }
    }

    /// Start rapid input mode
    pub fn start_rapid_input(&mut self) {
        self.in_rapid_input = true;
        self.rapid_input_buffer.clear();
    }

    /// End rapid input mode
    pub fn end_rapid_input(&mut self) {
        self.in_rapid_input = false;
        self.rapid_input_buffer.clear();
    }

    /// Add character to rapid input buffer
    pub fn push_char(&mut self, c: char) {
        self.rapid_input_buffer.push(c);
    }

    /// Get rapid input buffer content
    pub fn rapid_buffer(&self) -> &str {
        &self.rapid_input_buffer
    }

    /// Clear rapid input buffer
    pub fn clear_rapid_buffer(&mut self) {
        self.rapid_input_buffer.clear();
    }

    /// Check if in rapid input mode
    pub fn is_rapid_input(&self) -> bool {
        self.in_rapid_input
    }

    // =========================================================================
    // Last key time management
    // =========================================================================

    /// Set last key time
    pub fn set_last_key_time(&mut self, time: Instant) {
        self.last_key_time = Some(time);
    }

    /// Get last key time
    pub fn last_key_time(&self) -> Option<Instant> {
        self.last_key_time
    }

    /// Clear last key time
    pub fn clear_last_key_time(&mut self) {
        self.last_key_time = None;
    }
}

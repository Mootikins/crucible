//! RecordingMode, SessionType, and SessionState enums.

use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Recording granularity for session events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingMode {
    /// Coarse-grained recording (default): only major events
    Coarse,
    /// Granular recording: all events including keystroke-level details
    Granular,
}

impl FromStr for RecordingMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "coarse" => Ok(RecordingMode::Coarse),
            "granular" => Ok(RecordingMode::Granular),
            _ => Err(format!("Invalid recording mode: {}", s)),
        }
    }
}

impl std::fmt::Display for RecordingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecordingMode::Coarse => write!(f, "coarse"),
            RecordingMode::Granular => write!(f, "granular"),
        }
    }
}

/// Type of session, determines logging format and behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionType {
    /// User/assistant conversation (interactive chat)
    Chat,
    /// Autonomous agent actions (may run without user input)
    Agent,
    /// Programmatic workflow execution
    Workflow,
}

impl SessionType {
    /// Get the string prefix used in session IDs.
    pub fn as_prefix(&self) -> &'static str {
        match self {
            SessionType::Chat => "chat",
            SessionType::Agent => "agent",
            SessionType::Workflow => "workflow",
        }
    }
}

impl std::fmt::Display for SessionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_prefix())
    }
}

/// Current state of a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// Session is actively processing
    #[default]
    Active,
    /// Session is paused (not processing new events)
    Paused,
    /// Session is compacting old context
    Compacting,
    /// Session has ended
    Ended,
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionState::Active => write!(f, "active"),
            SessionState::Paused => write!(f, "paused"),
            SessionState::Compacting => write!(f, "compacting"),
            SessionState::Ended => write!(f, "ended"),
        }
    }
}

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

impl FromStr for SessionType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "chat" => Ok(SessionType::Chat),
            "agent" => Ok(SessionType::Agent),
            "workflow" => Ok(SessionType::Workflow),
            other => Err(format!("unknown session type: {other}")),
        }
    }
}

/// Why a session ended.
///
/// Surfaced to Lua via `session.end_reason` inside `crucible.on_session_end` handlers.
/// The string repr is the lowercase variant name (e.g. `"user"`, `"shutdown"`).
///
/// Variants:
/// - `User` — user explicitly ended the session (e.g. `session.end` RPC, `/quit`).
/// - `Error` — unrecoverable error terminated the session.
/// - `Timeout` — idle/execution timeout fired.
/// - `Shutdown` — daemon/process shutdown tore the Lua state down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndReason {
    User,
    Error,
    Timeout,
    Shutdown,
}

impl std::fmt::Display for EndReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EndReason::User => write!(f, "user"),
            EndReason::Error => write!(f, "error"),
            EndReason::Timeout => write!(f, "timeout"),
            EndReason::Shutdown => write!(f, "shutdown"),
        }
    }
}

impl FromStr for EndReason {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user" => Ok(EndReason::User),
            "error" => Ok(EndReason::Error),
            "timeout" => Ok(EndReason::Timeout),
            "shutdown" => Ok(EndReason::Shutdown),
            other => Err(format!("unknown end reason: {other}")),
        }
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

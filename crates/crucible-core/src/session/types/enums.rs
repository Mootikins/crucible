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
    /// A session a plugin started for its own work: a reflection review, a
    /// consolidation pass. Never a user's conversation. Reflection does not
    /// run on it, and the consolidation sample leaves it out.
    Plugin,
}

impl SessionType {
    /// Get the string prefix used in session IDs.
    pub fn as_prefix(&self) -> &'static str {
        match self {
            SessionType::Chat => "chat",
            SessionType::Agent => "agent",
            SessionType::Workflow => "workflow",
            SessionType::Plugin => "plugin",
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
            "plugin" => Ok(SessionType::Plugin),
            other => Err(format!("unknown session type: {other}")),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A plugin session has one spelling: the id prefix, the `FromStr`
    /// input, the `Display` output and the serde form all agree.
    #[test]
    fn session_type_plugin_has_one_spelling_everywhere() {
        assert_eq!("plugin".parse::<SessionType>(), Ok(SessionType::Plugin));
        assert_eq!(SessionType::Plugin.as_prefix(), "plugin");
        assert_eq!(SessionType::Plugin.to_string(), "plugin");

        let json = serde_json::to_string(&SessionType::Plugin).unwrap();
        assert_eq!(json, "\"plugin\"");
        let back: SessionType = serde_json::from_str(&json).unwrap();
        assert_eq!(back, SessionType::Plugin);
    }
}

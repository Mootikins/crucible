//! Session ID generation
//!
//! Format: `{type}-{YYYYMMDD}-{HHMM}-{4-char-hash}`
//! Example: `chat-20260104-1530-a1b2`

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

/// Session type discriminant
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionType {
    /// Interactive chat session
    Chat,
    /// Workflow execution session
    Workflow,
    /// MCP server session
    Mcp,
    /// Background subagent session (child of a parent session)
    Subagent,
}

impl fmt::Display for SessionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionType::Chat => write!(f, "chat"),
            SessionType::Workflow => write!(f, "workflow"),
            SessionType::Mcp => write!(f, "mcp"),
            SessionType::Subagent => write!(f, "sub"),
        }
    }
}

impl std::str::FromStr for SessionType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "chat" => Ok(SessionType::Chat),
            "workflow" => Ok(SessionType::Workflow),
            "mcp" => Ok(SessionType::Mcp),
            "sub" => Ok(SessionType::Subagent),
            other => Err(format!("unknown session type: {other}")),
        }
    }
}

/// A unique session identifier
///
/// Validated on construction - both `parse()` and serde deserialization
/// ensure the ID matches the expected format.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct SessionId(String);

// Custom Deserialize to validate format (prevents bypass of parse() validation)
impl<'de> Deserialize<'de> for SessionId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        SessionId::parse(&s).map_err(serde::de::Error::custom)
    }
}

impl SessionId {
    /// Generate a new session ID for the given type and timestamp
    pub fn new(session_type: SessionType, timestamp: DateTime<Utc>) -> Self {
        let date = timestamp.format("%Y%m%d");
        let time = timestamp.format("%H%M");

        // Generate unique hash from timestamp nanos + random
        let nanos = timestamp.timestamp_nanos_opt().unwrap_or(0);
        let random: u64 = rand::random();
        let input = format!("{nanos}{random}");
        let hash = blake3::hash(input.as_bytes());
        let hash_prefix = &hash.to_hex()[..4];

        Self(format!("{session_type}-{date}-{time}-{hash_prefix}"))
    }

    /// Generate a new chat session ID for the current time
    pub fn new_chat() -> Self {
        Self::new(SessionType::Chat, Utc::now())
    }

    /// Parse a session ID from string
    pub fn parse(s: &str) -> Result<Self, SessionIdError> {
        // Validate format: type-YYYYMMDD-HHMM-hash
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() != 4 {
            return Err(SessionIdError::InvalidFormat(s.to_string()));
        }

        // Validate type
        let _session_type: SessionType = parts[0]
            .parse()
            .map_err(|_| SessionIdError::InvalidType(parts[0].to_string()))?;

        // Validate date (YYYYMMDD)
        if parts[1].len() != 8 || !parts[1].chars().all(|c| c.is_ascii_digit()) {
            return Err(SessionIdError::InvalidDate(parts[1].to_string()));
        }

        // Validate time (HHMM)
        if parts[2].len() != 4 || !parts[2].chars().all(|c| c.is_ascii_digit()) {
            return Err(SessionIdError::InvalidTime(parts[2].to_string()));
        }

        // Validate hash (4 hex chars)
        if parts[3].len() != 4 || !parts[3].chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(SessionIdError::InvalidHash(parts[3].to_string()));
        }

        Ok(Self(s.to_string()))
    }

    /// Get the session type
    pub fn session_type(&self) -> SessionType {
        self.0
            .split('-')
            .next()
            .and_then(|s| s.parse().ok())
            .expect("SessionId should always have valid type")
    }

    /// Get the underlying string
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for SessionId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Errors that can occur when parsing a session ID
#[derive(Debug, thiserror::Error)]
pub enum SessionIdError {
    #[error("invalid session ID format: {0}")]
    InvalidFormat(String),
    #[error("invalid session type: {0}")]
    InvalidType(String),
    #[error("invalid date in session ID: {0}")]
    InvalidDate(String),
    #[error("invalid time in session ID: {0}")]
    InvalidTime(String),
    #[error("invalid hash in session ID: {0}")]
    InvalidHash(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_id_format() {
        let ts = DateTime::parse_from_rfc3339("2026-01-04T15:30:00Z")
            .unwrap()
            .to_utc();
        let id = SessionId::new(SessionType::Chat, ts);

        // Should start with chat-20260104-1530-
        assert!(id.as_str().starts_with("chat-20260104-1530-"));
        // Should have 4 char hash suffix
        let hash = id.as_str().split('-').last().unwrap();
        assert_eq!(hash.len(), 4);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_session_id_uniqueness() {
        let ts = Utc::now();
        let id1 = SessionId::new(SessionType::Chat, ts);
        let id2 = SessionId::new(SessionType::Chat, ts);
        // Different random component should yield different IDs
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_session_id_parse_valid() {
        let id = SessionId::parse("chat-20260104-1530-a1b2").unwrap();
        assert_eq!(id.session_type(), SessionType::Chat);
    }

    #[test]
    fn test_session_id_parse_workflow() {
        let id = SessionId::parse("workflow-20260104-1530-beef").unwrap();
        assert_eq!(id.session_type(), SessionType::Workflow);
    }

    #[test]
    fn test_session_id_parse_invalid_format() {
        assert!(SessionId::parse("invalid").is_err());
        assert!(SessionId::parse("chat-20260104").is_err());
        assert!(SessionId::parse("chat-20260104-1530").is_err());
    }

    #[test]
    fn test_session_id_parse_invalid_type() {
        assert!(SessionId::parse("unknown-20260104-1530-a1b2").is_err());
    }

    #[test]
    fn test_session_id_parse_invalid_date() {
        assert!(SessionId::parse("chat-2026010-1530-a1b2").is_err()); // 7 digits
        assert!(SessionId::parse("chat-abcdefgh-1530-a1b2").is_err()); // non-digits
    }

    #[test]
    fn test_session_id_parse_invalid_time() {
        assert!(SessionId::parse("chat-20260104-153-a1b2").is_err()); // 3 digits
        assert!(SessionId::parse("chat-20260104-abcd-a1b2").is_err()); // non-digits
    }

    #[test]
    fn test_session_id_parse_invalid_hash() {
        assert!(SessionId::parse("chat-20260104-1530-a1").is_err()); // 2 chars
        assert!(SessionId::parse("chat-20260104-1530-ghij").is_err()); // non-hex
    }

    #[test]
    fn test_session_type_display() {
        assert_eq!(SessionType::Chat.to_string(), "chat");
        assert_eq!(SessionType::Workflow.to_string(), "workflow");
        assert_eq!(SessionType::Mcp.to_string(), "mcp");
    }

    #[test]
    fn test_session_type_parse() {
        assert_eq!("chat".parse::<SessionType>().unwrap(), SessionType::Chat);
        assert_eq!(
            "workflow".parse::<SessionType>().unwrap(),
            SessionType::Workflow
        );
        assert_eq!("mcp".parse::<SessionType>().unwrap(), SessionType::Mcp);
        assert!("unknown".parse::<SessionType>().is_err());
    }

    #[test]
    fn test_session_id_serde() {
        let id = SessionId::parse("chat-20260104-1530-a1b2").unwrap();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"chat-20260104-1530-a1b2\"");

        let parsed: SessionId = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, id);
    }

    #[test]
    fn test_session_id_serde_rejects_invalid() {
        // Invalid IDs should fail deserialization (validates via parse())
        let result: Result<SessionId, _> = serde_json::from_str("\"invalid\"");
        assert!(result.is_err());

        let result: Result<SessionId, _> = serde_json::from_str("\"../../../etc/passwd\"");
        assert!(result.is_err());

        let result: Result<SessionId, _> = serde_json::from_str("\"unknown-20260104-1530-a1b2\"");
        assert!(result.is_err());
    }
}

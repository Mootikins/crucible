//! Event to Markdown conversion for session persistence.
//!
//! This module provides conversion of `SessionEvent` to markdown blocks
//! for persistence to kiln files. Each event becomes a human-readable
//! markdown section with consistent formatting.
//!
//! ## Format
//!
//! Events are rendered as markdown with:
//! - H2 header with ISO timestamp and event type
//! - Structured content based on event variant
//! - Horizontal rule separator
//!
//! ## Example Output
//!
//! ```markdown
//! ## 2025-12-14T15:30:45.123 - MessageReceived
//!
//! **Participant:** user
//!
//! > Help me implement the task harness
//!
//! ---
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crate::events::SessionEvent;
//!
//! let event = SessionEvent::MessageReceived {
//!     content: "Hello!".into(),
//!     participant_id: "user".into(),
//! };
//!
//! let markdown = event.to_markdown_block(Some(timestamp_ms));
//! ```

mod format;
mod parse;

#[cfg(test)]
mod tests;

use crate::events::{InternalSessionEvent, SessionEvent};

impl SessionEvent {
    pub fn event_type_name(&self) -> &'static str {
        match self {
            SessionEvent::MessageReceived { .. } => "MessageReceived",
            SessionEvent::AgentResponded { .. } => "AgentResponded",
            SessionEvent::AgentThinking { .. } => "AgentThinking",
            SessionEvent::ToolCalled { .. } => "ToolCalled",
            SessionEvent::ToolCompleted { .. } => "ToolCompleted",
            SessionEvent::SessionStarted { .. } => "SessionStarted",
            SessionEvent::SessionEnded { .. } => "SessionEnded",
            SessionEvent::TextDelta { .. } => "TextDelta",
            SessionEvent::InteractionRequested { .. } => "InteractionRequested",
            SessionEvent::InteractionCompleted { .. } => "InteractionCompleted",
            SessionEvent::DelegationSpawned { .. } => "DelegationSpawned",
            SessionEvent::DelegationCompleted { .. } => "DelegationCompleted",
            SessionEvent::DelegationFailed { .. } => "DelegationFailed",
            SessionEvent::Custom { .. } => "Custom",
            SessionEvent::Internal(inner) => inner.type_name(),
        }
    }

    pub fn to_markdown_block(&self, timestamp_ms: Option<u64>) -> String {
        let timestamp = format::format_timestamp(timestamp_ms);
        let event_type = self.event_type_name();
        let header = format!("## {} - {}\n\n", timestamp, event_type);

        let body = match self {
            SessionEvent::MessageReceived {
                content,
                participant_id,
            } => format::format_message_received(participant_id, content),

            SessionEvent::AgentResponded {
                content,
                tool_calls,
            } => format::format_agent_responded(content, tool_calls),

            SessionEvent::AgentThinking { thought } => format::format_agent_thinking(thought),

            SessionEvent::ToolCalled { name, args, .. } => format::format_tool_called(name, args),

            SessionEvent::ToolCompleted {
                name,
                result,
                error,
            } => format::format_tool_completed(name, result, error.as_deref()),

            SessionEvent::SessionStarted { config } => {
                let folder = config.folder.as_deref().unwrap_or(std::path::Path::new(""));
                format::format_session_started(&config.session_id, folder)
            }

            SessionEvent::SessionEnded { reason } => format::format_session_ended(reason),

            SessionEvent::TextDelta { delta, seq } => {
                format!("**Seq:** {}\n\n```\n{}\n```\n", seq, delta)
            }

            SessionEvent::InteractionRequested {
                request_id,
                request,
            } => {
                format!(
                    "**Request ID:** {}\n**Kind:** {}\n",
                    request_id,
                    request.kind()
                )
            }
            SessionEvent::InteractionCompleted {
                request_id,
                response,
            } => {
                let response_summary = match response {
                    crate::InteractionResponse::Ask(_) => "Ask response",
                    crate::InteractionResponse::AskBatch(_) => "AskBatch response",
                    crate::InteractionResponse::Edit(_) => "Edit response",
                    crate::InteractionResponse::Permission(_) => "Permission response",
                    crate::InteractionResponse::Popup(_) => "Popup response",
                    crate::InteractionResponse::Panel(_) => "Panel response",
                    crate::InteractionResponse::Cancelled => "Cancelled",
                };
                format!(
                    "**Request ID:** {}\n**Response:** {}\n",
                    request_id, response_summary
                )
            }

            SessionEvent::DelegationSpawned {
                delegation_id,
                prompt,
                parent_session_id,
                ..
            } => format::format_delegation_spawned(delegation_id, prompt, parent_session_id),

            SessionEvent::DelegationCompleted {
                delegation_id,
                result_summary,
                parent_session_id,
            } => format::format_delegation_completed(
                delegation_id,
                result_summary,
                parent_session_id,
            ),

            SessionEvent::DelegationFailed {
                delegation_id,
                error,
                parent_session_id,
            } => format::format_delegation_failed(delegation_id, error, parent_session_id),

            SessionEvent::Custom { name, payload } => format::format_custom_event(name, payload),

            SessionEvent::Internal(inner) => match inner.as_ref() {
                InternalSessionEvent::BashTaskSpawned { id, command } => {
                    format!(
                        "**Task ID:** `{}`\n**Command:**\n```\n{}\n```\n",
                        id, command
                    )
                }
                InternalSessionEvent::BashTaskCompleted {
                    id,
                    output,
                    exit_code,
                } => {
                    format!(
                        "**Task ID:** `{}`\n**Exit Code:** {}\n**Output:**\n```\n{}\n```\n",
                        id, exit_code, output
                    )
                }
                InternalSessionEvent::BashTaskFailed {
                    id,
                    error,
                    exit_code,
                } => {
                    let exit_code_str = match exit_code {
                        Some(code) => code.to_string(),
                        None => "none".to_string(),
                    };
                    format!(
                        "**Task ID:** `{}`\n**Exit Code:** {}\n**Error:** {}\n",
                        id, exit_code_str, error
                    )
                }
                InternalSessionEvent::BackgroundTaskCompleted { id, kind, summary } => {
                    format!(
                        "**Task ID:** `{}`\n**Kind:** {}\n**Summary:**\n> {}\n",
                        id, kind, summary
                    )
                }
                InternalSessionEvent::SubagentSpawned { id, prompt } => {
                    format!("**Subagent ID:** `{}`\n**Prompt:**\n> {}\n", id, prompt)
                }
                InternalSessionEvent::SubagentCompleted { id, result } => {
                    format!("**Subagent ID:** `{}`\n**Result:**\n> {}\n", id, result)
                }
                InternalSessionEvent::SubagentFailed { id, error } => {
                    format!("**Subagent ID:** `{}`\n**Error:** {}\n", id, error)
                }
                InternalSessionEvent::SessionCompacted { summary, new_file } => {
                    format!(
                        "**New File:** `{}`\n**Summary:**\n{}\n",
                        new_file.display(),
                        summary
                    )
                }
                _ => {
                    format!(
                        "**Internal Event:** {}\n{}\n",
                        inner.type_name(),
                        inner.summary(200)
                    )
                }
            },
        };

        format!("{}{}\n---\n", header, body)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Markdown to Event Parsing
// ─────────────────────────────────────────────────────────────────────────────

/// Errors that can occur during markdown parsing.
#[derive(Debug, Clone, thiserror::Error)]
pub enum MarkdownParseError {
    /// Missing or invalid header.
    #[error("Invalid header: {0}")]
    InvalidHeader(String),

    /// Unknown event type.
    #[error("Unknown event type: {0}")]
    UnknownEventType(String),

    /// Missing required field.
    #[error("Missing required field: {0}")]
    MissingField(String),

    /// Invalid field value.
    #[error("Invalid field value for {field}: {message}")]
    InvalidFieldValue { field: String, message: String },

    /// JSON parsing error.
    #[error("JSON parse error: {0}")]
    JsonParse(String),

    /// Invalid timestamp format.
    #[error("Invalid timestamp: {0}")]
    InvalidTimestamp(String),
}

/// Result type for markdown parsing operations.
pub type MarkdownParseResult<T> = Result<T, MarkdownParseError>;

impl SessionEvent {
    pub fn from_markdown_block(markdown: &str) -> MarkdownParseResult<(Self, u64)> {
        let lines: Vec<&str> = markdown.lines().collect();

        if lines.is_empty() {
            return Err(MarkdownParseError::InvalidHeader(
                "Empty markdown block".to_string(),
            ));
        }

        // Parse header (first line)
        let header = parse::parse_header(lines[0])?;

        // Get body (everything between header and trailing ---)
        let body_lines: Vec<&str> = lines[1..]
            .iter()
            .take_while(|line| line.trim() != "---")
            .copied()
            .collect();
        let body = body_lines.join("\n");

        // Parse based on event type
        let event = match header.event_type.as_str() {
            "MessageReceived" => parse::parse_message_received(&body)?,
            "AgentResponded" => parse::parse_agent_responded(&body)?,
            "AgentThinking" => parse::parse_agent_thinking(&body)?,
            "ToolCalled" => parse::parse_tool_called(&body)?,
            "ToolCompleted" => parse::parse_tool_completed(&body)?,
            "SessionStarted" => parse::parse_session_started(&body)?,
            "SessionCompacted" => parse::parse_session_compacted(&body)?,
            "SessionEnded" => parse::parse_session_ended(&body)?,
            "SubagentSpawned" => parse::parse_subagent_spawned(&body)?,
            "SubagentCompleted" => parse::parse_subagent_completed(&body)?,
            "SubagentFailed" => parse::parse_subagent_failed(&body)?,
            "BashTaskSpawned" => parse::parse_bash_task_spawned(&body)?,
            "BashTaskCompleted" => parse::parse_bash_task_completed(&body)?,
            "BashTaskFailed" => parse::parse_bash_task_failed(&body)?,
            "BackgroundTaskCompleted" => parse::parse_background_task_completed(&body)?,
            "Custom" => parse::parse_custom_event(&body)?,
            other => return Err(MarkdownParseError::UnknownEventType(other.to_string())),
        };

        Ok((event, header.timestamp_ms))
    }
}

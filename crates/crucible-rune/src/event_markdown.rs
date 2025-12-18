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
//! use crucible_rune::event_markdown::EventToMarkdown;
//! use crucible_rune::reactor::SessionEvent;
//!
//! let event = SessionEvent::MessageReceived {
//!     content: "Hello!".into(),
//!     participant_id: "user".into(),
//! };
//!
//! let markdown = event.to_markdown_block(Some(timestamp_ms));
//! ```

use std::path::{Path, PathBuf};

use crate::reactor::{SessionEvent, ToolCall};

/// Trait for converting events to markdown blocks.
pub trait EventToMarkdown {
    /// Convert the event to a markdown block string.
    ///
    /// # Arguments
    ///
    /// * `timestamp_ms` - Optional timestamp in milliseconds since UNIX epoch.
    ///   If None, uses current time.
    fn to_markdown_block(&self, timestamp_ms: Option<u64>) -> String;

    /// Get the event type name for the markdown header.
    fn event_type_name(&self) -> &'static str;
}

impl EventToMarkdown for SessionEvent {
    fn event_type_name(&self) -> &'static str {
        match self {
            SessionEvent::MessageReceived { .. } => "MessageReceived",
            SessionEvent::AgentResponded { .. } => "AgentResponded",
            SessionEvent::AgentThinking { .. } => "AgentThinking",
            SessionEvent::ToolCalled { .. } => "ToolCalled",
            SessionEvent::ToolCompleted { .. } => "ToolCompleted",
            SessionEvent::SessionStarted { .. } => "SessionStarted",
            SessionEvent::SessionCompacted { .. } => "SessionCompacted",
            SessionEvent::SessionEnded { .. } => "SessionEnded",
            SessionEvent::SubagentSpawned { .. } => "SubagentSpawned",
            SessionEvent::SubagentCompleted { .. } => "SubagentCompleted",
            SessionEvent::SubagentFailed { .. } => "SubagentFailed",
            SessionEvent::TextDelta { .. } => "TextDelta",
            SessionEvent::NoteParsed { .. } => "NoteParsed",
            SessionEvent::NoteCreated { .. } => "NoteCreated",
            SessionEvent::NoteModified { .. } => "NoteModified",
            SessionEvent::McpAttached { .. } => "McpAttached",
            SessionEvent::ToolDiscovered { .. } => "ToolDiscovered",
            SessionEvent::Custom { .. } => "Custom",
            // File events
            SessionEvent::FileChanged { .. } => "FileChanged",
            SessionEvent::FileDeleted { .. } => "FileDeleted",
            SessionEvent::FileMoved { .. } => "FileMoved",
            // Storage events
            SessionEvent::EntityStored { .. } => "EntityStored",
            SessionEvent::EntityDeleted { .. } => "EntityDeleted",
            SessionEvent::BlocksUpdated { .. } => "BlocksUpdated",
            SessionEvent::RelationStored { .. } => "RelationStored",
            SessionEvent::RelationDeleted { .. } => "RelationDeleted",
            SessionEvent::TagAssociated { .. } => "TagAssociated",
            // Embedding events
            SessionEvent::EmbeddingRequested { .. } => "EmbeddingRequested",
            SessionEvent::EmbeddingStored { .. } => "EmbeddingStored",
            SessionEvent::EmbeddingFailed { .. } => "EmbeddingFailed",
            SessionEvent::EmbeddingBatchComplete { .. } => "EmbeddingBatchComplete",
        }
    }

    fn to_markdown_block(&self, timestamp_ms: Option<u64>) -> String {
        let timestamp = format_timestamp(timestamp_ms);
        let event_type = self.event_type_name();
        let header = format!("## {} - {}\n\n", timestamp, event_type);

        let body = match self {
            SessionEvent::MessageReceived {
                content,
                participant_id,
            } => format_message_received(participant_id, content),

            SessionEvent::AgentResponded {
                content,
                tool_calls,
            } => format_agent_responded(content, tool_calls),

            SessionEvent::AgentThinking { thought } => format_agent_thinking(thought),

            SessionEvent::ToolCalled { name, args } => format_tool_called(name, args),

            SessionEvent::ToolCompleted {
                name,
                result,
                error,
            } => format_tool_completed(name, result, error.as_deref()),

            SessionEvent::SessionStarted { config } => {
                let folder = config
                    .folder
                    .as_ref()
                    .map(|p| p.as_path())
                    .unwrap_or(std::path::Path::new(""));
                format_session_started(&config.session_id, folder)
            }

            SessionEvent::SessionCompacted { summary, new_file } => {
                format_session_compacted(summary, new_file)
            }

            SessionEvent::SessionEnded { reason } => format_session_ended(reason),

            SessionEvent::SubagentSpawned { id, prompt } => format_subagent_spawned(id, prompt),

            SessionEvent::SubagentCompleted { id, result } => format_subagent_completed(id, result),

            SessionEvent::SubagentFailed { id, error } => format_subagent_failed(id, error),

            SessionEvent::TextDelta { delta, seq } => {
                format!("**Seq:** {}\n\n```\n{}\n```\n", seq, delta)
            }

            SessionEvent::NoteParsed {
                path,
                block_count,
                payload,
            } => {
                let mut out = format!(
                    "**Path:** `{}`\n**Blocks:** {}\n",
                    path.display(),
                    block_count
                );
                if let Some(p) = payload {
                    out.push_str(&format!("**Title:** {}\n", p.title));
                    if !p.tags.is_empty() {
                        out.push_str(&format!("**Tags:** {}\n", p.tags.join(", ")));
                    }
                }
                out
            }

            SessionEvent::NoteCreated { path, title } => {
                let title_str = title.as_deref().unwrap_or("(untitled)");
                format!("**Path:** `{}`\n**Title:** {}\n", path.display(), title_str)
            }

            SessionEvent::NoteModified { path, change_type } => {
                format!(
                    "**Path:** `{}`\n**Change:** {:?}\n",
                    path.display(),
                    change_type
                )
            }

            SessionEvent::McpAttached { server, tool_count } => {
                format!("**Server:** {}\n**Tools:** {}\n", server, tool_count)
            }

            SessionEvent::ToolDiscovered {
                name,
                source,
                schema,
            } => {
                let schema_str = schema
                    .as_ref()
                    .map(|s| {
                        format!(
                            "\n```json\n{}\n```\n",
                            serde_json::to_string_pretty(s).unwrap_or_default()
                        )
                    })
                    .unwrap_or_default();
                format!("**Name:** {}\n**Source:** {:?}{}", name, source, schema_str)
            }

            SessionEvent::Custom { name, payload } => format_custom_event(name, payload),

            // File events
            SessionEvent::FileChanged { path, kind } => {
                format!("**Path:** `{}`\n**Kind:** {:?}\n", path.display(), kind)
            }
            SessionEvent::FileDeleted { path } => {
                format!("**Path:** `{}`\n", path.display())
            }
            SessionEvent::FileMoved { from, to } => {
                format!(
                    "**From:** `{}`\n**To:** `{}`\n",
                    from.display(),
                    to.display()
                )
            }

            // Storage events
            SessionEvent::EntityStored {
                entity_id,
                entity_type,
            } => {
                format!("**Entity:** {:?}\n**ID:** {}\n", entity_type, entity_id)
            }
            SessionEvent::EntityDeleted {
                entity_id,
                entity_type,
            } => {
                format!("**Entity:** {:?}\n**ID:** {}\n", entity_type, entity_id)
            }
            SessionEvent::BlocksUpdated {
                entity_id,
                block_count,
            } => {
                format!("**Entity:** {}\n**Blocks:** {}\n", entity_id, block_count)
            }
            SessionEvent::RelationStored {
                from_id,
                to_id,
                relation_type,
            } => {
                format!(
                    "**From:** {}\n**To:** {}\n**Type:** {}\n",
                    from_id, to_id, relation_type
                )
            }
            SessionEvent::RelationDeleted {
                from_id,
                to_id,
                relation_type,
            } => {
                format!(
                    "**From:** {}\n**To:** {}\n**Type:** {}\n",
                    from_id, to_id, relation_type
                )
            }
            SessionEvent::TagAssociated { entity_id, tag } => {
                format!("**Entity:** {}\n**Tag:** #{}\n", entity_id, tag)
            }

            // Embedding events
            SessionEvent::EmbeddingRequested {
                entity_id,
                priority,
                ..
            } => {
                format!("**Entity:** {}\n**Priority:** {:?}\n", entity_id, priority)
            }
            SessionEvent::EmbeddingStored {
                entity_id,
                dimensions,
                ..
            } => {
                format!(
                    "**Entity:** {}\n**Dimensions:** {}\n",
                    entity_id, dimensions
                )
            }
            SessionEvent::EmbeddingFailed {
                entity_id, error, ..
            } => {
                format!("**Entity:** {}\n**Error:** {}\n", entity_id, error)
            }
            SessionEvent::EmbeddingBatchComplete {
                entity_id,
                count,
                duration_ms,
            } => {
                format!(
                    "**Entity:** {}\n**Count:** {}\n**Duration:** {}ms\n",
                    entity_id, count, duration_ms
                )
            }
        };

        format!("{}{}\n---\n", header, body)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Formatting helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Format timestamp from milliseconds since UNIX epoch to ISO 8601.
fn format_timestamp(timestamp_ms: Option<u64>) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let ms = timestamp_ms.unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    });

    // Convert to datetime components
    let secs = ms / 1000;
    let millis = ms % 1000;

    // Days since epoch
    let days = secs / 86400;
    let time_of_day = secs % 86400;

    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Approximate year/month/day (simplified - good enough for display)
    // This is a basic implementation; a real one would use chrono
    let (year, month, day) = days_to_ymd(days as i64);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}",
        year, month, day, hours, minutes, seconds, millis
    )
}

/// Convert days since UNIX epoch to year/month/day.
/// Simplified implementation - handles leap years approximately.
fn days_to_ymd(days: i64) -> (i64, u32, u32) {
    // Epoch is 1970-01-01
    let mut remaining_days = days;
    let mut year = 1970;

    // Find year
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    // Find month
    let days_in_months: [i64; 12] = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u32;
    for days_in_month in days_in_months.iter() {
        if remaining_days < *days_in_month {
            break;
        }
        remaining_days -= *days_in_month;
        month += 1;
    }

    let day = (remaining_days + 1) as u32;

    (year, month.min(12), day.min(31))
}

/// Check if a year is a leap year.
fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Format MessageReceived event body.
fn format_message_received(participant_id: &str, content: &str) -> String {
    let quoted_content = quote_content(content);
    format!(
        "**Participant:** {}\n\n{}\n",
        participant_id, quoted_content
    )
}

/// Format AgentResponded event body.
fn format_agent_responded(content: &str, tool_calls: &[ToolCall]) -> String {
    let mut body = String::new();

    if !content.is_empty() {
        body.push_str("**Content:**\n\n");
        body.push_str(content);
        body.push_str("\n\n");
    }

    if !tool_calls.is_empty() {
        body.push_str("**Tool Calls:**\n\n");
        for call in tool_calls {
            let args_str = serde_json::to_string(&call.args).unwrap_or_default();
            if let Some(call_id) = &call.call_id {
                body.push_str(&format!(
                    "- `{}` (id: {}): `{}`\n",
                    call.name, call_id, args_str
                ));
            } else {
                body.push_str(&format!("- `{}`: `{}`\n", call.name, args_str));
            }
        }
        body.push('\n');
    }

    body
}

/// Format AgentThinking event body.
fn format_agent_thinking(thought: &str) -> String {
    format!("*{}*\n", thought)
}

/// Format ToolCalled event body.
fn format_tool_called(name: &str, args: &serde_json::Value) -> String {
    let args_pretty = serde_json::to_string_pretty(args).unwrap_or_default();
    format!(
        "**Tool:** `{}`\n\n**Arguments:**\n```json\n{}\n```\n",
        name, args_pretty
    )
}

/// Format ToolCompleted event body.
fn format_tool_completed(name: &str, result: &str, error: Option<&str>) -> String {
    let mut body = format!("**Tool:** `{}`\n\n", name);

    if let Some(err) = error {
        body.push_str(&format!("**Error:** {}\n\n", err));
    }

    if !result.is_empty() {
        // Check if result looks like code/structured data
        if result.contains('\n') || result.len() > 100 {
            body.push_str("**Result:**\n```\n");
            body.push_str(result);
            body.push_str("\n```\n");
        } else {
            body.push_str(&format!("**Result:** {}\n", result));
        }
    }

    body
}

/// Format SessionStarted event body.
fn format_session_started(session_id: &str, folder: &Path) -> String {
    format!(
        "**Session ID:** `{}`\n**Folder:** `{}`\n",
        session_id,
        folder.display()
    )
}

/// Format SessionCompacted event body.
fn format_session_compacted(summary: &str, new_file: &Path) -> String {
    let mut body = format!("**New File:** `{}`\n\n", new_file.display());

    if !summary.is_empty() {
        body.push_str("**Summary:**\n\n");
        body.push_str(summary);
        body.push('\n');
    }

    body
}

/// Format SessionEnded event body.
fn format_session_ended(reason: &str) -> String {
    format!("**Reason:** {}\n", reason)
}

/// Format SubagentSpawned event body.
fn format_subagent_spawned(id: &str, prompt: &str) -> String {
    let quoted_prompt = quote_content(prompt);
    format!(
        "**Subagent ID:** `{}`\n\n**Prompt:**\n{}\n",
        id, quoted_prompt
    )
}

/// Format SubagentCompleted event body.
fn format_subagent_completed(id: &str, result: &str) -> String {
    format!(
        "**Subagent ID:** `{}`\n\n**Result:**\n{}\n",
        id,
        quote_content(result)
    )
}

/// Format SubagentFailed event body.
fn format_subagent_failed(id: &str, error: &str) -> String {
    format!("**Subagent ID:** `{}`\n\n**Error:** {}\n", id, error)
}

/// Format Custom event body.
fn format_custom_event(name: &str, payload: &serde_json::Value) -> String {
    let payload_pretty = serde_json::to_string_pretty(payload).unwrap_or_default();
    format!(
        "**Event Name:** `{}`\n\n**Payload:**\n```json\n{}\n```\n",
        name, payload_pretty
    )
}

/// Quote content as a blockquote, handling multiline.
fn quote_content(content: &str) -> String {
    if content.is_empty() {
        return String::new();
    }

    content
        .lines()
        .map(|line| format!("> {}", line))
        .collect::<Vec<_>>()
        .join("\n")
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

/// Trait for parsing markdown blocks into events.
pub trait MarkdownToEvent: Sized {
    /// Parse a markdown block into an event.
    ///
    /// # Arguments
    ///
    /// * `markdown` - The markdown block to parse (including header and separator).
    ///
    /// # Returns
    ///
    /// A tuple of (event, timestamp_ms) where timestamp is extracted from the header.
    fn from_markdown_block(markdown: &str) -> MarkdownParseResult<(Self, u64)>;
}

/// Parsed markdown block header.
#[derive(Debug, Clone)]
struct ParsedHeader {
    /// Timestamp in milliseconds since UNIX epoch.
    timestamp_ms: u64,
    /// Event type name.
    event_type: String,
}

/// Parse the header line of a markdown block.
///
/// Expected format: `## 2025-12-14T15:30:45.123 - EventType`
fn parse_header(line: &str) -> MarkdownParseResult<ParsedHeader> {
    let line = line.trim();

    // Must start with ##
    if !line.starts_with("## ") {
        return Err(MarkdownParseError::InvalidHeader(
            "Header must start with '## '".to_string(),
        ));
    }

    let content = &line[3..]; // Skip "## "

    // Split by " - " to get timestamp and event type
    let parts: Vec<&str> = content.splitn(2, " - ").collect();
    if parts.len() != 2 {
        return Err(MarkdownParseError::InvalidHeader(
            "Header must contain ' - ' separator".to_string(),
        ));
    }

    let timestamp_str = parts[0].trim();
    let event_type = parts[1].trim().to_string();

    let timestamp_ms = parse_iso_timestamp(timestamp_str)?;

    Ok(ParsedHeader {
        timestamp_ms,
        event_type,
    })
}

/// Parse an ISO 8601 timestamp string to milliseconds since UNIX epoch.
///
/// Expected format: `2025-12-14T15:30:45.123`
fn parse_iso_timestamp(s: &str) -> MarkdownParseResult<u64> {
    // Format: YYYY-MM-DDTHH:MM:SS.mmm
    let parts: Vec<&str> = s.split('T').collect();
    if parts.len() != 2 {
        return Err(MarkdownParseError::InvalidTimestamp(format!(
            "Expected 'T' separator in timestamp: {}",
            s
        )));
    }

    let date_parts: Vec<&str> = parts[0].split('-').collect();
    if date_parts.len() != 3 {
        return Err(MarkdownParseError::InvalidTimestamp(format!(
            "Invalid date format: {}",
            parts[0]
        )));
    }

    let year: i64 = date_parts[0].parse().map_err(|_| {
        MarkdownParseError::InvalidTimestamp(format!("Invalid year: {}", date_parts[0]))
    })?;
    let month: u32 = date_parts[1].parse().map_err(|_| {
        MarkdownParseError::InvalidTimestamp(format!("Invalid month: {}", date_parts[1]))
    })?;
    let day: u32 = date_parts[2].parse().map_err(|_| {
        MarkdownParseError::InvalidTimestamp(format!("Invalid day: {}", date_parts[2]))
    })?;

    // Time part may have .mmm for milliseconds
    let time_str = parts[1];
    let (time_part, millis) = if let Some(dot_pos) = time_str.find('.') {
        let ms_str = &time_str[dot_pos + 1..];
        let ms: u64 = ms_str.parse().map_err(|_| {
            MarkdownParseError::InvalidTimestamp(format!("Invalid milliseconds: {}", ms_str))
        })?;
        (&time_str[..dot_pos], ms)
    } else {
        (time_str, 0u64)
    };

    let time_parts: Vec<&str> = time_part.split(':').collect();
    if time_parts.len() != 3 {
        return Err(MarkdownParseError::InvalidTimestamp(format!(
            "Invalid time format: {}",
            time_part
        )));
    }

    let hours: u64 = time_parts[0].parse().map_err(|_| {
        MarkdownParseError::InvalidTimestamp(format!("Invalid hours: {}", time_parts[0]))
    })?;
    let minutes: u64 = time_parts[1].parse().map_err(|_| {
        MarkdownParseError::InvalidTimestamp(format!("Invalid minutes: {}", time_parts[1]))
    })?;
    let seconds: u64 = time_parts[2].parse().map_err(|_| {
        MarkdownParseError::InvalidTimestamp(format!("Invalid seconds: {}", time_parts[2]))
    })?;

    // Convert to days since epoch
    let days = ymd_to_days(year, month, day);

    // Calculate total milliseconds
    let time_ms = (hours * 3600 + minutes * 60 + seconds) * 1000 + millis;
    let total_ms = (days as u64) * 86400 * 1000 + time_ms;

    Ok(total_ms)
}

/// Convert year/month/day to days since UNIX epoch.
fn ymd_to_days(year: i64, month: u32, day: u32) -> i64 {
    let mut days: i64 = 0;

    // Count years from 1970
    if year >= 1970 {
        for y in 1970..year {
            days += if is_leap_year(y) { 366 } else { 365 };
        }
    } else {
        for y in year..1970 {
            days -= if is_leap_year(y) { 366 } else { 365 };
        }
    }

    // Count months in current year
    let days_in_months: [i64; 12] = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    for m in 0..((month - 1) as usize) {
        days += days_in_months[m];
    }

    // Add days (minus 1 since day 1 = 0 extra days)
    days += (day - 1) as i64;

    days
}

impl MarkdownToEvent for SessionEvent {
    fn from_markdown_block(markdown: &str) -> MarkdownParseResult<(Self, u64)> {
        let lines: Vec<&str> = markdown.lines().collect();

        if lines.is_empty() {
            return Err(MarkdownParseError::InvalidHeader(
                "Empty markdown block".to_string(),
            ));
        }

        // Parse header (first line)
        let header = parse_header(lines[0])?;

        // Get body (everything between header and trailing ---)
        let body_lines: Vec<&str> = lines[1..]
            .iter()
            .take_while(|line| line.trim() != "---")
            .copied()
            .collect();
        let body = body_lines.join("\n");

        // Parse based on event type
        let event = match header.event_type.as_str() {
            "MessageReceived" => parse_message_received(&body)?,
            "AgentResponded" => parse_agent_responded(&body)?,
            "AgentThinking" => parse_agent_thinking(&body)?,
            "ToolCalled" => parse_tool_called(&body)?,
            "ToolCompleted" => parse_tool_completed(&body)?,
            "SessionStarted" => parse_session_started(&body)?,
            "SessionCompacted" => parse_session_compacted(&body)?,
            "SessionEnded" => parse_session_ended(&body)?,
            "SubagentSpawned" => parse_subagent_spawned(&body)?,
            "SubagentCompleted" => parse_subagent_completed(&body)?,
            "SubagentFailed" => parse_subagent_failed(&body)?,
            "Custom" => parse_custom_event(&body)?,
            other => return Err(MarkdownParseError::UnknownEventType(other.to_string())),
        };

        Ok((event, header.timestamp_ms))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Individual event parsers
// ─────────────────────────────────────────────────────────────────────────────

/// Parse MessageReceived event from body.
fn parse_message_received(body: &str) -> MarkdownParseResult<SessionEvent> {
    let participant_id = extract_field(body, "**Participant:**")?;
    let content = extract_quoted_content(body);

    Ok(SessionEvent::MessageReceived {
        content,
        participant_id,
    })
}

/// Parse AgentResponded event from body.
fn parse_agent_responded(body: &str) -> MarkdownParseResult<SessionEvent> {
    let content = if body.contains("**Content:**") {
        extract_section_content(body, "**Content:**")
    } else {
        String::new()
    };

    let tool_calls = if body.contains("**Tool Calls:**") {
        parse_tool_calls_section(body)?
    } else {
        vec![]
    };

    Ok(SessionEvent::AgentResponded {
        content,
        tool_calls,
    })
}

/// Parse AgentThinking event from body.
fn parse_agent_thinking(body: &str) -> MarkdownParseResult<SessionEvent> {
    // Format: *thought text*
    let thought = body
        .lines()
        .find(|line| line.trim().starts_with('*') && line.trim().ends_with('*'))
        .map(|line| {
            let trimmed = line.trim();
            trimmed[1..trimmed.len() - 1].to_string()
        })
        .unwrap_or_default();

    Ok(SessionEvent::AgentThinking { thought })
}

/// Parse ToolCalled event from body.
fn parse_tool_called(body: &str) -> MarkdownParseResult<SessionEvent> {
    let name = extract_inline_code_field(body, "**Tool:**")?;
    let args = extract_json_block(body, "**Arguments:**")?;

    Ok(SessionEvent::ToolCalled { name, args })
}

/// Parse ToolCompleted event from body.
fn parse_tool_completed(body: &str) -> MarkdownParseResult<SessionEvent> {
    let name = extract_inline_code_field(body, "**Tool:**")?;

    let error = if body.contains("**Error:**") {
        Some(extract_field(body, "**Error:**")?)
    } else {
        None
    };

    let result = if body.contains("**Result:**") {
        extract_result_content(body)
    } else {
        String::new()
    };

    Ok(SessionEvent::ToolCompleted {
        name,
        result,
        error,
    })
}

/// Parse SessionStarted event from body.
fn parse_session_started(body: &str) -> MarkdownParseResult<SessionEvent> {
    let session_id = extract_inline_code_field(body, "**Session ID:**")?;
    let folder_str = extract_inline_code_field(body, "**Folder:**")?;
    let folder = PathBuf::from(folder_str);

    Ok(SessionEvent::SessionStarted {
        config: crate::reactor::SessionEventConfig::new(session_id).with_folder(folder),
    })
}

/// Parse SessionCompacted event from body.
fn parse_session_compacted(body: &str) -> MarkdownParseResult<SessionEvent> {
    let new_file_str = extract_inline_code_field(body, "**New File:**")?;
    let new_file = PathBuf::from(new_file_str);

    let summary = if body.contains("**Summary:**") {
        extract_section_content(body, "**Summary:**")
    } else {
        String::new()
    };

    Ok(SessionEvent::SessionCompacted { summary, new_file })
}

/// Parse SessionEnded event from body.
fn parse_session_ended(body: &str) -> MarkdownParseResult<SessionEvent> {
    let reason = extract_field(body, "**Reason:**")?;
    Ok(SessionEvent::SessionEnded { reason })
}

/// Parse SubagentSpawned event from body.
fn parse_subagent_spawned(body: &str) -> MarkdownParseResult<SessionEvent> {
    let id = extract_inline_code_field(body, "**Subagent ID:**")?;

    // Prompt is in a quoted section after **Prompt:**
    let prompt = if body.contains("**Prompt:**") {
        let prompt_section = body.split("**Prompt:**").nth(1).unwrap_or("").trim();
        extract_quoted_content(prompt_section)
    } else {
        String::new()
    };

    Ok(SessionEvent::SubagentSpawned { id, prompt })
}

/// Parse SubagentCompleted event from body.
fn parse_subagent_completed(body: &str) -> MarkdownParseResult<SessionEvent> {
    let id = extract_inline_code_field(body, "**Subagent ID:**")?;

    // Result is in a quoted section after **Result:**
    let result = if body.contains("**Result:**") {
        let result_section = body.split("**Result:**").nth(1).unwrap_or("").trim();
        extract_quoted_content(result_section)
    } else {
        String::new()
    };

    Ok(SessionEvent::SubagentCompleted { id, result })
}

/// Parse SubagentFailed event from body.
fn parse_subagent_failed(body: &str) -> MarkdownParseResult<SessionEvent> {
    let id = extract_inline_code_field(body, "**Subagent ID:**")?;
    let error = extract_field(body, "**Error:**")?;

    Ok(SessionEvent::SubagentFailed { id, error })
}

/// Parse Custom event from body.
fn parse_custom_event(body: &str) -> MarkdownParseResult<SessionEvent> {
    let name = extract_inline_code_field(body, "**Event Name:**")?;
    let payload = extract_json_block(body, "**Payload:**")?;

    Ok(SessionEvent::Custom { name, payload })
}

// ─────────────────────────────────────────────────────────────────────────────
// Extraction helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Extract a simple field value from the body.
/// Format: **Label:** value
fn extract_field(body: &str, label: &str) -> MarkdownParseResult<String> {
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(label) {
            let value = trimmed[label.len()..].trim();
            return Ok(value.to_string());
        }
    }
    Err(MarkdownParseError::MissingField(label.to_string()))
}

/// Extract a field value that's in inline code.
/// Format: **Label:** `value`
fn extract_inline_code_field(body: &str, label: &str) -> MarkdownParseResult<String> {
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(label) {
            let after_label = trimmed[label.len()..].trim();
            // Extract value between backticks
            if after_label.starts_with('`') {
                let end = after_label[1..].find('`').unwrap_or(after_label.len() - 1);
                return Ok(after_label[1..end + 1].to_string());
            }
            return Ok(after_label.to_string());
        }
    }
    Err(MarkdownParseError::MissingField(label.to_string()))
}

/// Extract quoted content (lines starting with > ).
fn extract_quoted_content(body: &str) -> String {
    body.lines()
        .filter(|line| line.trim().starts_with("> ") || line.trim() == ">")
        .map(|line| {
            let trimmed = line.trim();
            if trimmed == ">" {
                ""
            } else {
                &trimmed[2..] // Skip "> "
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Extract section content after a label, until the next label or end.
fn extract_section_content(body: &str, label: &str) -> String {
    let mut in_section = false;
    let mut content_lines = Vec::new();

    for line in body.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with(label) {
            in_section = true;
            continue;
        }

        if in_section {
            // Stop at next bold label
            if trimmed.starts_with("**") && trimmed.contains(":**") {
                break;
            }
            content_lines.push(line);
        }
    }

    // Trim leading/trailing empty lines
    let content = content_lines.join("\n");
    content.trim().to_string()
}

/// Extract result content (may be inline or in code block).
fn extract_result_content(body: &str) -> String {
    let after_result = body.split("**Result:**").nth(1).unwrap_or("").trim();

    // Check for code block
    if after_result.starts_with("\n```") || after_result.starts_with("```") {
        let block_content = after_result
            .lines()
            .skip_while(|line| !line.starts_with("```"))
            .skip(1) // Skip opening ```
            .take_while(|line| !line.starts_with("```"))
            .collect::<Vec<_>>()
            .join("\n");
        return block_content;
    }

    // Inline result on same line
    after_result.lines().next().unwrap_or("").trim().to_string()
}

/// Extract a JSON block after a label.
fn extract_json_block(body: &str, label: &str) -> MarkdownParseResult<serde_json::Value> {
    let after_label = body
        .split(label)
        .nth(1)
        .ok_or_else(|| MarkdownParseError::MissingField(label.to_string()))?;

    // Find the JSON code block
    let in_block = after_label
        .lines()
        .skip_while(|line| !line.trim().starts_with("```"))
        .skip(1) // Skip opening ```json
        .take_while(|line| !line.trim().starts_with("```"))
        .collect::<Vec<_>>()
        .join("\n");

    serde_json::from_str(&in_block).map_err(|e| MarkdownParseError::JsonParse(e.to_string()))
}

/// Parse tool calls section.
/// Format:
/// - `tool_name`: `{"arg": "value"}`
/// - `tool_name` (id: call_id): `{"arg": "value"}`
fn parse_tool_calls_section(body: &str) -> MarkdownParseResult<Vec<ToolCall>> {
    let mut tool_calls = Vec::new();

    let after_label = body.split("**Tool Calls:**").nth(1).unwrap_or("");

    for line in after_label.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("- `") {
            if trimmed.starts_with("**") {
                break; // Next section
            }
            continue;
        }

        // Parse: - `name`: `{...}` or - `name` (id: xyz): `{...}`
        // After "- ", content is: `name`: `{...}` or `name` (id: xyz): `{...}`
        let content = &trimmed[2..]; // Skip "- "

        // Content now starts with backtick. Find the closing backtick for the name.
        if !content.starts_with('`') {
            continue;
        }

        // Find the closing backtick for the tool name (starting search after opening backtick)
        let name_end = content[1..].find('`');
        if name_end.is_none() {
            continue;
        }
        let name_end = name_end.unwrap();
        let name = content[1..1 + name_end].to_string();

        // After the name backticks, we have: `: `{...}` or ` (id: xyz): `{...}`
        let after_name = &content[1 + name_end + 1..]; // Skip `name`

        // Check for call ID
        let (call_id, rest) = if after_name.trim_start().starts_with("(id:") {
            let id_start = after_name.find("(id:").unwrap() + 5; // Skip "(id: "
            let id_end = after_name[id_start..].find(')');
            if let Some(end_offset) = id_end {
                let id = after_name[id_start..id_start + end_offset]
                    .trim()
                    .to_string();
                (Some(id), &after_name[id_start + end_offset + 1..])
            } else {
                (None, after_name)
            }
        } else {
            (None, after_name)
        };

        // Now find the JSON between backticks: `: `{...}`
        // rest should be something like ": `{...}`"
        if let Some(json_start) = rest.find('`') {
            let json_content = &rest[json_start + 1..];
            if let Some(json_end) = json_content.find('`') {
                let json_str = &json_content[..json_end];
                if let Ok(args) = serde_json::from_str(json_str) {
                    let mut tc = ToolCall::new(name, args);
                    if let Some(id) = call_id {
                        tc = tc.with_call_id(id);
                    }
                    tool_calls.push(tc);
                }
            }
        }
    }

    Ok(tool_calls)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    use crate::reactor::SessionEventConfig;

    // Fixed timestamp for consistent test output: 2025-12-14T15:30:45.123 UTC
    // Calculated using: datetime.datetime(2025, 12, 14, 15, 30, 45, 123000, tzinfo=datetime.timezone.utc).timestamp() * 1000
    const TEST_TIMESTAMP_MS: u64 = 1765726245123;

    #[test]
    fn test_format_timestamp() {
        let ts = format_timestamp(Some(TEST_TIMESTAMP_MS));
        assert_eq!(ts, "2025-12-14T15:30:45.123");
    }

    #[test]
    fn test_format_timestamp_epoch() {
        let ts = format_timestamp(Some(0));
        assert_eq!(ts, "1970-01-01T00:00:00.000");
    }

    #[test]
    fn test_days_to_ymd_epoch() {
        let (y, m, d) = days_to_ymd(0);
        assert_eq!((y, m, d), (1970, 1, 1));
    }

    #[test]
    fn test_days_to_ymd_leap_year() {
        // 2000-03-01 - this tests leap year handling
        // Days from 1970-01-01 to 2000-03-01
        // 30 years: 10957 days (accounting for leap years)
        let (y, m, d) = days_to_ymd(11017); // 2000-03-01
        assert_eq!(y, 2000);
        assert_eq!(m, 3);
        assert_eq!(d, 1);
    }

    #[test]
    fn test_is_leap_year() {
        assert!(!is_leap_year(1970));
        assert!(is_leap_year(2000)); // divisible by 400
        assert!(!is_leap_year(1900)); // divisible by 100 but not 400
        assert!(is_leap_year(2024)); // divisible by 4
        assert!(!is_leap_year(2023));
    }

    #[test]
    fn test_quote_content_single_line() {
        let quoted = quote_content("Hello world");
        assert_eq!(quoted, "> Hello world");
    }

    #[test]
    fn test_quote_content_multiline() {
        let quoted = quote_content("Line 1\nLine 2\nLine 3");
        assert_eq!(quoted, "> Line 1\n> Line 2\n> Line 3");
    }

    #[test]
    fn test_quote_content_empty() {
        let quoted = quote_content("");
        assert_eq!(quoted, "");
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Event type name tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_event_type_name_message_received() {
        let event = SessionEvent::MessageReceived {
            content: "test".into(),
            participant_id: "user".into(),
        };
        assert_eq!(event.event_type_name(), "MessageReceived");
    }

    #[test]
    fn test_event_type_name_tool_called() {
        let event = SessionEvent::ToolCalled {
            name: "test".into(),
            args: json!({}),
        };
        assert_eq!(event.event_type_name(), "ToolCalled");
    }

    #[test]
    fn test_event_type_name_tool_completed() {
        let event = SessionEvent::ToolCompleted {
            name: "test".into(),
            result: "".into(),
            error: None,
        };
        assert_eq!(event.event_type_name(), "ToolCompleted");
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Full markdown conversion tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn message_event_to_markdown() {
        let event = SessionEvent::MessageReceived {
            content: "Help me implement the task harness".into(),
            participant_id: "user".into(),
        };

        let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

        assert!(md.contains("## 2025-12-14T15:30:45.123 - MessageReceived"));
        assert!(md.contains("**Participant:** user"));
        assert!(md.contains("> Help me implement the task harness"));
        assert!(md.ends_with("---\n"));
    }

    #[test]
    fn message_event_multiline_content() {
        let event = SessionEvent::MessageReceived {
            content: "Line 1\nLine 2\nLine 3".into(),
            participant_id: "assistant".into(),
        };

        let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

        assert!(md.contains("> Line 1\n> Line 2\n> Line 3"));
    }

    #[test]
    fn tool_event_to_markdown() {
        let event = SessionEvent::ToolCalled {
            name: "read_file".into(),
            args: json!({"path": "/tmp/test.txt"}),
        };

        let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

        assert!(md.contains("## 2025-12-14T15:30:45.123 - ToolCalled"));
        assert!(md.contains("**Tool:** `read_file`"));
        assert!(md.contains("**Arguments:**"));
        assert!(md.contains("\"path\": \"/tmp/test.txt\""));
        assert!(md.ends_with("---\n"));
    }

    #[test]
    fn tool_completed_event_to_markdown() {
        let event = SessionEvent::ToolCompleted {
            name: "read_file".into(),
            result: "File contents here".into(),
            error: None,
        };

        let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

        assert!(md.contains("## 2025-12-14T15:30:45.123 - ToolCompleted"));
        assert!(md.contains("**Tool:** `read_file`"));
        assert!(md.contains("**Result:** File contents here"));
        assert!(!md.contains("**Error:**"));
    }

    #[test]
    fn tool_completed_with_error_to_markdown() {
        let event = SessionEvent::ToolCompleted {
            name: "read_file".into(),
            result: "".into(),
            error: Some("File not found".into()),
        };

        let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

        assert!(md.contains("**Error:** File not found"));
    }

    #[test]
    fn tool_completed_long_result_to_markdown() {
        let long_result = "Line 1\nLine 2\nLine 3\nMore content here that spans multiple lines";
        let event = SessionEvent::ToolCompleted {
            name: "search".into(),
            result: long_result.into(),
            error: None,
        };

        let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

        // Long results should be in code blocks
        assert!(md.contains("**Result:**\n```\n"));
        assert!(md.contains(long_result));
        assert!(md.contains("\n```\n"));
    }

    #[test]
    fn agent_responded_to_markdown() {
        let event = SessionEvent::AgentResponded {
            content: "I'll help you with that.".into(),
            tool_calls: vec![ToolCall::new("read_file", json!({"path": "/tmp/test.txt"}))],
        };

        let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

        assert!(md.contains("## 2025-12-14T15:30:45.123 - AgentResponded"));
        assert!(md.contains("**Content:**"));
        assert!(md.contains("I'll help you with that."));
        assert!(md.contains("**Tool Calls:**"));
        assert!(md.contains("- `read_file`: `{\"path\":\"/tmp/test.txt\"}`"));
    }

    #[test]
    fn agent_responded_with_call_id_to_markdown() {
        let event = SessionEvent::AgentResponded {
            content: "".into(),
            tool_calls: vec![
                ToolCall::new("search", json!({"query": "test"})).with_call_id("call_123")
            ],
        };

        let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

        assert!(md.contains("- `search` (id: call_123):"));
    }

    #[test]
    fn agent_thinking_to_markdown() {
        let event = SessionEvent::AgentThinking {
            thought: "Analyzing the codebase...".into(),
        };

        let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

        assert!(md.contains("## 2025-12-14T15:30:45.123 - AgentThinking"));
        assert!(md.contains("*Analyzing the codebase...*"));
    }

    #[test]
    fn session_started_to_markdown() {
        let event = SessionEvent::SessionStarted {
            config: SessionEventConfig::new("2025-12-14T1530-task")
                .with_folder("/kiln/Sessions/2025-12-14T1530-task")
                .with_max_context_tokens(100_000),
        };

        let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

        assert!(md.contains("## 2025-12-14T15:30:45.123 - SessionStarted"));
        assert!(md.contains("**Session ID:** `2025-12-14T1530-task`"));
        assert!(md.contains("**Folder:** `/kiln/Sessions/2025-12-14T1530-task`"));
    }

    #[test]
    fn session_compacted_to_markdown() {
        let event = SessionEvent::SessionCompacted {
            summary: "Discussed task harness implementation.".into(),
            new_file: PathBuf::from("/kiln/Sessions/test/001-context.md"),
        };

        let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

        assert!(md.contains("## 2025-12-14T15:30:45.123 - SessionCompacted"));
        assert!(md.contains("**New File:** `/kiln/Sessions/test/001-context.md`"));
        assert!(md.contains("**Summary:**"));
        assert!(md.contains("Discussed task harness implementation."));
    }

    #[test]
    fn session_ended_to_markdown() {
        let event = SessionEvent::SessionEnded {
            reason: "User closed session".into(),
        };

        let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

        assert!(md.contains("## 2025-12-14T15:30:45.123 - SessionEnded"));
        assert!(md.contains("**Reason:** User closed session"));
    }

    #[test]
    fn subagent_spawned_to_markdown() {
        let event = SessionEvent::SubagentSpawned {
            id: "sub_abc123".into(),
            prompt: "Find all files related to task harness".into(),
        };

        let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

        assert!(md.contains("## 2025-12-14T15:30:45.123 - SubagentSpawned"));
        assert!(md.contains("**Subagent ID:** `sub_abc123`"));
        assert!(md.contains("**Prompt:**"));
        assert!(md.contains("> Find all files related to task harness"));
    }

    #[test]
    fn subagent_completed_to_markdown() {
        let event = SessionEvent::SubagentCompleted {
            id: "sub_abc123".into(),
            result: "Found 5 files.".into(),
        };

        let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

        assert!(md.contains("## 2025-12-14T15:30:45.123 - SubagentCompleted"));
        assert!(md.contains("**Subagent ID:** `sub_abc123`"));
        assert!(md.contains("**Result:**"));
        assert!(md.contains("> Found 5 files."));
    }

    #[test]
    fn subagent_failed_to_markdown() {
        let event = SessionEvent::SubagentFailed {
            id: "sub_abc123".into(),
            error: "Timeout exceeded".into(),
        };

        let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

        assert!(md.contains("## 2025-12-14T15:30:45.123 - SubagentFailed"));
        assert!(md.contains("**Subagent ID:** `sub_abc123`"));
        assert!(md.contains("**Error:** Timeout exceeded"));
    }

    #[test]
    fn custom_event_to_markdown() {
        let event = SessionEvent::Custom {
            name: "my_custom_event".into(),
            payload: json!({"key": "value", "count": 42}),
        };

        let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

        assert!(md.contains("## 2025-12-14T15:30:45.123 - Custom"));
        assert!(md.contains("**Event Name:** `my_custom_event`"));
        assert!(md.contains("**Payload:**"));
        assert!(md.contains("\"key\": \"value\""));
        assert!(md.contains("\"count\": 42"));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Edge cases
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn event_with_empty_content() {
        let event = SessionEvent::MessageReceived {
            content: "".into(),
            participant_id: "user".into(),
        };

        let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

        // Should still have structure, just no quoted content
        assert!(md.contains("**Participant:** user"));
    }

    #[test]
    fn agent_responded_empty_content_with_tools() {
        let event = SessionEvent::AgentResponded {
            content: "".into(),
            tool_calls: vec![ToolCall::new("test", json!({}))],
        };

        let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

        // Should have tool calls but no content section
        assert!(!md.contains("**Content:**"));
        assert!(md.contains("**Tool Calls:**"));
    }

    #[test]
    fn agent_responded_no_tools() {
        let event = SessionEvent::AgentResponded {
            content: "Just text".into(),
            tool_calls: vec![],
        };

        let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

        assert!(md.contains("**Content:**"));
        assert!(!md.contains("**Tool Calls:**"));
    }

    #[test]
    fn markdown_ends_with_separator() {
        let event = SessionEvent::SessionEnded {
            reason: "done".into(),
        };

        let md = event.to_markdown_block(Some(TEST_TIMESTAMP_MS));

        assert!(md.ends_with("---\n"), "Markdown should end with separator");
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Markdown to Event parsing tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_parse_header_valid() {
        let header = parse_header("## 2025-12-14T15:30:45.123 - MessageReceived").unwrap();
        assert_eq!(header.timestamp_ms, TEST_TIMESTAMP_MS);
        assert_eq!(header.event_type, "MessageReceived");
    }

    #[test]
    fn test_parse_header_invalid_no_hash() {
        let result = parse_header("2025-12-14T15:30:45.123 - MessageReceived");
        assert!(matches!(result, Err(MarkdownParseError::InvalidHeader(_))));
    }

    #[test]
    fn test_parse_header_invalid_no_separator() {
        let result = parse_header("## 2025-12-14T15:30:45.123 MessageReceived");
        assert!(matches!(result, Err(MarkdownParseError::InvalidHeader(_))));
    }

    #[test]
    fn test_parse_iso_timestamp_valid() {
        let ts = parse_iso_timestamp("2025-12-14T15:30:45.123").unwrap();
        assert_eq!(ts, TEST_TIMESTAMP_MS);
    }

    #[test]
    fn test_parse_iso_timestamp_no_millis() {
        let ts = parse_iso_timestamp("2025-12-14T15:30:45").unwrap();
        // Should be the same but without milliseconds
        assert_eq!(ts, TEST_TIMESTAMP_MS - 123);
    }

    #[test]
    fn test_parse_iso_timestamp_epoch() {
        let ts = parse_iso_timestamp("1970-01-01T00:00:00.000").unwrap();
        assert_eq!(ts, 0);
    }

    #[test]
    fn test_ymd_to_days_epoch() {
        let days = ymd_to_days(1970, 1, 1);
        assert_eq!(days, 0);
    }

    #[test]
    fn test_ymd_to_days_2025() {
        // 2025-12-14 should be correct number of days from epoch
        let days = ymd_to_days(2025, 12, 14);
        // Verify by checking timestamp calculation
        let timestamp_ms = (days as u64) * 86400 * 1000;
        // This should be midnight on 2025-12-14
        assert!(timestamp_ms < TEST_TIMESTAMP_MS);
        assert!(timestamp_ms + 86400 * 1000 > TEST_TIMESTAMP_MS);
    }

    #[test]
    fn test_extract_field() {
        let body = "**Participant:** user\n**Other:** value";
        assert_eq!(extract_field(body, "**Participant:**").unwrap(), "user");
    }

    #[test]
    fn test_extract_inline_code_field() {
        let body = "**Tool:** `read_file`\n**Other:** value";
        assert_eq!(
            extract_inline_code_field(body, "**Tool:**").unwrap(),
            "read_file"
        );
    }

    #[test]
    fn test_extract_quoted_content() {
        let body = "> Line 1\n> Line 2\n> Line 3";
        let content = extract_quoted_content(body);
        assert_eq!(content, "Line 1\nLine 2\nLine 3");
    }

    #[test]
    fn test_extract_quoted_content_empty_line() {
        let body = "> Line 1\n>\n> Line 2";
        let content = extract_quoted_content(body);
        assert_eq!(content, "Line 1\n\nLine 2");
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Round-trip tests (to_markdown -> from_markdown)
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn roundtrip_message_received() {
        let original = SessionEvent::MessageReceived {
            content: "Help me implement the task harness".into(),
            participant_id: "user".into(),
        };

        let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
        let (parsed, timestamp) = SessionEvent::from_markdown_block(&md).unwrap();

        assert_eq!(timestamp, TEST_TIMESTAMP_MS);
        match parsed {
            SessionEvent::MessageReceived {
                content,
                participant_id,
            } => {
                assert_eq!(content, "Help me implement the task harness");
                assert_eq!(participant_id, "user");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn roundtrip_message_received_multiline() {
        let original = SessionEvent::MessageReceived {
            content: "Line 1\nLine 2\nLine 3".into(),
            participant_id: "assistant".into(),
        };

        let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
        let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

        match parsed {
            SessionEvent::MessageReceived {
                content,
                participant_id,
            } => {
                assert_eq!(content, "Line 1\nLine 2\nLine 3");
                assert_eq!(participant_id, "assistant");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn roundtrip_agent_thinking() {
        let original = SessionEvent::AgentThinking {
            thought: "Analyzing the codebase...".into(),
        };

        let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
        let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

        match parsed {
            SessionEvent::AgentThinking { thought } => {
                assert_eq!(thought, "Analyzing the codebase...");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn roundtrip_tool_called() {
        let original = SessionEvent::ToolCalled {
            name: "read_file".into(),
            args: json!({"path": "/tmp/test.txt"}),
        };

        let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
        let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

        match parsed {
            SessionEvent::ToolCalled { name, args } => {
                assert_eq!(name, "read_file");
                assert_eq!(args["path"], "/tmp/test.txt");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn roundtrip_tool_completed_inline() {
        let original = SessionEvent::ToolCompleted {
            name: "read_file".into(),
            result: "File contents here".into(),
            error: None,
        };

        let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
        let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

        match parsed {
            SessionEvent::ToolCompleted {
                name,
                result,
                error,
            } => {
                assert_eq!(name, "read_file");
                assert_eq!(result, "File contents here");
                assert!(error.is_none());
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn roundtrip_tool_completed_code_block() {
        let long_result = "Line 1\nLine 2\nLine 3\nMore content here that spans multiple lines";
        let original = SessionEvent::ToolCompleted {
            name: "search".into(),
            result: long_result.into(),
            error: None,
        };

        let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
        let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

        match parsed {
            SessionEvent::ToolCompleted {
                name,
                result,
                error,
            } => {
                assert_eq!(name, "search");
                assert_eq!(result, long_result);
                assert!(error.is_none());
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn roundtrip_tool_completed_with_error() {
        let original = SessionEvent::ToolCompleted {
            name: "read_file".into(),
            result: "".into(),
            error: Some("File not found".into()),
        };

        let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
        let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

        match parsed {
            SessionEvent::ToolCompleted {
                name,
                result: _,
                error,
            } => {
                assert_eq!(name, "read_file");
                assert_eq!(error, Some("File not found".to_string()));
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn roundtrip_session_started() {
        let original = SessionEvent::SessionStarted {
            config: SessionEventConfig::new("2025-12-14T1530-task")
                .with_folder("/kiln/Sessions/2025-12-14T1530-task"),
        };

        let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
        let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

        match parsed {
            SessionEvent::SessionStarted { config } => {
                assert_eq!(config.session_id, "2025-12-14T1530-task");
                assert_eq!(
                    config.folder,
                    Some(PathBuf::from("/kiln/Sessions/2025-12-14T1530-task"))
                );
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn roundtrip_session_compacted() {
        let original = SessionEvent::SessionCompacted {
            summary: "Discussed task harness implementation.".into(),
            new_file: PathBuf::from("/kiln/Sessions/test/001-context.md"),
        };

        let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
        let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

        match parsed {
            SessionEvent::SessionCompacted { summary, new_file } => {
                assert_eq!(summary, "Discussed task harness implementation.");
                assert_eq!(
                    new_file,
                    PathBuf::from("/kiln/Sessions/test/001-context.md")
                );
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn roundtrip_session_ended() {
        let original = SessionEvent::SessionEnded {
            reason: "User closed session".into(),
        };

        let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
        let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

        match parsed {
            SessionEvent::SessionEnded { reason } => {
                assert_eq!(reason, "User closed session");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn roundtrip_subagent_spawned() {
        let original = SessionEvent::SubagentSpawned {
            id: "sub_abc123".into(),
            prompt: "Find all files related to task harness".into(),
        };

        let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
        let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

        match parsed {
            SessionEvent::SubagentSpawned { id, prompt } => {
                assert_eq!(id, "sub_abc123");
                assert_eq!(prompt, "Find all files related to task harness");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn roundtrip_subagent_completed() {
        let original = SessionEvent::SubagentCompleted {
            id: "sub_abc123".into(),
            result: "Found 5 files.".into(),
        };

        let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
        let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

        match parsed {
            SessionEvent::SubagentCompleted { id, result } => {
                assert_eq!(id, "sub_abc123");
                assert_eq!(result, "Found 5 files.");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn roundtrip_subagent_failed() {
        let original = SessionEvent::SubagentFailed {
            id: "sub_abc123".into(),
            error: "Timeout exceeded".into(),
        };

        let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
        let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

        match parsed {
            SessionEvent::SubagentFailed { id, error } => {
                assert_eq!(id, "sub_abc123");
                assert_eq!(error, "Timeout exceeded");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn roundtrip_custom_event() {
        let original = SessionEvent::Custom {
            name: "my_custom_event".into(),
            payload: json!({"key": "value", "count": 42}),
        };

        let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
        let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

        match parsed {
            SessionEvent::Custom { name, payload } => {
                assert_eq!(name, "my_custom_event");
                assert_eq!(payload["key"], "value");
                assert_eq!(payload["count"], 42);
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn roundtrip_agent_responded_with_content_only() {
        let original = SessionEvent::AgentResponded {
            content: "Just text".into(),
            tool_calls: vec![],
        };

        let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
        let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

        match parsed {
            SessionEvent::AgentResponded {
                content,
                tool_calls,
            } => {
                assert_eq!(content, "Just text");
                assert!(tool_calls.is_empty());
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn roundtrip_agent_responded_with_tool_calls() {
        let original = SessionEvent::AgentResponded {
            content: "I'll help you with that.".into(),
            tool_calls: vec![ToolCall::new("read_file", json!({"path": "/tmp/test.txt"}))],
        };

        let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
        let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

        match parsed {
            SessionEvent::AgentResponded {
                content,
                tool_calls,
            } => {
                assert_eq!(content, "I'll help you with that.");
                assert_eq!(tool_calls.len(), 1);
                assert_eq!(tool_calls[0].name, "read_file");
                assert_eq!(tool_calls[0].args["path"], "/tmp/test.txt");
            }
            _ => panic!("Wrong event type"),
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Error case tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn parse_unknown_event_type() {
        let md = "## 2025-12-14T15:30:45.123 - UnknownEvent\n\nSome content\n\n---\n";
        let result = SessionEvent::from_markdown_block(md);
        assert!(matches!(
            result,
            Err(MarkdownParseError::UnknownEventType(_))
        ));
    }

    #[test]
    fn parse_empty_block() {
        let result = SessionEvent::from_markdown_block("");
        assert!(matches!(result, Err(MarkdownParseError::InvalidHeader(_))));
    }

    #[test]
    fn parse_missing_required_field() {
        // MessageReceived without Participant
        let md = "## 2025-12-14T15:30:45.123 - MessageReceived\n\n> Some content\n\n---\n";
        let result = SessionEvent::from_markdown_block(md);
        assert!(matches!(result, Err(MarkdownParseError::MissingField(_))));
    }
}

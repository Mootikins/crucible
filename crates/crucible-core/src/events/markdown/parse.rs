//! Markdown → event parsers.
//!
//! Per-variant parsers invoked by `SessionEvent::from_markdown_block`
//! plus the header parser and generic field-extraction helpers. All
//! items are `pub(super)` so the tests module can reach them.

use std::path::PathBuf;

use super::{MarkdownParseError, MarkdownParseResult};
use crate::events::{InternalSessionEvent, SessionEvent, SessionEventConfig, ToolCall};

/// Parsed markdown block header.
#[derive(Debug, Clone)]
pub(super) struct ParsedHeader {
    /// Timestamp in milliseconds since UNIX epoch.
    pub(super) timestamp_ms: u64,
    /// Event type name.
    pub(super) event_type: String,
}

/// Parse the header line of a markdown block.
///
/// Expected format: `## 2025-12-14T15:30:45.123 - EventType`
pub(super) fn parse_header(line: &str) -> MarkdownParseResult<ParsedHeader> {
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
pub(super) fn parse_iso_timestamp(s: &str) -> MarkdownParseResult<u64> {
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
pub(super) fn ymd_to_days(year: i64, month: u32, day: u32) -> i64 {
    let mut days: i64 = 0;

    // Count years from 1970
    if year >= 1970 {
        for y in 1970..year {
            days += if super::format::is_leap_year(y) {
                366
            } else {
                365
            };
        }
    } else {
        for y in year..1970 {
            days -= if super::format::is_leap_year(y) {
                366
            } else {
                365
            };
        }
    }

    // Count months in current year
    let days_in_months: [i64; 12] = if super::format::is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    for &day_count in days_in_months.iter().take((month - 1) as usize) {
        days += day_count;
    }

    // Add days (minus 1 since day 1 = 0 extra days)
    days += (day - 1) as i64;

    days
}

// ─────────────────────────────────────────────────────────────────────────────
// Individual event parsers
// ─────────────────────────────────────────────────────────────────────────────

/// Parse MessageReceived event from body.
pub(super) fn parse_message_received(body: &str) -> MarkdownParseResult<SessionEvent> {
    let participant_id = extract_field(body, "**Participant:**")?;
    let content = extract_quoted_content(body);

    Ok(SessionEvent::MessageReceived {
        content,
        participant_id,
    })
}

/// Parse AgentResponded event from body.
pub(super) fn parse_agent_responded(body: &str) -> MarkdownParseResult<SessionEvent> {
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
pub(super) fn parse_agent_thinking(body: &str) -> MarkdownParseResult<SessionEvent> {
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
pub(super) fn parse_tool_called(body: &str) -> MarkdownParseResult<SessionEvent> {
    let name = extract_inline_code_field(body, "**Tool:**")?;
    let args = extract_json_block(body, "**Arguments:**")?;

    Ok(SessionEvent::ToolCalled {
        name,
        args,
        description: None,
        source: None,
    })
}

/// Parse ToolCompleted event from body.
pub(super) fn parse_tool_completed(body: &str) -> MarkdownParseResult<SessionEvent> {
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
pub(super) fn parse_session_started(body: &str) -> MarkdownParseResult<SessionEvent> {
    let session_id = extract_inline_code_field(body, "**Session ID:**")?;
    let folder_str = extract_inline_code_field(body, "**Folder:**")?;
    let folder = PathBuf::from(folder_str);

    Ok(SessionEvent::SessionStarted {
        config: SessionEventConfig::new(session_id).with_folder(folder),
    })
}

/// Parse SessionCompacted event from body.
pub(super) fn parse_session_compacted(body: &str) -> MarkdownParseResult<SessionEvent> {
    let new_file_str = extract_inline_code_field(body, "**New File:**")?;
    let new_file = PathBuf::from(new_file_str);

    let summary = if body.contains("**Summary:**") {
        extract_section_content(body, "**Summary:**")
    } else {
        String::new()
    };

    Ok(SessionEvent::internal(
        InternalSessionEvent::SessionCompacted { summary, new_file },
    ))
}

/// Parse SessionEnded event from body.
pub(super) fn parse_session_ended(body: &str) -> MarkdownParseResult<SessionEvent> {
    let reason = extract_field(body, "**Reason:**")?;
    Ok(SessionEvent::SessionEnded { reason })
}

/// Parse SubagentSpawned event from body.
pub(super) fn parse_subagent_spawned(body: &str) -> MarkdownParseResult<SessionEvent> {
    let id = extract_inline_code_field(body, "**Subagent ID:**")?;

    // Prompt is in a quoted section after **Prompt:**
    let prompt = if body.contains("**Prompt:**") {
        let prompt_section = body.split("**Prompt:**").nth(1).unwrap_or("").trim();
        extract_quoted_content(prompt_section)
    } else {
        String::new()
    };

    Ok(SessionEvent::internal(
        InternalSessionEvent::SubagentSpawned { id, prompt },
    ))
}

/// Parse SubagentCompleted event from body.
pub(super) fn parse_subagent_completed(body: &str) -> MarkdownParseResult<SessionEvent> {
    let id = extract_inline_code_field(body, "**Subagent ID:**")?;

    // Result is in a quoted section after **Result:**
    let result = if body.contains("**Result:**") {
        let result_section = body.split("**Result:**").nth(1).unwrap_or("").trim();
        extract_quoted_content(result_section)
    } else {
        String::new()
    };

    Ok(SessionEvent::internal(
        InternalSessionEvent::SubagentCompleted { id, result },
    ))
}

/// Parse SubagentFailed event from body.
pub(super) fn parse_subagent_failed(body: &str) -> MarkdownParseResult<SessionEvent> {
    let id = extract_inline_code_field(body, "**Subagent ID:**")?;
    let error = extract_field(body, "**Error:**")?;

    Ok(SessionEvent::internal(
        InternalSessionEvent::SubagentFailed { id, error },
    ))
}

pub(super) fn parse_bash_task_spawned(body: &str) -> MarkdownParseResult<SessionEvent> {
    let id = extract_inline_code_field(body, "**Task ID:**")?;
    let command = extract_code_block(body, "**Command:**")?;

    Ok(SessionEvent::internal(
        InternalSessionEvent::BashTaskSpawned { id, command },
    ))
}

pub(super) fn parse_bash_task_completed(body: &str) -> MarkdownParseResult<SessionEvent> {
    let id = extract_inline_code_field(body, "**Task ID:**")?;
    let exit_code_str = extract_field(body, "**Exit Code:**")?;
    let exit_code: i32 =
        exit_code_str
            .parse()
            .map_err(|_| MarkdownParseError::InvalidFieldValue {
                field: "Exit Code".to_string(),
                message: format!("Invalid integer: {}", exit_code_str),
            })?;
    let output = extract_code_block(body, "**Output:**").unwrap_or_default();

    Ok(SessionEvent::internal(
        InternalSessionEvent::BashTaskCompleted {
            id,
            output,
            exit_code,
        },
    ))
}

pub(super) fn parse_bash_task_failed(body: &str) -> MarkdownParseResult<SessionEvent> {
    let id = extract_inline_code_field(body, "**Task ID:**")?;
    let exit_code_str = extract_field(body, "**Exit Code:**")?;
    let exit_code = if exit_code_str == "none" {
        None
    } else {
        Some(
            exit_code_str
                .parse()
                .map_err(|_| MarkdownParseError::InvalidFieldValue {
                    field: "Exit Code".to_string(),
                    message: format!("Invalid integer: {}", exit_code_str),
                })?,
        )
    };
    let error = extract_field(body, "**Error:**")?;

    Ok(SessionEvent::internal(
        InternalSessionEvent::BashTaskFailed {
            id,
            error,
            exit_code,
        },
    ))
}

pub(super) fn parse_background_task_completed(body: &str) -> MarkdownParseResult<SessionEvent> {
    let id = extract_inline_code_field(body, "**Task ID:**")?;
    let kind = extract_field(body, "**Kind:**")?;
    let summary = if body.contains("**Summary:**") {
        let summary_section = body.split("**Summary:**").nth(1).unwrap_or("").trim();
        extract_quoted_content(summary_section)
    } else {
        String::new()
    };

    Ok(SessionEvent::internal(
        InternalSessionEvent::BackgroundTaskCompleted { id, kind, summary },
    ))
}

/// Parse Custom event from body.
pub(super) fn parse_custom_event(body: &str) -> MarkdownParseResult<SessionEvent> {
    let name = extract_inline_code_field(body, "**Event Name:**")?;
    let payload = extract_json_block(body, "**Payload:**")?;

    Ok(SessionEvent::Custom { name, payload })
}

// ─────────────────────────────────────────────────────────────────────────────
// Extraction helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Extract a simple field value from the body.
/// Format: **Label:** value
pub(super) fn extract_field(body: &str, label: &str) -> MarkdownParseResult<String> {
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(stripped) = trimmed.strip_prefix(label) {
            let value = stripped.trim();
            return Ok(value.to_string());
        }
    }
    Err(MarkdownParseError::MissingField(label.to_string()))
}

/// Extract a field value that's in inline code.
/// Format: **Label:** `value`
pub(super) fn extract_inline_code_field(body: &str, label: &str) -> MarkdownParseResult<String> {
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(stripped) = trimmed.strip_prefix(label) {
            let after_label = stripped.trim();
            // Extract value between backticks
            if let Some(stripped_backtick) = after_label.strip_prefix('`') {
                let end = stripped_backtick.find('`').unwrap_or(after_label.len() - 1);
                return Ok(stripped_backtick[..end].to_string());
            }
            return Ok(after_label.to_string());
        }
    }
    Err(MarkdownParseError::MissingField(label.to_string()))
}

/// Extract quoted content (lines starting with > ).
pub(super) fn extract_quoted_content(body: &str) -> String {
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
pub(super) fn extract_section_content(body: &str, label: &str) -> String {
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

pub(super) fn extract_code_block(body: &str, label: &str) -> MarkdownParseResult<String> {
    let after_label = body
        .split(label)
        .nth(1)
        .ok_or_else(|| MarkdownParseError::MissingField(label.to_string()))?;

    let content = after_label
        .lines()
        .skip_while(|line| !line.trim().starts_with("```"))
        .skip(1)
        .take_while(|line| !line.trim().starts_with("```"))
        .collect::<Vec<_>>()
        .join("\n");

    Ok(content)
}

/// Extract result content (may be inline or in code block).
pub(super) fn extract_result_content(body: &str) -> String {
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
pub(super) fn extract_json_block(
    body: &str,
    label: &str,
) -> MarkdownParseResult<serde_json::Value> {
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
pub(super) fn parse_tool_calls_section(body: &str) -> MarkdownParseResult<Vec<ToolCall>> {
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

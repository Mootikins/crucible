//! Event → markdown formatting helpers.
//!
//! Pure functions used by `SessionEvent::to_markdown_block` to render
//! each event variant's body. Timestamp formatting helpers and
//! `quote_content` live here too so they're co-located with the only
//! code that calls them.

use std::path::Path;

use crate::events::ToolCall;

/// Format timestamp from milliseconds since UNIX epoch to ISO 8601.
pub(super) fn format_timestamp(timestamp_ms: Option<u64>) -> String {
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
pub(super) fn days_to_ymd(days: i64) -> (i64, u32, u32) {
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
pub(super) fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

pub(super) fn format_message_received(participant_id: &str, content: &str) -> String {
    let quoted_content = quote_content(content);
    format!(
        "**Participant:** {}\n\n{}\n",
        participant_id, quoted_content
    )
}

pub(super) fn format_agent_responded(content: &str, tool_calls: &[ToolCall]) -> String {
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

pub(super) fn format_agent_thinking(thought: &str) -> String {
    format!("*{}*\n", thought)
}

pub(super) fn format_tool_called(name: &str, args: &serde_json::Value) -> String {
    let args_pretty = serde_json::to_string_pretty(args).unwrap_or_default();
    format!(
        "**Tool:** `{}`\n\n**Arguments:**\n```json\n{}\n```\n",
        name, args_pretty
    )
}

pub(super) fn format_tool_completed(name: &str, result: &str, error: Option<&str>) -> String {
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

pub(super) fn format_session_started(session_id: &str, folder: &Path) -> String {
    format!(
        "**Session ID:** `{}`\n**Folder:** `{}`\n",
        session_id,
        folder.display()
    )
}

pub(super) fn format_delegation_spawned(
    delegation_id: &str,
    prompt: &str,
    parent_session_id: &str,
) -> String {
    let quoted_prompt = quote_content(prompt);
    format!(
        "**Delegation ID:** `{}`\n**Parent Session:** `{}`\n\n**Prompt:**\n{}\n",
        delegation_id, parent_session_id, quoted_prompt
    )
}

pub(super) fn format_delegation_completed(
    delegation_id: &str,
    result_summary: &str,
    parent_session_id: &str,
) -> String {
    format!(
        "**Delegation ID:** `{}`\n**Parent Session:** `{}`\n\n**Result:**\n{}\n",
        delegation_id,
        parent_session_id,
        quote_content(result_summary)
    )
}

pub(super) fn format_delegation_failed(
    delegation_id: &str,
    error: &str,
    parent_session_id: &str,
) -> String {
    format!(
        "**Delegation ID:** `{}`\n**Parent Session:** `{}`\n\n**Error:** {}\n",
        delegation_id, parent_session_id, error
    )
}

pub(super) fn format_session_ended(reason: &str) -> String {
    format!("**Reason:** {}\n", reason)
}

pub(super) fn format_custom_event(name: &str, payload: &serde_json::Value) -> String {
    let payload_pretty = serde_json::to_string_pretty(payload).unwrap_or_default();
    format!(
        "**Event Name:** `{}`\n\n**Payload:**\n```json\n{}\n```\n",
        name, payload_pretty
    )
}

/// Quote content as a blockquote, handling multiline.
pub(super) fn quote_content(content: &str) -> String {
    if content.is_empty() {
        return String::new();
    }

    content
        .lines()
        .map(|line| format!("> {}", line))
        .collect::<Vec<_>>()
        .join("\n")
}

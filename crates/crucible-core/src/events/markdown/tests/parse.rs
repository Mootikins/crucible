use super::TEST_TIMESTAMP_MS;
use crate::events::markdown::parse::{
    extract_field, extract_inline_code_field, extract_quoted_content, parse_header,
    parse_iso_timestamp, ymd_to_days,
};
use crate::events::markdown::MarkdownParseError;
use crate::events::SessionEvent;

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

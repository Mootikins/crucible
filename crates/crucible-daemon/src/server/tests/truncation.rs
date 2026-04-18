// --- Golden tests for UTF-8–safe truncation logic ---
//
// These capture the current behavior of the truncation pattern used in
// `handle_grep_request` (the `floor_char_boundary(100)` call). The helper
// below mirrors that inline logic so we can test it in isolation.

/// Mirror of the inline truncation logic in `handle_grep_request`.
fn truncate_utf8_safe(line: &str, max_bytes: usize) -> String {
    if line.len() > max_bytes {
        let end = line.floor_char_boundary(max_bytes);
        format!("{}...", &line[..end])
    } else {
        line.to_string()
    }
}

#[test]
fn truncation_ascii_under_limit() {
    let line = "a".repeat(50);
    let result = truncate_utf8_safe(&line, 100);
    assert_eq!(
        result, line,
        "under-limit ASCII should be returned verbatim"
    );
}

#[test]
fn truncation_ascii_exactly_at_limit() {
    let line = "a".repeat(100);
    let result = truncate_utf8_safe(&line, 100);
    assert_eq!(
        result, line,
        "exactly-at-limit ASCII should be returned verbatim (no trailing ...)"
    );
}

#[test]
fn truncation_ascii_over_limit() {
    let line = "a".repeat(120);
    let result = truncate_utf8_safe(&line, 100);
    assert_eq!(result.len(), 103, "100 chars + 3 for '...'");
    assert!(result.ends_with("..."));
    assert_eq!(&result[..100], &"a".repeat(100));
}

#[test]
fn truncation_multibyte_2byte_boundary() {
    // 'é' is U+00E9 → 2 bytes in UTF-8. Placing it at byte 99-100
    // means the char straddles the boundary. floor_char_boundary(100)
    // should round down to 99 (start of the char).
    let mut line = "a".repeat(99);
    line.push('é'); // bytes 99-100 (total 101)
    let result = truncate_utf8_safe(&line, 100);
    // GOLDEN: captures current behavior — floor rounds to 99
    assert_eq!(&result[..99], &"a".repeat(99));
    assert!(result.ends_with("..."));
    assert_eq!(result.len(), 99 + 3);
}

#[test]
fn truncation_cjk_3byte_boundary() {
    // Each CJK char ('中') is 3 bytes. 33 chars = 99 bytes. 34 chars = 102 bytes.
    let line: String = "中".repeat(34);
    assert_eq!(line.len(), 102);
    let result = truncate_utf8_safe(&line, 100);
    // GOLDEN: captures current behavior — floor rounds 100 down to 99
    // (byte 99 is mid-char), keeping 33 CJK chars (99 bytes).
    let expected_prefix: String = "中".repeat(33);
    assert!(result.starts_with(&expected_prefix));
    assert!(result.ends_with("..."));
    assert_eq!(result.len(), 99 + 3);
}

#[test]
fn truncation_emoji_4byte_boundary() {
    // 🚀 is U+1F680 → 4 bytes in UTF-8.
    // 97 ASCII bytes + 4-byte emoji = 101 bytes total → over limit.
    // floor_char_boundary(100) rounds down to 97 (start of the emoji).
    let mut line = "a".repeat(97);
    line.push('🚀'); // bytes 97-100 (total 101)
    assert_eq!(line.len(), 101);
    let result = truncate_utf8_safe(&line, 100);
    // GOLDEN: captures current behavior — floor rounds to 97
    assert_eq!(&result[..97], &"a".repeat(97));
    assert!(result.ends_with("..."));
    assert_eq!(result.len(), 97 + 3);
}

#[test]
fn truncation_empty_line() {
    let result = truncate_utf8_safe("", 100);
    assert_eq!(result, "", "empty string should be returned verbatim");
}

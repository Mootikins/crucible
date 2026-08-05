//! Tool-card rendering and result-summary tests.
//!
//! Split out of `tool_render.rs` for the 1500-line file-size gate, and
//! attached with `#[path]` rather than moved into `tui/oil/tests/` because
//! `summary_key` and `collapse_result` are private to the module under test.

use super::*;
use crate::tui::oil::viewport_cache::ToolSourceDisplay;
use crucible_oil::render::render_to_plain_text;
use std::sync::Arc;
use test_case::test_case;

fn test_tool(name: &str, args: &str, complete: bool) -> CachedToolCall {
    let mut tool = CachedToolCall::new("tool-1", name, args);
    if complete {
        tool.mark_complete();
    }
    tool
}

fn test_tool_with_output(name: &str, args: &str, output: &str, complete: bool) -> CachedToolCall {
    let mut tool = CachedToolCall::new("tool-1", name, args);
    tool.append_output(output);
    if complete {
        tool.mark_complete();
    }
    tool
}

#[test]
fn format_tool_args_empty() {
    assert_eq!(format_tool_args(""), "");
    assert_eq!(format_tool_args("{}"), "");
}

#[test]
fn format_tool_args_json_object() {
    let args = r#"{"path": "foo.txt", "content": "hello"}"#;
    let result = format_tool_args(args);
    assert!(result.contains("path="));
    assert!(result.contains("content="));
}

#[test]
fn format_tool_args_truncates_long_values() {
    let args =
        r#"{"content": "this is a very long string that should be truncated at some point"}"#;
    let result = format_tool_args(args);
    assert!(result.contains("…"));
}

#[test]
fn summarize_tool_result_read_file() {
    let result = summarize_tool_result("mcp_read", "line1\nline2\nline3");
    assert!(result.is_some());
    assert!(result.unwrap().contains("lines"));
}

#[test]
fn summarize_tool_result_glob() {
    let result = summarize_tool_result("mcp_glob", "file1.rs\nfile2.rs\nfile3.rs");
    assert_eq!(result, Some("3 files".to_string()));
}

#[test]
fn summarize_tool_result_grep() {
    let result = summarize_tool_result("mcp_grep", "file.rs:10: match1\nfile.rs:20: match2");
    assert_eq!(result, Some("2 matches".to_string()));
}

#[test]
fn summarize_tool_result_edit_success() {
    let result = summarize_tool_result("mcp_edit", "Edit applied successfully");
    assert_eq!(result, Some("applied".to_string()));
}

#[test]
fn summarize_tool_result_bash_short() {
    let result = summarize_tool_result("mcp_bash", "OK");
    assert_eq!(result, Some("OK".to_string()));
}

#[test]
fn summarize_tool_result_bash_long_returns_none() {
    let result = summarize_tool_result("mcp_bash", "line1\nline2\nline3\nline4");
    assert!(result.is_none());
}

/// Divergence A4: a delegated card's name is the agent's prose `title`
/// run through `humanize_tool_title`, so the summary table has to answer
/// to the humanized spelling as well as the snake_case one. The prose
/// forms carry no underscore at all, which is why a snake_case-only
/// normalizer would not have been enough.
///
/// The namespaced spelling `mcp__crucible__read_file` is deliberately
/// absent — see [`a_namespaced_tool_reaches_no_summary_arm`]. Nothing is
/// lost: a delegated agent's call is humanized to `Read`/`Read File`
/// before it ever reaches here.
#[test_case("read_file", "3 lines"; "internal_snake_case")]
#[test_case("mcp_read", "3 lines"; "mcp_prefixed")]
#[test_case("Read File", "3 lines"; "acp_title_two_words")]
#[test_case("Read", "3 lines"; "acp_title_one_word")]
fn every_spelling_of_read_summarizes_the_same(name: &str, expected: &str) {
    assert_eq!(
        summarize_tool_result(name, "line1\nline2\nline3"),
        Some(expected.to_string()),
        "`{name}` did not reach the read arm of the summary table"
    );
}

#[test_case("glob", "2 files"; "glob_internal")]
#[test_case("Glob", "2 files"; "glob_acp_title")]
fn every_spelling_of_glob_summarizes_the_same(name: &str, expected: &str) {
    assert_eq!(
        summarize_tool_result(name, "a.rs\nb.rs"),
        Some(expected.to_string())
    );
}

#[test_case("grep", "2 matches"; "grep_internal")]
#[test_case("Grep", "2 matches"; "grep_acp_title")]
fn every_spelling_of_grep_summarizes_the_same(name: &str, expected: &str) {
    assert_eq!(
        summarize_tool_result(name, "a.rs:1: x\nb.rs:2: y"),
        Some(expected.to_string())
    );
}

#[test_case("edit"; "edit_internal")]
#[test_case("mcp_edit"; "edit_mcp")]
#[test_case("Edit"; "edit_acp_title")]
fn every_spelling_of_edit_summarizes_the_same(name: &str) {
    assert_eq!(
        summarize_tool_result(name, "Edit applied successfully"),
        Some("applied".to_string())
    );
}

/// The normalization must not *widen* the table. `edit_file` and
/// `write_file` were outside the `Edit`/`Write` arms before A4 and stay
/// outside them: humanizing maps them to `Edit File`/`Write File`, which
/// no arm lists. Both tools answer with one short line that
/// `collapse_result` returns verbatim, so nothing is lost.
#[test_case("edit_file", "Edit applied successfully"; "edit_file_is_not_edit")]
#[test_case("write_file", "written successfully"; "write_file_is_not_write")]
fn compound_internal_names_stay_out_of_the_short_arms(name: &str, result: &str) {
    assert_eq!(summarize_tool_result(name, result), None);
}

/// A `__` in the name is a foreign namespace — `mcp__<server>__<tool>`,
/// `plugin_<name>__<tool>`. That tool's `write` is somebody else's `write`,
/// and `collapse_result`'s `Write` arm answers *unconditionally*: it
/// replaces the whole result with the literal word `written`. Normalizing a
/// namespaced name into the internal table therefore destroys the output of
/// every MCP or plugin tool whose trailing segment happens to be `write` or
/// `edit`, on every card, for users who never touch ACP.
#[test_case("mcp__crucible__write"; "mcp_crucible_write")]
#[test_case("mcp__crucible__edit"; "mcp_crucible_edit")]
#[test_case("mcp__fs__write"; "mcp_third_party_write")]
#[test_case("plugin_foo__write"; "plugin_write")]
#[test_case("plugin_foo__edit"; "plugin_edit")]
fn a_namespaced_tool_keeps_its_whole_result(name: &str) {
    let long = "first line of real output\n\
                second line the user needs to see\n\
                third line, well past the sixty-character short-result branch";
    assert_eq!(
        collapse_result(name, long, None),
        None,
        "`{name}` had its result replaced by a one-word summary"
    );
}

/// Same rule on the other table. The derived summaries are not
/// word-for-word destructive like `collapse_result`, but they still hide a
/// foreign tool's output behind a count invented for Crucible's own tools.
#[test_case("mcp__crucible__read_file", "alpha\nbeta\ngamma"; "namespaced_read")]
#[test_case("mcp__fs__glob", "a.rs\nb.rs"; "namespaced_glob")]
#[test_case("plugin_foo__write", "Report written to the log\nand here is what it says"; "namespaced_write")]
fn a_namespaced_tool_reaches_no_summary_arm(name: &str, result: &str) {
    assert_eq!(
        summarize_tool_result(name, result),
        None,
        "`{name}` was summarized as if it were an internal tool"
    );
}

/// The other half of the table: the long-result fallback in
/// `collapse_result` keys on the same identity.
#[test_case("write", Some("written"); "write_internal")]
#[test_case("Write", Some("written"); "write_acp_title")]
#[test_case("mcp_write", Some("written"); "write_mcp")]
#[test_case("edit", Some("applied"); "edit_internal")]
#[test_case("Edit", Some("applied"); "edit_acp_title")]
#[test_case("read_file", None; "read_has_no_long_fallback")]
fn collapse_result_keys_on_the_humanized_name(name: &str, expected: Option<&str>) {
    let long = "a line that is definitely longer than sixty characters so the \
                short-result branch cannot claim it first";
    assert_eq!(
        collapse_result(name, long, None),
        expected.map(str::to_string)
    );
}

#[test]
fn format_output_tail_short_output() {
    let node = format_output_tail("line1\nline2", "  ", 80);
    let plain = render_to_plain_text(&node, 80);
    assert!(plain.contains("line1"));
    assert!(plain.contains("line2"));
    assert!(!plain.contains("…"));
}

#[test]
fn format_output_tail_truncates_long_output() {
    let node = format_output_tail("line1\nline2\nline3\nline4\nline5", "  ", 80);
    let plain = render_to_plain_text(&node, 80);
    assert!(
        plain.contains("(2 more lines)"),
        "Should show count: {:?}",
        plain
    );
    assert!(plain.contains("line5"));
}

#[test]
fn format_output_tail_count_line_has_bar_prefix() {
    let node = format_output_tail("a\nb\nc\nd\ne\nf", "  ", 80);
    let plain = render_to_plain_text(&node, 80);
    let first_line = plain.lines().next().unwrap();
    assert!(
        first_line.contains("│"),
        "Count line should have bar: {:?}",
        first_line
    );
    assert!(
        first_line.contains("(3 more lines)"),
        "Should show count: {:?}",
        first_line
    );
    assert!(
        !first_line.contains("…"),
        "Should not have ellipsis, just parenthetical: {:?}",
        first_line
    );
}

#[test]
fn tool_result_bounded_overflow_indicator() {
    let long_output = (1..=10)
        .map(|i| format!("line{}", i))
        .collect::<Vec<_>>()
        .join("\n");
    let node = format_output_tail(&long_output, "   ", 80);
    let plain = render_to_plain_text(&node, 80);
    assert!(
        plain.contains("(7 more lines)"),
        "Long output should show overflow indicator: {:?}",
        plain
    );
    assert!(
        plain.contains("line8") && plain.contains("line9") && plain.contains("line10"),
        "Should show last 3 lines: {:?}",
        plain
    );
}

#[test]
fn tool_result_short_no_cap() {
    let short_output = "line1\nline2\nline3";
    let node = format_output_tail(short_output, "   ", 80);
    let plain = render_to_plain_text(&node, 80);
    assert!(
        !plain.contains("more lines"),
        "Short output should not show indicator: {:?}",
        plain
    );
    assert!(
        plain.contains("line1") && plain.contains("line2") && plain.contains("line3"),
        "All lines should be visible: {:?}",
        plain
    );
}

#[test]
fn summarize_read_tool_preserves_closing_bracket() {
    let result = "[Directory Context: /home/user/project]";
    let summary = summarize_tool_result("mcp_read", result);
    assert!(
        summary.as_ref().is_some_and(|s| s.ends_with(']')),
        "Should preserve closing bracket: {:?}",
        summary
    );
}

#[test]
fn render_tool_call_complete() {
    let tool = test_tool_with_output("mcp_read", r#"{"path": "test.rs"}"#, "content", true);
    let node = tool.render_compact(80);
    let plain = render_to_plain_text(&node, 80);
    assert!(plain.contains("✓"), "Should show checkmark: {:?}", plain);
    assert!(
        plain.contains("Read"),
        "Should show tool name (title-cased, without mcp_ prefix): {:?}",
        plain
    );
}

#[test]
fn render_complete_includes_diff_body_when_diffs_present() {
    use crucible_core::types::acp::FileDiff;

    let mut tool = test_tool_with_output(
        "edit",
        r#"{"path": "src/foo.rs"}"#,
        r#"{"success": true}"#,
        true,
    );
    tool.diffs = vec![FileDiff::from_contents(
        "src/foo.rs",
        Some("OLD_LINE\n".to_string()),
        "CHANGED_LINE\n".to_string(),
    )];
    let node = tool.render_compact(100);
    let plain = render_to_plain_text(&node, 100);

    assert!(
        plain.contains("Edit"),
        "Should still show tool header: {:?}",
        plain
    );
    assert!(
        plain.contains("src/foo.rs"),
        "Diff header should show path: {:?}",
        plain
    );
    assert!(
        plain.contains("CHANGED_LINE"),
        "Diff body should show added line: {:?}",
        plain
    );
}

#[test]
fn render_complete_hides_diff_body_when_show_diffs_off() {
    use crucible_core::types::acp::FileDiff;

    let mut tool = test_tool_with_output(
        "edit",
        r#"{"path": "src/foo.rs"}"#,
        r#"{"success": true}"#,
        true,
    );
    tool.diffs = vec![FileDiff::from_contents(
        "src/foo.rs",
        Some("OLD_LINE\n".to_string()),
        "CHANGED_LINE\n".to_string(),
    )];

    let on = tool.render_compact_with(0, 100, true);
    let on_plain = render_to_plain_text(&on, 100);
    assert!(
        on_plain.contains("CHANGED_LINE"),
        "show_diffs=true must render diff body: {:?}",
        on_plain
    );

    let off = tool.render_compact_with(0, 100, false);
    let off_plain = render_to_plain_text(&off, 100);
    assert!(
        off_plain.contains("Edit"),
        "show_diffs=false must still render the tool header: {:?}",
        off_plain
    );
    assert!(
        !off_plain.contains("CHANGED_LINE"),
        "show_diffs=false must omit diff body: {:?}",
        off_plain
    );
    assert!(
        !off_plain.contains("OLD_LINE"),
        "show_diffs=false must omit removed line text: {:?}",
        off_plain
    );
}

#[test]
fn render_complete_with_multiple_diffs_renders_all() {
    use crucible_core::types::acp::FileDiff;

    let mut tool = test_tool_with_output("edit", r#"{}"#, r#"{"ok":true}"#, true);
    tool.diffs = vec![
        FileDiff::from_contents("a.rs", Some("X\n".into()), "ALPHA_NEW\n".to_string()),
        FileDiff::from_contents("b.rs", Some("Y\n".into()), "BETA_NEW\n".to_string()),
    ];
    let node = tool.render_compact(100);
    let plain = render_to_plain_text(&node, 100);

    assert!(
        plain.contains("ALPHA_NEW"),
        "first diff visible: {:?}",
        plain
    );
    assert!(
        plain.contains("BETA_NEW"),
        "second diff visible: {:?}",
        plain
    );
    assert!(
        plain.contains("a.rs") && plain.contains("b.rs"),
        "both paths: {:?}",
        plain
    );
}

#[test]
fn render_running_omits_diff_body_even_if_diffs_present() {
    use crucible_core::types::acp::FileDiff;

    let mut tool = test_tool("edit", r#"{"path": "in_flight.rs"}"#, false);
    tool.diffs = vec![FileDiff::from_contents(
        "in_flight.rs",
        Some(String::new()),
        "PARTIAL_OUTPUT\n".to_string(),
    )];
    let node = tool.render_compact(100);
    let plain = render_to_plain_text(&node, 100);

    assert!(
        !plain.contains("PARTIAL_OUTPUT"),
        "in-flight tool should not render diff content yet: {:?}",
        plain
    );
}

#[test]
fn render_tool_call_in_progress() {
    let tool = test_tool("mcp_bash", r#"{"command": "ls"}"#, false);
    let node = tool.render_compact(80);
    let plain = render_to_plain_text(&node, 80);
    assert!(
        plain.contains("Bash"),
        "Should show tool name (title-cased, without mcp_ prefix): {:?}",
        plain
    );
}

#[test]
fn render_tool_call_with_error() {
    let mut tool = test_tool("mcp_bash", r#"{"command": "false"}"#, false);
    tool.set_error("Command failed with exit code 1".to_string());
    let node = tool.render_compact(80);
    let plain = render_to_plain_text(&node, 80);
    assert!(plain.contains("✗"), "Should show error icon: {:?}", plain);
    assert!(
        plain.contains("Command failed"),
        "Should show error message: {:?}",
        plain
    );
}

#[test]
fn render_tool_call_collapses_short_result() {
    let tool = test_tool_with_output("unknown_tool", "{}", "OK", true);
    let node = tool.render_compact(80);
    let plain = render_to_plain_text(&node, 80);
    assert!(
        plain.contains("→ OK"),
        "Short result should collapse to one line: {:?}",
        plain
    );
}

#[test]
fn format_tool_args_unicode_truncation() {
    let long_jp = "日本語".repeat(20);
    let args = format!(r#"{{"content": "{}"}}"#, long_jp);
    let result = format_tool_args(&args);
    assert!(result.contains("…"), "Should truncate: {}", result);
    assert!(!result.is_empty());
}

#[test]
fn unwrap_json_result_plain_json_string() {
    let json_string = r#""total 528\ndrwxr-xr-x""#;
    let result = unwrap_json_result(json_string);
    assert_eq!(result, "total 528\ndrwxr-xr-x");
    assert!(!result.starts_with('"'));
}

#[test]
fn unwrap_json_result_wrapped_object() {
    let json_obj = r#"{"result": "file contents"}"#;
    let result = unwrap_json_result(json_obj);
    assert_eq!(result, "file contents");
}

#[test]
fn unwrap_json_result_plain_text() {
    let plain = "just plain text";
    let result = unwrap_json_result(plain);
    assert_eq!(result, "just plain text");
}

#[test]
fn tool_result_with_json_encoded_newlines() {
    let json_result = r#""line1\nline2\nline3""#;
    let tool = test_tool_with_output("mcp_bash", r#"{"command": "ls"}"#, json_result, true);
    let node = tool.render_compact(80);
    let plain = render_to_plain_text(&node, 80);
    assert!(
        plain.contains("│ line1") || plain.contains("→"),
        "Should decode escaped newlines and show lines: {:?}",
        plain
    );
    assert!(
        !plain.contains(r#"\n"#),
        "Should not show literal backslash-n: {:?}",
        plain
    );
}

#[test]
fn tool_with_multiline_output_no_blank_line() {
    let tool = test_tool_with_output(
        "mcp_bash",
        r#"{"command": "ls"}"#,
        "line1\nline2\nline3",
        true,
    );
    let node = tool.render_compact(80);
    let plain = render_to_plain_text(&node, 80);
    let lines: Vec<&str> = plain.lines().collect();

    assert!(lines[0].contains("✓"), "First line should have checkmark");
    if lines.len() > 1 {
        assert!(
            !lines[1].trim().is_empty(),
            "No blank line between header and output: {:?}",
            lines
        );
    }
}

#[test]
fn format_output_tail_no_leading_blank() {
    let node = format_output_tail("line1\nline2\nline3", "   ", 80);
    let plain = render_to_plain_text(&node, 80);
    let lines: Vec<&str> = plain.lines().collect();
    assert!(
        !lines.is_empty() && !lines[0].trim().is_empty(),
        "First line should not be blank: {:?}",
        lines
    );
}

#[test]
fn format_tool_result_no_leading_blank() {
    let node = format_tool_result("mcp_bash", "line1\nline2\nline3", 80);
    let plain = render_to_plain_text(&node, 80);
    let lines: Vec<&str> = plain.lines().collect();
    assert!(
        !lines.is_empty() && !lines[0].trim().is_empty(),
        "First line should not be blank: {:?}",
        lines
    );
}
#[test]
fn error_message_uses_terminal_width_not_hardcoded() {
    // Test that error messages respect terminal width, not hardcoded 50 chars
    let mut tool = test_tool("mcp_bash", r#"{"command": "test"}"#, false);
    let long_error = "a".repeat(120); // 120-char error message
    tool.set_error(long_error.clone());

    // Render at width=120 (wide terminal)
    let node = tool.render_compact(80);
    let plain = render_to_plain_text(&node, 120);

    // The full error should be visible at width=120
    // With the bug (hardcoded 50), the error is truncated to 50 chars + ellipsis
    // With the fix, it should use the terminal width (120) and show the full error
    // Assert: the full 120-char error appears in output (not truncated to 50)
    assert!(
        plain.contains(&"a".repeat(100)),
        "Full error should be visible at width=120 (not truncated to 50): {}",
        plain
    );
}

#[test]
fn error_message_fits_within_terminal_width() {
    // Test that error messages are not truncated to hardcoded 50 at width=80
    let mut tool = test_tool("mcp_bash", r#"{"command": "test"}"#, false);
    let long_error = "Connection failed: ".to_string() + &"x".repeat(100);
    tool.set_error(long_error.clone());

    // Render at width=80
    let node = tool.render_compact(80);
    let plain = render_to_plain_text(&node, 80);

    // The error should NOT be truncated to hardcoded 50 chars
    // At width=80, we have room for more than 50 chars
    // So the error should show more than 50 chars (or the full error if it fits)
    // With the bug, it's truncated to 50 + ellipsis
    // With the fix, it should use the terminal width (80)
    assert!(
        plain.contains(&"x".repeat(50)),
        "Error should show more than 50 chars at width=80 (not hardcoded truncation): {}",
        plain
    );
}

#[test]
fn error_with_cjk_no_panic() {
    // Test that CJK error messages don't panic and are not truncated to hardcoded 50
    let mut tool = test_tool("mcp_bash", r#"{"command": "test"}"#, false);
    let cjk_error = "错误：连接超时，请检查网络设置并重试操作。这是一个很长的错误消息用于测试。";
    tool.set_error(cjk_error.to_string());

    // Render at width=80 — should not panic
    let node = tool.render_compact(80);
    let plain = render_to_plain_text(&node, 80);

    // Verify every line fits within width
    for line in plain.lines() {
        let width = crucible_oil::ansi::visible_width(line);
        assert!(
            width <= 80,
            "CJK line exceeds terminal width (80): {} chars: {}",
            width,
            line
        );
    }

    // Verify the full CJK error is visible (not truncated to hardcoded 50)
    // Extract the error portion (after the arrow) and check it's longer than 50 chars
    let error_line = plain.lines().find(|l: &&str| l.contains("→")).unwrap_or("");
    let error_portion = error_line.split("→").nth(1).unwrap_or("");
    let error_visible_width = crucible_oil::ansi::visible_width(error_portion);
    assert!(
        error_visible_width > 50,
        "CJK error should show more than 50 chars (not hardcoded truncation). Got width: {}: {}",
        error_visible_width,
        plain
    );
}

#[test]
fn short_error_fully_visible_at_wide_terminal() {
    let mut tool = test_tool("mcp_bash", r#"{"command": "test"}"#, false);
    let error = "Connection refused: port 8080 is already in use by another process running on this machine";
    tool.set_error(error.to_string());

    let node = tool.render_compact(80);
    let plain = render_to_plain_text(&node, 120);

    assert!(
        plain.contains("Connection refused"),
        "Error start should be visible: {}",
        plain
    );
    assert!(
        plain.contains("running on this machine"),
        "Error end should be visible (not truncated to 50): {}",
        plain
    );
}

#[test_case("read_file", "", "" ; "empty string")]
#[test_case("read_file", "{}", "" ; "empty object")]
#[test_case("read_file", r#"{"path": "src/lib.rs"}"#, "src/lib.rs" ; "path key")]
#[test_case("Read", r#"{"filePath": "/home/user/test.rs"}"#, "/home/user/test.rs" ; "camelCase file path")]
#[test_case("bash", r#"{"command": "ls -la", "timeout": 5000}"#, "ls -la" ; "command key")]
#[test_case("semantic_search", r#"{"query": "auth patterns", "limit": 10}"#, "auth patterns" ; "query key")]
#[test_case("read_file", r#"{"limit": 10, "path": "src/main.rs"}"#, "src/main.rs" ; "priority key over first key")]
#[test_case("clone", r#"{"repo": "crucible"}"#, "crucible" ; "fallback to first value")]
#[test_case("counter", r#"{"count": 42}"#, "42" ; "non-string value")]
fn format_primary_arg_extracts_expected(tool: &str, args: &str, expected: &str) {
    assert_eq!(format_primary_arg_for(tool, args), expected);
}

#[test]
fn format_primary_arg_returns_full_value_no_truncation() {
    // Truncation is the renderer's job (width-aware); format_primary_arg
    // just normalizes to a single line.
    let long_path = "a".repeat(60);
    let args = format!(r#"{{"path": "{}"}}"#, long_path);
    let result = format_primary_arg_for("read_file", &args);
    assert_eq!(result, long_path);
    assert!(!result.contains("…"));
}

#[test]
fn fit_arg_to_width_passes_through_when_short() {
    assert_eq!(fit_arg_to_width("hello", 80), "hello");
}

#[test]
fn fit_arg_to_width_truncates_with_ellipsis() {
    let long = "abcdefghijklmnopqrstuvwxyz";
    let result = fit_arg_to_width(long, 15);
    assert!(
        result.ends_with('…'),
        "should end with ellipsis: {result:?}"
    );
    assert!(crucible_oil::ansi::visible_width(&result) <= 15);
}

#[test]
fn fit_arg_to_width_returns_empty_when_budget_zero() {
    // Strict width contract: budget=0 means caller has no room → drop.
    assert_eq!(fit_arg_to_width("a long string here", 0), "");
}

#[test]
fn fit_arg_to_width_single_col_returns_ellipsis() {
    let result = fit_arg_to_width("a long string here", 1);
    assert_eq!(result, "…");
    assert_eq!(crucible_oil::ansi::visible_width(&result), 1);
}

#[test]
fn fit_arg_to_width_narrow_budget_does_not_overflow() {
    for budget in 2..=20 {
        let result = fit_arg_to_width("abcdefghijklmnopqrstuvwxyz", budget);
        assert!(
            crucible_oil::ansi::visible_width(&result) <= budget,
            "budget={} produced width={} for {result:?}",
            budget,
            crucible_oil::ansi::visible_width(&result)
        );
    }
}

#[test]
fn fit_arg_to_width_empty() {
    assert_eq!(fit_arg_to_width("", 80), "");
}

#[test]
fn compact_read_file_shows_path() {
    let tool = test_tool_with_output("mcp_read", r#"{"path": "src/lib.rs"}"#, "content", true);
    let node = tool.render_compact(80);
    let plain = render_to_plain_text(&node, 80);
    assert!(plain.contains("✓"), "Should show checkmark: {:?}", plain);
    assert!(plain.contains("Read"), "Should show tool name: {:?}", plain);
    assert!(
        plain.contains("src/lib.rs"),
        "Should show path inline: {:?}",
        plain
    );
    assert!(
        !plain.contains("path="),
        "Should NOT show key=value format: {:?}",
        plain
    );
    assert!(
        !plain.contains('(') || !plain.contains(')'),
        "Should NOT have parens around args: {:?}",
        plain
    );
}

#[test]
fn compact_bash_shows_command() {
    let tool = test_tool_with_output("mcp_bash", r#"{"command": "ls -la"}"#, "file1\nfile2", true);
    let node = tool.render_compact(80);
    let plain = render_to_plain_text(&node, 80);
    assert!(plain.contains("Bash"), "Should show tool name: {:?}", plain);
    assert!(
        plain.contains("ls -la"),
        "Should show command inline: {:?}",
        plain
    );
    assert!(
        !plain.contains("command="),
        "Should NOT show key=value: {:?}",
        plain
    );
}

#[test]
fn compact_no_args_no_parens() {
    let tool = test_tool_with_output("get_kiln_info", "{}", "kiln data", true);
    let node = tool.render_compact(80);
    let plain = render_to_plain_text(&node, 80);
    assert!(
        plain.contains("Get Kiln Info"),
        "Should show tool name: {:?}",
        plain
    );
    assert!(
        !plain.contains("()"),
        "Should NOT have empty parens: {:?}",
        plain
    );
}

#[test]
fn bash_command_uses_full_terminal_width() {
    // Long command that fits in 120 cols but not in the old hardcoded 40-char cap.
    let cmd = "cd /home/moot/crucible && git log --oneline -n 20 | head -50";
    let args = format!(r#"{{"command": "{}"}}"#, cmd);
    let tool = test_tool("bash", &args, false);
    let node = tool.render_compact(120);
    let plain = render_to_plain_text(&node, 120);
    assert!(
        plain.contains("git log --oneline -n 20"),
        "wide terminal should show full command, not 40-char truncation: {:?}",
        plain
    );
}

#[test]
fn bash_command_truncation_respects_width_not_hardcoded() {
    // Long command at width=80: must truncate to fit, but show MORE than the
    // old hardcoded 40 chars.
    let cmd = "x".repeat(200);
    let args = format!(r#"{{"command": "{}"}}"#, cmd);
    let tool = test_tool("bash", &args, false);
    let node = tool.render_compact(80);
    let plain = render_to_plain_text(&node, 80);

    for line in plain.lines() {
        let w = crucible_oil::ansi::visible_width(line);
        assert!(
            w <= 80,
            "line wider than terminal width: {} - {:?}",
            w,
            line
        );
    }

    let header_line = plain
        .lines()
        .find(|l| l.contains("Bash"))
        .expect("header line");
    let visible = crucible_oil::ansi::visible_width(header_line);
    assert!(
        visible > 50,
        "at width=80 the header should fill more than 50 chars (old cap was 40): {} - {:?}",
        visible,
        header_line
    );
}

#[test]
fn tool_header_respects_narrow_terminal_width() {
    // Regression for the MIN_ARG_WIDTH=10 floor that previously overrode
    // the caller's budget on narrow terminals, blowing the header past
    // the terminal width. Sweep widths from a 24-col mobile terminal up
    // to a typical 80-col split pane.
    let cmd = "x".repeat(120);
    let args = format!(r#"{{"command": "{}"}}"#, cmd);
    let tool = test_tool("bash", &args, false);
    for width in [24usize, 30, 40, 50, 60, 80] {
        let node = tool.render_compact(width);
        let plain = render_to_plain_text(&node, width);
        for line in plain.lines() {
            let w = crucible_oil::ansi::visible_width(line);
            assert!(
                w <= width,
                "width={} produced line of width {}: {:?}",
                width,
                w,
                line
            );
        }
    }
}

#[test_case(ToolSourceDisplay::Core, "[core]", false ; "core renders no badge")]
#[test_case(ToolSourceDisplay::Crucible, "[crucible]", false ; "crucible renders no badge")]
#[test_case(ToolSourceDisplay::Mcp { server: Arc::from("gmail") }, "[mcp:gmail]", true ; "mcp renders badge")]
#[test_case(ToolSourceDisplay::Plugin { name: Arc::from("oci") }, "[plugin:oci]", true ; "plugin renders badge")]
#[test_case(ToolSourceDisplay::Acp { agent: Some(Arc::from("claude")) }, "[acp:claude]", true ; "acp renders badge naming the agent")]
#[test_case(ToolSourceDisplay::Acp { agent: None }, "[acp]", true ; "acp from a pre-badge recording renders an anonymous badge")]
fn source_badge_visibility(source: ToolSourceDisplay, badge: &str, should_show: bool) {
    let mut tool = test_tool_with_output("bash", r#"{"command": "ls"}"#, "ok", true);
    tool.source = Some(source);
    let node = tool.render_compact(80);
    let plain = render_to_plain_text(&node, 80);
    assert_eq!(
        plain.contains(badge),
        should_show,
        "badge {badge} visibility mismatch: {plain:?}"
    );
}

#[test]
fn summarize_read_file_counts_lines_correctly() {
    // read_file results should show actual line count, not "1 lines"
    let content = "line1\nline2\nline3\nline4\nline5";
    let result = summarize_tool_result("read_file", content);
    assert_eq!(result, Some("5 lines".to_string()));
}

#[test]
fn summarize_read_file_does_not_extract_spill_reference_as_summary() {
    // If a spill reference somehow gets to summarize, it should not be shown as-is
    let spill_ref = "[200 lines, 15KB — full output in $CRU_SESSION_DIR/tools/read-file-1.txt]";
    let result = summarize_tool_result("read_file", spill_ref);
    // Should not contain the full spill path
    assert!(
        !result
            .as_ref()
            .is_some_and(|s| s.contains("$CRU_SESSION_DIR")),
        "Should not show spill path in summary: {:?}",
        result
    );
}

#[test]
fn summarize_bash_spill_reference_not_shown_raw() {
    let spill_ref = "[500 lines, 25KB — full output in $CRU_SESSION_DIR/tools/bash-1.txt]";
    let result = summarize_tool_result("bash", spill_ref);
    // Spill references are multi-line or >60 chars, so bash should return None
    assert!(
        result.is_none() || !result.as_ref().unwrap().contains("$CRU_SESSION_DIR"),
        "Bash spill ref should not be shown as summary: {:?}",
        result
    );
}

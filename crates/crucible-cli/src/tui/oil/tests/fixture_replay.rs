//! JSONL fixture replay tests
//!
//! Replay the actual demo JSONL recordings through OilChatApp + TestRuntime,
//! checking for content duplication and rendering invariants frame-by-frame.
//!
//! These tests catch the same bugs visible in demo GIFs without needing
//! a daemon, PTY, or built binary.

use std::path::Path;

use crate::tui::oil::ansi::strip_ansi;
use crate::tui::oil::app::{App, ViewContext};
use crate::tui::oil::chat_app::OilChatApp;
use crate::tui::oil::chat_runner::session_event_to_chat_msgs;
use crate::tui::oil::focus::FocusContext;
use crate::tui::oil::TestRuntime;

// ---------------------------------------------------------------------------
// JSONL parsing
// ---------------------------------------------------------------------------

/// Parse a JSONL fixture file into a sequence of ChatAppMsg.
///
/// Skips the header line (has "version" key), footer line (has "ended_at"),
/// and keypress events. Uses the same mapping as the real replay consumer.
fn parse_fixture(path: &Path) -> Vec<crate::tui::oil::chat_app::ChatAppMsg> {
    let content = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("Failed to read fixture {}: {e}", path.display()));

    let mut messages = Vec::new();

    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }

        let value: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Skip header (has "version" field)
        if value.get("version").is_some() {
            continue;
        }
        // Skip footer (has "ended_at" field)
        if value.get("ended_at").is_some() {
            continue;
        }

        let event_type = match value.get("event").and_then(|v| v.as_str()) {
            Some(e) => e,
            None => continue,
        };

        let data = value
            .get("data")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        let msgs = session_event_to_chat_msgs(event_type, &data);
        messages.extend(msgs);
    }

    messages
}

// ---------------------------------------------------------------------------
// Duplication detection
// ---------------------------------------------------------------------------

/// A rendering violation found during replay.
#[derive(Debug)]
struct Violation {
    frame: usize,
    kind: ViolationKind,
    detail: String,
}

#[derive(Debug)]
enum ViolationKind {
    /// Same line appears in both stdout (graduated) and viewport
    DuplicateAcrossBoundary,
    /// Same paragraph appears multiple times in stdout
    DuplicateInStdout,
    /// Double blank lines in content area
    DoubleBlankLine,
}

/// Check for content appearing in both stdout and viewport.
///
/// We only flag lines that appear *multiple times* in the combined output
/// where at least one instance is in stdout and one in viewport. Short lines,
/// UI chrome, and lines that could legitimately repeat (tool descriptions,
/// status indicators) are excluded by a minimum length threshold.
fn check_cross_boundary_duplication(
    frame: usize,
    stdout: &str,
    viewport: &str,
    violations: &mut Vec<Violation>,
) {
    let stdout_lines: Vec<&str> = stdout.lines().collect();
    let viewport_lines: Vec<&str> = viewport.lines().collect();

    // Build a set of "substantial" stdout lines (unique content, not UI chrome)
    let mut stdout_content: std::collections::HashMap<&str, usize> =
        std::collections::HashMap::new();

    for line in &stdout_lines {
        let trimmed = line.trim();
        // Skip trivial lines
        if trimmed.len() <= 20
            || trimmed.is_empty()
            || trimmed.chars().all(|c| {
                c == '▄'
                    || c == '▀'
                    || c == '─'
                    || c == '│'
                    || c == '┌'
                    || c == '┐'
                    || c == '└'
                    || c == '┘'
                    || c == ' '
                    || c == '●'
            })
        {
            continue;
        }
        *stdout_content.entry(trimmed).or_insert(0) += 1;
    }

    for viewport_line in &viewport_lines {
        let vp_trimmed = viewport_line.trim();
        if let Some(&stdout_count) = stdout_content.get(vp_trimmed) {
            // Only flag if the same line appears multiple times AND crosses the boundary.
            // A tool description appearing once in stdout (graduated tool call) and once
            // in viewport (new tool call) is legitimate — it's a shared description string,
            // not duplicated content. We flag it if the SAME content block is split.
            //
            // Heuristic: count total appearances. If >2, it's likely a real duplication bug.
            let viewport_count = viewport_lines
                .iter()
                .filter(|l| l.trim() == vp_trimmed)
                .count();
            let total = stdout_count + viewport_count;
            if total > 2 {
                violations.push(Violation {
                    frame,
                    kind: ViolationKind::DuplicateAcrossBoundary,
                    detail: format!(
                        "Line appears {} times total ({} in stdout, {} in viewport): {:?}",
                        total, stdout_count, viewport_count, vp_trimmed
                    ),
                });
            }
        }
    }
}

/// Check for duplicate paragraphs within stdout.
fn check_stdout_paragraph_duplication(frame: usize, stdout: &str, violations: &mut Vec<Violation>) {
    let lines: Vec<&str> = stdout.lines().collect();
    let mut paragraphs: Vec<String> = Vec::new();
    let mut current = String::new();

    for line in &lines {
        if line.trim().is_empty() {
            if !current.trim().is_empty() {
                paragraphs.push(current.trim().to_string());
            }
            current.clear();
        } else {
            current.push_str(line.trim());
            current.push(' ');
        }
    }
    if !current.trim().is_empty() {
        paragraphs.push(current.trim().to_string());
    }

    // Check for duplicate paragraphs (>20 chars to avoid false positives on short phrases).
    // Exclude structural headers that legitimately repeat across agent turns
    // (e.g., "┌─ Thinking…" appears once per thinking phase).
    for i in 0..paragraphs.len() {
        for j in (i + 1)..paragraphs.len() {
            if paragraphs[i].len() > 20
                && paragraphs[i] == paragraphs[j]
                && !is_repeatable_structural_header(&paragraphs[i])
            {
                violations.push(Violation {
                    frame,
                    kind: ViolationKind::DuplicateInStdout,
                    detail: format!(
                        "Paragraph appears twice in stdout: {:?}",
                        &paragraphs[i][..paragraphs[i].len().min(80)]
                    ),
                });
            }
        }
    }
}

/// Headers that legitimately repeat across agent turns (multiple thinking phases,
/// multiple tool groups, etc.). These are structural, not content duplication.
fn is_repeatable_structural_header(paragraph: &str) -> bool {
    // Thinking block headers: "┌─ Thinking…" or "┌─ Thought (N words)"
    paragraph.contains("Thinking\u{2026}") || paragraph.contains("Thought (")
}

/// Check for double blank lines in content.
fn check_double_blank_lines(frame: usize, content: &str, violations: &mut Vec<Violation>) {
    let lines: Vec<&str> = content.lines().collect();
    for i in 0..lines.len().saturating_sub(1) {
        if lines[i].trim().is_empty() && lines[i + 1].trim().is_empty() {
            // Ignore double blanks in trailing whitespace (after content ends)
            let remaining_has_content = lines[i + 2..].iter().any(|l| {
                let t = l.trim();
                !t.is_empty() && !t.chars().all(|c| c == '▄' || c == '▀' || c == ' ')
            });
            if remaining_has_content {
                violations.push(Violation {
                    frame,
                    kind: ViolationKind::DoubleBlankLine,
                    detail: format!("Double blank line at lines {} and {}", i, i + 1),
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Replay engine
// ---------------------------------------------------------------------------

struct ReplayResult {
    violations: Vec<Violation>,
    total_frames: usize,
}

fn replay_fixture(path: &Path, width: u16, height: u16) -> ReplayResult {
    let messages = parse_fixture(path);
    assert!(
        !messages.is_empty(),
        "Fixture produced no messages: {}",
        path.display()
    );

    let mut app = OilChatApp::default();
    let mut runtime = TestRuntime::new(width, height);
    let focus = FocusContext::new();
    let mut violations = Vec::new();
    let mut frame = 0;

    for msg in &messages {
        app.on_message(msg.clone());

        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        runtime.render(&tree);

        // Feed graduation back (simulates real chat_runner flow)
        let graduated_keys = runtime.last_graduated_keys();
        if !graduated_keys.is_empty() {
            app.mark_graduated(graduated_keys);
        }

        frame += 1;

        // Run checks on every frame
        let stdout = strip_ansi(runtime.stdout_content());
        let viewport = strip_ansi(runtime.viewport_content());

        check_cross_boundary_duplication(frame, &stdout, &viewport, &mut violations);
        check_stdout_paragraph_duplication(frame, &stdout, &mut violations);

        // Only check stdout for double blank lines (viewport has layout padding)
        check_double_blank_lines(frame, &stdout, &mut violations);
    }

    ReplayResult {
        violations,
        total_frames: frame,
    }
}

fn fixture_path(name: &str) -> std::path::PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_root = std::path::Path::new(manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .expect("Could not find workspace root");
    workspace_root.join("assets/fixtures").join(name)
}

/// Replay a fixture, checking thinking-before-text invariant on EVERY frame.
/// Returns the final output and any frame that violated the invariant.
/// Uses show_thinking=false to match the default user config and test collapsed mode.
fn replay_checking_thinking_order(
    path: &Path,
    width: u16,
    height: u16,
) -> (String, Vec<(usize, String)>) {
    let messages = parse_fixture(path);
    assert!(
        !messages.is_empty(),
        "Fixture produced no messages: {}",
        path.display()
    );

    let mut app = OilChatApp::default();
    app.set_show_thinking(false);
    let mut runtime = TestRuntime::new(width, height);
    let focus = FocusContext::new();
    let mut violations = Vec::new();

    for (frame, msg) in messages.iter().enumerate() {
        app.on_message(msg.clone());

        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        runtime.render(&tree);

        let graduated_keys = runtime.last_graduated_keys();
        if !graduated_keys.is_empty() {
            app.mark_graduated(graduated_keys);
        }

        // Check viewport only for thinking-after-text violations.
        // Stdout contains graduated content from earlier responses;
        // mixing it with viewport content creates false positives
        // when tool groups stay in viewport during streaming.
        let viewport = strip_ansi(runtime.viewport_content());
        let combined = viewport;

        // Look for text content appearing BEFORE a Thought summary within
        // the SAME assistant response. Tool call lines (✓) reset the tracking
        // since they mark a response boundary.
        let lines: Vec<&str> = combined.lines().collect();
        let mut saw_text_in_response = false;
        let mut saw_thought_after_text = false;
        for line in &lines {
            let trimmed = line.trim();
            // Skip non-content lines
            if trimmed.is_empty()
                || trimmed.chars().all(|c| "▄▀─│┌┐└┘ >".contains(c))
                || trimmed.starts_with('◐')
                || trimmed.starts_with('*')
                || trimmed.starts_with("· ")
                || trimmed.starts_with("* Found")
                || trimmed.starts_with("NORMAL")
                || trimmed.starts_with("PERMISSION")
                || trimmed.starts_with("— ctx")
            {
                continue;
            }
            // Tool calls mark response boundaries — reset per-response tracking.
            // Includes both completed (✓) and running (braille spinner) tool indicators.
            if trimmed.starts_with('✓')
                || trimmed.starts_with('●')
                || trimmed.starts_with('⠋')
                || trimmed.starts_with('⠙')
                || trimmed.starts_with('⠹')
                || trimmed.starts_with('⠸')
                || trimmed.starts_with('⠼')
                || trimmed.starts_with('⠴')
                || trimmed.starts_with('⠦')
                || trimmed.starts_with('⠧')
                || trimmed.starts_with('⠇')
                || trimmed.starts_with('⠏')
                || trimmed.contains("Read Note")
                || trimmed.contains("│ {")
                || trimmed.contains("Read note content")
            {
                saw_text_in_response = false;
                continue;
            }
            // Is this a Thought summary line?
            if trimmed.contains('\u{25C7}') && trimmed.contains("Thought") {
                if saw_text_in_response {
                    saw_thought_after_text = true;
                }
                // Reset — this Thought starts a new section
                saw_text_in_response = false;
                continue;
            }
            // Is this assistant text content? (indented with 3+ spaces)
            if line.starts_with("   ") && !trimmed.is_empty() {
                saw_text_in_response = true;
            }
        }

        if saw_thought_after_text {
            violations.push((frame, combined));
        }
    }

    let stdout = strip_ansi(runtime.stdout_content());
    let viewport = strip_ansi(runtime.viewport_content());
    let final_output = format!("=== STDOUT ===\n{stdout}\n=== VIEWPORT ===\n{viewport}");
    (final_output, violations)
}

fn assert_no_violations(result: &ReplayResult) {
    if !result.violations.is_empty() {
        let mut msg = format!(
            "Found {} violation(s) across {} frames:\n",
            result.violations.len(),
            result.total_frames
        );
        for v in &result.violations {
            msg.push_str(&format!(
                "  Frame {}: [{:?}] {}\n",
                v.frame, v.kind, v.detail
            ));
        }
        panic!("{}", msg);
    }
}

// ---------------------------------------------------------------------------
// Tests: demo.jsonl
// ---------------------------------------------------------------------------

#[test]
fn replay_demo_fixture_80x24() {
    let path = fixture_path("demo.jsonl");
    if !path.exists() {
        eprintln!("SKIPPED: fixture not found at {}", path.display());
        return;
    }
    let result = replay_fixture(&path, 80, 24);
    assert_no_violations(&result);
}

#[test]
fn replay_demo_fixture_120x40() {
    let path = fixture_path("demo.jsonl");
    if !path.exists() {
        return;
    }
    let result = replay_fixture(&path, 120, 40);
    assert_no_violations(&result);
}

#[test]
fn replay_demo_fixture_60x20() {
    let path = fixture_path("demo.jsonl");
    if !path.exists() {
        return;
    }
    let result = replay_fixture(&path, 60, 20);
    assert_no_violations(&result);
}

// ---------------------------------------------------------------------------
// Tests: acp-demo.jsonl
// ---------------------------------------------------------------------------

#[test]
fn replay_acp_demo_fixture_80x24() {
    let path = fixture_path("acp-demo.jsonl");
    if !path.exists() {
        return;
    }
    let result = replay_fixture(&path, 80, 24);
    assert_no_violations(&result);
}

#[test]
fn replay_acp_demo_fixture_120x40() {
    let path = fixture_path("acp-demo.jsonl");
    if !path.exists() {
        return;
    }
    let result = replay_fixture(&path, 120, 40);
    assert_no_violations(&result);
}

// ---------------------------------------------------------------------------
// Tests: delegation-demo.jsonl
// ---------------------------------------------------------------------------

#[test]
fn replay_delegation_demo_fixture_80x24() {
    let path = fixture_path("delegation-demo.jsonl");
    if !path.exists() {
        return;
    }
    let result = replay_fixture(&path, 80, 24);
    assert_no_violations(&result);
}

#[test]
fn replay_delegation_demo_fixture_120x40() {
    let path = fixture_path("delegation-demo.jsonl");
    if !path.exists() {
        return;
    }
    let result = replay_fixture(&path, 120, 40);
    assert_no_violations(&result);
}

// ---------------------------------------------------------------------------
// Tests: thinking ordering A/B comparison
//
// Fixture A: recorded from `cru session send --raw` with straggler thinking
//            events arriving AFTER text_delta runs (the daemon bug).
// Fixture B: same events but stragglers moved before text_delta runs (fixed order).
//
// Both should produce thinking BEFORE text in the final output because
// append_thinking() is position-aware and inserts before trailing text.
// ---------------------------------------------------------------------------

#[test]
fn thinking_order_fixture_a_buggy_events() {
    let path = fixture_path("thinking_order_A_buggy.jsonl");
    if !path.exists() {
        eprintln!("SKIPPED: fixture not found at {}", path.display());
        return;
    }
    let (_, violations) = replay_checking_thinking_order(&path, 80, 24);
    assert!(
        violations.is_empty(),
        "Fixture A (buggy event order): found {} frames where Thought appeared after text.\n\
         First violation at frame {}:\n{}",
        violations.len(),
        violations[0].0,
        violations[0].1,
    );
}

#[test]
fn thinking_order_fixture_b_fixed_events() {
    let path = fixture_path("thinking_order_B_fixed.jsonl");
    if !path.exists() {
        eprintln!("SKIPPED: fixture not found at {}", path.display());
        return;
    }
    let (_, violations) = replay_checking_thinking_order(&path, 80, 24);
    assert!(
        violations.is_empty(),
        "Fixture B (fixed event order): found {} frames where Thought appeared after text.\n\
         First violation at frame {}:\n{}",
        violations.len(),
        violations[0].0,
        violations[0].1,
    );
}

/// A/B: both fixtures should have zero ordering violations across all frames.
#[test]
fn thinking_order_ab_comparison() {
    let path_a = fixture_path("thinking_order_A_buggy.jsonl");
    let path_b = fixture_path("thinking_order_B_fixed.jsonl");
    if !path_a.exists() || !path_b.exists() {
        eprintln!("SKIPPED: A/B fixtures not found");
        return;
    }

    let (_, violations_a) = replay_checking_thinking_order(&path_a, 80, 24);
    let (_, violations_b) = replay_checking_thinking_order(&path_b, 80, 24);

    assert!(
        violations_a.is_empty(),
        "Fixture A: {} frames with ordering violation",
        violations_a.len()
    );
    assert!(
        violations_b.is_empty(),
        "Fixture B: {} frames with ordering violation",
        violations_b.len()
    );
}


// ---------------------------------------------------------------------------
// Tests: reproduce-formatting.jsonl
// ---------------------------------------------------------------------------

#[test]
fn replay_reproduce_formatting_80x24() {
    let path = fixture_path("reproduce-formatting.jsonl");
    if !path.exists() {
        eprintln!("SKIPPED: fixture not found at {}", path.display());
        return;
    }
    let result = replay_fixture(&path, 80, 24);
    assert_no_violations(&result);
}

#[test]
fn replay_reproduce_formatting_67x24() {
    let path = fixture_path("reproduce-formatting.jsonl");
    if !path.exists() {
        return;
    }
    // Narrower terminal matching the user's reported width
    let result = replay_fixture(&path, 67, 24);
    assert_no_violations(&result);
}

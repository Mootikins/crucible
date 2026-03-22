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

    // Check for duplicate paragraphs (>20 chars to avoid false positives on short phrases)
    for i in 0..paragraphs.len() {
        for j in (i + 1)..paragraphs.len() {
            if paragraphs[i].len() > 20 && paragraphs[i] == paragraphs[j] {
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

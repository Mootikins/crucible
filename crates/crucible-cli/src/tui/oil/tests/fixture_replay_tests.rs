//! JSONL fixture replay + color-aware snapshot tests.
//!
//! Replays real session recordings through the new container model,
//! checking rendering invariants frame-by-frame. Also captures styled
//! (ANSI color) snapshots to verify color correctness.

use std::path::Path;

use crate::tui::oil::app::App;
use crate::tui::oil::chat_app::OilChatApp;
use crate::tui::oil::chat_runner::SessionEventStream;
use crucible_oil::ansi::strip_ansi;
use crucible_oil::node::BRAILLE_SPINNER_FRAMES;
use crucible_oil::node::SPINNER_FRAMES;

use super::vt100_runtime::Vt100TestRuntime;

// ─── JSONL Parsing ─────────────────────────────────────────────────────────

/// Translate a recording into the `ChatAppMsg` stream the TUI would receive on
/// replay, through the production [`SessionEventStream`] — the same converter
/// `chat_runner` feeds from the daemon. Re-implementing its turn state here
/// would leave these snapshots pinning a fiction: they would keep passing while
/// the shipped rule regressed.
fn parse_fixture(path: &Path) -> Vec<crate::tui::oil::chat_app::ChatAppMsg> {
    let content = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("Failed to read fixture {}: {e}", path.display()));

    let mut stream = SessionEventStream::new();
    let mut messages = Vec::new();

    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }

        let value: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Skip header/footer
        if value.get("version").is_some() || value.get("ended_at").is_some() {
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

        messages.extend(stream.translate(event_type, &data));
    }

    messages
}

use super::helpers::fixture_path;

// ─── Replay Infrastructure ─────────────────────────────────────────────────

struct ReplayResult {
    violations: Vec<String>,
    total_frames: usize,
    final_output: String,
    #[allow(dead_code)]
    final_styled: String,
}

fn replay_fixture(path: &Path, width: u16, height: u16) -> ReplayResult {
    let messages = parse_fixture(path);
    assert!(
        !messages.is_empty(),
        "Fixture produced no messages: {}",
        path.display()
    );

    let mut app = OilChatApp::default();
    let mut vt = Vt100TestRuntime::new(width, height);
    let mut violations = Vec::new();
    let mut frame = 0;

    for msg in &messages {
        app.on_message(msg.clone());
        vt.render_frame(&mut app);
        frame += 1;

        // Check scrollback for spinners after every frame
        let scrollback = vt.scrollback_contents();
        if !scrollback.is_empty() {
            let stripped = strip_ansi(&scrollback);
            for ch in SPINNER_FRAMES.iter().chain(BRAILLE_SPINNER_FRAMES.iter()) {
                if stripped.contains(*ch) {
                    violations.push(format!(
                        "Frame {}: spinner '{}' found in scrollback",
                        frame, ch
                    ));
                }
            }
        }
    }

    // Use scrollback from the real vt100 (not tall parser) + final screen.
    // The tall parser can show artifacts from intermediate viewport frames.
    let scrollback = vt.scrollback_contents();
    let screen = vt.screen_contents();
    let combined = format!("{}\n{}", scrollback, screen);
    let final_output = strip_ansi(&combined);
    let final_styled = vt.screen_contents_styled();

    ReplayResult {
        violations,
        total_frames: frame,
        final_output,
        final_styled,
    }
}

// ─── Tests: demo.jsonl ─────────────────────────────────────────────────────

#[test]
fn replay_demo_80x24() {
    let path = fixture_path("demo.jsonl");
    if !path.exists() {
        eprintln!("Skipping: {} not found", path.display());
        return;
    }

    let result = replay_fixture(&path, 80, 24);

    assert!(
        result.violations.is_empty(),
        "Invariant violations in demo.jsonl at 80x24 ({} frames):\n{}",
        result.total_frames,
        result.violations.join("\n")
    );

    // Content should be present
    assert!(
        result.final_output.len() > 100,
        "Demo fixture should produce substantial output"
    );
}

#[test]
fn replay_demo_120x40() {
    let path = fixture_path("demo.jsonl");
    if !path.exists() {
        return;
    }

    let result = replay_fixture(&path, 120, 40);

    assert!(
        result.violations.is_empty(),
        "Invariant violations in demo.jsonl at 120x40 ({} frames):\n{}",
        result.total_frames,
        result.violations.join("\n")
    );
}

#[test]
fn replay_demo_60x20() {
    let path = fixture_path("demo.jsonl");
    if !path.exists() {
        return;
    }

    let result = replay_fixture(&path, 60, 20);

    assert!(
        result.violations.is_empty(),
        "Invariant violations in demo.jsonl at 60x20 ({} frames):\n{}",
        result.total_frames,
        result.violations.join("\n")
    );
}

// ─── Tests: Color-aware styled snapshots ───────────────────────────────────

#[test]
fn styled_snapshot_basic_conversation() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.on_message(crate::tui::oil::chat_app::ChatAppMsg::UserMessage(
        "What is Rust?".into(),
    ));
    app.on_message(crate::tui::oil::chat_app::ChatAppMsg::ThinkingDelta(
        "simple question about programming languages".into(),
    ));
    app.on_message(crate::tui::oil::chat_app::ChatAppMsg::TextDelta(
        "Rust is a systems programming language focused on safety and performance.".into(),
    ));
    app.on_message(crate::tui::oil::chat_app::ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    // Capture styled output with ANSI codes — this verifies colors
    let styled = vt.screen_contents_styled();
    insta::assert_snapshot!("styled_basic_conversation", styled);
}

/// A completed read, in colour.
///
/// `Read File` is the ACP spelling — what `humanize_tool_title` stores for a
/// delegated agent's read. Until divergence **A4** was fixed this snapshot
/// recorded the bug: the summary table keyed on `read_file` alone, so this
/// card painted the file body into the transcript while the identical
/// internal read collapsed to `→ 3 lines`. The body-line styling that used to
/// live here is now pinned by `styled_snapshot_tool_call_with_body`, which
/// uses a tool the table deliberately does not summarize.
#[test]
fn styled_snapshot_tool_call() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.on_message(crate::tui::oil::chat_app::ChatAppMsg::UserMessage(
        "Read a file".into(),
    ));
    app.on_message(crate::tui::oil::chat_app::ChatAppMsg::ToolCall {
        name: "Read File".into(),
        args: r#"{"path": "src/main.rs"}"#.into(),
        call_id: Some("c1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
        diffs: Vec::new(),
        auto_approved: None,
    });
    app.on_message(crate::tui::oil::chat_app::ChatAppMsg::ToolResultDelta {
        name: "Read File".into(),
        delta: "fn main() {\n    println!(\"Hello\");\n}".into(),
        call_id: Some("c1".into()),
    });
    app.on_message(crate::tui::oil::chat_app::ChatAppMsg::ToolResultComplete {
        name: "Read File".into(),
        call_id: Some("c1".into()),
    });
    app.on_message(crate::tui::oil::chat_app::ChatAppMsg::TextDelta(
        "The file contains a simple hello world program.".into(),
    ));
    app.on_message(crate::tui::oil::chat_app::ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    let styled = vt.screen_contents_styled();
    insta::assert_snapshot!("styled_tool_call", styled);
}

/// The other half of a tool card: the result body, in colour.
///
/// A multi-line `bash` result has no entry in the summary table and is too
/// long for `collapse_result`'s one-line branch, so it renders through
/// `format_output_tail` — the `│`-prefixed dim rows. This is the only styled
/// snapshot that covers them.
#[test]
fn styled_snapshot_tool_call_with_body() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.on_message(crate::tui::oil::chat_app::ChatAppMsg::UserMessage(
        "Show me the file".into(),
    ));
    app.on_message(crate::tui::oil::chat_app::ChatAppMsg::ToolCall {
        name: "bash".into(),
        args: r#"{"command": "cat src/main.rs"}"#.into(),
        call_id: Some("c1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
        diffs: Vec::new(),
        auto_approved: None,
    });
    app.on_message(crate::tui::oil::chat_app::ChatAppMsg::ToolResultDelta {
        name: "bash".into(),
        delta: "fn main() {\n    println!(\"Hello\");\n}".into(),
        call_id: Some("c1".into()),
    });
    app.on_message(crate::tui::oil::chat_app::ChatAppMsg::ToolResultComplete {
        name: "bash".into(),
        call_id: Some("c1".into()),
    });
    app.on_message(crate::tui::oil::chat_app::ChatAppMsg::TextDelta(
        "The file contains a simple hello world program.".into(),
    ));
    app.on_message(crate::tui::oil::chat_app::ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    let styled = vt.screen_contents_styled();
    insta::assert_snapshot!("styled_tool_call_with_body", styled);
}

#[test]
fn styled_snapshot_thinking_collapsed() {
    // show_thinking=off: graduated thinking collapses to "◇ Thought (N words)".
    let mut app = OilChatApp::init();
    app.set_show_thinking(false);
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.on_message(crate::tui::oil::chat_app::ChatAppMsg::UserMessage(
        "Think about this".into(),
    ));
    app.on_message(crate::tui::oil::chat_app::ChatAppMsg::ThinkingDelta(
        "Deep analysis of the question at hand with multiple considerations".into(),
    ));
    app.on_message(crate::tui::oil::chat_app::ChatAppMsg::TextDelta(
        "Here is my conclusion.".into(),
    ));
    app.on_message(crate::tui::oil::chat_app::ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    let styled = vt.screen_contents_styled();
    insta::assert_snapshot!("styled_thinking_collapsed", styled);
}

#[test]
fn styled_snapshot_thinking_expanded_after_graduation() {
    // show_thinking=on: graduated thinking keeps the expanded content.
    let mut app = OilChatApp::init();
    app.set_show_thinking(true);
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.on_message(crate::tui::oil::chat_app::ChatAppMsg::UserMessage(
        "Think about this".into(),
    ));
    app.on_message(crate::tui::oil::chat_app::ChatAppMsg::ThinkingDelta(
        "Deep analysis of the question at hand with multiple considerations".into(),
    ));
    app.on_message(crate::tui::oil::chat_app::ChatAppMsg::TextDelta(
        "Here is my conclusion.".into(),
    ));
    app.on_message(crate::tui::oil::chat_app::ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    let styled = vt.screen_contents_styled();
    insta::assert_snapshot!("styled_thinking_expanded_after_graduation", styled);
}

// ─── Reproduce formatting fixture ──────────────────────────────────────────

#[test]
fn replay_reproduce_formatting_80x24() {
    let path = fixture_path("reproduce-formatting.jsonl");
    if !path.exists() {
        eprintln!("Skipping: {} not found", path.display());
        return;
    }

    let result = replay_fixture(&path, 80, 24);

    assert!(
        result.violations.is_empty(),
        "Invariant violations ({} frames):\n{}",
        result.total_frames,
        result.violations.join("\n")
    );
}

// ─── ACP demo fixture ──────────────────────────────────────────────────────

#[test]
fn replay_acp_demo_80x24() {
    let path = fixture_path("acp-demo.jsonl");
    if !path.exists() {
        eprintln!("Skipping: {} not found", path.display());
        return;
    }

    let result = replay_fixture(&path, 80, 24);

    assert!(
        result.violations.is_empty(),
        "Invariant violations ({} frames):\n{}",
        result.total_frames,
        result.violations.join("\n")
    );
}

// ─── Reproduce fixture (spacing + thinking bugs) ──────────────────────────

#[test]
fn replay_reproduce_124x59() {
    let path = fixture_path("reproduce.jsonl");
    if !path.exists() {
        eprintln!("Skipping: {} not found", path.display());
        return;
    }

    let result = replay_fixture(&path, 124, 59);

    assert!(
        result.violations.is_empty(),
        "Invariant violations in reproduce.jsonl at 124x59 ({} frames):\n{}",
        result.total_frames,
        result.violations.join("\n")
    );

    assert!(
        result.final_output.len() > 200,
        "Reproduce fixture should produce substantial output"
    );

    // No double blank lines anywhere
    let lines: Vec<&str> = result.final_output.lines().collect();
    for i in 0..lines.len().saturating_sub(1) {
        if lines[i].trim().is_empty() && lines[i + 1].trim().is_empty() {
            let before = if i > 0 {
                lines[i - 1].trim()
            } else {
                "(start)"
            };
            let after = if i + 2 < lines.len() {
                lines[i + 2].trim()
            } else {
                "(end)"
            };
            panic!(
                "Double blank at line {} (between {:?} and {:?})",
                i, before, after
            );
        }
    }

    // TODO: Adjacent tools in scrollback can show intermediate gaps from
    // viewport-to-scrollback transitions. This is a rendering infrastructure
    // issue (viewport margin scrolls into scrollback) not a ContainerList bug.
}

#[test]
fn replay_reproduce_80x24() {
    let path = fixture_path("reproduce.jsonl");
    if !path.exists() {
        return;
    }

    let result = replay_fixture(&path, 80, 24);

    assert!(
        result.violations.is_empty(),
        "Invariant violations in reproduce.jsonl at 80x24 ({} frames):\n{}",
        result.total_frames,
        result.violations.join("\n")
    );
}

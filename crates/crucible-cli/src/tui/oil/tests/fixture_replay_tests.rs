//! JSONL fixture replay + color-aware snapshot tests.
//!
//! Replays real session recordings through the new container model,
//! checking rendering invariants frame-by-frame. Also captures styled
//! (ANSI color) snapshots to verify color correctness.

use std::path::Path;

use crate::tui::oil::app::App;
use crate::tui::oil::chat_app::OilChatApp;
use crate::tui::oil::chat_runner::session_event_to_chat_msgs;
use crucible_oil::ansi::strip_ansi;
use crucible_oil::node::SPINNER_FRAMES;
use crucible_oil::node::BRAILLE_SPINNER_FRAMES;

use super::vt100_runtime::Vt100TestRuntime;

// ─── JSONL Parsing ─────────────────────────────────────────────────────────

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

        let msgs = session_event_to_chat_msgs(event_type, &data);
        messages.extend(msgs);
    }

    messages
}

fn fixture_path(name: &str) -> std::path::PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_root = std::path::Path::new(manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .expect("Could not find workspace root");
    workspace_root.join("assets/fixtures").join(name)
}

// ─── Replay Infrastructure ─────────────────────────────────────────────────

struct ReplayResult {
    violations: Vec<String>,
    total_frames: usize,
    final_output: String,
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

    let final_output = strip_ansi(&vt.full_history());
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

#[test]
fn styled_snapshot_thinking_collapsed() {
    let mut app = OilChatApp::init();
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

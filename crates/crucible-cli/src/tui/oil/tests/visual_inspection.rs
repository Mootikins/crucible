//! Visual inspection tests — render real fixtures and dump output for review.
//!
//! These are NOT assertion-based tests. They render fixtures through the
//! real terminal path and write the output to /tmp/ for manual inspection.
//! Run with `--nocapture` to see output inline.

use std::path::Path;

use crate::tui::oil::app::App;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::chat_runner::session_event_to_chat_msgs;
use crucible_oil::ansi::strip_ansi;

use super::vt100_runtime::Vt100TestRuntime;

fn parse_fixture(path: &Path) -> Vec<ChatAppMsg> {
    let content = std::fs::read_to_string(path).unwrap();
    let mut messages = Vec::new();
    let mut saw_text_delta = false;
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let value: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if value.get("version").is_some() || value.get("ended_at").is_some() {
            continue;
        }
        let event_type = match value.get("event").and_then(|v| v.as_str()) {
            Some(e) => e,
            None => continue,
        };
        if event_type == "text_delta" {
            saw_text_delta = true;
        } else if event_type == "user_message" {
            saw_text_delta = false;
        }
        if event_type == "thinking" && saw_text_delta {
            continue;
        }
        let data = value.get("data").cloned().unwrap_or_default();
        for msg in session_event_to_chat_msgs(event_type, &data) {
            if saw_text_delta
                && event_type == "message_complete"
                && matches!(&msg, ChatAppMsg::TextDelta(_))
            {
                continue;
            }
            messages.push(msg);
        }
    }
    messages
}

fn fixture_path(name: &str) -> std::path::PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_root = std::path::Path::new(manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root");
    workspace_root.join("assets/fixtures").join(name)
}

/// Render a fixture and return (scrollback, viewport) as stripped text.
fn render_fixture_full(name: &str, width: u16, height: u16) -> (String, String) {
    let path = fixture_path(name);
    if !path.exists() {
        return (format!("FIXTURE NOT FOUND: {}", path.display()), String::new());
    }

    let messages = parse_fixture(&path);
    let mut app = OilChatApp::default();
    let mut vt = Vt100TestRuntime::new(width, height);

    for msg in &messages {
        app.on_message(msg.clone());
    }
    // Final render to flush all graduations
    vt.render_frame(&mut app);

    let scrollback = strip_ansi(&vt.scrollback_contents());
    let viewport = strip_ansi(&vt.screen_contents());
    (scrollback, viewport)
}

/// Render a fixture with styled output (ANSI colors preserved).
fn render_fixture_styled(name: &str, width: u16, height: u16) -> String {
    let path = fixture_path(name);
    if !path.exists() {
        return format!("FIXTURE NOT FOUND: {}", path.display());
    }

    let messages = parse_fixture(&path);
    let mut app = OilChatApp::default();
    let mut vt = Vt100TestRuntime::new(width, height);

    for msg in &messages {
        app.on_message(msg.clone());
    }
    vt.render_frame(&mut app);

    let full = vt.full_history();
    full
}

// ─── Dump tests ────────────────────────────────────────────────────────────

#[test]
fn dump_parity_test_80x24() {
    let (scrollback, viewport) = render_fixture_full("parity-test.jsonl", 80, 24);
    let output = format!(
        "=== PARITY TEST 80x24 ===\n\
         --- SCROLLBACK ---\n{}\n\
         --- VIEWPORT ---\n{}\n\
         === END ===",
        scrollback, viewport
    );
    std::fs::write("/tmp/crucible-visual-parity-80x24.txt", &output).ok();
    eprintln!("{}", output);
}

#[test]
fn dump_demo_80x24() {
    let (scrollback, viewport) = render_fixture_full("demo.jsonl", 80, 24);
    let output = format!(
        "=== DEMO 80x24 ===\n\
         --- SCROLLBACK ---\n{}\n\
         --- VIEWPORT ---\n{}\n\
         === END ===",
        scrollback, viewport
    );
    std::fs::write("/tmp/crucible-visual-demo-80x24.txt", &output).ok();
    eprintln!("{}", output);
}

#[test]
fn dump_reproduce_formatting_80x24() {
    let (scrollback, viewport) = render_fixture_full("reproduce-formatting.jsonl", 80, 24);
    let output = format!(
        "=== REPRODUCE FORMATTING 80x24 ===\n\
         --- SCROLLBACK ---\n{}\n\
         --- VIEWPORT ---\n{}\n\
         === END ===",
        scrollback, viewport
    );
    std::fs::write("/tmp/crucible-visual-repro-80x24.txt", &output).ok();
    eprintln!("{}", output);
}

#[test]
fn dump_parity_test_styled() {
    let styled = render_fixture_styled("parity-test.jsonl", 80, 24);
    std::fs::write("/tmp/crucible-visual-parity-styled.txt", &styled).ok();
    // Print styled to terminal for visual inspection with --nocapture
    eprintln!("{}", styled);
}

#[test]
fn dump_demo_styled() {
    let styled = render_fixture_styled("demo.jsonl", 80, 24);
    std::fs::write("/tmp/crucible-visual-demo-styled.txt", &styled).ok();
}

// ─── Synthetic scenarios ───────────────────────────────────────────────────

#[test]
fn dump_synthetic_multi_turn_with_tools() {
    let mut app = OilChatApp::default();
    let mut vt = Vt100TestRuntime::new(80, 40);

    // Turn 1: simple Q&A
    app.on_message(ChatAppMsg::UserMessage("What is 2+2?".into()));
    app.on_message(ChatAppMsg::TextDelta("The answer is **4**.".into()));
    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    // Turn 2: thinking + tools + continuation
    app.on_message(ChatAppMsg::UserMessage("Read my config and explain it".into()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::ThinkingDelta(
        "I need to read the config file first to understand what settings are configured."
            .into(),
    ));
    app.on_message(ChatAppMsg::TextDelta("Let me check your configuration.".into()));

    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".into(),
        args: r#"{"path": "config.toml"}"#.into(),
        call_id: Some("call-1".into()),
        description: Some("Read file contents".into()),
        source: Some("Core".into()),
        lua_primary_arg: None,
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "read_file".into(),
        delta: "[database]\nhost = \"localhost\"\nport = 5432\n".into(),
        call_id: Some("call-1".into()),
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".into(),
        call_id: Some("call-1".into()),
    });
    vt.render_frame(&mut app);

    // Continuation text after tool
    app.on_message(ChatAppMsg::TextDelta(
        "Your configuration has the following settings:\n\n\
         - **Database host**: `localhost`\n\
         - **Database port**: `5432`\n\n\
         This is a standard PostgreSQL configuration pointing to a local instance."
            .into(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    let scrollback = strip_ansi(&vt.scrollback_contents());
    let viewport = strip_ansi(&vt.screen_contents());
    let output = format!(
        "=== SYNTHETIC MULTI-TURN 80x40 ===\n\
         --- SCROLLBACK ---\n{}\n\
         --- VIEWPORT ---\n{}\n\
         === END ===",
        scrollback, viewport
    );
    std::fs::write("/tmp/crucible-visual-synthetic.txt", &output).ok();
    eprintln!("{}", output);
}

#[test]
fn dump_synthetic_error_and_subagent() {
    let mut app = OilChatApp::default();
    let mut vt = Vt100TestRuntime::new(80, 30);

    app.on_message(ChatAppMsg::UserMessage("Deploy the app".into()));
    app.on_message(ChatAppMsg::TextDelta("I'll start the deployment.".into()));

    // Tool with error
    app.on_message(ChatAppMsg::ToolCall {
        name: "bash".into(),
        args: r#"{"command": "deploy.sh"}"#.into(),
        call_id: Some("call-1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
    });
    app.on_message(ChatAppMsg::ToolResultError {
        name: "bash".into(),
        error: "Permission denied: /usr/local/bin/deploy.sh".into(),
        call_id: Some("call-1".into()),
    });

    // Subagent
    app.on_message(ChatAppMsg::SubagentSpawned {
        id: "agent-1".into(),
        prompt: "Fix deployment permissions and retry".into(),
    });
    app.on_message(ChatAppMsg::SubagentCompleted {
        id: "agent-1".into(),
        summary: "Fixed permissions, deployment successful".into(),
    });

    app.on_message(ChatAppMsg::TextDelta(
        "The deployment ran into a permissions issue, but the subagent fixed it."
            .into(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    let scrollback = strip_ansi(&vt.scrollback_contents());
    let viewport = strip_ansi(&vt.screen_contents());
    let output = format!(
        "=== SYNTHETIC ERROR + SUBAGENT 80x30 ===\n\
         --- SCROLLBACK ---\n{}\n\
         --- VIEWPORT ---\n{}\n\
         === END ===",
        scrollback, viewport
    );
    std::fs::write("/tmp/crucible-visual-error-subagent.txt", &output).ok();
    eprintln!("{}", output);
}

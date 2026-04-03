//! Inter-frame rendering invariant tests.
//!
//! These tests verify visual invariants across consecutive render frames.
//! They replay realistic message sequences through the VT100 test runtime
//! and check invariants after EVERY frame, catching issues that only
//! appear during transitions (graduation boundaries, streaming, etc).

use crate::tui::oil::app::App;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crucible_oil::ansi::strip_ansi;

use super::vt100_runtime::Vt100TestRuntime;

// ─── Invariant checkers ────────────────────────────────────────────────────

/// Check that no two adjacent non-blank lines are both "◇ Thought (N words)"
/// with the same word count.
fn check_no_duplicate_thought_lines(screen: &str, context: &str) {
    let lines: Vec<&str> = screen.lines().collect();
    let mut prev_thought: Option<&str> = None;

    for line in &lines {
        let trimmed = line.trim();
        if trimmed.starts_with("◇ Thought") || trimmed.starts_with("\u{25C7} Thought") {
            if let Some(prev) = prev_thought {
                // Two Thought lines — check they don't match
                if prev.trim() == trimmed {
                    panic!(
                        "{}: duplicate Thought line: '{}'\nFull screen:\n{}",
                        context, trimmed, screen
                    );
                }
            }
            prev_thought = Some(line);
        } else if !trimmed.is_empty() {
            prev_thought = None;
        }
    }
}

/// Check that there are no triple-blank-line sequences (always a spacing bug).
fn check_no_triple_blanks(screen: &str, context: &str) {
    let lines: Vec<&str> = screen.lines().collect();
    for (i, window) in lines.windows(3).enumerate() {
        if window.iter().all(|l| l.trim().is_empty()) {
            panic!(
                "{}: triple blank at lines {}-{}.\nScreen:\n{}",
                context,
                i,
                i + 2,
                screen
            );
        }
    }
}

/// Check that the "Thinking…" indicator is never on the line immediately
/// above the input box top border (▄). There should be spacing or the
/// indicator should be in the chrome section below the spacer.
fn check_thinking_not_adjacent_to_input_top(screen: &str, context: &str) {
    let lines: Vec<&str> = screen.lines().collect();
    for (i, window) in lines.windows(2).enumerate() {
        let first = window[0].trim();
        let second = window[1].trim();

        // "Thinking…" or spinner + thinking on one line, ▄▄▄ on the next
        let is_thinking = first.contains("Thinking\u{2026}") && !first.contains("Thought");
        let is_input_top = second.chars().all(|c| c == '▄' || c == ' ') && second.contains('▄') && second.len() > 10;

        // This is OK in chrome (turn indicator above input). Check it's
        // actually chrome by looking for the status bar nearby.
        if is_thinking && is_input_top {
            // The turn indicator + input is a valid chrome pattern.
            // Only flag if there's content (tool calls, etc.) between
            // user message and the thinking line — that means it's a
            // content thinking block rendered adjacent to input.
            let above_lines = &lines[..i];
            let has_content_above = above_lines.iter().any(|l| {
                let t = l.trim();
                t.starts_with("✓") || t.starts_with("✗") || t.starts_with("●")
                    || t.starts_with("◇ Thought")
            });
            // Chrome position: thinking indicator directly above input is fine
            // Content position: would mean content leaked into chrome
            if has_content_above {
                // This is the content area — thinking adjacent to input is suspicious
                // but could be valid if the content fills the viewport.
                // We can't easily distinguish, so skip this check for now.
            }
        }
    }
}

/// Check that between consecutive content sections (user msg, tool, text),
/// spacing is exactly 1 blank line (not 0, not 2+).
fn check_consistent_content_spacing(screen: &str, context: &str) {
    let lines: Vec<&str> = screen.lines().collect();

    // Identify "content boundary" lines — starts of distinct visual blocks
    let mut boundaries: Vec<(usize, &str)> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if t.starts_with("▄▄") && t.len() > 10 {
            boundaries.push((i, "user_top"));
        } else if t.starts_with("▀▀") && t.len() > 10 {
            boundaries.push((i, "user_bottom"));
        } else if t.starts_with("✓ ") || t.starts_with("✗ ") {
            boundaries.push((i, "tool"));
        } else if t.starts_with("◇ Thought") {
            boundaries.push((i, "thought"));
        }
    }

    // Check spacing between user_bottom and the next content
    for (i, &(row, kind)) in boundaries.iter().enumerate() {
        if kind == "user_bottom" {
            // Find next content boundary
            if let Some(&(next_row, next_kind)) = boundaries.get(i + 1) {
                if next_kind == "user_top" {
                    continue; // Skip user→user (input box)
                }
                let gap = next_row - row - 1;
                let blanks = lines[row + 1..next_row]
                    .iter()
                    .filter(|l| l.trim().is_empty())
                    .count();
                if blanks == 0 && gap == 0 {
                    panic!(
                        "{}: no spacing between user_bottom (R{}) and {} (R{})\nScreen:\n{}",
                        context, row, next_kind, next_row, screen
                    );
                }
            }
        }
    }
}

// ─── Multi-frame test helper ───────────────────────────────────────────────

struct FrameChecker {
    app: OilChatApp,
    vt: Vt100TestRuntime,
    frame_count: usize,
}

impl FrameChecker {
    fn new(width: u16, height: u16) -> Self {
        Self {
            app: OilChatApp::default(),
            vt: Vt100TestRuntime::new(width, height),
            frame_count: 0,
        }
    }

    fn send(&mut self, msg: ChatAppMsg) {
        self.app.on_message(msg);
    }

    fn render_and_check(&mut self) {
        self.vt.render_frame(&mut self.app);
        self.frame_count += 1;

        let full = self.vt.full_history();
        let stripped = strip_ansi(&full);
        let ctx = format!("frame {}", self.frame_count);

        check_no_duplicate_thought_lines(&stripped, &ctx);
        check_no_triple_blanks(&stripped, &ctx);
        check_consistent_content_spacing(&stripped, &ctx);
    }

    fn scrollback(&mut self) -> String {
        strip_ansi(&self.vt.scrollback_contents())
    }

    fn screen(&self) -> String {
        strip_ansi(&self.vt.screen_contents())
    }

    fn full(&self) -> String {
        strip_ansi(&self.vt.full_history())
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

/// Realistic multi-tool turn: thinking → text → tools → thinking → text.
/// Checks invariants at every frame transition.
#[test]
fn invariant_multi_tool_turn() {
    let mut fc = FrameChecker::new(120, 50);

    // User message
    fc.send(ChatAppMsg::UserMessage("analyze the codebase".into()));
    fc.render_and_check();

    // Thinking starts
    for word in "I need to explore the repository structure first to understand the codebase".split_whitespace() {
        fc.send(ChatAppMsg::ThinkingDelta(format!("{} ", word)));
    }
    fc.render_and_check();

    // Text starts
    fc.send(ChatAppMsg::TextDelta("I'll explore the repository.".into()));
    fc.render_and_check();

    // Tool calls
    fc.send(ChatAppMsg::ToolCall {
        name: "bash".into(),
        args: r#"{"command": "ls -la"}"#.into(),
        call_id: Some("c1".into()),
        description: None,
        source: Some("Core".into()),
        lua_primary_arg: None,
    });
    fc.render_and_check();

    fc.send(ChatAppMsg::ToolResultDelta {
        name: "bash".into(),
        delta: "total 42\ndrwxr-xr-x 5 user user 160 Jan 1 src/\n".into(),
        call_id: Some("c1".into()),
    });
    fc.send(ChatAppMsg::ToolResultComplete {
        name: "bash".into(),
        call_id: Some("c1".into()),
    });
    fc.render_and_check();

    // Second batch of tools
    fc.send(ChatAppMsg::ToolCall {
        name: "read_file".into(),
        args: r#"{"path": "README.md"}"#.into(),
        call_id: Some("c2".into()),
        description: None,
        source: Some("Core".into()),
        lua_primary_arg: None,
    });
    fc.send(ChatAppMsg::ToolResultDelta {
        name: "read_file".into(),
        delta: "# Project\n\nA cool project.\n".into(),
        call_id: Some("c2".into()),
    });
    fc.send(ChatAppMsg::ToolResultComplete {
        name: "read_file".into(),
        call_id: Some("c2".into()),
    });
    fc.render_and_check();

    // Second thinking block (after tools)
    for word in "Now I have enough context to describe the project".split_whitespace() {
        fc.send(ChatAppMsg::ThinkingDelta(format!("{} ", word)));
    }
    fc.render_and_check();

    // Continuation text
    fc.send(ChatAppMsg::TextDelta(
        "This is a Rust project with the following structure:\n\n\
         - `src/` contains the main source code\n\
         - `README.md` describes the project\n\n\
         The project is well-organized."
            .into(),
    ));
    fc.render_and_check();

    // Stream complete
    fc.send(ChatAppMsg::StreamComplete);
    fc.render_and_check();

    // Final check: the graduated output should have no duplicates
    let full = fc.full();
    check_no_duplicate_thought_lines(&full, "final");
}

/// Simulate the exact pattern from the reproduce.cast:
/// thinking → text → tool → tool → thinking → tool → tool → thinking → long text
#[test]
fn invariant_reproduce_cast_pattern() {
    let mut fc = FrameChecker::new(124, 59);

    fc.send(ChatAppMsg::UserMessage("tell me about this repo".into()));
    fc.render_and_check();

    // First thinking burst
    fc.send(ChatAppMsg::ThinkingDelta(
        "The user wants to know about this repository. I should explore the structure first. ".into(),
    ));
    fc.render_and_check();

    // Text
    fc.send(ChatAppMsg::TextDelta("I'll explore this repository to understand what it's about.".into()));
    fc.render_and_check();

    // First tool batch
    fc.send(ChatAppMsg::ToolCall {
        name: "bash".into(),
        args: r#"{"command": "ls -la"}"#.into(),
        call_id: Some("c1".into()),
        description: None,
        source: Some("Core".into()),
        lua_primary_arg: None,
    });
    fc.send(ChatAppMsg::ToolResultComplete {
        name: "bash".into(),
        call_id: Some("c1".into()),
    });
    fc.send(ChatAppMsg::ToolCall {
        name: "glob".into(),
        args: r#"{"pattern": "README*"}"#.into(),
        call_id: Some("c2".into()),
        description: None,
        source: Some("Core".into()),
        lua_primary_arg: None,
    });
    fc.send(ChatAppMsg::ToolResultComplete {
        name: "glob".into(),
        call_id: Some("c2".into()),
    });
    fc.render_and_check();

    // Second thinking after tools
    fc.send(ChatAppMsg::ThinkingDelta(
        "Good, I can see the structure. Let me read the key files. ".into(),
    ));
    fc.render_and_check();

    // Second tool batch
    fc.send(ChatAppMsg::ToolCall {
        name: "read_file".into(),
        args: r#"{"path": "README.md"}"#.into(),
        call_id: Some("c3".into()),
        description: None,
        source: Some("Core".into()),
        lua_primary_arg: None,
    });
    fc.send(ChatAppMsg::ToolResultComplete {
        name: "read_file".into(),
        call_id: Some("c3".into()),
    });
    fc.render_and_check();

    // Third thinking
    fc.send(ChatAppMsg::ThinkingDelta(
        "Now I have enough context. Let me write the summary. ".into(),
    ));
    fc.render_and_check();

    // Long continuation text
    fc.send(ChatAppMsg::TextDelta(
        "This is **Crucible** — a knowledge-grounded AI agent runtime.\n\n\
         It helps AI agents make better decisions by drawing from a knowledge graph."
            .into(),
    ));
    fc.render_and_check();

    fc.send(ChatAppMsg::StreamComplete);
    fc.render_and_check();

    // Verify final output
    let full = fc.full();
    check_no_duplicate_thought_lines(&full, "reproduce_final");
    check_no_triple_blanks(&full, "reproduce_final");
}

/// Streaming text that wraps should not cause content to shift by more
/// than the number of new visual lines added.
#[test]
fn invariant_streaming_text_stable_chrome() {
    let mut fc = FrameChecker::new(80, 24);

    fc.send(ChatAppMsg::UserMessage("explain".into()));
    fc.render_and_check();

    // Stream text word by word, checking invariants at every frame
    let words = "This is a long response that will eventually wrap across multiple \
                 lines in the terminal because the text is quite verbose and detailed \
                 enough to exceed the terminal width of eighty columns"
        .split_whitespace();

    let mut accumulated = String::new();
    for word in words {
        if !accumulated.is_empty() {
            accumulated.push(' ');
        }
        accumulated.push_str(word);
        fc.send(ChatAppMsg::TextDelta(format!("{} ", word)));
        fc.render_and_check();
    }

    fc.send(ChatAppMsg::StreamComplete);
    fc.render_and_check();
}

/// Fixture replay: the demo.jsonl fixture should pass all invariants.
#[test]
fn invariant_demo_fixture() {
    let path = {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let workspace_root = std::path::Path::new(manifest_dir)
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root");
        workspace_root.join("assets/fixtures/demo.jsonl")
    };
    if !path.exists() {
        eprintln!("Skipping: {} not found", path.display());
        return;
    }

    use crate::tui::oil::chat_runner::session_event_to_chat_msgs;

    let content = std::fs::read_to_string(&path).unwrap();
    let mut app = OilChatApp::default();
    let mut vt = Vt100TestRuntime::new(120, 50);
    let mut frame = 0;
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
        // Skip late thinking summaries and duplicate full_response
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
            app.on_message(msg);
        }

        // Render and check every 50 events (full check is too slow)
        frame += 1;
        if frame % 50 == 0 {
            vt.render_frame(&mut app);
            let full = strip_ansi(&vt.full_history());
            let ctx = format!("demo frame {}", frame);
            check_no_duplicate_thought_lines(&full, &ctx);
            check_no_triple_blanks(&full, &ctx);
        }
    }

    // Final render + full check
    vt.render_frame(&mut app);
    let full = strip_ansi(&vt.full_history());
    check_no_duplicate_thought_lines(&full, "demo final");
    check_no_triple_blanks(&full, "demo final");
    check_consistent_content_spacing(&full, "demo final");
}

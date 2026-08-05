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

/// Check that thinking never appears as two separate `◇ Thought` nodes with
/// only blank lines between them. All thinking within a turn must combine
/// into one node — split nodes mean graduation created a second container.
fn check_no_split_thinking_nodes(screen: &str, context: &str) {
    let lines: Vec<&str> = screen.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.starts_with("◇ Thought") || trimmed.starts_with("\u{25C7} Thought") {
            // Found a Thought line. Skip blank lines and check if next content
            // is another Thought (with no intervening tool/text/user content).
            let mut j = i + 1;
            while j < lines.len() && lines[j].trim().is_empty() {
                j += 1;
            }
            if j < lines.len() {
                let next = lines[j].trim();
                if next.starts_with("◇ Thought") || next.starts_with("\u{25C7} Thought") {
                    panic!(
                        "{}: split thinking nodes at lines {} and {}:\n  '{}'\n  '{}'\nFull screen:\n{}",
                        context, i, j, trimmed, next, screen
                    );
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
}

/// Check that every pair of adjacent top-level content containers has exactly
/// 1 blank line between them, EXCEPT adjacent tool calls which should have 0.
///
/// Only checks spacing between container headers (◇ Thought, ✓ Tool, user bars),
/// not within container content (e.g., inside expanded thinking blocks or
/// multi-line markdown). The `┌─ Thought` expanded header and its content are
/// treated as one block.
fn check_spacing_between_non_tool_containers(screen: &str, context: &str) {
    let lines: Vec<&str> = screen.lines().collect();

    #[derive(Debug, Clone, Copy, PartialEq)]
    enum BlockKind {
        ThoughtCollapsed, // ◇ Thought (N words)
        ThoughtExpanded,  // ┌─ Thought/Thinking header (expanded view)
        Tool,
        UserBottom, // ▀▀▀ bottom bar of user message
    }

    // First pass: identify regions inside expanded thinking (┌─ ... to next container)
    let mut in_expanded_thinking = false;
    let mut expanded_ranges: Vec<(usize, usize)> = Vec::new();
    let mut expand_start = 0;

    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if t.starts_with("\u{250c}\u{2500}") || t.starts_with("┌─") {
            // Start of expanded thinking block
            in_expanded_thinking = true;
            expand_start = i;
        } else if in_expanded_thinking {
            // End when we hit a container header or chrome
            let is_container = t.starts_with("◇ Thought")
                || t.starts_with("\u{25C7} Thought")
                || t.starts_with("✓ ")
                || t.starts_with("✗ ")
                || t.starts_with("● ")
                || (t.chars().all(|c| c == '▄' || c == ' ') && t.contains('▄') && t.len() > 10);
            if is_container {
                expanded_ranges.push((expand_start, i - 1));
                in_expanded_thinking = false;
            }
        }
    }
    if in_expanded_thinking {
        expanded_ranges.push((expand_start, lines.len() - 1));
    }

    let in_expanded =
        |row: usize| -> bool { expanded_ranges.iter().any(|&(s, e)| row >= s && row <= e) };

    // Classify only container-level headers
    let mut blocks: Vec<(usize, BlockKind)> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if in_expanded(i) {
            // Inside expanded thinking — treat the header line only
            let t = line.trim();
            if (t.starts_with("\u{250c}\u{2500}") || t.starts_with("┌─"))
                && i == expanded_ranges
                    .iter()
                    .find(|&&(s, _)| s == i)
                    .map(|&(s, _)| s)
                    .unwrap_or(usize::MAX)
            {
                blocks.push((i, BlockKind::ThoughtExpanded));
            }
            continue;
        }

        let t = line.trim();
        if t.starts_with("◇ Thought") || t.starts_with("\u{25C7} Thought") {
            blocks.push((i, BlockKind::ThoughtCollapsed));
        } else if t.starts_with("✓ ") || t.starts_with("✗ ") || t.starts_with("● ") {
            blocks.push((i, BlockKind::Tool));
        } else if t.chars().all(|c| c == '▀' || c == ' ') && t.contains('▀') && t.len() > 10 {
            blocks.push((i, BlockKind::UserBottom));
        }
    }

    // Check spacing between consecutive container headers
    for window in blocks.windows(2) {
        let (row_a, kind_a) = window[0];
        let (row_b, kind_b) = window[1];

        // For expanded thinking, find the end of its content
        let effective_end_a = if kind_a == BlockKind::ThoughtExpanded {
            expanded_ranges
                .iter()
                .find(|&&(s, _)| s == row_a)
                .map(|&(_, e)| e)
                .unwrap_or(row_a)
        } else if kind_a == BlockKind::Tool {
            // Tool may have continuation lines (│)
            let mut end = row_a;
            for (offset, line) in lines[(row_a + 1)..row_b].iter().enumerate() {
                let t = line.trim();
                if t.starts_with("│") || t.starts_with("│") {
                    end = row_a + 1 + offset;
                } else {
                    break;
                }
            }
            end
        } else {
            row_a
        };

        let gap_lines = &lines[effective_end_a + 1..row_b];
        let blanks = gap_lines.iter().filter(|l| l.trim().is_empty()).count();
        let non_blanks = gap_lines.iter().filter(|l| !l.trim().is_empty()).count();

        // Only check when there's a clean gap (no intervening content)
        if non_blanks > 0 {
            continue;
        }

        // Skip if first block is at R0 — spacing may be in off-screen scrollback
        if row_a == 0 {
            continue;
        }

        let both_tools = kind_a == BlockKind::Tool && kind_b == BlockKind::Tool;

        if both_tools {
            if blanks > 0 {
                panic!(
                    "{}: unexpected gap ({} blanks) between adjacent tools at R{} and R{}\nScreen:\n{}",
                    context, blanks, row_a, row_b, screen
                );
            }
        } else if blanks == 0 && effective_end_a + 1 < row_b {
            // Should have spacing but doesn't — direct adjacency
        } else if blanks == 0 && effective_end_a + 1 == row_b {
            panic!(
                "{}: no spacing between {:?} (R{}) and {:?} (R{})\nScreen:\n{}",
                context, kind_a, row_a, kind_b, row_b, screen
            );
        }
    }
}

/// The screen should never show BOTH a graduated `◇ Thought (N words)` AND
/// an active `Thinking… (N words)` with the same word count. Same count means
/// the graduated copy wasn't absorbed — it's a duplicate.
fn check_no_simultaneous_thought_and_thinking(screen: &str, context: &str) {
    use std::collections::HashSet;
    let mut graduated_counts: HashSet<String> = HashSet::new();

    for line in screen.lines() {
        let t = line.trim();
        // Extract word count from "◇ Thought (N words)"
        if (t.starts_with("◇ Thought") || t.starts_with("\u{25C7} Thought"))
            && t.contains(" words)")
        {
            if let Some(count) = extract_word_count(t) {
                graduated_counts.insert(count);
            }
        }
    }

    if graduated_counts.is_empty() {
        return;
    }

    for line in screen.lines() {
        let t = line.trim();
        if t.contains("Thinking\u{2026}") && !t.contains("Thought") && t.contains(" words)") {
            if let Some(count) = extract_word_count(t) {
                if graduated_counts.contains(&count) {
                    panic!(
                        "{}: simultaneous graduated Thought and active Thinking with same count '{}'\nScreen:\n{}",
                        context, count, screen
                    );
                }
            }
        }
    }
}

/// Adjacent thinking indicators (no intervening content) must have strictly
/// increasing word counts. Two thoughts separated by tools/text are from
/// different turns and may have any word counts.
fn check_thinking_word_count_monotonic(screen: &str, context: &str) {
    let lines: Vec<&str> = screen.lines().collect();
    let mut last_thought: Option<(usize, usize)> = None; // (line_num, count)

    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        let is_thought = t.starts_with("◇ Thought") || t.starts_with("\u{25C7} Thought");
        let is_thinking = t.contains("Thinking\u{2026}") && !t.contains("Thought");

        if (is_thought || is_thinking) && t.contains(" words)") {
            if let Some(count_str) = extract_word_count(t) {
                if let Ok(n) = count_str.parse::<usize>() {
                    if let Some((prev_line, prev_count)) = last_thought {
                        // Check if there's only blank lines between prev and current
                        let between = &lines[prev_line + 1..i];
                        let only_blanks = between.iter().all(|l| l.trim().is_empty());
                        if only_blanks && n <= prev_count {
                            panic!(
                                "{}: adjacent thinking word count not monotonic: {} (R{}) >= {} (R{})\n\
                                 Adjacent thoughts with no intervening content must increase.\nScreen:\n{}",
                                context, prev_count, prev_line, n, i, screen
                            );
                        }
                    }
                    last_thought = Some((i, n));
                }
            }
        } else if !t.is_empty() {
            // Non-thinking, non-blank content resets tracking
            // (thoughts separated by tools/text are from different turns)
            last_thought = None;
        }
    }
}

/// Extract the word count string from a line like "◇ Thought (42 words)" or "◐ Thinking… (42 words)"
fn extract_word_count(line: &str) -> Option<String> {
    let start = line.find('(')? + 1;
    let end = line.find(" words)")?;
    if start < end {
        Some(line[start..end].to_string())
    } else {
        None
    }
}

// check_turn_indicator_spacing_symmetry was removed — the turn indicator is
// now part of the chrome group (tight with input). The blank line above chrome
// is content→chrome separation via Padding { top: 1 }, which is correct.

// check_no_thinking_in_content_and_chrome was removed — it flagged cross-turn
// thinking (graduated Thought from turn 1 + active Thinking for turn 2), which
// is correct behavior. check_no_simultaneous_thought_and_thinking covers the
// actual bug (same word count = same turn duplicated in both places).

// ─── Multi-frame test helper ───────────────────────────────────────────────

struct FrameChecker {
    app: OilChatApp,
    vt: Vt100TestRuntime,
    frame_count: usize,
    /// Last N frames for diagnostic output on failure.
    frame_history: Vec<String>,
}

const FRAME_HISTORY_SIZE: usize = 5;

impl FrameChecker {
    fn new(width: u16, height: u16) -> Self {
        Self {
            app: OilChatApp::default(),
            vt: Vt100TestRuntime::new(width, height),
            frame_count: 0,
            frame_history: Vec::new(),
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

        // Save frame for diagnostic context
        if self.frame_history.len() >= FRAME_HISTORY_SIZE {
            self.frame_history.remove(0);
        }
        self.frame_history
            .push(format!("=== {} ===\n{}", ctx, stripped));

        check_no_duplicate_thought_lines(&stripped, &ctx);
        check_no_triple_blanks(&stripped, &ctx);
        check_consistent_content_spacing(&stripped, &ctx);
    }

    #[allow(dead_code)]
    fn scrollback(&mut self) -> String {
        strip_ansi(&self.vt.scrollback_contents())
    }

    fn full(&self) -> String {
        strip_ansi(&self.vt.full_history())
    }

    /// Dump recent frame history for debugging.
    #[allow(dead_code)]
    fn dump_history(&self) -> String {
        self.frame_history.join("\n\n")
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
    for word in "I need to explore the repository structure first to understand the codebase"
        .split_whitespace()
    {
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
        diffs: Vec::new(),
        auto_approved: None,
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
        diffs: Vec::new(),
        auto_approved: None,
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
        "The user wants to know about this repository. I should explore the structure first. "
            .into(),
    ));
    fc.render_and_check();

    // Text
    fc.send(ChatAppMsg::TextDelta(
        "I'll explore this repository to understand what it's about.".into(),
    ));
    fc.render_and_check();

    // First tool batch
    fc.send(ChatAppMsg::ToolCall {
        name: "bash".into(),
        args: r#"{"command": "ls -la"}"#.into(),
        call_id: Some("c1".into()),
        description: None,
        source: Some("Core".into()),
        lua_primary_arg: None,
        diffs: Vec::new(),
        auto_approved: None,
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
        diffs: Vec::new(),
        auto_approved: None,
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
        diffs: Vec::new(),
        auto_approved: None,
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
    let path = super::helpers::fixture_path("demo.jsonl");

    use crate::tui::oil::chat_runner::SessionEventStream;

    let content = std::fs::read_to_string(&path).unwrap();
    let mut app = OilChatApp::default();
    let mut vt = Vt100TestRuntime::new(120, 50);
    let mut frame = 0;
    // The production converter, not a copy of it, so this replay matches what a
    // live console renders.
    //
    // It is *not* the thing that guards reasoning-replay suppression: the only
    // thought check here, `check_no_duplicate_thought_lines`, fires on two
    // adjacent identical collapsed headers, so a replay appended into an
    // existing thinking node is invisible to it — this test stays green with
    // the suppression removed. The count assertions in
    // `session_event_stream_tests::recorded_fixtures_render_exactly_their_non_replayed_thoughts`
    // are what pin that.
    let mut stream = SessionEventStream::new();

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
        let data = value.get("data").cloned().unwrap_or_default();
        for msg in stream.translate(event_type, &data) {
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

// ─── Soft invariant checkers (return violation string instead of panicking) ─

fn soft_check<F: FnOnce()>(f: F) -> Option<String> {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    match result {
        Ok(()) => None,
        Err(e) => {
            let msg = if let Some(s) = e.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = e.downcast_ref::<&str>() {
                s.to_string()
            } else {
                "unknown panic".to_string()
            };
            Some(msg)
        }
    }
}

/// The ACP presentation-parity pairs, checked after every single event.
///
/// These are the only fixtures carrying `"source":"Acp:claude"` or a
/// `tool_call_diff_update` — a diff that lands on a card *after* the card was
/// announced, which no other recording in `assets/fixtures` produces. The
/// story tests render one frame from each; nothing swept the frames in
/// between, so a transient geometry break on the ACP-only path (the moment the
/// late diff expands an already-painted card, say) had nowhere to surface.
///
/// They are small enough to run the full checker set on every frame rather
/// than sampling, and they go through the production `SessionEventStream`, so
/// this replays what a live console would render.
#[test]
fn invariant_acp_parity_fixtures_every_frame() {
    use crate::tui::oil::chat_runner::SessionEventStream;

    for fixture in [
        "acp_parity_internal.jsonl",
        "acp_parity_delegated.jsonl",
        "acp_parity_read_internal.jsonl",
        "acp_parity_read_delegated.jsonl",
    ] {
        let content = super::helpers::read_fixture(fixture);
        let mut app = OilChatApp::default();
        let mut vt = Vt100TestRuntime::new(80, 24);
        let mut stream = SessionEventStream::new();
        let mut frame = 0usize;

        for line in content.lines() {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            // The recording header and footer carry no `event`.
            let Some(event_type) = value.get("event").and_then(|v| v.as_str()) else {
                continue;
            };
            let data = value.get("data").cloned().unwrap_or_default();
            for msg in stream.translate(event_type, &data) {
                app.on_message(msg);
            }

            vt.render_frame(&mut app);
            frame += 1;
            let full = strip_ansi(&vt.full_history());
            let screen = strip_ansi(&vt.screen_contents());
            let ctx = format!("{fixture} frame {frame} (event={event_type})");

            check_no_duplicate_thought_lines(&full, &ctx);
            check_no_triple_blanks(&full, &ctx);
            check_consistent_content_spacing(&full, &ctx);
            check_no_split_thinking_nodes(&screen, &ctx);
            check_no_simultaneous_thought_and_thinking(&screen, &ctx);
            check_thinking_word_count_monotonic(&screen, &ctx);
            // `check_spacing_between_non_tool_containers` is deliberately not
            // in this list. It classifies any line starting `● ` as a tool
            // card, but that glyph is also the assistant's response bullet, so
            // on a turn whose text is immediately followed by a pending tool
            // (which is every frame of these fixtures) it reads " ● I'll fix
            // the greeting." and " ● Edit File greeting.rs" as two adjacent
            // tools and demands they be flush. The blank line between a
            // paragraph and a tool card is correct; the checker cannot tell
            // the two bullets apart from stripped text.
        }

        assert!(
            frame >= 7,
            "{fixture} produced only {frame} frames — the fixture lost events"
        );
    }
}

/// Replay reproduce.jsonl frame-by-frame, rendering and checking ALL invariants
/// after EVERY SINGLE event. Collects all violations across all frames, then
/// fails with a comprehensive report.
///
/// Terminal size matches the original recording: 124 cols × 59 rows.
#[test]
fn invariant_reproduce_jsonl_every_frame() {
    use crate::tui::oil::chat_runner::session_event_to_chat_msgs;

    let path = super::helpers::fixture_path("reproduce.jsonl");

    let content = std::fs::read_to_string(&path).unwrap();
    let mut app = OilChatApp::default();
    // Match the actual session: thinking was collapsed (show_thinking=false)
    app.set_show_thinking(false);
    let mut vt = Vt100TestRuntime::new(124, 59);
    let mut frame: usize = 0;
    let mut saw_text_delta = false;
    let mut violations: Vec<String> = Vec::new();

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
        // This one deliberately feeds the *un-deduped* worst case: raw
        // `session_event_to_chat_msgs` with no reasoning-replay suppression, so
        // every thinking event in the recording reaches the app including the
        // end-of-stream replays. The layout invariants below must hold on that
        // input too — a console is not allowed to break its geometry just
        // because it was handed more thoughts than a deduped stream would send.
        // (The suppression itself is pinned in `session_event_stream_tests`.)
        // The only thing skipped here is `message_complete`'s full_response
        // text, which would double-count what already streamed as text_delta.

        // Skip non-rendering events
        if event_type == "precognition_complete"
            || event_type == "interaction_requested"
            || event_type == "interaction_completed"
            || event_type == "post_llm_call"
        {
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

        // Render after EVERY event
        vt.render_frame(&mut app);
        frame += 1;

        let full = strip_ansi(&vt.full_history());
        let screen = strip_ansi(&vt.screen_contents());
        let ctx = format!(
            "frame {} (seq={}, event={})",
            frame,
            value.get("seq").and_then(|v| v.as_u64()).unwrap_or(0),
            event_type,
        );

        // Check ALL invariants on every frame, collecting violations
        let checks: Vec<Box<dyn FnOnce() + '_>> = vec![
            Box::new(|| check_no_duplicate_thought_lines(&full, &ctx)),
            Box::new(|| check_no_triple_blanks(&full, &ctx)),
            Box::new(|| check_consistent_content_spacing(&full, &ctx)),
            Box::new(|| check_no_split_thinking_nodes(&screen, &ctx)),
            Box::new(|| check_spacing_between_non_tool_containers(&screen, &ctx)),
            Box::new(|| check_no_simultaneous_thought_and_thinking(&screen, &ctx)),
            Box::new(|| check_thinking_word_count_monotonic(&screen, &ctx)),
        ];

        for check in checks {
            if let Some(msg) = soft_check(check) {
                // Extract just the violation type (strip frame-specific context)
                let short = msg.lines().next().unwrap_or(&msg).to_string();
                // Deduplicate by violation category (text after the context prefix)
                let category = if let Some(pos) = short.find("): ") {
                    &short[pos + 3..]
                } else {
                    &short
                };
                // Keep first occurrence of each distinct violation category
                if !violations.iter().any(|v| {
                    let v_cat = if let Some(p) = v.find("): ") {
                        &v[p + 3..]
                    } else {
                        v
                    };
                    v_cat == category
                }) {
                    violations.push(short);
                }
            }
        }
    }

    assert!(frame > 100, "Expected many frames, got {}", frame);

    // Final comprehensive check
    let full = strip_ansi(&vt.full_history());
    let screen = strip_ansi(&vt.screen_contents());
    type FinalCheck<'a> = (&'a str, Box<dyn FnOnce() + 'a>);
    let final_checks: Vec<FinalCheck<'_>> = vec![
        (
            "final:duplicate_thought",
            Box::new(|| check_no_duplicate_thought_lines(&full, "reproduce final")),
        ),
        (
            "final:triple_blanks",
            Box::new(|| check_no_triple_blanks(&full, "reproduce final")),
        ),
        (
            "final:content_spacing",
            Box::new(|| check_consistent_content_spacing(&full, "reproduce final")),
        ),
        (
            "final:split_thinking",
            Box::new(|| check_no_split_thinking_nodes(&screen, "reproduce final screen")),
        ),
        (
            "final:simultaneous_thought_thinking",
            Box::new(|| {
                check_no_simultaneous_thought_and_thinking(&screen, "reproduce final screen")
            }),
        ),
        (
            "final:monotonic_word_count",
            Box::new(|| check_thinking_word_count_monotonic(&screen, "reproduce final screen")),
        ),
    ];

    for (label, check) in final_checks {
        if let Some(msg) = soft_check(check) {
            violations.push(format!(
                "[{}] {}",
                label,
                msg.lines().next().unwrap_or(&msg)
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "reproduce.jsonl invariant violations ({} unique across {} frames):\n{}",
        violations.len(),
        frame,
        violations
            .iter()
            .enumerate()
            .map(|(i, v)| format!("  {}. {}", i + 1, v))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

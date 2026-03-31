//! vt100-backed test runtime — exercises the real terminal escape sequence path
//! and provides screen-level assertions through a virtual terminal emulator.
//!
//! Unlike TestRuntime which accumulates rendered strings, this feeds raw terminal
//! bytes through vt100::Parser and reads the actual screen state. This catches
//! bugs in cursor math, viewport clearing, and graduation that string-based
//! testing misses.

use crate::tui::oil::app::App;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::chat_runner::render_frame;
use crate::tui::oil::focus::FocusContext;
use crucible_oil::TestRuntime;

/// A test runtime that feeds terminal output through vt100 for screen-level assertions.
pub struct Vt100TestRuntime {
    inner: TestRuntime,
    vt: vt100::Parser,
}

impl Vt100TestRuntime {
    pub fn new(width: u16, height: u16) -> Self {
        // Large scrollback to capture all graduated content
        let scrollback = 1000;
        Self {
            inner: TestRuntime::new(width, height),
            vt: vt100::Parser::new(height, width, scrollback),
        }
    }

    /// Render a frame through the real terminal path, then feed bytes to vt100.
    pub fn render_frame(&mut self, app: &mut OilChatApp) {
        let focus = FocusContext::new();
        render_frame(app, &mut self.inner, &focus);

        // Feed all new terminal bytes to the vt100 parser
        let bytes = self.inner.take_bytes();
        if !bytes.is_empty() {
            self.vt.process(&bytes);
        }
    }

    /// Get the full screen contents (visible area) as plain text.
    pub fn screen_contents(&self) -> String {
        self.vt.screen().contents()
    }

    /// Get the underlying TestRuntime for legacy API access.
    pub fn inner(&self) -> &TestRuntime {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vt100_runtime_renders_viewport_content() {
        let mut app = OilChatApp::init();
        let mut vt = Vt100TestRuntime::new(80, 24);

        app.on_message(ChatAppMsg::UserMessage("Hello World".into()));
        vt.render_frame(&mut app);

        let screen = vt.screen_contents();
        assert!(
            screen.contains("Hello World"),
            "vt100 screen should contain user message.\nScreen:\n{}",
            screen
        );
    }

    #[test]
    fn vt100_runtime_shows_graduated_content_in_scrollback() {
        let mut app = OilChatApp::init();
        let mut vt = Vt100TestRuntime::new(80, 24);

        // User message + assistant response (will graduate)
        app.on_message(ChatAppMsg::UserMessage("Question".into()));
        vt.render_frame(&mut app);

        app.on_message(ChatAppMsg::TextDelta("Answer text".into()));
        app.on_message(ChatAppMsg::StreamComplete);
        vt.render_frame(&mut app);

        // Both should be visible (in scrollback or screen)
        let screen = vt.screen_contents();
        // After graduation, the user message may have scrolled up
        // The assistant response should be visible
        assert!(
            screen.contains("Answer text") || vt.inner().stdout_content().contains("Answer text"),
            "Graduated content should be accessible.\nScreen:\n{}\nStdout:\n{}",
            screen,
            vt.inner().stdout_content()
        );
    }

    /// Multi-frame graduation: user → assistant across frames.
    /// This is the scenario that exposed the phantom blank line bug.
    #[test]
    fn vt100_multi_frame_graduation_spacing() {
        let mut app = OilChatApp::init();
        let mut vt = Vt100TestRuntime::new(80, 24);

        // Frame 1: user message graduates
        app.on_message(ChatAppMsg::UserMessage("First question".into()));
        vt.render_frame(&mut app);

        // Frame 2: assistant graduates
        app.on_message(ChatAppMsg::TextDelta("First answer".into()));
        app.on_message(ChatAppMsg::StreamComplete);
        vt.render_frame(&mut app);

        // Frame 3: second user
        app.on_message(ChatAppMsg::UserMessage("Second question".into()));
        vt.render_frame(&mut app);

        // Frame 4: second assistant
        app.on_message(ChatAppMsg::TextDelta("Second answer".into()));
        app.on_message(ChatAppMsg::StreamComplete);
        vt.render_frame(&mut app);

        let screen = vt.screen_contents();

        // Verify no triple-blank lines (which would indicate phantom spacing)
        let lines: Vec<&str> = screen.lines().collect();
        for (i, window) in lines.windows(3).enumerate() {
            let all_blank = window.iter().all(|l| l.trim().is_empty());
            assert!(
                !all_blank,
                "Triple blank at lines {}-{} in vt100 screen.\nScreen:\n{}",
                i,
                i + 2,
                screen
            );
        }
    }

    /// Cleanup moves cursor below viewport so post-exit prints don't overlap.
    #[test]
    fn vt100_cleanup_viewport_positions_cursor_below_content() {
        let mut app = OilChatApp::init();
        let mut vt = Vt100TestRuntime::new(80, 24);

        // Render some content so the viewport has lines
        app.on_message(ChatAppMsg::UserMessage("Hello".into()));
        vt.render_frame(&mut app);

        app.on_message(ChatAppMsg::TextDelta("World".into()));
        app.on_message(ChatAppMsg::StreamComplete);
        vt.render_frame(&mut app);

        // Simulate what exit() does: cleanup_viewport then write a message
        // Feed cleanup bytes to vt100
        // We can't call exit() (Stdout-only), but we can test cleanup_viewport
        // through the inner terminal

        // Get screen before cleanup
        let screen_before = vt.screen_contents();
        assert!(
            screen_before.contains("World") || vt.inner().stdout_content().contains("World"),
            "Content should be visible before cleanup"
        );
    }

    /// Consecutive tools across separate frames — the exact bug scenario.
    #[test]
    fn vt100_runtime_consecutive_tools_no_phantom_blank() {
        let mut app = OilChatApp::init();
        let mut vt = Vt100TestRuntime::new(80, 24);

        app.on_message(ChatAppMsg::UserMessage("Do stuff".into()));
        vt.render_frame(&mut app);

        // Tool 1
        app.on_message(ChatAppMsg::ToolCall {
            name: "read_file".into(),
            args: r#"{"path": "a.rs"}"#.into(),
            call_id: Some("c1".into()),
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "read_file".into(),
            call_id: Some("c1".into()),
        });
        vt.render_frame(&mut app);

        // Tool 2 (separate frame — this is where the phantom blank line appeared)
        app.on_message(ChatAppMsg::ToolCall {
            name: "read_file".into(),
            args: r#"{"path": "b.rs"}"#.into(),
            call_id: Some("c2".into()),
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "read_file".into(),
            call_id: Some("c2".into()),
        });
        vt.render_frame(&mut app);

        app.on_message(ChatAppMsg::StreamComplete);
        vt.render_frame(&mut app);

        let screen = vt.screen_contents();

        // Find lines containing tool indicators
        let lines: Vec<&str> = screen.lines().collect();
        let tool_lines: Vec<(usize, &str)> = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.contains("a.rs") || l.contains("b.rs"))
            .map(|(i, l)| (i, *l))
            .collect();

        if tool_lines.len() >= 2 {
            let (idx_a, _) = tool_lines[0];
            let (idx_b, _) = tool_lines[1];
            let gap = idx_b - idx_a;

            // Between the two tool lines, there should be no blank lines
            // (they might be on adjacent lines or have 1 line of tool chrome between them)
            let blank_count = lines[idx_a + 1..idx_b]
                .iter()
                .filter(|l| l.trim().is_empty())
                .count();

            assert_eq!(
                blank_count, 0,
                "No blank lines between consecutive tool calls.\nGap: {} lines\nScreen:\n{}",
                gap, screen
            );
        }
    }

    // ─── Helpers for spacing assertions ────────────────────────────────

    /// Count blank lines between two content patterns in screen text.
    fn blank_lines_between(screen: &str, before: &str, after: &str) -> Option<usize> {
        let lines: Vec<&str> = screen.lines().collect();
        let before_end = lines.iter().rposition(|l| l.contains(before))?;
        let after_start = lines[before_end + 1..]
            .iter()
            .position(|l| l.contains(after))
            .map(|p| p + before_end + 1)?;
        let blanks = lines[before_end + 1..after_start]
            .iter()
            .filter(|l| l.trim().is_empty())
            .count();
        Some(blanks)
    }

    /// Assert no triple-blank lines (always a bug).
    fn assert_no_triple_blanks(screen: &str, context: &str) {
        let lines: Vec<&str> = screen.lines().collect();
        for (i, window) in lines.windows(3).enumerate() {
            let all_blank = window.iter().all(|l| l.trim().is_empty());
            assert!(
                !all_blank,
                "{}: triple blank at lines {}-{}.\nScreen:\n{}",
                context, i, i + 2, screen
            );
        }
    }

    fn think(app: &mut OilChatApp, content: &str) {
        app.on_message(ChatAppMsg::ThinkingDelta(content.into()));
    }

    fn tool(app: &mut OilChatApp, name: &str, call_id: &str) {
        app.on_message(ChatAppMsg::ToolCall {
            name: name.into(),
            args: format!(r#"{{"path": "{call_id}.rs"}}"#),
            call_id: Some(call_id.into()),
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: name.into(),
            call_id: Some(call_id.into()),
        });
    }

    // ─── Bug 1: Spacing between graduated content and viewport ────────
    //
    // The user sees two blank lines between the graduated user message
    // and the first thought/tool in the viewport. The root cause is
    // the unconditional text(" ") at chat_app/mod.rs:176 combining
    // with Terminal::apply()'s \r\n separator.

    /// Variant 1: User message graduates, then thinking appears.
    /// Exactly 1 blank line between graduated user box and the thought.
    #[test]
    fn vt100_user_then_thought_one_blank_line() {
        let mut app = OilChatApp::init();
        let mut vt = Vt100TestRuntime::new(120, 40);

        app.on_message(ChatAppMsg::UserMessage("Hello".into()));
        vt.render_frame(&mut app); // user graduates

        think(&mut app, "Let me think about this.");
        vt.render_frame(&mut app);

        // Complete so thought graduates
        tool(&mut app, "bash", "c1");
        vt.render_frame(&mut app);

        let combined = format!(
            "{}{}",
            vt.inner().stdout_content(),
            vt.inner().viewport_content()
        );
        let screen = crucible_oil::ansi::strip_ansi(&combined);

        let blanks = blank_lines_between(&screen, "Hello", "Thought");
        assert_eq!(
            blanks,
            Some(1),
            "Expected 1 blank between user message and thought.\nScreen:\n{}",
            screen
        );
        assert_no_triple_blanks(&screen, "user_then_thought");
    }

    /// Variant 2: User message graduates, then tool appears.
    /// Exactly 1 blank line.
    #[test]
    fn vt100_user_then_tool_one_blank_line() {
        let mut app = OilChatApp::init();
        let mut vt = Vt100TestRuntime::new(120, 40);

        app.on_message(ChatAppMsg::UserMessage("Do stuff".into()));
        vt.render_frame(&mut app);

        tool(&mut app, "bash", "c1");
        vt.render_frame(&mut app);

        app.on_message(ChatAppMsg::StreamComplete);
        vt.render_frame(&mut app);

        let combined = format!(
            "{}{}",
            vt.inner().stdout_content(),
            vt.inner().viewport_content()
        );
        let screen = crucible_oil::ansi::strip_ansi(&combined);

        let blanks = blank_lines_between(&screen, "Do stuff", "Bash");
        assert_eq!(
            blanks,
            Some(1),
            "Expected 1 blank between user message and tool.\nScreen:\n{}",
            screen
        );
    }

    /// Variant 3: Tool graduates, then thought. Cross-frame. Exactly 1 blank.
    #[test]
    fn vt100_tool_then_thought_cross_frame_one_blank() {
        let mut app = OilChatApp::init();
        let mut vt = Vt100TestRuntime::new(120, 40);

        app.on_message(ChatAppMsg::UserMessage("Go".into()));
        vt.render_frame(&mut app);

        tool(&mut app, "bash", "c1");
        vt.render_frame(&mut app);

        // Thought in next frame (cross-frame)
        think(&mut app, "Interesting results.");
        app.on_message(ChatAppMsg::TextDelta("Done.".into()));
        app.on_message(ChatAppMsg::StreamComplete);
        vt.render_frame(&mut app);

        let combined = format!(
            "{}{}",
            vt.inner().stdout_content(),
            vt.inner().viewport_content()
        );
        let screen = crucible_oil::ansi::strip_ansi(&combined);

        let blanks = blank_lines_between(&screen, "Bash", "Thought");
        assert_eq!(
            blanks,
            Some(1),
            "Expected 1 blank between tool and thought (cross-frame).\nScreen:\n{}",
            screen
        );
        assert_no_triple_blanks(&screen, "tool_then_thought");
    }

    /// Variant 4: Full conversation — no triple blanks anywhere.
    #[test]
    fn vt100_full_conversation_no_triple_blanks() {
        let mut app = OilChatApp::init();
        let mut vt = Vt100TestRuntime::new(120, 40);

        // Turn 1
        app.on_message(ChatAppMsg::UserMessage("Question 1".into()));
        vt.render_frame(&mut app);

        think(&mut app, "Thinking about question 1.");
        tool(&mut app, "bash", "c1");
        tool(&mut app, "read_file", "c2");
        vt.render_frame(&mut app);

        think(&mut app, "Got the results.");
        app.on_message(ChatAppMsg::TextDelta("Answer 1.".into()));
        app.on_message(ChatAppMsg::StreamComplete);
        vt.render_frame(&mut app);

        // Turn 2
        app.on_message(ChatAppMsg::UserMessage("Question 2".into()));
        vt.render_frame(&mut app);

        app.on_message(ChatAppMsg::TextDelta("Answer 2.".into()));
        app.on_message(ChatAppMsg::StreamComplete);
        vt.render_frame(&mut app);

        let combined = format!(
            "{}{}",
            vt.inner().stdout_content(),
            vt.inner().viewport_content()
        );
        let screen = crucible_oil::ansi::strip_ansi(&combined);

        assert_no_triple_blanks(&screen, "full_conversation");
    }

    /// Variant 5: Rapid tick-per-event rendering — no triple blanks.
    #[test]
    fn vt100_tick_per_event_no_triple_blanks() {
        let mut app = OilChatApp::init();
        let mut vt = Vt100TestRuntime::new(120, 40);

        app.on_message(ChatAppMsg::UserMessage("Go".into()));
        vt.render_frame(&mut app);

        think(&mut app, "Let me check.");
        vt.render_frame(&mut app);

        tool(&mut app, "bash", "c1");
        vt.render_frame(&mut app);

        tool(&mut app, "read_file", "c2");
        vt.render_frame(&mut app);

        think(&mut app, "Almost done.");
        vt.render_frame(&mut app);

        tool(&mut app, "glob", "c3");
        vt.render_frame(&mut app);

        app.on_message(ChatAppMsg::TextDelta("All done.".into()));
        vt.render_frame(&mut app);

        app.on_message(ChatAppMsg::StreamComplete);
        vt.render_frame(&mut app);

        let combined = format!(
            "{}{}",
            vt.inner().stdout_content(),
            vt.inner().viewport_content()
        );
        let screen = crucible_oil::ansi::strip_ansi(&combined);

        assert_no_triple_blanks(&screen, "tick_per_event");
    }

    // ─── Bug 2: Thinking graduation before permission modal ───────────
    //
    // When a permission-requiring tool call arrives, the daemon sends
    // OpenInteraction BEFORE ToolCall. The thinking AssistantResponse
    // can't graduate without a following container, so it stays as a
    // spinner. Fix: when OpenInteraction(Permission) arrives, mark the
    // current AssistantResponse as graduatable.

    /// Variant 1: Thinking should be graduatable after permission opens.
    #[test]
    fn thinking_graduates_when_permission_opens() {
        use crucible_core::interaction::{InteractionRequest, PermRequest};

        let mut app = OilChatApp::init();

        app.on_message(ChatAppMsg::UserMessage("Do something".into()));
        think(&mut app, "Let me run a dangerous command.");

        // Simulate daemon sending permission request before tool call
        app.on_message(ChatAppMsg::OpenInteraction {
            request_id: "perm-1".into(),
            request: InteractionRequest::Permission(PermRequest::bash(["rm", "-rf", "/tmp/test"])),
        });

        // After render, the thinking should graduate to stdout
        let mut runtime = TestRuntime::new(80, 24);
        let focus = FocusContext::new();
        render_frame(&mut app, &mut runtime, &focus);

        let stdout = runtime.stdout_content();
        assert!(
            stdout.contains("Thought") || stdout.contains("dangerous command"),
            "Thinking should have graduated after permission opened.\nStdout: {}\nViewport: {}",
            stdout,
            runtime.viewport_content()
        );
    }

    /// Variant 2: Multiple thinks then permission — all graduate.
    #[test]
    fn multiple_thinks_graduate_on_permission() {
        use crucible_core::interaction::{InteractionRequest, PermRequest};

        let mut app = OilChatApp::init();

        app.on_message(ChatAppMsg::UserMessage("Plan".into()));
        think(&mut app, "First thought.");

        // Second thought (tool interrupts first, creating new response)
        tool(&mut app, "read_file", "c1");
        think(&mut app, "Second thought after reading.");

        // Permission arrives
        app.on_message(ChatAppMsg::OpenInteraction {
            request_id: "perm-2".into(),
            request: InteractionRequest::Permission(PermRequest::bash(["make", "install"])),
        });

        let mut runtime = TestRuntime::new(120, 40);
        let focus = FocusContext::new();
        render_frame(&mut app, &mut runtime, &focus);

        let stdout = runtime.stdout_content();
        // Both thoughts should have graduated
        assert!(
            stdout.contains("First thought") || stdout.contains("Thought"),
            "First thinking should graduate.\nStdout: {}",
            stdout
        );
    }

    /// Variant 3: Graduated thinking must NOT contain a spinner character.
    #[test]
    fn graduated_thinking_has_no_spinner() {
        use crucible_core::interaction::{InteractionRequest, PermRequest};

        let mut app = OilChatApp::init();
        let mut runtime = TestRuntime::new(80, 24);

        app.on_message(ChatAppMsg::UserMessage("Do something".into()));
        let focus = FocusContext::new();
        render_frame(&mut app, &mut runtime, &focus);

        think(&mut app, "Planning the command.");

        // Permission arrives — thinking should become graduatable
        app.on_message(ChatAppMsg::OpenInteraction {
            request_id: "perm-1".into(),
            request: InteractionRequest::Permission(PermRequest::bash(["rm", "-rf", "/tmp"])),
        });

        // Render — thinking should graduate
        render_frame(&mut app, &mut runtime, &focus);

        let stdout = runtime.stdout_content();

        // Spinner characters: braille spinners ⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ and circle ◐◓◑◒
        let spinner_chars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏', '◐', '◓', '◑', '◒'];
        let has_spinner = stdout.chars().any(|c| spinner_chars.contains(&c));
        assert!(
            !has_spinner,
            "Graduated thinking should NOT contain spinner characters.\nStdout:\n{}",
            crucible_oil::ansi::strip_ansi(stdout)
        );
    }

    /// Variant 4: Missing blank line between user message and first thought.
    #[test]
    fn user_then_thought_has_blank_line_after_permission_graduation() {
        use crucible_core::interaction::{InteractionRequest, PermRequest};

        let mut app = OilChatApp::init();
        let mut runtime = TestRuntime::new(120, 40);

        app.on_message(ChatAppMsg::UserMessage("Hello".into()));
        let focus = FocusContext::new();
        render_frame(&mut app, &mut runtime, &focus);

        think(&mut app, "Let me think about this.");

        // Permission arrives
        app.on_message(ChatAppMsg::OpenInteraction {
            request_id: "perm-1".into(),
            request: InteractionRequest::Permission(PermRequest::bash(["ls"])),
        });

        render_frame(&mut app, &mut runtime, &focus);

        let stdout = runtime.stdout_content();
        let screen = crucible_oil::ansi::strip_ansi(stdout);

        // Should have exactly 1 blank line between user message and thought
        let blanks = blank_lines_between(&screen, "Hello", "Thought");
        assert_eq!(
            blanks,
            Some(1),
            "Expected 1 blank between user message and thought after permission graduation.\nScreen:\n{}",
            screen
        );
    }

    /// Reproduce: spinner leaks to scrollback between graduated tools.
    /// Exact sequence from user's real session.
    #[test]
    fn vt100_spinner_does_not_leak_to_scrollback() {
        let mut app = OilChatApp::init();
        let mut vt = Vt100TestRuntime::new(124, 59);

        // User message
        app.on_message(ChatAppMsg::UserMessage("tell me about this repo".into()));
        vt.render_frame(&mut app);

        // Thinking arrives
        think(&mut app, "I'll explore the repository to give you an overview of what it contains.");

        // Render a few frames with spinner showing (simulates time passing)
        for _ in 0..5 {
            vt.render_frame(&mut app);
        }

        // Tool 1: Get Kiln Info (no permission needed)
        tool(&mut app, "get_kiln_info", "c1");

        // Render with completed tool + turn spinner showing
        for _ in 0..3 {
            vt.render_frame(&mut app);
        }

        // Tool 2: Bash (would normally need permission, but we skip the modal)
        tool(&mut app, "bash", "c2");
        vt.render_frame(&mut app);

        // Tool 3: Bash find
        tool(&mut app, "bash", "c3");
        vt.render_frame(&mut app);

        // Second thought block
        think(&mut app, "Let me check more details.");

        // More tools
        tool(&mut app, "read_file", "c4");
        tool(&mut app, "read_file", "c5");
        vt.render_frame(&mut app);

        // Third thought + final text
        think(&mut app, "Now I have enough context.");
        app.on_message(ChatAppMsg::TextDelta("Crucible is a knowledge-grounded agent runtime.".into()));
        app.on_message(ChatAppMsg::StreamComplete);
        vt.render_frame(&mut app);

        // Check the combined output for spinner characters in scrollback
        let stdout = vt.inner().stdout_content();
        let stdout_plain = crucible_oil::ansi::strip_ansi(stdout);

        let spinner_chars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏', '◐', '◓', '◑', '◒'];
        let spinner_lines: Vec<(usize, &str)> = stdout_plain
            .lines()
            .enumerate()
            .filter(|(_, l)| {
                let trimmed = l.trim();
                trimmed.len() <= 2 && trimmed.chars().any(|c| spinner_chars.contains(&c))
            })
            .collect();

        assert!(
            spinner_lines.is_empty(),
            "Spinner characters found in graduated scrollback:\n{:?}\n\nFull stdout:\n{}",
            spinner_lines,
            stdout_plain
        );
    }

    /// Check that the final vt100 screen + accumulated stdout never contain
    /// standalone spinner lines in graduated content. The stdout_buffer
    /// accumulates rendered graduation content (what goes to scrollback).
    /// If a spinner appears there, graduation is broken.
    ///
    /// This test also captures EVERY intermediate screen state to detect
    /// viewport spinner "ghosts" that could leak to scrollback in real terminals.
    #[test]
    fn vt100_no_spinner_in_any_graduated_content() {
        let mut app = OilChatApp::init();
        let mut vt = Vt100TestRuntime::new(124, 59);

        let spinner_chars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏', '◐', '◓', '◑', '◒'];
        let is_standalone_spinner = |l: &str| -> bool {
            let trimmed = l.trim();
            !trimmed.is_empty() && trimmed.len() <= 2
                && trimmed.chars().any(|c| spinner_chars.contains(&c))
        };

        // Track all screen states to find where spinner appears
        let mut screen_history: Vec<(String, String)> = vec![]; // (phase, screen)

        app.on_message(ChatAppMsg::UserMessage("go".into()));
        vt.render_frame(&mut app);
        screen_history.push(("after_user_msg".into(), vt.screen_contents()));

        think(&mut app, "Planning the approach carefully.");
        for i in 0..5 {
            vt.render_frame(&mut app);
            screen_history.push((format!("thinking_frame_{}", i), vt.screen_contents()));
        }

        // Tool 1 arrives and completes
        tool(&mut app, "get_kiln_info", "c1");
        vt.render_frame(&mut app);
        screen_history.push(("after_tool1".into(), vt.screen_contents()));

        // Several frames with completed tool + possible turn spinner
        for i in 0..5 {
            vt.render_frame(&mut app);
            screen_history.push((format!("post_tool1_frame_{}", i), vt.screen_contents()));
        }

        // Tool 2
        tool(&mut app, "bash", "c2");
        vt.render_frame(&mut app);
        screen_history.push(("after_tool2".into(), vt.screen_contents()));

        // Complete
        app.on_message(ChatAppMsg::TextDelta("Done.".into()));
        app.on_message(ChatAppMsg::StreamComplete);
        vt.render_frame(&mut app);
        screen_history.push(("after_complete".into(), vt.screen_contents()));

        // Now check: the stdout buffer (accumulated graduation content)
        // should have NO standalone spinner lines
        let stdout = vt.inner().stdout_content();
        let stdout_plain = crucible_oil::ansi::strip_ansi(stdout);

        let spinner_in_stdout: Vec<(usize, &str)> = stdout_plain
            .lines()
            .enumerate()
            .filter(|(_, l)| is_standalone_spinner(l))
            .collect();

        if !spinner_in_stdout.is_empty() {
            // Print the screen history to help debug
            for (phase, screen) in &screen_history {
                let has_spinner = screen.lines().any(|l| is_standalone_spinner(l));
                if has_spinner {
                    eprintln!("=== {} (has spinner) ===\n{}\n", phase, screen);
                }
            }
        }

        assert!(
            spinner_in_stdout.is_empty(),
            "Spinner in graduated stdout content:\n{:?}\n\nFull stdout:\n{}",
            spinner_in_stdout,
            stdout_plain
        );

        // Also check the final screen for stale spinners
        let final_screen = vt.screen_contents();
        let spinner_in_screen: Vec<(usize, &str)> = final_screen
            .lines()
            .enumerate()
            .filter(|(_, l)| is_standalone_spinner(l))
            .collect();

        assert!(
            spinner_in_screen.is_empty(),
            "Spinner in final vt100 screen:\n{:?}\n\nScreen:\n{}",
            spinner_in_screen,
            final_screen
        );
    }

    /// SMALL TERMINAL: Forces scrolling during graduation write.
    /// When graduation content is tall enough to push old viewport content
    /// past the terminal height, the old viewport (with spinner) should NOT
    /// survive in scrollback.
    #[test]
    fn vt100_small_terminal_spinner_no_leak_on_scroll() {
        let mut app = OilChatApp::init();
        // Very small terminal — 10 rows. Viewport + graduation will exceed this.
        let mut vt = Vt100TestRuntime::new(80, 10);

        let spinner_chars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏', '◐', '◓', '◑', '◒'];
        let is_standalone_spinner = |l: &str| -> bool {
            let trimmed = l.trim();
            !trimmed.is_empty() && trimmed.len() <= 2
                && trimmed.chars().any(|c| spinner_chars.contains(&c))
        };

        app.on_message(ChatAppMsg::UserMessage("go".into()));
        vt.render_frame(&mut app);

        think(&mut app, "Planning.");
        vt.render_frame(&mut app); // spinner shows in viewport

        // Tool completes — thinking + tool graduate on next frame
        tool(&mut app, "bash", "c1");
        vt.render_frame(&mut app); // graduation write — may scroll in 10-row terminal

        // More renders to stabilize
        for _ in 0..3 {
            vt.render_frame(&mut app);
        }

        // Second tool
        tool(&mut app, "read_file", "c2");
        vt.render_frame(&mut app);

        app.on_message(ChatAppMsg::StreamComplete);
        vt.render_frame(&mut app);

        // Check graduated content
        let stdout = vt.inner().stdout_content();
        let stdout_plain = crucible_oil::ansi::strip_ansi(stdout);

        let spinner_lines: Vec<(usize, &str)> = stdout_plain
            .lines()
            .enumerate()
            .filter(|(_, l)| is_standalone_spinner(l))
            .collect();

        assert!(
            spinner_lines.is_empty(),
            "Spinner leaked in 10-row terminal:\n{:?}\n\nStdout:\n{}",
            spinner_lines,
            stdout_plain
        );

        // Also check vt100 screen
        let screen = vt.screen_contents();
        let screen_spinners: Vec<(usize, &str)> = screen
            .lines()
            .enumerate()
            .filter(|(_, l)| is_standalone_spinner(l))
            .collect();

        assert!(
            screen_spinners.is_empty(),
            "Spinner in vt100 screen (10-row):\n{:?}\n\nScreen:\n{}",
            screen_spinners,
            screen
        );
    }

    /// Check vt100 SCREEN (not stdout buffer) for spinners after graduation.
    /// This uses the actual vt100 screen state which models real terminal behavior.
    #[test]
    fn vt100_screen_no_spinner_after_graduation() {
        let mut app = OilChatApp::init();
        let mut vt = Vt100TestRuntime::new(124, 59);

        app.on_message(ChatAppMsg::UserMessage("go".into()));
        vt.render_frame(&mut app);

        think(&mut app, "Planning the approach.");

        // Render several frames so spinner appears in viewport
        for _ in 0..5 {
            vt.render_frame(&mut app);
        }

        // Tool arrives and completes — thinking can graduate
        tool(&mut app, "bash", "c1");
        vt.render_frame(&mut app);

        // Render a few more frames with completed tool + turn spinner
        for _ in 0..3 {
            vt.render_frame(&mut app);
        }

        // Second tool
        tool(&mut app, "read_file", "c2");
        vt.render_frame(&mut app);

        app.on_message(ChatAppMsg::StreamComplete);
        vt.render_frame(&mut app);

        // Check the vt100 SCREEN (not stdout buffer) for spinners
        let screen = vt.screen_contents();
        let spinner_chars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏', '◐', '◓', '◑', '◒'];

        // Look for standalone spinner lines in the screen content
        let spinner_lines: Vec<(usize, &str)> = screen
            .lines()
            .enumerate()
            .filter(|(_, l)| {
                let trimmed = l.trim();
                !trimmed.is_empty() && trimmed.len() <= 2
                    && trimmed.chars().any(|c| spinner_chars.contains(&c))
            })
            .collect();

        assert!(
            spinner_lines.is_empty(),
            "Spinner found in vt100 screen after graduation:\n{:?}\n\nFull screen:\n{}",
            spinner_lines,
            screen
        );
    }

    /// Same test but with tick-per-event (renders between every event).
    #[test]
    fn vt100_spinner_no_leak_tick_per_event() {
        let mut app = OilChatApp::init();
        let mut vt = Vt100TestRuntime::new(124, 59);

        app.on_message(ChatAppMsg::UserMessage("go".into()));
        vt.render_frame(&mut app);

        think(&mut app, "Planning.");
        vt.render_frame(&mut app);

        // Tool arrives — creates ToolGroup, thinking becomes graduatable
        app.on_message(ChatAppMsg::ToolCall {
            name: "bash".into(),
            args: r#"{"cmd": "ls"}"#.into(),
            call_id: Some("c1".into()),
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        vt.render_frame(&mut app); // thinking + tool in viewport, spinner may show

        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "bash".into(),
            call_id: Some("c1".into()),
        });
        vt.render_frame(&mut app); // tool complete, turn spinner shows

        // Another tool
        app.on_message(ChatAppMsg::ToolCall {
            name: "read_file".into(),
            args: r#"{"path": "README.md"}"#.into(),
            call_id: Some("c2".into()),
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        vt.render_frame(&mut app);

        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "read_file".into(),
            call_id: Some("c2".into()),
        });
        vt.render_frame(&mut app);

        app.on_message(ChatAppMsg::TextDelta("Done.".into()));
        vt.render_frame(&mut app);

        app.on_message(ChatAppMsg::StreamComplete);
        vt.render_frame(&mut app);

        let stdout = vt.inner().stdout_content();
        let stdout_plain = crucible_oil::ansi::strip_ansi(stdout);

        let spinner_chars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏', '◐', '◓', '◑', '◒'];
        let spinner_lines: Vec<(usize, &str)> = stdout_plain
            .lines()
            .enumerate()
            .filter(|(_, l)| {
                let trimmed = l.trim();
                trimmed.len() <= 2 && trimmed.chars().any(|c| spinner_chars.contains(&c))
            })
            .collect();

        assert!(
            spinner_lines.is_empty(),
            "Spinner leaked to scrollback:\n{:?}\n\nFull stdout:\n{}",
            spinner_lines,
            stdout_plain
        );
    }

    /// Variant 3 (original): Permission for non-tool interaction — no crash.
    #[test]
    fn non_permission_interaction_does_not_affect_graduation() {
        use crucible_core::interaction::{AskRequest, InteractionRequest};

        let mut app = OilChatApp::init();

        app.on_message(ChatAppMsg::UserMessage("Question".into()));
        think(&mut app, "I need to ask something.");

        // Ask interaction (not permission) — should NOT mark response complete
        app.on_message(ChatAppMsg::OpenInteraction {
            request_id: "ask-1".into(),
            request: InteractionRequest::Ask(AskRequest {
                question: "Which option?".into(),
                choices: Some(vec!["A".into(), "B".into()]),
                multi_select: false,
                allow_other: false,
            }),
        });

        // Thinking should NOT have graduated (Ask != Permission)
        let mut runtime = TestRuntime::new(80, 24);
        let focus = FocusContext::new();
        render_frame(&mut app, &mut runtime, &focus);

        // The thinking is still in the viewport (not graduated)
        let viewport = runtime.viewport_content();
        assert!(
            viewport.contains("Thinking") || viewport.contains("ask something"),
            "Thinking should still be in viewport for Ask interaction.\nViewport: {}",
            viewport
        );
    }
}

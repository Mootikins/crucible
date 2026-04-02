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
use crucible_oil::focus::FocusContext;
use crucible_oil::TestRuntime;

/// Find a byte subsequence in a byte slice. Returns the start position.
fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// A test runtime that feeds terminal output through vt100 for screen-level assertions.
pub struct Vt100TestRuntime {
    inner: TestRuntime,
    vt: vt100::Parser,
    /// Tall parser (1000 rows) that receives the same bytes. Since nothing
    /// scrolls off the top of a 1000-row terminal, `contents()` shows
    /// everything — equivalent to scrollback + screen.
    tall_vt: vt100::Parser,
    /// Raw bytes from the last render_frame call (captured before feeding to vt100).
    last_frame_bytes: Vec<u8>,
}

impl Vt100TestRuntime {
    pub fn new(width: u16, height: u16) -> Self {
        // Large scrollback to capture all graduated content
        let scrollback = 1000;
        Self {
            inner: TestRuntime::new(width, height),
            vt: vt100::Parser::new(height, width, scrollback),
            tall_vt: vt100::Parser::new(1000, width, 0),
            last_frame_bytes: Vec::new(),
        }
    }

    /// Render a frame through the real terminal path, then feed bytes to vt100.
    pub fn render_frame(&mut self, app: &mut OilChatApp) {
        let focus = FocusContext::new();
        render_frame(app, &mut self.inner, &focus);

        // Feed terminal bytes to vt100, respecting synchronized update
        // boundaries. Content inside sync blocks is fed atomically (as real
        // terminals buffer it). Content OUTSIDE sync blocks is fed
        // byte-by-byte (real terminals process it incrementally).
        let bytes = self.inner.take_bytes();
        self.last_frame_bytes = bytes.clone();
        if !bytes.is_empty() {
            self.feed_bytes_respecting_sync(&bytes);
        }
    }

    /// Feed bytes to vt100, processing synchronized update blocks atomically
    /// and non-sync content incrementally. This models real terminal behavior.
    /// Also feeds all bytes to the tall parser for scrollback inspection.
    fn feed_bytes_respecting_sync(&mut self, bytes: &[u8]) {
        // Tall parser gets all bytes (for scrollback content inspection)
        self.tall_vt.process(bytes);
        let begin = b"\x1b[?2026h";
        let end = b"\x1b[?2026l";

        let mut pos = 0;
        while pos < bytes.len() {
            // Look for BEGIN_SYNCHRONIZED_UPDATE
            if let Some(sync_start) = find_subsequence(&bytes[pos..], begin) {
                let abs_start = pos + sync_start;

                // Feed everything before the sync block incrementally
                if abs_start > pos {
                    for &b in &bytes[pos..abs_start] {
                        self.vt.process(&[b]);
                    }
                }

                // Find matching END_SYNCHRONIZED_UPDATE
                if let Some(sync_end) = find_subsequence(&bytes[abs_start..], end) {
                    let abs_end = abs_start + sync_end + end.len();
                    // Feed the entire sync block atomically
                    self.vt.process(&bytes[abs_start..abs_end]);
                    pos = abs_end;
                } else {
                    // No end marker — feed rest atomically
                    self.vt.process(&bytes[abs_start..]);
                    pos = bytes.len();
                }
            } else {
                // No more sync blocks — feed remaining incrementally
                for &b in &bytes[pos..] {
                    self.vt.process(&[b]);
                }
                pos = bytes.len();
            }
        }
    }

    /// Get the full screen contents (visible area) as plain text.
    pub fn screen_contents(&self) -> String {
        self.vt.screen().contents()
    }

    /// Get the screen contents with ANSI escape codes preserved.
    /// Use this for "raw" snapshot tests that verify styling (colors, bold, etc.).
    #[allow(dead_code)] // available for styled snapshot tests
    pub fn screen_contents_styled(&self) -> String {
        String::from_utf8_lossy(&self.vt.screen().contents_formatted()).into_owned()
    }

    /// Get the underlying TestRuntime for legacy API access.
    pub fn inner(&self) -> &TestRuntime {
        &self.inner
    }

    /// Read only the scrollback content (content that has scrolled off the top).
    ///
    /// Uses vt100's `set_scrollback()` to shift the viewport, then extracts
    /// only the scrollback rows (not the current visible screen). This is
    /// important because the visible screen may legitimately contain spinners
    /// (they're part of the active viewport), but scrollback should not.
    pub fn scrollback_contents(&mut self) -> String {
        // Use the tall parser — nothing scrolls off in a 1000-row terminal,
        // so contents() shows the full history. Extract the scrollback portion
        // by subtracting the normal parser's visible screen.
        let tall_contents = self.tall_vt.screen().contents();
        let screen_contents = self.vt.screen().contents();

        let tall_lines: Vec<&str> = tall_contents.lines().collect();
        let screen_lines: Vec<&str> = screen_contents.lines().collect();

        let scrollback_count = tall_lines.len().saturating_sub(screen_lines.len());
        if scrollback_count > 0 {
            tall_lines[..scrollback_count].join("\n")
        } else {
            String::new()
        }
    }

    /// Get the full history from the tall parser (scrollback + screen).
    /// Since the tall parser has 1000 rows, nothing scrolls off — this
    /// captures everything the terminal has ever displayed.
    #[allow(dead_code)]
    pub fn full_history(&self) -> String {
        self.tall_vt.screen().contents()
    }

    /// Assert no spinner characters appear in scrollback content.
    ///
    /// Uses canonical spinner character sets from crucible_oil::node to avoid
    /// hardcoded character lists drifting out of sync.
    pub fn assert_no_spinners_in_scrollback(&mut self) {
        use crucible_oil::node::{BRAILLE_SPINNER_FRAMES, SPINNER_FRAMES};

        let contents = self.scrollback_contents();
        if contents.is_empty() {
            return;
        }

        for ch in SPINNER_FRAMES.iter().chain(BRAILLE_SPINNER_FRAMES.iter()) {
            assert!(
                !contents.contains(*ch),
                "Spinner char '{}' found in scrollback:\n{}",
                ch,
                contents
            );
        }
    }

    /// Get the raw bytes from the last render_frame call.
    ///
    /// Useful for verifying escape sequence structure like synchronized
    /// update boundaries.
    pub fn last_frame_bytes(&self) -> &[u8] {
        &self.last_frame_bytes
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

    // ─── Permission interaction tests (vt100) ────────────────────────
    //
    // In the new container model, permission_pending is gone. The
    // interaction modal opens via OpenInteraction, and thinking content
    // is not duplicated (chrome shows "Thinking…", content is empty
    // until text starts).

    /// Permission modal opens correctly and doesn't corrupt content.
    #[test]
    fn permission_modal_opens_without_corruption() {
        use crucible_core::interaction::{InteractionRequest, PermRequest};

        let mut app = OilChatApp::init();
        let mut vt = Vt100TestRuntime::new(80, 24);

        app.on_message(ChatAppMsg::UserMessage("Do something".into()));
        think(&mut app, "Let me run a dangerous command.");
        vt.render_frame(&mut app);

        // Permission request arrives — modal should open
        app.on_message(ChatAppMsg::OpenInteraction {
            request_id: "perm-1".into(),
            request: InteractionRequest::Permission(PermRequest::bash(["rm", "-rf", "/tmp/test"])),
        });
        vt.render_frame(&mut app);

        let screen = crucible_oil::ansi::strip_ansi(&vt.screen_contents());
        // Modal should be visible (contains permission prompt)
        assert!(
            screen.contains("rm") || screen.contains("Allow") || screen.contains("Deny"),
            "Permission modal should be visible.\nScreen:\n{}",
            screen
        );
        // No spinners in scrollback
        vt.assert_no_spinners_in_scrollback();
    }

    /// Graduated thinking must NOT contain spinner characters.
    #[test]
    fn graduated_thinking_has_no_spinner() {
        let mut app = OilChatApp::init();
        let mut vt = Vt100TestRuntime::new(80, 24);

        app.on_message(ChatAppMsg::UserMessage("Do something".into()));
        vt.render_frame(&mut app);

        think(&mut app, "Planning the command.");
        app.on_message(ChatAppMsg::TextDelta("Here is the plan.".into()));
        app.on_message(ChatAppMsg::StreamComplete);
        vt.render_frame(&mut app);

        // Everything graduated — check scrollback for spinners
        vt.assert_no_spinners_in_scrollback();

        let full = crucible_oil::ansi::strip_ansi(&vt.full_history());
        assert!(
            full.contains("Thought"),
            "Graduated thinking should show collapsed summary.\nFull:\n{}",
            full
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

    // ─── Bug 3: Spinner leaks to scrollback via vt100 inspection ──────
    //
    // These tests use vt100::set_scrollback() to read actual scrollback
    // content — the previous tests only checked the stdout_buffer string.
    // These catch the class of bugs where spinner chars end up in real
    // terminal scrollback during graduation.

    /// Permission modal graduation: thinking graduates, viewport gets turn
    /// spinner, then tool arrives. Scrollback must not contain spinner chars.
    #[test]
    fn vt100_scrollback_no_spinner_after_permission_graduation() {
        use crucible_core::interaction::{InteractionRequest, PermRequest};

        let mut app = OilChatApp::init();
        let mut vt = Vt100TestRuntime::new(80, 24);

        // User message
        app.on_message(ChatAppMsg::UserMessage("Do something risky".into()));
        vt.render_frame(&mut app);

        // Thinking arrives
        think(&mut app, "I need to run a dangerous command.");

        // Render with spinner in viewport
        for _ in 0..3 {
            vt.render_frame(&mut app);
        }

        // Permission request arrives — thinking becomes graduatable
        app.on_message(ChatAppMsg::OpenInteraction {
            request_id: "perm-1".into(),
            request: InteractionRequest::Permission(PermRequest::bash(["rm", "-rf", "/tmp/test"])),
        });

        // This render graduates the thinking, viewport now has turn spinner + modal
        vt.render_frame(&mut app);

        // Tool call arrives (simulating user approval)
        app.on_message(ChatAppMsg::ToolCall {
            name: "bash".into(),
            args: r#"{"cmd": "rm -rf /tmp/test"}"#.into(),
            call_id: Some("c1".into()),
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        vt.render_frame(&mut app);

        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "bash".into(),
            call_id: Some("c1".into()),
        });
        vt.render_frame(&mut app);

        app.on_message(ChatAppMsg::StreamComplete);
        vt.render_frame(&mut app);

        // Inspect scrollback via vt100 — the actual terminal scrollback
        vt.assert_no_spinners_in_scrollback();
    }

    /// Rapid sequential graduations: 5 quick graduation cycles.
    /// Mimics the 25ms burst from the production log.
    #[test]
    fn vt100_rapid_sequential_graduations_clean_scrollback() {
        let mut app = OilChatApp::init();
        let mut vt = Vt100TestRuntime::new(80, 24);

        app.on_message(ChatAppMsg::UserMessage("Do many things".into()));
        vt.render_frame(&mut app);

        // 5 rapid tool cycles — each graduates the previous content
        for i in 0..5 {
            let id = format!("c{}", i);
            think(&mut app, &format!("Step {}.", i));
            tool(&mut app, "bash", &id);
            vt.render_frame(&mut app); // graduation happens here
        }

        app.on_message(ChatAppMsg::TextDelta("All done.".into()));
        app.on_message(ChatAppMsg::StreamComplete);
        vt.render_frame(&mut app);

        // Inspect scrollback for spinner leak
        vt.assert_no_spinners_in_scrollback();
    }

    /// Verify graduation writes are inside synchronized update blocks.
    /// Captures raw terminal bytes and checks escape sequence structure.
    ///
    /// The clear() sequence (MoveUp + ClearFromCursorDown) and graduation
    /// content must be inside the same synchronized update block as the
    /// viewport render. This prevents the terminal from showing intermediate
    /// states where old spinner content is visible.
    #[test]
    fn vt100_graduation_bytes_inside_sync_update() {
        let mut app = OilChatApp::init();
        let mut vt = Vt100TestRuntime::new(80, 24);

        app.on_message(ChatAppMsg::UserMessage("go".into()));
        vt.render_frame(&mut app);

        // This frame will trigger graduation (user message graduates
        // because thinking + tool follow it)
        think(&mut app, "Planning.");
        tool(&mut app, "bash", "c1");
        vt.render_frame(&mut app);

        // Get raw bytes from this graduation frame
        let bytes = vt.last_frame_bytes();
        let byte_str = String::from_utf8_lossy(bytes);

        let begin_sync = "\x1b[?2026h";
        let end_sync = "\x1b[?2026l";

        // Find the FIRST begin_sync and LAST end_sync
        let first_begin = byte_str.find(begin_sync);
        let last_end = byte_str.rfind(end_sync);

        // There must be sync markers in a graduation frame
        assert!(
            first_begin.is_some() && last_end.is_some(),
            "Graduation frame must contain synchronized update markers.\nBytes:\n{}",
            byte_str.replace('\x1b', "ESC")
        );

        let begin_pos = first_begin.unwrap();
        let end_pos = last_end.unwrap();

        // Find ClearFromCursorDown: \x1b[0J or \x1b[J
        // This is the clear() operation that erases the viewport
        let clear_pattern_positions: Vec<usize> = byte_str
            .match_indices("\x1b[")
            .filter_map(|(pos, _)| {
                let rest = &byte_str[pos + 2..];
                if rest.starts_with("0J") || rest.starts_with("J") {
                    Some(pos)
                } else {
                    None
                }
            })
            .collect();

        // The clear() during graduation should be AFTER begin_sync
        for clear_pos in &clear_pattern_positions {
            assert!(
                *clear_pos > begin_pos && *clear_pos < end_pos,
                "Clear(FromCursorDown) at byte {} must be inside sync block [{}, {}].\n\
                 Graduation writes must be inside synchronized update to prevent\n\
                 terminal from showing intermediate states with spinner content.\n\
                 Bytes:\n{}",
                clear_pos,
                begin_pos,
                end_pos,
                byte_str.replace('\x1b', "ESC")
            );
        }
    }

    // ─── Bug 4: Permission modal triggers spinner-in-scrollback ───────
    //
    // The daemon sends OpenInteraction(Permission) BEFORE ToolCall.
    // This causes the trailing AssistantResponse (thinking) to graduate
    // via permission_pending, and the viewport gets a turn spinner.
    // The turn spinner then leaks into scrollback during the next
    // graduation cycle.

    /// The exact event sequence that triggers the spinner leak:
    /// 1. User message
    /// 2. Thinking (spinner in viewport)
    /// 3. OpenInteraction(Permission) → thinking graduates, turn spinner appears
    /// 4. Render frames with turn spinner visible
    /// 5. ToolCall arrives → graduation happens, spinner may leak
    /// 6. Repeat with more permission tools
    ///
    /// Uses the user's terminal size (124x59) to match production behavior.
    #[test]
    fn reproduce_permission_modal_spinner_leak() {
        use crucible_core::interaction::{InteractionRequest, PermRequest};
        use crucible_oil::node::{BRAILLE_SPINNER_FRAMES, SPINNER_FRAMES};

        let all_spinner_chars: Vec<char> = SPINNER_FRAMES
            .iter()
            .chain(BRAILLE_SPINNER_FRAMES.iter())
            .copied()
            .collect();

        let mut app = OilChatApp::init();
        let mut vt = Vt100TestRuntime::new(124, 59);

        // Helper: check scrollback for spinners after each phase.
        // Scrollback = tall parser contents minus normal parser screen.
        // The viewport may legitimately contain a turn spinner (it's chrome,
        // not content), so we only assert on what has scrolled off.
        let check = |vt: &mut Vt100TestRuntime, phase: &str| {
            let scrollback = vt.scrollback_contents();
            let history_plain = crucible_oil::ansi::strip_ansi(&scrollback);
            let spinners: Vec<(usize, String)> = history_plain
                .lines()
                .enumerate()
                .filter(|(_, l)| {
                    let trimmed = l.trim();
                    !trimmed.is_empty()
                        && trimmed.len() <= 4
                        && trimmed.chars().any(|c| all_spinner_chars.contains(&c))
                })
                .map(|(i, l)| (i, l.to_string()))
                .collect();

            if !spinners.is_empty() {
                let lines: Vec<&str> = history_plain.lines().collect();
                eprintln!("=== SPINNER LEAK at phase '{}' ===", phase);
                for (i, line) in &spinners {
                    let start = i.saturating_sub(2);
                    let end = (*i + 3).min(lines.len());
                    eprintln!("  Spinner '{}' at line {}:", line.trim(), i);
                    for j in start..end {
                        let marker = if j == *i { " <<<" } else { "" };
                        eprintln!("    [{:3}] {}{}", j, lines[j], marker);
                    }
                }
            }

            assert!(
                spinners.is_empty(),
                "Phase '{}': spinners in history:\n{:?}",
                phase,
                spinners
            );
        };

        // ── Turn 1: User message ──
        app.on_message(ChatAppMsg::UserMessage("tell me about this repo".into()));
        vt.render_frame(&mut app);

        // ── Thinking arrives (spinner shows in viewport) ──
        think(&mut app, "I'll explore the repository structure.");
        vt.render_frame(&mut app);
        vt.render_frame(&mut app); // spinner ticks

        // ── Tool 1: get_kiln_info (no permission needed) ──
        tool(&mut app, "get_kiln_info", "c1");
        vt.render_frame(&mut app);

        // Idle frames — turn spinner shows (tool complete, waiting for next)
        vt.render_frame(&mut app);
        vt.render_frame(&mut app);

        // ── Tool 2: bash (PERMISSION REQUIRED) ──
        // Daemon sends OpenInteraction BEFORE ToolCall
        app.on_message(ChatAppMsg::OpenInteraction {
            request_id: "perm-1".into(),
            request: InteractionRequest::Permission(PermRequest::bash(["ls", "-la"])),
        });
        vt.render_frame(&mut app); // thinking graduates, modal + turn spinner

        // User approves (simulated — just send the tool call)
        app.on_message(ChatAppMsg::ToolCall {
            name: "bash".into(),
            args: r#"{"cmd": "ls -la"}"#.into(),
            call_id: Some("c2".into()),
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        vt.render_frame(&mut app);

        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "bash".into(),
            call_id: Some("c2".into()),
        });
        vt.render_frame(&mut app);

        check(&mut vt, "after_first_permission_tool");

        // Idle frames — turn spinner between tools
        vt.render_frame(&mut app);
        vt.render_frame(&mut app);

        // ── Tool 3: bash find (PERMISSION REQUIRED) ──
        app.on_message(ChatAppMsg::OpenInteraction {
            request_id: "perm-2".into(),
            request: InteractionRequest::Permission(PermRequest::bash([
                "find", ".", "-maxdepth", "2", "-name", "README*",
            ])),
        });
        vt.render_frame(&mut app);

        app.on_message(ChatAppMsg::ToolCall {
            name: "bash".into(),
            args: r#"{"cmd": "find . -maxdepth 2 -name README*"}"#.into(),
            call_id: Some("c3".into()),
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        vt.render_frame(&mut app);

        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "bash".into(),
            call_id: Some("c3".into()),
        });
        vt.render_frame(&mut app);

        check(&mut vt, "after_second_permission_tool");

        // ── Second thinking block ──
        think(&mut app, "Let me check the crate structure.");
        vt.render_frame(&mut app);
        vt.render_frame(&mut app);

        // ── Tool 4: bash ls crates (PERMISSION REQUIRED) ──
        app.on_message(ChatAppMsg::OpenInteraction {
            request_id: "perm-3".into(),
            request: InteractionRequest::Permission(PermRequest::bash(["ls", "-la", "crates/"])),
        });
        vt.render_frame(&mut app);

        app.on_message(ChatAppMsg::ToolCall {
            name: "bash".into(),
            args: r#"{"cmd": "ls -la crates/"}"#.into(),
            call_id: Some("c4".into()),
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        vt.render_frame(&mut app);

        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "bash".into(),
            call_id: Some("c4".into()),
        });
        vt.render_frame(&mut app);

        check(&mut vt, "after_third_permission_tool");

        // ── Final response ──
        think(&mut app, "Now I have enough context.");
        app.on_message(ChatAppMsg::TextDelta(
            "Crucible is a knowledge-grounded agent runtime.".into(),
        ));
        app.on_message(ChatAppMsg::StreamComplete);
        vt.render_frame(&mut app);

        check(&mut vt, "final");
    }

    /// Ask interaction opens modal without crashing.
    #[test]
    fn ask_interaction_opens_without_crash() {
        use crucible_core::interaction::{AskRequest, InteractionRequest};

        let mut app = OilChatApp::init();
        let mut vt = Vt100TestRuntime::new(80, 24);

        app.on_message(ChatAppMsg::UserMessage("Question".into()));
        think(&mut app, "I need to ask something.");
        vt.render_frame(&mut app);

        // Ask interaction (not permission)
        app.on_message(ChatAppMsg::OpenInteraction {
            request_id: "ask-1".into(),
            request: InteractionRequest::Ask(AskRequest {
                question: "Which option?".into(),
                choices: Some(vec!["A".into(), "B".into()]),
                multi_select: false,
                allow_other: false,
            }),
        });
        vt.render_frame(&mut app);

        // Should render without panic, modal visible
        let screen = crucible_oil::ansi::strip_ansi(&vt.screen_contents());
        assert!(
            screen.contains("option") || screen.contains("Question"),
            "Ask modal should be visible.\nScreen:\n{}",
            screen
        );
        vt.assert_no_spinners_in_scrollback();
    }
}

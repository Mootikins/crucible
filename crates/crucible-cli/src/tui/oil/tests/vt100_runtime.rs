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

    /// Get the vt100 screen for direct inspection.
    pub fn screen(&self) -> &vt100::Screen {
        self.vt.screen()
    }

    /// Number of lines in scrollback (graduated content that scrolled off screen).
    pub fn scrollback_len(&self) -> usize {
        self.vt.screen().scrollback()
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
}

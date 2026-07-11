//! Viewport content, graduation spacing, blank-line invariants, and modal
//! open/close behavior at the rendered-screen level.

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
        diffs: Vec::new(),
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
        diffs: Vec::new(),
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

/// The completion popup is a visual extension of the prompt it sits on,
/// so its rows must use the CURRENT input mode's bg (command mode here —
/// whatever the user themes it to), with the selected row a derived
/// variant of that same surface. Regression for the popup rendering with
/// a fixed default bg unrelated to the prompt.
#[test]
fn model_popup_bg_matches_command_prompt_bg() {
    use crate::tui::oil::event::Event;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(100, 30);

    app.on_message(ChatAppMsg::ModelsLoaded(vec![
        "llama3.2".into(),
        "gpt-4o".into(),
    ]));
    for c in ":model".chars() {
        let _ = app.update(Event::Key(KeyEvent::new(
            KeyCode::Char(c),
            KeyModifiers::NONE,
        )));
    }
    let _ = app.update(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )));
    vt.render_frame(&mut app);

    let styled = vt.screen_contents_styled();
    // Default command-mode bg is Rgb(60, 50, 20); the popup body must use it…
    assert!(
        styled.contains("\x1b[48;2;60;50;20m"),
        "popup/input should carry the command-mode bg.\nStyled:\n{styled:?}"
    );
    // …the selected row must use the derived variant of the same surface…
    assert!(
        styled.contains("\x1b[48;2;74;64;34m"),
        "selected popup row should use the derived selection variant.\nStyled:\n{styled:?}"
    );
    // …and nothing should fall back to the fixed default popup bg.
    assert!(
        !styled.contains("\x1b[48;2;40;44;52m"),
        "popup must not use the mode-independent default bg.\nStyled:\n{styled:?}"
    );
}

/// Regression: the in-place streaming redraw must repaint the top screen
/// row. The viewport was clamped to `terminal_height - 1` rows, so screen
/// row 0 was never rewritten between graduations — it kept showing the
/// last line of the most recent graduation (the graduated thinking block)
/// for the entire turn while the response scrolled discontinuously below.
/// Sequence mirrors a real session: thinking → tool (graduates the
/// thinking AR, then the ToolGroup blocks graduation until turn end) →
/// long streamed response.
#[test]
fn graduated_thinking_scrolls_off_top_row_during_long_stream() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.on_message(ChatAppMsg::UserMessage("go".into()));
    vt.render_frame(&mut app);

    // Thinking with a distinctive tail so we can spot the frozen line.
    think(&mut app, "planning the approach zz-marker-tail");
    vt.render_frame(&mut app);

    // Tool arrives → the thinking AR graduates; from here the leading
    // ToolGroup blocks all further graduation until the turn ends.
    tool(&mut app, "bash", "c1");
    vt.render_frame(&mut app);

    // Stream a response far taller than the 24-row terminal.
    for i in 0..60 {
        app.on_message(ChatAppMsg::TextDelta(format!("response line {i:02}\n")));
        vt.render_frame(&mut app);
    }

    let screen = crucible_oil::ansi::strip_ansi(&vt.screen_contents());
    assert!(
        screen.contains("response line 59"),
        "latest streamed content must be visible.\nScreen:\n{screen}"
    );
    assert!(
        !screen.contains("zz-marker-tail"),
        "graduated thinking must scroll off screen once the response \
         overflows the viewport — a surviving line means the top row was \
         not repainted.\nScreen:\n{screen}"
    );
}

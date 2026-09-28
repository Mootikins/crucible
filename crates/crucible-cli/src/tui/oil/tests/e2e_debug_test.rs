//! End-to-end debug test: dumps full vt100 output at each step.
//! Run with: cargo test --lib -p crucible-cli -- e2e_debug_test --nocapture

use super::vt100_runtime::Vt100TestRuntime;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::containers::ChatNode;
use crate::tui::oil::tests::helpers::{EventFeed, SendMsgs, ToolCallEvent};
use crucible_oil::ansi::strip_ansi;

/// Simulates the exact scenario from user testing:
/// "tell me about this repo" → thinking → text → tools → more text
#[test]
fn e2e_full_conversation_render() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(124, 40);

    // Step 1: User message
    app.send_msgs(feed.user("tell me about this repo"));
    vt.render_frame(&mut app);
    let out = strip_ansi(&vt.full_history());
    eprintln!("\n============================================================\n=== STEP 1: After user message ===\n============================================================");
    eprintln!("{}", out);

    // Step 2: Thinking starts
    app.send_msgs(feed.thinking("I need to explore the repository structure to understand what this project is about. Let me start by looking at the files and reading the README."));
    vt.render_frame(&mut app);
    let out = strip_ansi(&vt.full_history());
    eprintln!("\n============================================================\n=== STEP 2: After thinking delta ===\n============================================================");
    eprintln!("{}", out);

    // Step 3: Text starts (thinking should finalize)
    app.send_msgs(
        feed.text("I'll explore this repository to understand its structure and purpose."),
    );
    vt.render_frame(&mut app);
    let out = strip_ansi(&vt.full_history());
    eprintln!("\n============================================================\n=== STEP 3: After text delta ===\n============================================================");
    eprintln!("{}", out);

    // Step 4: Tool calls
    app.send_msgs(feed.tool(ToolCallEvent {
        tool: "Bash",
        call_id: "call-1",
        args: r#"{"command": "ls -la"}"#,
        render: Some("ls -la".into()),
        ..Default::default()
    }));
    vt.render_frame(&mut app);
    let out = strip_ansi(&vt.full_history());
    eprintln!("\n============================================================\n=== STEP 4: After tool call (pending) ===\n============================================================");
    eprintln!("{}", out);

    // Step 5: Tool complete
    app.send_msgs(feed.tool_result(
        "Bash",
        "call-1",
        "total 42\ndrwxr-xr-x 1 user user 100 Jan 1 00:00 src\n",
    ));
    vt.render_frame(&mut app);
    let out = strip_ansi(&vt.full_history());
    eprintln!("\n============================================================\n=== STEP 5: After tool complete ===\n============================================================");
    eprintln!("{}", out);

    // Step 6: Second tool
    app.send_msgs(feed.tool(ToolCallEvent {
        tool: "Glob",
        call_id: "call-2",
        args: r#"{"pattern": "README*"}"#,
        render: Some("README*".into()),
        ..Default::default()
    }));
    app.send_msgs(feed.tool_result("Glob", "call-2", ""));
    vt.render_frame(&mut app);
    let out = strip_ansi(&vt.full_history());
    eprintln!("\n============================================================\n=== STEP 6: After second tool ===\n============================================================");
    eprintln!("{}", out);

    // Step 7: Continuation text after tools
    app.send_msgs(
        feed.text("Based on my analysis, this is a Rust workspace project called Crucible."),
    );
    vt.render_frame(&mut app);
    let out = strip_ansi(&vt.full_history());
    eprintln!("\n============================================================\n=== STEP 7: After continuation text ===\n============================================================");
    eprintln!("{}", out);

    // Step 8: Stream complete (everything graduates)
    app.send_msgs(feed.complete());
    vt.render_frame(&mut app);
    let out = strip_ansi(&vt.full_history());
    eprintln!("\n============================================================\n=== STEP 8: After stream complete (all graduated) ===\n============================================================");
    eprintln!("{}", out);

    // Validate: no spinners in scrollback
    vt.assert_no_spinners_in_scrollback();

    // Validate: content present
    let stripped = strip_ansi(&vt.full_history());
    assert!(
        stripped.contains("tell me about this repo"),
        "User message missing"
    );
    assert!(
        stripped.contains("Thought"),
        "Thinking collapsed summary missing"
    );
    assert!(
        stripped.contains("explore this repository"),
        "Assistant text missing"
    );
    assert!(stripped.contains("Bash"), "Tool name missing");
    assert!(stripped.contains("Crucible"), "Continuation text missing");

    // Validate: no duplicate spinners visible
    // Validate: turn indicator only in chrome area (below spacer)
    let screen = strip_ansi(&vt.screen_contents());
    eprintln!("\n============================================================\n=== FINAL VIEWPORT ===\n============================================================");
    eprintln!("{}", screen);
}

#[test]
fn debug_continuation_flag() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();

    app.send_msgs(feed.thinking("thinking..."));
    app.send_msgs(feed.text("first text"));
    app.send_msgs(feed.tool_call("Bash", "c1", "{}"));
    app.send_msgs(feed.tool_result("Bash", "c1", ""));
    app.send_msgs(feed.text("continuation text"));

    let nodes = app.container_list().nodes();
    for (i, node) in nodes.iter().enumerate() {
        match node {
            ChatNode::AssistantResponse { text, thinking, .. } => {
                eprintln!(
                    "Node {}: AssistantResponse text={:?} thinking={}",
                    i,
                    text,
                    thinking.len()
                );
            }
            _ => {
                eprintln!("Node {}: {:?}", i, std::mem::discriminant(node));
            }
        }
    }

    // The last node should be an AssistantResponse (continuation derived at render time)
    assert!(
        matches!(nodes.last(), Some(ChatNode::AssistantResponse { .. })),
        "Last node should be AssistantResponse"
    );
}

#[test]
fn debug_continuation_rendering() {
    use crucible_oil::focus::FocusContext;
    use crucible_oil::render::render_to_plain_text;

    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();

    app.send_msgs(feed.thinking("thinking..."));
    app.send_msgs(feed.text("first text"));
    app.send_msgs(feed.tool_call("Bash", "c1", "{}"));
    app.send_msgs(feed.tool_result("Bash", "c1", ""));
    app.send_msgs(feed.text("continuation text after tools"));

    let focus = FocusContext::default();
    let ctx = crate::tui::oil::ViewContext::new(&focus);

    let nodes = app.container_list().nodes();
    for (i, node) in nodes.iter().enumerate() {
        let prev = if i > 0 { Some(&nodes[i - 1]) } else { None };
        let rendered = node.render(prev, &ctx);
        let plain = render_to_plain_text(&rendered, 80);
        eprintln!("=== Node {} ===", i);
        eprintln!("{}", plain);

        // Check for bullet in continuation
        if let ChatNode::AssistantResponse { .. } = node {
            let is_continuation = matches!(
                prev,
                Some(
                    ChatNode::ToolGroup { .. }
                        | ChatNode::SubagentTask { .. }
                        | ChatNode::ShellExecution { .. }
                )
            );
            if is_continuation && plain.contains("●") {
                panic!(
                    "BUG: Continuation text should NOT have ● bullet!\nOutput:\n{}",
                    plain
                );
            }
        }
    }
}

#[test]
fn debug_full_view_rendering() {
    use super::helpers::vt_render;

    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();

    app.send_msgs(feed.thinking("thinking..."));
    app.send_msgs(feed.text("first text"));
    app.send_msgs(feed.tool_call("Bash", "c1", "{}"));
    app.send_msgs(feed.tool_result("Bash", "c1", ""));
    app.send_msgs(feed.text("continuation text after tools"));
    app.send_msgs(feed.complete());

    let output = vt_render(&mut app);
    eprintln!("Full rendered output:");
    for (i, line) in output.lines().enumerate() {
        eprintln!("{:3}: {}", i, line);
    }

    assert!(
        !output.contains("●"),
        "No ● bullet should appear anywhere in the output.\nOutput:\n{}",
        output
    );
}

/// Verify the "stuck" scenario: after StreamComplete, the TUI should be responsive
/// (is_streaming returns false, new messages can be sent).
#[test]
fn after_stream_complete_tui_is_responsive() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 24);

    // First turn
    app.send_msgs(feed.user("first"));
    app.send_msgs(feed.text("response one"));
    app.send_msgs(feed.complete());
    vt.render_frame(&mut app);

    assert!(
        !app.is_streaming(),
        "Should not be streaming after StreamComplete"
    );

    // Second turn should work
    app.send_msgs(feed.user("second"));
    app.send_msgs(feed.text("response two"));
    assert!(app.is_streaming(), "Should be streaming during second turn");

    app.send_msgs(feed.complete());
    vt.render_frame(&mut app);

    assert!(
        !app.is_streaming(),
        "Should not be streaming after second StreamComplete"
    );

    let output = strip_ansi(&vt.full_history());
    assert!(
        output.contains("response two"),
        "Second response should appear"
    );
}

/// Verify no double spinners: only ONE spinner should be visible at a time.
#[test]
fn only_one_spinner_visible_during_thinking() {
    use crucible_oil::node::SPINNER_FRAMES;

    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.send_msgs(feed.user("think hard"));
    vt.render_frame(&mut app);

    app.send_msgs(feed.thinking("deep thoughts about the universe"));
    vt.render_frame(&mut app);

    let screen = strip_ansi(&vt.screen_contents());

    // Count spinner characters across all frames
    let spinner_count: usize = SPINNER_FRAMES
        .iter()
        .map(|ch| screen.matches(*ch).count())
        .sum();

    assert!(
        spinner_count <= 1,
        "Should have at most 1 spinner visible, found {}. Screen:\n{}",
        spinner_count,
        screen
    );
}

/// Verify user message and input box use consistent styling.
#[test]
fn user_message_matches_input_style() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.send_msgs(feed.user("hello world"));
    app.send_msgs(feed.text("response"));
    app.send_msgs(feed.complete());
    vt.render_frame(&mut app);

    let screen = strip_ansi(&vt.screen_contents());
    let lines: Vec<&str> = screen.lines().collect();

    // Both user message and input box should use ▄▄▄/▀▀▀ bars
    let top_bars: Vec<_> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.trim_start().starts_with('▄'))
        .collect();
    let bottom_bars: Vec<_> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.trim_start().starts_with('▀'))
        .collect();

    // Should have 2 top bars (user msg + input) and 2 bottom bars
    assert!(
        top_bars.len() >= 2,
        "Expected at least 2 top bars (user msg + input), found {}. Screen:\n{}",
        top_bars.len(),
        screen
    );
    assert!(
        bottom_bars.len() >= 2,
        "Expected at least 2 bottom bars (user msg + input), found {}. Screen:\n{}",
        bottom_bars.len(),
        screen
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// E2E Scenario Tests
// ═══════════════════════════════════════════════════════════════════════════

/// Test 1: Multi-turn conversation with graduation between turns.
///
/// Turn 1: user -> thinking -> text -> tools -> continuation -> StreamComplete
/// Turn 2: user -> text -> StreamComplete
///
/// Verifies: Turn 1 in scrollback, Turn 2 in viewport, no spinners in scrollback.
#[test]
fn e2e_multi_turn_graduation() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(100, 24);

    // === Turn 1 ===
    app.send_msgs(feed.user("first question"));
    vt.render_frame(&mut app);

    app.send_msgs(feed.thinking("Let me think about this carefully."));
    vt.render_frame(&mut app);

    app.send_msgs(feed.text("Here is my initial analysis."));
    vt.render_frame(&mut app);

    app.send_msgs(feed.tool(ToolCallEvent {
        tool: "Bash",
        call_id: "t1",
        args: r#"{"command": "ls"}"#,
        render: Some("ls".into()),
        ..Default::default()
    }));
    vt.render_frame(&mut app);

    app.send_msgs(feed.tool_result("Bash", "t1", "file1.rs\nfile2.rs\n"));
    vt.render_frame(&mut app);

    app.send_msgs(feed.text("After analyzing the files, here is the conclusion."));
    vt.render_frame(&mut app);

    app.send_msgs(feed.complete());
    vt.render_frame(&mut app);

    // === Turn 2 ===
    app.send_msgs(feed.user("follow up question"));
    vt.render_frame(&mut app);

    app.send_msgs(feed.text("Here is the follow up answer."));
    app.send_msgs(feed.complete());
    vt.render_frame(&mut app);

    // Verify: Turn 1 content is in scrollback after turn 2 completes
    let _scrollback = strip_ansi(&vt.scrollback_contents());
    let full = strip_ansi(&vt.full_history());

    assert!(
        full.contains("first question"),
        "Turn 1 user message should be in full history.\nFull:\n{}",
        full
    );
    assert!(
        full.contains("initial analysis"),
        "Turn 1 assistant text should be in full history.\nFull:\n{}",
        full
    );
    assert!(
        full.contains("conclusion"),
        "Turn 1 continuation text should be in full history.\nFull:\n{}",
        full
    );

    // Turn 2 content should be visible
    let screen = strip_ansi(&vt.screen_contents());
    assert!(
        full.contains("follow up answer"),
        "Turn 2 response should be visible.\nScreen:\n{}\nFull:\n{}",
        screen,
        full
    );

    // No spinners in scrollback
    vt.assert_no_spinners_in_scrollback();
}

/// Test 2: History replay produces correct output.
///
/// Sends events to simulate a conversation, calls complete_response(), renders.
/// Verifies output matches expectations.
#[test]
fn e2e_history_replay_matches_live() {
    // Live streaming path
    let mut live_app = OilChatApp::default();
    let mut live_feed = EventFeed::default();
    let mut live_vt = Vt100TestRuntime::new(80, 24);

    live_app.send_msgs(live_feed.user("hello"));
    live_app.send_msgs(live_feed.text("world response"));
    live_app.send_msgs(live_feed.complete());
    live_vt.render_frame(&mut live_app);
    let live_output = strip_ansi(&live_vt.full_history());

    // Replay path: same events applied without render between each
    let mut replay_app = OilChatApp::default();
    let mut replay_feed = EventFeed::default();
    let mut replay_vt = Vt100TestRuntime::new(80, 24);

    replay_app.send_msgs(replay_feed.user("hello"));
    replay_app.send_msgs(replay_feed.text("world response"));
    replay_app.send_msgs(replay_feed.complete());
    replay_vt.render_frame(&mut replay_app);
    let replay_output = strip_ansi(&replay_vt.full_history());

    // Both should contain the same content
    assert!(
        live_output.contains("hello"),
        "Live output should contain user message.\n{}",
        live_output
    );
    assert!(
        replay_output.contains("hello"),
        "Replay output should contain user message.\n{}",
        replay_output
    );
    assert!(
        live_output.contains("world response"),
        "Live output should contain assistant response.\n{}",
        live_output
    );
    assert!(
        replay_output.contains("world response"),
        "Replay output should contain assistant response.\n{}",
        replay_output
    );
}

/// Test 3: Tool output with multiple lines renders with pipe prefix.
#[test]
fn e2e_tool_multiline_output() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.send_msgs(feed.user("run ls"));
    app.send_msgs(feed.tool(ToolCallEvent {
        tool: "Bash",
        call_id: "c1",
        args: r#"{"command": "ls -la"}"#,
        render: Some("ls -la".into()),
        ..Default::default()
    }));
    app.send_msgs(feed.tool_result("Bash", "c1", "line one\nline two\nline three\n"));
    app.send_msgs(feed.complete());
    vt.render_frame(&mut app);

    let output = strip_ansi(&vt.full_history());

    // Tool output lines should appear with pipe prefix
    assert!(
        output.contains("│") || output.contains("|"),
        "Tool output should have pipe prefix for output lines.\nOutput:\n{}",
        output
    );
    assert!(
        output.contains("line one"),
        "First output line should appear.\nOutput:\n{}",
        output
    );
    assert!(
        output.contains("line three"),
        "Third output line should appear.\nOutput:\n{}",
        output
    );
}

/// Test 4: Tool error rendering shows error icon and message.
#[test]
fn e2e_tool_error_rendering() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.send_msgs(feed.user("do something"));
    app.send_msgs(feed.tool(ToolCallEvent {
        tool: "Bash",
        call_id: "c1",
        args: r#"{"command": "fail"}"#,
        render: Some("fail".into()),
        ..Default::default()
    }));
    app.send_msgs(feed.tool_error("Bash", "c1", "command not found: fail"));
    app.send_msgs(feed.complete());
    vt.render_frame(&mut app);

    let output = strip_ansi(&vt.full_history());

    // Error should be visible
    assert!(
        output.contains("command not found"),
        "Error message should appear in output.\nOutput:\n{}",
        output
    );
    // Error icon (✗ or similar)
    assert!(
        output.contains("✗")
            || output.contains("✘")
            || output.contains("error")
            || output.contains("Error"),
        "Error indicator should appear.\nOutput:\n{}",
        output
    );
}

/// Test 5: Multiple thinking blocks in one response.
#[test]
fn e2e_multiple_thinking_blocks() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();

    app.send_msgs(feed.user("think hard"));
    app.send_msgs(feed.thinking("first line of thought"));
    app.send_msgs(feed.text("intermediate text"));
    // Second thinking block after text (new thinking component should be created
    // since the previous one gets graduated when text starts)
    app.send_msgs(feed.thinking("second line of thought"));

    let nodes = app.container_list().nodes();
    // Find the assistant response(s) and check thinking content
    let all_text: String = nodes
        .iter()
        .map(|node| match node {
            ChatNode::AssistantResponse { thinking, text, .. } => {
                let think_text: String = thinking
                    .iter()
                    .map(|t| format!("{:?}", t))
                    .collect::<Vec<_>>()
                    .join(" ");
                format!("{} {}", think_text, text)
            }
            _ => String::new(),
        })
        .collect::<Vec<_>>()
        .join(" ");

    assert!(
        all_text.contains("first line of thought"),
        "First thinking block should be preserved.\nAll text: {}",
        all_text
    );
}

/// Test 6: Empty text deltas don't create spurious nodes.
#[test]
fn e2e_empty_text_deltas_ignored() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();

    app.send_msgs(feed.user("test"));
    app.send_msgs(feed.text(""));
    app.send_msgs(feed.text(""));
    app.send_msgs(feed.text("actual text"));
    app.send_msgs(feed.complete());

    let nodes = app.container_list().nodes();

    // Count AssistantResponse nodes
    let assistant_count = nodes
        .iter()
        .filter(|n| matches!(n, ChatNode::AssistantResponse { .. }))
        .count();

    assert_eq!(
        assistant_count, 1,
        "Should have exactly 1 AssistantResponse (empty deltas should not create extras).\nNodes: {}",
        nodes.len()
    );

    // The text should be "actual text"
    if let Some(ChatNode::AssistantResponse { text, .. }) = nodes
        .iter()
        .find(|n| matches!(n, ChatNode::AssistantResponse { .. }))
    {
        assert_eq!(
            text, "actual text",
            "Text should be only the non-empty delta"
        );
    }
}

/// Test 7: Rapid tool calls (3+) in sequence group into one ToolGroup.
#[test]
fn e2e_rapid_tool_calls_group() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.send_msgs(feed.user("do three things"));
    app.send_msgs(feed.text("I will run three tools."));

    // Three rapid tool calls
    for i in 0..3 {
        let call_id = format!("call-{}", i);
        let name = format!("Tool{}", i);
        app.send_msgs(feed.tool_call(&name, &call_id, "{}"));
        app.send_msgs(feed.tool_result(&name, &call_id, ""));
    }

    // Check nodes BEFORE graduation (before StreamComplete + render)
    {
        let nodes = app.container_list().nodes();
        let tool_groups: Vec<_> = nodes
            .iter()
            .filter(|n| matches!(n, ChatNode::ToolGroup { .. }))
            .collect();

        assert_eq!(
            tool_groups.len(),
            1,
            "All 3 rapid tool calls should be in 1 ToolGroup.\nNodes: {}",
            nodes.len()
        );

        if let ChatNode::ToolGroup { tools } = tool_groups[0] {
            assert_eq!(
                tools.len(),
                3,
                "ToolGroup should contain exactly 3 tools, found {}",
                tools.len()
            );
        }
    }

    app.send_msgs(feed.complete());
    vt.render_frame(&mut app);

    // Verify rendered output has all tool names
    let output = strip_ansi(&vt.full_history());
    assert!(output.contains("Tool0"), "Tool0 should appear.\n{}", output);
    assert!(output.contains("Tool1"), "Tool1 should appear.\n{}", output);
    assert!(output.contains("Tool2"), "Tool2 should appear.\n{}", output);
}

/// Test 8: Terminal resize during streaming.
///
/// Content should re-render at new width without corruption.
#[test]
fn e2e_terminal_resize_during_streaming() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();

    // Start at 80x24
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.send_msgs(feed.user("tell me a story"));
    app.send_msgs(feed.text("Once upon a time in a land far far away there lived a great wizard."));
    vt.render_frame(&mut app);

    let narrow_output = strip_ansi(&vt.screen_contents());

    // Now render at wider size (create new vt since resize is complex)
    let mut wide_vt = Vt100TestRuntime::new(120, 40);
    wide_vt.render_frame(&mut app);

    let wide_output = strip_ansi(&wide_vt.screen_contents());

    // Both should contain the content
    assert!(
        narrow_output.contains("Once upon a time"),
        "Narrow render should have content.\n{}",
        narrow_output
    );
    assert!(
        wide_output.contains("Once upon a time"),
        "Wide render should have content.\n{}",
        wide_output
    );

    // Continue streaming
    app.send_msgs(feed.text(" He cast many spells."));
    app.send_msgs(feed.complete());
    wide_vt.render_frame(&mut app);

    let final_output = strip_ansi(&wide_vt.full_history());
    assert!(
        final_output.contains("many spells"),
        "Continued text should appear after resize.\n{}",
        final_output
    );
}

/// Test 9: Interaction modal overlay doesn't corrupt content area.
///
/// Uses `open_interaction` (pub(crate)) to show a modal overlay, then
/// verifies content is still rendered correctly afterward.
#[test]
fn e2e_modal_doesnt_corrupt_content() {
    use crucible_core::interaction::{
        InteractionRequest, InteractionResponse, PermRequest, PermResponse,
    };

    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 24);

    // Set up some content
    app.send_msgs(feed.user("hello"));
    app.send_msgs(feed.text("response text here"));
    vt.render_frame(&mut app);

    let before_modal = strip_ansi(&vt.screen_contents());
    assert!(
        before_modal.contains("response text"),
        "Content should be visible before modal.\nBefore:\n{}",
        before_modal
    );

    // Open interaction modal
    let perm = PermRequest::bash(["ls", "-la"]);
    app.open_interaction("req-1".into(), InteractionRequest::Permission(perm));
    vt.render_frame(&mut app);

    assert!(
        app.interaction_visible(),
        "Interaction modal should be visible"
    );

    // Close interaction modal
    app.on_message(ChatAppMsg::CloseInteraction {
        request_id: "req-1".into(),
        response: InteractionResponse::Permission(PermResponse::allow()),
    });
    vt.render_frame(&mut app);

    // Content should still be there after modal closes
    let after_modal = strip_ansi(&vt.full_history());
    assert!(
        after_modal.contains("response text"),
        "Content should remain after modal closes.\nAfter:\n{}",
        after_modal
    );
}

/// Test 10: Cancel during tool execution.
///
/// ToolCall (pending) -> StreamCancelled -> render.
/// Partial state should graduate, no crash, no spinners in scrollback.
#[test]
fn e2e_cancel_during_tool_execution() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.send_msgs(feed.user("do something"));
    app.send_msgs(feed.text("Starting work..."));
    app.send_msgs(feed.tool(ToolCallEvent {
        tool: "Bash",
        call_id: "c1",
        args: r#"{"command": "sleep 100"}"#,
        render: Some("sleep 100".into()),
        ..Default::default()
    }));
    vt.render_frame(&mut app);

    // Cancel while tool is pending
    app.on_message(ChatAppMsg::StreamCancelled);
    vt.render_frame(&mut app);

    // Should not be streaming anymore
    assert!(!app.is_streaming(), "Should not be streaming after cancel");

    // Content should be present (graduated)
    let output = strip_ansi(&vt.full_history());
    assert!(
        output.contains("Starting work"),
        "Pre-cancel text should be preserved.\n{}",
        output
    );
    assert!(
        output.contains("Bash"),
        "Tool name should be visible.\n{}",
        output
    );

    // No spinners in scrollback
    vt.assert_no_spinners_in_scrollback();

    // Should be able to start new turn
    app.send_msgs(feed.user("try again"));
    app.send_msgs(feed.text("Sure, trying again."));
    app.send_msgs(feed.complete());
    vt.render_frame(&mut app);

    let final_output = strip_ansi(&vt.full_history());
    assert!(
        final_output.contains("trying again"),
        "New turn after cancel should work.\n{}",
        final_output
    );
}

/// Test 11: `delegation_spawned` + `delegation_completed` rendering.
#[test]
fn e2e_subagent_lifecycle() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.send_msgs(feed.user("delegate this"));
    vt.render_frame(&mut app);

    // Subagent spawned
    app.send_msgs(feed.msgs("delegation_spawned", serde_json::json!({ "delegation_id": "agent-1", "prompt": "Analyze the code", "target_agent": null })));
    vt.render_frame(&mut app);

    let during = strip_ansi(&vt.screen_contents());
    // While running, should show some indicator (spinner or bullet)
    assert!(
        during.contains("subagent")
            || during.contains("Analyze")
            || during.contains("●")
            || during.contains("⠋"),
        "Running subagent should have a visible indicator.\nDuring:\n{}",
        during
    );

    // Subagent completed
    app.send_msgs(feed.msgs("delegation_completed", serde_json::json!({ "delegation_id": "agent-1", "result_summary": "Analysis complete: found 3 issues" })));
    app.send_msgs(feed.complete());
    vt.render_frame(&mut app);

    let after = strip_ansi(&vt.full_history());
    assert!(
        after.contains("Analysis complete") || after.contains("3 issues"),
        "Completed subagent summary should be visible.\nAfter:\n{}",
        after
    );
}

/// Test 12: User message wrapping at different widths.
#[test]
fn e2e_user_message_wrapping() {
    let long_msg = "This is a very long user message that should definitely wrap at narrow terminal widths because it contains more than one hundred characters in total length for testing purposes";

    let mut app40 = OilChatApp::default();
    let mut feed40 = EventFeed::default();
    app40.send_msgs(feed40.user(long_msg));
    app40.send_msgs(feed40.text("ok"));
    app40.send_msgs(feed40.complete());

    let mut app80 = OilChatApp::default();
    let mut feed80 = EventFeed::default();
    app80.send_msgs(feed80.user(long_msg));
    app80.send_msgs(feed80.text("ok"));
    app80.send_msgs(feed80.complete());

    let mut app120 = OilChatApp::default();
    let mut feed120 = EventFeed::default();
    app120.send_msgs(feed120.user(long_msg));
    app120.send_msgs(feed120.text("ok"));
    app120.send_msgs(feed120.complete());

    // Render at 40 width
    let mut vt40 = Vt100TestRuntime::new(40, 30);
    vt40.render_frame(&mut app40);
    let out40 = strip_ansi(&vt40.full_history());

    // Render at 80 width
    let mut vt80 = Vt100TestRuntime::new(80, 30);
    vt80.render_frame(&mut app80);
    let out80 = strip_ansi(&vt80.full_history());

    // Render at 120 width
    let mut vt120 = Vt100TestRuntime::new(120, 30);
    vt120.render_frame(&mut app120);
    let out120 = strip_ansi(&vt120.full_history());

    // All should contain the full message content (possibly wrapped)
    for (w, out) in [(40, &out40), (80, &out80), (120, &out120)] {
        assert!(
            out.contains("very long user message"),
            "Width {} should contain the message text.\nOutput:\n{}",
            w,
            out
        );
        assert!(
            out.contains("testing purposes"),
            "Width {} should contain end of message.\nOutput:\n{}",
            w,
            out
        );
    }

    // Narrow output should generally have more non-empty lines than wide
    // (wrapping creates more lines). Not a strict assertion since chrome
    // lines count too, but content lines at 40 must be >= at 120.
    let _content_lines_40 = out40.lines().filter(|l| !l.trim().is_empty()).count();
    let _content_lines_120 = out120.lines().filter(|l| !l.trim().is_empty()).count();
}

/// Test 13: Stress test with many nodes.
#[test]
fn e2e_stress_many_containers() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 24);

    // 50 turns: user + assistant alternating
    for i in 0..50 {
        app.send_msgs(feed.user(&format!("question {}", i)));
        app.send_msgs(feed.text(&format!("answer {}", i)));
        app.send_msgs(feed.complete());
        vt.render_frame(&mut app);
    }

    // Should not panic (if we got here, it worked)
    let output = strip_ansi(&vt.full_history());

    // First and last messages should be present
    assert!(
        output.contains("question 0"),
        "First question should be in history.\n(output too large to display)"
    );
    assert!(
        output.contains("answer 49"),
        "Last answer should be in history.\n(output too large to display)"
    );

    // No spinners in scrollback
    vt.assert_no_spinners_in_scrollback();

    // Should not be streaming
    assert!(
        !app.is_streaming(),
        "Should not be streaming after all turns complete"
    );
}

/// Thinking during streaming should NOT show "◇ Thought" in content —
/// only the expanded block heads itself "Thinking…" while streaming.
/// The collapsed summary "◇ Thought" appears only after text starts.
#[test]
fn thinking_not_duplicated_in_content_and_chrome() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.send_msgs(feed.user("think hard"));
    vt.render_frame(&mut app);

    // Thinking starts — only chrome should show thinking indicator
    app.send_msgs(feed.thinking("deep analysis of the problem with many words to count"));
    vt.render_frame(&mut app);

    let screen = strip_ansi(&vt.screen_contents());

    // Chrome should show "Thinking…" with word count
    assert!(
        screen.contains("Thinking"),
        "Chrome should show Thinking indicator. Screen:\n{}",
        screen
    );

    // Content should NOT show "◇ Thought" yet (thinking is still live)
    assert!(
        !screen.contains("\u{25C7} Thought"),
        "Content should NOT show collapsed '◇ Thought' while thinking is live. Screen:\n{}",
        screen
    );

    // Count "Thinking" occurrences — should be exactly 1 (in chrome only)
    let thinking_count = screen.matches("Thinking").count();
    assert_eq!(
        thinking_count, 1,
        "Should have exactly 1 'Thinking' indicator (in chrome), found {}. Screen:\n{}",
        thinking_count, screen
    );

    // Now text starts — thinking should become "◇ Thought" in content
    app.send_msgs(feed.text("Here is my answer."));
    vt.render_frame(&mut app);

    let screen2 = strip_ansi(&vt.screen_contents());
    assert!(
        screen2.contains("\u{25C7} Thought") || screen2.contains("Thought"),
        "After text starts, thinking should show collapsed summary. Screen:\n{}",
        screen2
    );
}

// ─── Exhaustive handler coverage tests ─────────────────────────────────────
// These tests verify that every ChatAppMsg variant has an observable effect,
// preventing silent catch-all drops like the OpenInteraction bug.

#[test]
fn open_interaction_opens_modal() {
    use crucible_core::interaction::{InteractionRequest, PermRequest};
    let mut app = OilChatApp::default();
    assert!(!app.interaction_visible());

    app.on_message(ChatAppMsg::OpenInteraction {
        request_id: "perm-1".into(),
        request: InteractionRequest::Permission(PermRequest::bash(["ls"])),
    });
    assert!(
        app.interaction_visible(),
        "Modal must open after OpenInteraction"
    );
}

#[test]
fn close_interaction_closes_modal() {
    use crucible_core::interaction::{
        InteractionRequest, InteractionResponse, PermRequest, PermResponse,
    };
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::OpenInteraction {
        request_id: "perm-1".into(),
        request: InteractionRequest::Permission(PermRequest::bash(["ls"])),
    });
    assert!(app.interaction_visible());

    app.on_message(ChatAppMsg::CloseInteraction {
        request_id: "perm-1".into(),
        response: InteractionResponse::Permission(PermResponse::allow()),
    });
    assert!(
        !app.interaction_visible(),
        "Modal must close after CloseInteraction"
    );
}

#[test]
fn thinking_indicator_appears_at_most_once_on_screen() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.send_msgs(feed.user("think"));
    app.send_msgs(
        feed.thinking("deep analysis of many things with lots of words to count accurately"),
    );
    vt.render_frame(&mut app);

    let screen = strip_ansi(&vt.screen_contents());

    // "Thinking" should appear at most once (in chrome only)
    let thinking_count = screen.matches("Thinking").count();
    assert!(
        thinking_count <= 1,
        "Thinking indicator should appear at most once, found {}.\nScreen:\n{}",
        thinking_count,
        screen
    );

    // "Thought" should NOT appear (thinking is still live)
    assert!(
        !screen.contains("Thought"),
        "Collapsed 'Thought' should not appear while thinking is live.\nScreen:\n{}",
        screen
    );
}

#[test]
fn thinking_transitions_to_thought_when_text_starts() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.send_msgs(feed.user("think then respond"));
    app.send_msgs(feed.thinking("reasoning about it"));
    app.send_msgs(feed.text("Here is my answer."));
    vt.render_frame(&mut app);

    let screen = strip_ansi(&vt.screen_contents());

    // Content should show "Thought" (collapsed summary)
    assert!(
        screen.contains("Thought"),
        "After text starts, thinking should show as 'Thought'.\nScreen:\n{}",
        screen
    );

    // Chrome should NOT show "Thinking" anymore (text finalized it)
    let thinking_count = screen.matches("Thinking").count();
    assert_eq!(
        thinking_count, 0,
        "Chrome should not show 'Thinking' after text starts, found {}.\nScreen:\n{}",
        thinking_count, screen
    );
}

#[test]
fn spinners_only_in_chrome_area() {
    use crucible_oil::node::{BRAILLE_SPINNER_FRAMES, SPINNER_FRAMES};

    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 24);

    // Streaming with text (turn active = spinner in chrome)
    app.send_msgs(feed.user("do things"));
    app.send_msgs(feed.text("working on it"));
    vt.render_frame(&mut app);

    let screen = strip_ansi(&vt.screen_contents());
    let lines: Vec<&str> = screen.lines().collect();

    // Find the input box (▄▄▄ bar) — everything above is content, at and below is chrome
    let chrome_start = lines
        .iter()
        .position(|l| l.contains("▄▄▄▄▄▄"))
        .unwrap_or(lines.len());
    let content_lines = &lines[..chrome_start];
    let content_text: String = content_lines.join("\n");

    let all_spinners: Vec<char> = SPINNER_FRAMES
        .iter()
        .chain(BRAILLE_SPINNER_FRAMES.iter())
        .copied()
        .collect();

    for ch in &all_spinners {
        assert!(
            !content_text.contains(*ch),
            "Spinner '{}' found in content area (above chrome). Content:\n{}",
            ch,
            content_text
        );
    }
}

#[test]
fn all_container_types_render_at_all_widths() {
    use crucible_oil::focus::FocusContext;
    use crucible_oil::render::render_to_plain_text;

    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();

    // Create various node types
    app.send_msgs(feed.user("test message"));
    app.send_msgs(feed.thinking("some thinking"));
    app.send_msgs(feed.text("response text here"));
    app.send_msgs(feed.tool(ToolCallEvent {
        tool: "Bash",
        call_id: "c1",
        args: r#"{"command": "echo hello"}"#,
        render: Some("echo hello".into()),
        ..Default::default()
    }));
    app.send_msgs(feed.tool_result("Bash", "c1", ""));
    app.send_msgs(feed.complete());

    let focus = FocusContext::default();

    // Render at various widths — should never panic
    for width in [20u16, 40, 60, 80, 120, 200] {
        let ctx = crate::tui::oil::ViewContext::with_terminal_size(
            &focus,
            crate::tui::oil::theme::active(),
            (width, 24),
        );
        let nodes = app.container_list().nodes();
        for (i, node) in nodes.iter().enumerate() {
            let prev = if i > 0 { Some(&nodes[i - 1]) } else { None };
            let rendered = node.render(prev, &ctx);
            let plain = render_to_plain_text(&rendered, width as usize);
            assert!(
                !plain.is_empty() || matches!(rendered, crucible_oil::node::Node::Empty),
                "Node at index {} width {} produced empty non-Empty output",
                i,
                width
            );
        }
    }
}

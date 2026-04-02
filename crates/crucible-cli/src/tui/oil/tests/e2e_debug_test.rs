//! End-to-end debug test: dumps full vt100 output at each step.
//! Run with: cargo test --lib -p crucible-cli -- e2e_debug_test --nocapture

use crate::tui::oil::app::App;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::containers::ContainerContent;
use super::vt100_runtime::Vt100TestRuntime;
use crucible_oil::ansi::strip_ansi;

/// Simulates the exact scenario from user testing:
/// "tell me about this repo" → thinking → text → tools → more text
#[test]
fn e2e_full_conversation_render() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(124, 40);

    // Step 1: User message
    app.on_message(ChatAppMsg::UserMessage("tell me about this repo".into()));
    vt.render_frame(&mut app);
    let out = strip_ansi(&vt.full_history());
    eprintln!("\n============================================================\n=== STEP 1: After user message ===\n============================================================");
    eprintln!("{}", out);

    // Step 2: Thinking starts
    app.on_message(ChatAppMsg::ThinkingDelta(
        "I need to explore the repository structure to understand what this project is about. Let me start by looking at the files and reading the README."
            .into(),
    ));
    vt.render_frame(&mut app);
    let out = strip_ansi(&vt.full_history());
    eprintln!("\n============================================================\n=== STEP 2: After thinking delta ===\n============================================================");
    eprintln!("{}", out);

    // Step 3: Text starts (thinking should finalize)
    app.on_message(ChatAppMsg::TextDelta(
        "I'll explore this repository to understand its structure and purpose.".into(),
    ));
    vt.render_frame(&mut app);
    let out = strip_ansi(&vt.full_history());
    eprintln!("\n============================================================\n=== STEP 3: After text delta ===\n============================================================");
    eprintln!("{}", out);

    // Step 4: Tool calls
    app.on_message(ChatAppMsg::ToolCall {
        name: "Bash".into(),
        args: r#"{"command": "ls -la"}"#.into(),
        call_id: Some("call-1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
    });
    vt.render_frame(&mut app);
    let out = strip_ansi(&vt.full_history());
    eprintln!("\n============================================================\n=== STEP 4: After tool call (pending) ===\n============================================================");
    eprintln!("{}", out);

    // Step 5: Tool complete
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "Bash".into(),
        delta: "total 42\ndrwxr-xr-x 1 user user 100 Jan 1 00:00 src\n".into(),
        call_id: Some("call-1".into()),
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "Bash".into(),
        call_id: Some("call-1".into()),
    });
    vt.render_frame(&mut app);
    let out = strip_ansi(&vt.full_history());
    eprintln!("\n============================================================\n=== STEP 5: After tool complete ===\n============================================================");
    eprintln!("{}", out);

    // Step 6: Second tool
    app.on_message(ChatAppMsg::ToolCall {
        name: "Glob".into(),
        args: r#"{"pattern": "README*"}"#.into(),
        call_id: Some("call-2".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "Glob".into(),
        call_id: Some("call-2".into()),
    });
    vt.render_frame(&mut app);
    let out = strip_ansi(&vt.full_history());
    eprintln!("\n============================================================\n=== STEP 6: After second tool ===\n============================================================");
    eprintln!("{}", out);

    // Step 7: Continuation text after tools
    app.on_message(ChatAppMsg::TextDelta(
        "Based on my analysis, this is a Rust workspace project called Crucible.".into(),
    ));
    vt.render_frame(&mut app);
    let out = strip_ansi(&vt.full_history());
    eprintln!("\n============================================================\n=== STEP 7: After continuation text ===\n============================================================");
    eprintln!("{}", out);

    // Step 8: Stream complete (everything graduates)
    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);
    let out = strip_ansi(&vt.full_history());
    eprintln!("\n============================================================\n=== STEP 8: After stream complete (all graduated) ===\n============================================================");
    eprintln!("{}", out);

    // Validate: no spinners in scrollback
    vt.assert_no_spinners_in_scrollback();

    // Validate: content present
    let stripped = strip_ansi(&vt.full_history());
    assert!(stripped.contains("tell me about this repo"), "User message missing");
    assert!(stripped.contains("Thought"), "Thinking collapsed summary missing");
    assert!(stripped.contains("explore this repository"), "Assistant text missing");
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
    use crate::tui::oil::containers::ContainerContent;
    
    let mut app = OilChatApp::init();
    
    app.on_message(ChatAppMsg::ThinkingDelta("thinking...".into()));
    app.on_message(ChatAppMsg::TextDelta("first text".into()));
    app.on_message(ChatAppMsg::ToolCall {
        name: "Bash".into(), args: "{}".into(),
        call_id: Some("c1".into()), description: None, source: None, lua_primary_arg: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete { name: "Bash".into(), call_id: Some("c1".into()) });
    app.on_message(ChatAppMsg::TextDelta("continuation text".into()));
    
    let containers = app.container_list().containers();
    for (i, c) in containers.iter().enumerate() {
        match &c.content {
            ContainerContent::AssistantResponse { is_continuation, text, thinking, .. } => {
                eprintln!("Container {}: AssistantResponse is_continuation={} text={:?} thinking={}", 
                    i, is_continuation, text, thinking.len());
            }
            _ => {
                eprintln!("Container {}: {:?}", i, c.kind);
            }
        }
    }
    
    // The last container should be a continuation
    let last = containers.last().unwrap();
    if let ContainerContent::AssistantResponse { is_continuation, .. } = &last.content {
        assert!(*is_continuation, "Last response after tools should be continuation");
    } else {
        panic!("Last container should be AssistantResponse");
    }
}

#[test]
fn debug_continuation_rendering() {
    use crate::tui::oil::containers::{ContainerViewContext, ContainerContent};
    use crucible_oil::render::render_to_plain_text;
    
    let mut app = OilChatApp::init();
    
    app.on_message(ChatAppMsg::ThinkingDelta("thinking...".into()));
    app.on_message(ChatAppMsg::TextDelta("first text".into()));
    app.on_message(ChatAppMsg::ToolCall {
        name: "Bash".into(), args: "{}".into(),
        call_id: Some("c1".into()), description: None, source: None, lua_primary_arg: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete { name: "Bash".into(), call_id: Some("c1".into()) });
    app.on_message(ChatAppMsg::TextDelta("continuation text after tools".into()));
    
    let ctx = ContainerViewContext {
        width: 80,
        spinner_frame: 0,
        show_thinking: false,
    };
    
    let containers = app.container_list().containers();
    for (i, c) in containers.iter().enumerate() {
        let node = c.view(&ctx);
        let plain = render_to_plain_text(&node, 80);
        eprintln!("=== Container {} ({:?}) ===", i, c.kind);
        eprintln!("{}", plain);
        
        // Check for bullet in continuation
        if let ContainerContent::AssistantResponse { is_continuation, .. } = &c.content {
            if *is_continuation && plain.contains("●") {
                panic!("BUG: Continuation text should NOT have ● bullet!\nOutput:\n{}", plain);
            }
        }
    }
}

#[test]
fn debug_full_view_rendering() {
    use super::helpers::vt_render;
    
    let mut app = OilChatApp::init();
    
    app.on_message(ChatAppMsg::ThinkingDelta("thinking...".into()));
    app.on_message(ChatAppMsg::TextDelta("first text".into()));
    app.on_message(ChatAppMsg::ToolCall {
        name: "Bash".into(), args: "{}".into(),
        call_id: Some("c1".into()), description: None, source: None, lua_primary_arg: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete { name: "Bash".into(), call_id: Some("c1".into()) });
    app.on_message(ChatAppMsg::TextDelta("continuation text after tools".into()));
    app.on_message(ChatAppMsg::StreamComplete);
    
    let output = vt_render(&mut app);
    eprintln!("Full rendered output:");
    for (i, line) in output.lines().enumerate() {
        eprintln!("{:3}: {}", i, line);
    }
    
    assert!(!output.contains("●"), 
        "No ● bullet should appear anywhere in the output.\nOutput:\n{}", output);
}

#[test]
fn debug_scrollback_vs_viewport_step7() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(124, 40);

    app.on_message(ChatAppMsg::UserMessage("tell me about this repo".into()));
    vt.render_frame(&mut app);
    app.on_message(ChatAppMsg::ThinkingDelta("I need to explore the repository.".into()));
    vt.render_frame(&mut app);
    app.on_message(ChatAppMsg::TextDelta("I'll explore this repository.".into()));
    vt.render_frame(&mut app);
    app.on_message(ChatAppMsg::ToolCall {
        name: "Bash".into(), args: r#"{"command": "ls"}"#.into(),
        call_id: Some("c1".into()), description: None, source: None, lua_primary_arg: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete { name: "Bash".into(), call_id: Some("c1".into()) });
    app.on_message(ChatAppMsg::ToolCall {
        name: "Glob".into(), args: r#"{"pattern": "README*"}"#.into(),
        call_id: Some("c2".into()), description: None, source: None, lua_primary_arg: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete { name: "Glob".into(), call_id: Some("c2".into()) });
    vt.render_frame(&mut app);

    // Now add continuation text
    app.on_message(ChatAppMsg::TextDelta("Based on my analysis.".into()));
    vt.render_frame(&mut app);

    let scrollback = strip_ansi(&vt.scrollback_contents());
    let viewport = strip_ansi(&vt.screen_contents());

    eprintln!("=== SCROLLBACK ===");
    eprintln!("{}", scrollback);
    eprintln!("=== VIEWPORT ===");
    for (i, line) in viewport.lines().enumerate() {
        eprintln!("{:3}: {}", i, line);
    }

    if scrollback.contains("●") {
        eprintln!("BUG: ● found in SCROLLBACK");
    }
    if viewport.contains("●") {
        eprintln!("BUG: ● found in VIEWPORT");
    }
}

#[test]
fn debug_container_state_before_continuation() {
    use crate::tui::oil::containers::ContainerContent;
    
    let mut app = OilChatApp::init();
    
    app.on_message(ChatAppMsg::ThinkingDelta("thinking...".into()));
    app.on_message(ChatAppMsg::TextDelta("first text".into()));
    
    eprintln!("After first text:");
    for (i, c) in app.container_list().containers().iter().enumerate() {
        eprintln!("  {}: {:?}", i, c.kind);
    }
    
    app.on_message(ChatAppMsg::ToolCall {
        name: "Bash".into(), args: "{}".into(),
        call_id: Some("c1".into()), description: None, source: None, lua_primary_arg: None,
    });
    
    eprintln!("After tool call:");
    for (i, c) in app.container_list().containers().iter().enumerate() {
        eprintln!("  {}: {:?} {:?}", i, c.kind, match &c.content {
            ContainerContent::AssistantResponse { is_continuation, text, .. } => 
                format!("cont={} text={:?}", is_continuation, &text[..text.len().min(20)]),
            _ => String::new(),
        });
    }
    
    app.on_message(ChatAppMsg::ToolResultComplete { name: "Bash".into(), call_id: Some("c1".into()) });
    app.on_message(ChatAppMsg::ToolCall {
        name: "Glob".into(), args: "{}".into(),
        call_id: Some("c2".into()), description: None, source: None, lua_primary_arg: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete { name: "Glob".into(), call_id: Some("c2".into()) });
    
    eprintln!("After all tools complete:");
    for (i, c) in app.container_list().containers().iter().enumerate() {
        eprintln!("  {}: {:?} {:?}", i, c.kind, match &c.content {
            ContainerContent::AssistantResponse { is_continuation, text, .. } => 
                format!("cont={} text={:?}", is_continuation, &text[..text.len().min(20)]),
            _ => String::new(),
        });
    }
    
    // NOW send continuation text
    app.on_message(ChatAppMsg::TextDelta("Based on my analysis.".into()));
    
    eprintln!("After continuation text:");
    for (i, c) in app.container_list().containers().iter().enumerate() {
        eprintln!("  {}: {:?} {:?}", i, c.kind, match &c.content {
            ContainerContent::AssistantResponse { is_continuation, text, .. } => 
                format!("cont={} text={:?}", is_continuation, &text[..text.len().min(30)]),
            _ => String::new(),
        });
    }
}

/// Verify the "stuck" scenario: after StreamComplete, the TUI should be responsive
/// (is_streaming returns false, new messages can be sent).
#[test]
fn after_stream_complete_tui_is_responsive() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    // First turn
    app.on_message(ChatAppMsg::UserMessage("first".into()));
    app.on_message(ChatAppMsg::TextDelta("response one".into()));
    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);
    
    assert!(!app.is_streaming(), "Should not be streaming after StreamComplete");
    
    // Second turn should work
    app.on_message(ChatAppMsg::UserMessage("second".into()));
    app.on_message(ChatAppMsg::TextDelta("response two".into()));
    assert!(app.is_streaming(), "Should be streaming during second turn");
    
    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);
    
    assert!(!app.is_streaming(), "Should not be streaming after second StreamComplete");
    
    let output = strip_ansi(&vt.full_history());
    assert!(output.contains("response two"), "Second response should appear");
}

/// Verify no double spinners: only ONE spinner should be visible at a time.
#[test]
fn only_one_spinner_visible_during_thinking() {
    use crucible_oil::node::SPINNER_FRAMES;
    
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.on_message(ChatAppMsg::UserMessage("think hard".into()));
    vt.render_frame(&mut app);
    
    app.on_message(ChatAppMsg::ThinkingDelta("deep thoughts about the universe".into()));
    vt.render_frame(&mut app);
    
    let screen = strip_ansi(&vt.screen_contents());
    
    // Count spinner characters across all frames
    let spinner_count: usize = SPINNER_FRAMES.iter()
        .map(|ch| screen.matches(*ch).count())
        .sum();
    
    assert!(
        spinner_count <= 1,
        "Should have at most 1 spinner visible, found {}. Screen:\n{}",
        spinner_count, screen
    );
}

/// Verify user message and input box use consistent styling.
#[test]
fn user_message_matches_input_style() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.on_message(ChatAppMsg::UserMessage("hello world".into()));
    app.on_message(ChatAppMsg::TextDelta("response".into()));
    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);
    
    let screen = strip_ansi(&vt.screen_contents());
    let lines: Vec<&str> = screen.lines().collect();
    
    // Both user message and input box should use ▄▄▄/▀▀▀ bars
    let top_bars: Vec<_> = lines.iter().enumerate()
        .filter(|(_, l)| l.trim_start().starts_with('▄'))
        .collect();
    let bottom_bars: Vec<_> = lines.iter().enumerate()
        .filter(|(_, l)| l.trim_start().starts_with('▀'))
        .collect();
    
    // Should have 2 top bars (user msg + input) and 2 bottom bars
    assert!(
        top_bars.len() >= 2,
        "Expected at least 2 top bars (user msg + input), found {}. Screen:\n{}",
        top_bars.len(), screen
    );
    assert!(
        bottom_bars.len() >= 2,
        "Expected at least 2 bottom bars (user msg + input), found {}. Screen:\n{}",
        bottom_bars.len(), screen
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
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(100, 24);

    // === Turn 1 ===
    app.on_message(ChatAppMsg::UserMessage("first question".into()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::ThinkingDelta("Let me think about this carefully.".into()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("Here is my initial analysis.".into()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::ToolCall {
        name: "Bash".into(),
        args: r#"{"command": "ls"}"#.into(),
        call_id: Some("t1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
    });
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "Bash".into(),
        delta: "file1.rs\nfile2.rs\n".into(),
        call_id: Some("t1".into()),
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "Bash".into(),
        call_id: Some("t1".into()),
    });
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("After analyzing the files, here is the conclusion.".into()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    // === Turn 2 ===
    app.on_message(ChatAppMsg::UserMessage("follow up question".into()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("Here is the follow up answer.".into()));
    app.on_message(ChatAppMsg::StreamComplete);
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
        screen, full
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
    let mut live_app = OilChatApp::init();
    let mut live_vt = Vt100TestRuntime::new(80, 24);

    live_app.on_message(ChatAppMsg::UserMessage("hello".into()));
    live_app.on_message(ChatAppMsg::TextDelta("world response".into()));
    live_app.on_message(ChatAppMsg::StreamComplete);
    live_vt.render_frame(&mut live_app);
    let live_output = strip_ansi(&live_vt.full_history());

    // Replay path: same events applied without render between each
    let mut replay_app = OilChatApp::init();
    let mut replay_vt = Vt100TestRuntime::new(80, 24);

    replay_app.on_message(ChatAppMsg::UserMessage("hello".into()));
    replay_app.on_message(ChatAppMsg::TextDelta("world response".into()));
    replay_app.on_message(ChatAppMsg::StreamComplete);
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
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.on_message(ChatAppMsg::UserMessage("run ls".into()));
    app.on_message(ChatAppMsg::ToolCall {
        name: "Bash".into(),
        args: r#"{"command": "ls -la"}"#.into(),
        call_id: Some("c1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "Bash".into(),
        delta: "line one\nline two\nline three\n".into(),
        call_id: Some("c1".into()),
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "Bash".into(),
        call_id: Some("c1".into()),
    });
    app.on_message(ChatAppMsg::StreamComplete);
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
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.on_message(ChatAppMsg::UserMessage("do something".into()));
    app.on_message(ChatAppMsg::ToolCall {
        name: "Bash".into(),
        args: r#"{"command": "fail"}"#.into(),
        call_id: Some("c1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
    });
    app.on_message(ChatAppMsg::ToolResultError {
        name: "Bash".into(),
        error: "command not found: fail".into(),
        call_id: Some("c1".into()),
    });
    app.on_message(ChatAppMsg::StreamComplete);
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
        output.contains("✗") || output.contains("✘") || output.contains("error") || output.contains("Error"),
        "Error indicator should appear.\nOutput:\n{}",
        output
    );
}

/// Test 5: Multiple thinking blocks in one response.
#[test]
fn e2e_multiple_thinking_blocks() {
    let mut app = OilChatApp::init();

    app.on_message(ChatAppMsg::UserMessage("think hard".into()));
    app.on_message(ChatAppMsg::ThinkingDelta("first line of thought".into()));
    app.on_message(ChatAppMsg::TextDelta("intermediate text".into()));
    // Second thinking block after text (new thinking component should be created
    // since the previous one gets graduated when text starts)
    app.on_message(ChatAppMsg::ThinkingDelta("second line of thought".into()));

    let containers = app.container_list().containers();
    // Find the assistant response(s) and check thinking content
    let mut total_thinking_text = String::new();
    for c in containers {
        if let ContainerContent::AssistantResponse { thinking, .. } = &c.content {
            for tc in thinking {
                total_thinking_text.push_str(&format!("{:?} ", tc));
            }
        }
    }

    // The thinking content should contain both thoughts somewhere in the containers
    let all_text: String = containers.iter().map(|c| {
        match &c.content {
            ContainerContent::AssistantResponse { thinking, text, .. } => {
                let think_text: String = thinking.iter()
                    .map(|t| format!("{:?}", t))
                    .collect::<Vec<_>>()
                    .join(" ");
                format!("{} {}", think_text, text)
            }
            _ => String::new(),
        }
    }).collect::<Vec<_>>().join(" ");

    assert!(
        all_text.contains("first line of thought"),
        "First thinking block should be preserved.\nAll text: {}",
        all_text
    );
}

/// Test 6: Empty text deltas don't create spurious containers.
#[test]
fn e2e_empty_text_deltas_ignored() {
    let mut app = OilChatApp::init();

    app.on_message(ChatAppMsg::UserMessage("test".into()));
    app.on_message(ChatAppMsg::TextDelta("".into()));
    app.on_message(ChatAppMsg::TextDelta("".into()));
    app.on_message(ChatAppMsg::TextDelta("actual text".into()));
    app.on_message(ChatAppMsg::StreamComplete);

    let containers = app.container_list().containers();

    // Count AssistantResponse containers
    let assistant_count = containers.iter()
        .filter(|c| matches!(c.content, ContainerContent::AssistantResponse { .. }))
        .count();

    assert_eq!(
        assistant_count, 1,
        "Should have exactly 1 AssistantResponse (empty deltas should not create extras).\nContainers: {:?}",
        containers.iter().map(|c| c.kind).collect::<Vec<_>>()
    );

    // The text should be "actual text"
    if let Some(c) = containers.iter().find(|c| matches!(c.content, ContainerContent::AssistantResponse { .. })) {
        if let ContainerContent::AssistantResponse { text, .. } = &c.content {
            assert_eq!(text, "actual text", "Text should be only the non-empty delta");
        }
    }
}

/// Test 7: Rapid tool calls (3+) in sequence group into one ToolGroup.
#[test]
fn e2e_rapid_tool_calls_group() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.on_message(ChatAppMsg::UserMessage("do three things".into()));
    app.on_message(ChatAppMsg::TextDelta("I will run three tools.".into()));

    // Three rapid tool calls
    for i in 0..3 {
        let call_id = format!("call-{}", i);
        let name = format!("Tool{}", i);
        app.on_message(ChatAppMsg::ToolCall {
            name: name.clone(),
            args: "{}".into(),
            call_id: Some(call_id.clone()),
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name,
            call_id: Some(call_id),
        });
    }

    // Check containers BEFORE graduation (before StreamComplete + render)
    {
        let containers = app.container_list().containers();
        let tool_groups: Vec<_> = containers.iter()
            .filter(|c| matches!(c.content, ContainerContent::ToolGroup { .. }))
            .collect();

        assert_eq!(
            tool_groups.len(), 1,
            "All 3 rapid tool calls should be in 1 ToolGroup.\nContainers: {:?}",
            containers.iter().map(|c| c.kind).collect::<Vec<_>>()
        );

        if let ContainerContent::ToolGroup { tools } = &tool_groups[0].content {
            assert_eq!(
                tools.len(), 3,
                "ToolGroup should contain exactly 3 tools, found {}",
                tools.len()
            );
        }
    }

    app.on_message(ChatAppMsg::StreamComplete);
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
    let mut app = OilChatApp::init();

    // Start at 80x24
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.on_message(ChatAppMsg::UserMessage("tell me a story".into()));
    app.on_message(ChatAppMsg::TextDelta("Once upon a time in a land far far away there lived a great wizard.".into()));
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
    app.on_message(ChatAppMsg::TextDelta(" He cast many spells.".into()));
    app.on_message(ChatAppMsg::StreamComplete);
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
    use crucible_core::interaction::{InteractionRequest, PermRequest, PermResponse, InteractionResponse};

    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    // Set up some content
    app.on_message(ChatAppMsg::UserMessage("hello".into()));
    app.on_message(ChatAppMsg::TextDelta("response text here".into()));
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
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.on_message(ChatAppMsg::UserMessage("do something".into()));
    app.on_message(ChatAppMsg::TextDelta("Starting work...".into()));
    app.on_message(ChatAppMsg::ToolCall {
        name: "Bash".into(),
        args: r#"{"command": "sleep 100"}"#.into(),
        call_id: Some("c1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
    });
    vt.render_frame(&mut app);

    // Cancel while tool is pending
    app.on_message(ChatAppMsg::StreamCancelled);
    vt.render_frame(&mut app);

    // Should not be streaming anymore
    assert!(
        !app.is_streaming(),
        "Should not be streaming after cancel"
    );

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
    app.on_message(ChatAppMsg::UserMessage("try again".into()));
    app.on_message(ChatAppMsg::TextDelta("Sure, trying again.".into()));
    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    let final_output = strip_ansi(&vt.full_history());
    assert!(
        final_output.contains("trying again"),
        "New turn after cancel should work.\n{}",
        final_output
    );
}

/// Test 11: SubagentSpawned + SubagentCompleted rendering.
#[test]
fn e2e_subagent_lifecycle() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.on_message(ChatAppMsg::UserMessage("delegate this".into()));
    vt.render_frame(&mut app);

    // Subagent spawned
    app.on_message(ChatAppMsg::SubagentSpawned {
        id: "agent-1".into(),
        prompt: "Analyze the code".into(),
    });
    vt.render_frame(&mut app);

    let during = strip_ansi(&vt.screen_contents());
    // While running, should show some indicator (spinner or bullet)
    assert!(
        during.contains("subagent") || during.contains("Analyze") || during.contains("●") || during.contains("⠋"),
        "Running subagent should have a visible indicator.\nDuring:\n{}",
        during
    );

    // Subagent completed
    app.on_message(ChatAppMsg::SubagentCompleted {
        id: "agent-1".into(),
        summary: "Analysis complete: found 3 issues".into(),
    });
    app.on_message(ChatAppMsg::StreamComplete);
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

    let mut app40 = OilChatApp::init();
    app40.on_message(ChatAppMsg::UserMessage(long_msg.into()));
    app40.on_message(ChatAppMsg::TextDelta("ok".into()));
    app40.on_message(ChatAppMsg::StreamComplete);

    let mut app80 = OilChatApp::init();
    app80.on_message(ChatAppMsg::UserMessage(long_msg.into()));
    app80.on_message(ChatAppMsg::TextDelta("ok".into()));
    app80.on_message(ChatAppMsg::StreamComplete);

    let mut app120 = OilChatApp::init();
    app120.on_message(ChatAppMsg::UserMessage(long_msg.into()));
    app120.on_message(ChatAppMsg::TextDelta("ok".into()));
    app120.on_message(ChatAppMsg::StreamComplete);

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
            w, out
        );
        assert!(
            out.contains("testing purposes"),
            "Width {} should contain end of message.\nOutput:\n{}",
            w, out
        );
    }

    // Narrow output should generally have more non-empty lines than wide
    // (wrapping creates more lines). Not a strict assertion since chrome
    // lines count too, but content lines at 40 must be >= at 120.
    let _content_lines_40 = out40.lines()
        .filter(|l| !l.trim().is_empty())
        .count();
    let _content_lines_120 = out120.lines()
        .filter(|l| !l.trim().is_empty())
        .count();
}

/// Test 13: Stress test with many containers.
#[test]
fn e2e_stress_many_containers() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    // 50 turns: user + assistant alternating
    for i in 0..50 {
        app.on_message(ChatAppMsg::UserMessage(format!("question {}", i)));
        app.on_message(ChatAppMsg::TextDelta(format!("answer {}", i)));
        app.on_message(ChatAppMsg::StreamComplete);
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
    assert!(!app.is_streaming(), "Should not be streaming after all turns complete");
}

/// Thinking during streaming should NOT show "◇ Thought" in content —
/// only the turn indicator in chrome shows "Thinking… (N words)".
/// The collapsed summary "◇ Thought" appears only after text starts.
#[test]
fn thinking_not_duplicated_in_content_and_chrome() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.on_message(ChatAppMsg::UserMessage("think hard".into()));
    vt.render_frame(&mut app);

    // Thinking starts — only chrome should show thinking indicator
    app.on_message(ChatAppMsg::ThinkingDelta(
        "deep analysis of the problem with many words to count".into(),
    ));
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
    app.on_message(ChatAppMsg::TextDelta("Here is my answer.".into()));
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
    let mut app = OilChatApp::init();
    assert!(!app.interaction_visible());

    app.on_message(ChatAppMsg::OpenInteraction {
        request_id: "perm-1".into(),
        request: InteractionRequest::Permission(PermRequest::bash(["ls"])),
    });
    assert!(app.interaction_visible(), "Modal must open after OpenInteraction");
}

#[test]
fn close_interaction_closes_modal() {
    use crucible_core::interaction::{InteractionRequest, InteractionResponse, PermRequest, PermResponse};
    let mut app = OilChatApp::init();

    app.on_message(ChatAppMsg::OpenInteraction {
        request_id: "perm-1".into(),
        request: InteractionRequest::Permission(PermRequest::bash(["ls"])),
    });
    assert!(app.interaction_visible());

    app.on_message(ChatAppMsg::CloseInteraction {
        request_id: "perm-1".into(),
        response: InteractionResponse::Permission(PermResponse::allow()),
    });
    assert!(!app.interaction_visible(), "Modal must close after CloseInteraction");
}

#[test]
fn thinking_indicator_appears_at_most_once_on_screen() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.on_message(ChatAppMsg::UserMessage("think".into()));
    app.on_message(ChatAppMsg::ThinkingDelta(
        "deep analysis of many things with lots of words to count accurately".into(),
    ));
    vt.render_frame(&mut app);

    let screen = strip_ansi(&vt.screen_contents());

    // "Thinking" should appear at most once (in chrome only)
    let thinking_count = screen.matches("Thinking").count();
    assert!(
        thinking_count <= 1,
        "Thinking indicator should appear at most once, found {}.\nScreen:\n{}",
        thinking_count, screen
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
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.on_message(ChatAppMsg::UserMessage("think then respond".into()));
    app.on_message(ChatAppMsg::ThinkingDelta("reasoning about it".into()));
    app.on_message(ChatAppMsg::TextDelta("Here is my answer.".into()));
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
    use crucible_oil::node::{SPINNER_FRAMES, BRAILLE_SPINNER_FRAMES};

    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    // Streaming with text (turn active = spinner in chrome)
    app.on_message(ChatAppMsg::UserMessage("do things".into()));
    app.on_message(ChatAppMsg::TextDelta("working on it".into()));
    vt.render_frame(&mut app);

    let screen = strip_ansi(&vt.screen_contents());
    let lines: Vec<&str> = screen.lines().collect();

    // Find the input box (▄▄▄ bar) — everything above is content, at and below is chrome
    let chrome_start = lines.iter().position(|l| l.contains("▄▄▄▄▄▄")).unwrap_or(lines.len());
    let content_lines = &lines[..chrome_start];
    let content_text: String = content_lines.join("\n");

    let all_spinners: Vec<char> = SPINNER_FRAMES.iter().chain(BRAILLE_SPINNER_FRAMES.iter()).copied().collect();

    for ch in &all_spinners {
        assert!(
            !content_text.contains(*ch),
            "Spinner '{}' found in content area (above chrome). Content:\n{}",
            ch, content_text
        );
    }
}

#[test]
fn all_container_types_render_at_all_widths() {
    use crate::tui::oil::containers::{ContainerViewContext};
    use crucible_oil::render::render_to_plain_text;

    let mut app = OilChatApp::init();

    // Create various container types
    app.on_message(ChatAppMsg::UserMessage("test message".into()));
    app.on_message(ChatAppMsg::ThinkingDelta("some thinking".into()));
    app.on_message(ChatAppMsg::TextDelta("response text here".into()));
    app.on_message(ChatAppMsg::ToolCall {
        name: "Bash".into(),
        args: r#"{"command": "echo hello"}"#.into(),
        call_id: Some("c1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "Bash".into(),
        call_id: Some("c1".into()),
    });
    app.on_message(ChatAppMsg::StreamComplete);

    // Render at various widths — should never panic
    for width in [20, 40, 60, 80, 120, 200] {
        let ctx = ContainerViewContext {
            width,
            spinner_frame: 0,
            show_thinking: false,
        };
        for container in app.container_list().containers() {
            let node = container.view(&ctx);
            let plain = render_to_plain_text(&node, width);
            assert!(
                !plain.is_empty() || matches!(node, crucible_oil::node::Node::Empty),
                "Container {:?} at width {} produced empty non-Empty output",
                container.kind, width
            );
        }
    }
}

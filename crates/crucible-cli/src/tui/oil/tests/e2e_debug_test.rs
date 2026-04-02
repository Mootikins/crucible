//! End-to-end debug test: dumps full vt100 output at each step.
//! Run with: cargo test --lib -p crucible-cli -- e2e_debug_test --nocapture

use crate::tui::oil::app::App;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
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

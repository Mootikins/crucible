//! Snapshot tests for ChatApp visual states

use crate::tui::oil::ansi::strip_ansi;
use crate::tui::oil::app::{App, ViewContext};
use crate::tui::oil::chat_app::{ChatAppMsg, ChatMode, OilChatApp};
use crate::tui::oil::focus::FocusContext;
use crate::tui::oil::planning::FramePlanner;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crucible_core::traits::chat::PrecognitionNoteInfo;
use insta::assert_snapshot;

fn render_app(app: &OilChatApp) -> String {
    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    let tree = app.view(&ctx);
    let mut planner = FramePlanner::new(80, 24);
    let snapshot = planner.plan(&tree);
    // Use screen_with_overlays() to get graduated stdout, viewport, and overlay content
    // This shows what the user would see in the terminal including popups/notifications
    strip_ansi(&snapshot.screen_with_overlays(80))
}

fn render_app_raw(app: &OilChatApp) -> String {
    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    let tree = app.view(&ctx);
    let mut planner = FramePlanner::new(80, 24);
    let snapshot = planner.plan(&tree);
    snapshot.viewport_with_overlays(80)
}

fn render_node_with_planner(node: &crate::tui::oil::Node, width: u16, height: u16) -> String {
    let mut planner = FramePlanner::new(width, height);
    let snapshot = planner.plan(node);
    strip_ansi(&snapshot.screen_with_overlays(width as usize))
}

#[test]
fn snapshot_empty_chat_view() {
    let app = OilChatApp::default();
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_notification_overlays_content() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Hello".to_string()));
    app.on_message(ChatAppMsg::TextDelta(
        "Hi there! How can I help you today?".to_string(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);

    app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
    )));
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_user_and_assistant_exchange() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("What is 2+2?".to_string()));
    app.on_message(ChatAppMsg::TextDelta("The answer is ".to_string()));
    app.on_message(ChatAppMsg::TextDelta("4.".to_string()));
    app.on_message(ChatAppMsg::StreamComplete);
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_streaming_in_progress() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Tell me a story".to_string()));
    app.on_message(ChatAppMsg::TextDelta("Once upon a time".to_string()));
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_streaming_with_spinner() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Generate something".to_string()));
    app.on_message(ChatAppMsg::TextDelta("Working on it...".to_string()));

    // Tick to advance spinner
    for _ in 0..3 {
        app.update(crate::tui::oil::event::Event::Tick);
    }

    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_streaming_no_text_yet() {
    // Spinner should appear immediately after sending a message,
    // before any tokens arrive from the daemon
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Do something".to_string()));
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_ordered_list_numbering() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("List things".to_string()));
    // Stream an ordered list with \n\n separators (how LLMs typically send them)
    app.on_message(ChatAppMsg::TextDelta("1. First item\n\n".to_string()));
    app.on_message(ChatAppMsg::TextDelta("2. Second item\n\n".to_string()));
    app.on_message(ChatAppMsg::TextDelta("3. Third item".to_string()));
    app.on_message(ChatAppMsg::StreamComplete);
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_tool_call_pending() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Read a file".to_string()));
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".to_string(),
        args: r#"{"path":"README.md","offset":1,"limit":100}"#.to_string(),
        call_id: None,
    });
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_tool_call_complete() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Read a file".to_string()));
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".to_string(),
        args: r#"{"path":"README.md"}"#.to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "read_file".to_string(),
        delta: "# README\n\nThis is the content.\n".to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".to_string(),
        call_id: None,
    });
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_tool_output_many_lines_shows_count() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Run ls".to_string()));
    app.on_message(ChatAppMsg::ToolCall {
        name: "mcp_bash".to_string(),
        args: r#"{"command":"ls -la"}"#.to_string(),
        call_id: None,
    });
    let output = "file1.txt\nfile2.txt\nfile3.txt\nfile4.txt\nfile5.txt\n\
                  file6.txt\nfile7.txt\nfile8.txt\nfile9.txt\nfile10.txt";
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "mcp_bash".to_string(),
        delta: output.to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "mcp_bash".to_string(),
        call_id: None,
    });
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_read_tool_preserves_closing_bracket() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Read file".to_string()));
    app.on_message(ChatAppMsg::ToolCall {
        name: "mcp_read".to_string(),
        args: r#"{"filePath":"/home/user/test.rs"}"#.to_string(),
        call_id: None,
    });
    let output = "<file>\n00001| fn main() {\n00002|     println!(\"hello\");\n00003| }\n</file>\n\n[Directory Context: /home/user/project]";
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "mcp_read".to_string(),
        delta: output.to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "mcp_read".to_string(),
        call_id: None,
    });
    assert_snapshot!(render_app(&app));
}

/// Issue: Multiple consecutive tool calls should not have gaps between them.
/// They should be rendered tightly grouped.
#[test]
fn snapshot_multiple_tools_no_gaps() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Explore repo".to_string()));

    // First tool call
    app.on_message(ChatAppMsg::ToolCall {
        name: "bash".to_string(),
        args: r#"{"command":"ls -la"}"#.to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "bash".to_string(),
        delta: "file1.txt\nfile2.txt".to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "bash".to_string(),
        call_id: None,
    });

    // Second tool call - should be tight against first
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".to_string(),
        args: r#"{"path":"README.md"}"#.to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "read_file".to_string(),
        delta: "# README".to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".to_string(),
        call_id: None,
    });

    // Third tool call - should be tight against second
    app.on_message(ChatAppMsg::ToolCall {
        name: "bash".to_string(),
        args: r#"{"command":"cat Cargo.toml"}"#.to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "bash".to_string(),
        delta: "[package]\nname = \"test\"".to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "bash".to_string(),
        call_id: None,
    });

    assert_snapshot!(render_app(&app));
}

/// Issue: Text before tool call, then tool, then more text.
/// Tools should not have excessive spacing from surrounding text.
#[test]
fn snapshot_text_tool_text_spacing() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage(
        "Tell me about the repo".to_string(),
    ));

    // Initial assistant text
    app.on_message(ChatAppMsg::TextDelta(
        "Let me explore the repository for you.".to_string(),
    ));

    // Tool call
    app.on_message(ChatAppMsg::ToolCall {
        name: "bash".to_string(),
        args: r#"{"command":"ls"}"#.to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "bash".to_string(),
        delta: "README.md\nCargo.toml".to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "bash".to_string(),
        call_id: None,
    });

    // Text after tool - this is continuation (no bullet)
    app.on_message(ChatAppMsg::TextDelta(
        "The repository contains the standard Rust project files.".to_string(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);

    assert_snapshot!(render_app(&app));
}

/// Text → Tool → Text → Tool → Text: multiple tool interruptions with continuation text.
#[test]
fn snapshot_sequential_tool_calls_with_text() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Analyze the project".to_string()));

    // First text segment
    app.on_message(ChatAppMsg::TextDelta(
        "Let me check the project structure.".to_string(),
    ));

    // First tool call
    app.on_message(ChatAppMsg::ToolCall {
        name: "bash".to_string(),
        args: r#"{"command":"ls src/"}"#.to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "bash".to_string(),
        delta: "main.rs\nlib.rs".to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "bash".to_string(),
        call_id: None,
    });

    // Continuation text after first tool
    app.on_message(ChatAppMsg::TextDelta(
        "Found two source files. Let me read the main one.".to_string(),
    ));

    // Second tool call
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".to_string(),
        args: r#"{"path":"src/main.rs"}"#.to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "read_file".to_string(),
        delta: "fn main() {\n    println!(\"hello\");\n}".to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".to_string(),
        call_id: None,
    });

    // Final continuation text
    app.on_message(ChatAppMsg::TextDelta(
        "The project has a simple main function that prints hello.".to_string(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);

    assert_snapshot!(render_app(&app));
}

/// Bullet (unordered) list rendering.
#[test]
fn snapshot_bullet_list() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("What files?".to_string()));
    app.on_message(ChatAppMsg::TextDelta(
        "Here are the key files:\n\n\
         - `README.md` — project overview\n\
         - `Cargo.toml` — dependencies\n\
         - `src/main.rs` — entry point\n\
         - `src/lib.rs` — library root"
            .to_string(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);
    assert_snapshot!(render_app(&app));
}

/// Nested list: numbered items with bullet sub-items.
#[test]
fn snapshot_nested_list_numbered_with_bullets() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage(
        "Describe the architecture".to_string(),
    ));
    app.on_message(ChatAppMsg::TextDelta(
        "The system has three layers:\n\n\
         1. **Frontend**\n\
         \x20\x20 - React components\n\
         \x20\x20 - State management\n\
         \x20\x20 - Routing\n\n\
         2. **Backend**\n\
         \x20\x20 - REST API\n\
         \x20\x20 - Authentication\n\
         \x20\x20 - Database access\n\n\
         3. **Infrastructure**\n\
         \x20\x20 - Docker containers\n\
         \x20\x20 - CI/CD pipeline"
            .to_string(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);
    assert_snapshot!(render_app(&app));
}

/// Numbered list interrupted by a tool call, then continued.
#[test]
fn snapshot_numbered_list_across_tool_boundary() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Steps to fix the bug".to_string()));

    // First two list items
    app.on_message(ChatAppMsg::TextDelta(
        "Here's the plan:\n\n\
         1. Read the failing test\n\n\
         2. Identify the root cause"
            .to_string(),
    ));

    // Tool call interrupts the list
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".to_string(),
        args: r#"{"path":"tests/regression.rs"}"#.to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "read_file".to_string(),
        delta: "#[test]\nfn test_regression() {\n    assert!(false);\n}".to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".to_string(),
        call_id: None,
    });

    // Continue with remaining list items
    app.on_message(ChatAppMsg::TextDelta(
        "Now I can see the issue. Continuing:\n\n\
         3. Fix the assertion\n\n\
         4. Run the test suite\n\n\
         5. Commit the fix"
            .to_string(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);

    assert_snapshot!(render_app(&app));
}

/// Bullet list interrupted by tool call — checks continuation indentation.
#[test]
fn snapshot_bullet_list_across_tool_boundary() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Check the files".to_string()));

    // Start with bullet list
    app.on_message(ChatAppMsg::TextDelta(
        "Checking these files:\n\n\
         - `main.rs`\n\
         - `lib.rs`"
            .to_string(),
    ));

    // Tool call
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".to_string(),
        args: r#"{"path":"src/main.rs"}"#.to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "read_file".to_string(),
        delta: "fn main() {}".to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".to_string(),
        call_id: None,
    });

    // Continue with analysis after tool
    app.on_message(ChatAppMsg::TextDelta(
        "The main file is minimal. Key observations:\n\n\
         - No error handling\n\
         - No logging setup\n\
         - Missing CLI argument parsing"
            .to_string(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);

    assert_snapshot!(render_app(&app));
}

/// Deeply nested list: bullets inside numbered inside bullets.
#[test]
fn snapshot_deeply_nested_list() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Project structure".to_string()));
    app.on_message(ChatAppMsg::TextDelta(
        "Project overview:\n\n\
         - **Source code**\n\
         \x20\x20 1. `src/main.rs` — entry point\n\
         \x20\x20 2. `src/lib.rs` — library\n\
         \x20\x20 3. `src/utils/` — helpers\n\
         \x20\x20\x20\x20\x20 - `format.rs`\n\
         \x20\x20\x20\x20\x20 - `parse.rs`\n\
         - **Configuration**\n\
         \x20\x20 1. `Cargo.toml`\n\
         \x20\x20 2. `.cargo/config.toml`\n\
         - **Documentation**\n\
         \x20\x20 1. `README.md`\n\
         \x20\x20 2. `CHANGELOG.md`"
            .to_string(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);
    assert_snapshot!(render_app(&app));
}

/// Sequential tool calls mid-stream: text → tool1 (complete) → tool2 (running).
/// Captured during streaming before StreamComplete.
#[test]
fn snapshot_sequential_tools_mid_stream() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Check the project".to_string()));

    // First text
    app.on_message(ChatAppMsg::TextDelta(
        "Let me look at the files.".to_string(),
    ));

    // First tool — complete
    app.on_message(ChatAppMsg::ToolCall {
        name: "bash".to_string(),
        args: r#"{"command":"ls"}"#.to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "bash".to_string(),
        delta: "src/\nCargo.toml".to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "bash".to_string(),
        call_id: None,
    });

    // Continuation text
    app.on_message(ChatAppMsg::TextDelta(
        "Now let me read the main file.".to_string(),
    ));

    // Second tool — still running
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".to_string(),
        args: r#"{"path":"src/main.rs"}"#.to_string(),
        call_id: None,
    });

    // Snapshot mid-stream: second tool is pending
    assert_snapshot!(render_app(&app));
}

/// Sequential tool calls: text → tool1 complete → text → tool2 complete → still streaming.
/// The turn is still active so turn-level spinner should appear.
#[test]
fn snapshot_sequential_tools_all_complete_still_streaming() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Analyze everything".to_string()));

    // First text
    app.on_message(ChatAppMsg::TextDelta("Starting analysis.".to_string()));

    // First tool
    app.on_message(ChatAppMsg::ToolCall {
        name: "bash".to_string(),
        args: r#"{"command":"ls -la"}"#.to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "bash".to_string(),
        delta: "total 42\ndrwxr-xr-x  5 user".to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "bash".to_string(),
        call_id: None,
    });

    // Continuation text
    app.on_message(ChatAppMsg::TextDelta(
        "Found the directory listing. Let me check the config.".to_string(),
    ));

    // Second tool
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".to_string(),
        args: r#"{"path":"Cargo.toml"}"#.to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "read_file".to_string(),
        delta: "[package]\nname = \"myproject\"".to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".to_string(),
        call_id: None,
    });

    // No StreamComplete — still streaming. Turn spinner should show.
    assert_snapshot!(render_app(&app));
}

/// Parallel tool calls: two tools with the same name issued before either gets results.
/// This tests that results are delivered to the correct tool, not mixed up.
#[test]
fn snapshot_parallel_tool_calls_same_name() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Read both files".to_string()));

    app.on_message(ChatAppMsg::TextDelta(
        "I'll read both files in parallel.".to_string(),
    ));

    // Both tool calls arrive before any results (with distinct call_ids)
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".to_string(),
        args: r#"{"path":"README.md"}"#.to_string(),
        call_id: Some("call-readme".to_string()),
    });
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".to_string(),
        args: r#"{"path":"Cargo.toml"}"#.to_string(),
        call_id: Some("call-cargo".to_string()),
    });

    // Results for first tool (README.md) — matched by call_id
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "read_file".to_string(),
        delta: "# My Project".to_string(),
        call_id: Some("call-readme".to_string()),
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".to_string(),
        call_id: Some("call-readme".to_string()),
    });

    // Results for second tool (Cargo.toml) — matched by call_id
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "read_file".to_string(),
        delta: "[package]".to_string(),
        call_id: Some("call-cargo".to_string()),
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".to_string(),
        call_id: Some("call-cargo".to_string()),
    });

    app.on_message(ChatAppMsg::StreamComplete);
    assert_snapshot!(render_app(&app));
}

/// Back-to-back tool calls without text between them — should be grouped.
#[test]
fn snapshot_back_to_back_tools_no_text() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Read all files".to_string()));

    // Initial text
    app.on_message(ChatAppMsg::TextDelta("Let me read everything.".to_string()));

    // Tool 1
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".to_string(),
        args: r#"{"path":"README.md"}"#.to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "read_file".to_string(),
        delta: "# My Project".to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".to_string(),
        call_id: None,
    });

    // Tool 2 — no text between, should group with tool 1
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".to_string(),
        args: r#"{"path":"Cargo.toml"}"#.to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "read_file".to_string(),
        delta: "[package]".to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".to_string(),
        call_id: None,
    });

    // Tool 3 — no text between, should group with tools 1-2
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".to_string(),
        args: r#"{"path":"src/main.rs"}"#.to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "read_file".to_string(),
        delta: "fn main() {}".to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".to_string(),
        call_id: None,
    });

    app.on_message(ChatAppMsg::StreamComplete);
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_error_displayed_as_notification() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Do something".to_string()));
    app.on_message(ChatAppMsg::Error("Connection failed: timeout".to_string()));
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_popup_open() {
    let mut app = OilChatApp::default();

    // Open popup with F1
    app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
        KeyCode::F(1),
        KeyModifiers::NONE,
    )));

    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_popup_with_selection_moved() {
    let mut app = OilChatApp::default();

    // Open popup with F1
    app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
        KeyCode::F(1),
        KeyModifiers::NONE,
    )));

    // Move selection down twice
    app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
        KeyCode::Down,
        KeyModifiers::NONE,
    )));
    app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
        KeyCode::Down,
        KeyModifiers::NONE,
    )));

    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_status_bar_plan_mode() {
    let mut app = OilChatApp::default();
    app.set_mode(ChatMode::Plan);
    app.on_message(ChatAppMsg::ContextUsage {
        used: 5000,
        total: 128000,
    });
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_status_bar_normal_mode() {
    let mut app = OilChatApp::default();
    app.set_mode(ChatMode::Normal);
    app.on_message(ChatAppMsg::ContextUsage {
        used: 64000,
        total: 128000,
    });
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_status_bar_auto_mode() {
    let mut app = OilChatApp::default();
    app.set_mode(ChatMode::Auto);
    app.on_message(ChatAppMsg::ContextUsage {
        used: 100000,
        total: 128000,
    });
    assert_snapshot!(render_app(&app));
}

#[test]
fn status_bar_modes_and_ctx_visible_across_widths() {
    use crate::tui::oil::components::StatusBar;

    for mode in [ChatMode::Normal, ChatMode::Plan, ChatMode::Auto] {
        for width in [80u16, 120u16, 200u16] {
            let bar = StatusBar::new()
                .mode(mode)
                .model("gpt-4o")
                .context(64000, 128000);
            let rendered = render_node_with_planner(&bar.emergency_view(), width, 4);

            let expected_mode = match mode {
                ChatMode::Normal => "NORMAL",
                ChatMode::Plan => "PLAN",
                ChatMode::Auto => "AUTO",
            };

            assert!(
                rendered.contains(expected_mode),
                "mode label {expected_mode} should be visible at width {width}: {rendered:?}"
            );
            assert!(
                rendered.contains("ctx"),
                "ctx suffix should be visible at width {width}: {rendered:?}"
            );
        }
    }
}

#[test]
fn status_bar_layout_regression_full_pipeline() {
    use crate::tui::oil::components::StatusBar;

    let bar = StatusBar::new()
        .mode(ChatMode::Normal)
        .model("gpt-4o")
        .context(64000, 128000);

    let rendered = render_node_with_planner(&bar.emergency_view(), 80, 4);
    let line = rendered.lines().next().unwrap_or("");

    assert!(
        line.starts_with(" NORMAL "),
        "mode label should remain at left edge: {line:?}"
    );
    assert!(
        line.contains("50% ctx"),
        "context suffix should remain visible: {line:?}"
    );
    assert!(
        line.ends_with("50% ctx"),
        "context suffix should be right-aligned: {line:?}"
    );
}

#[test]
fn snapshot_status_bar_layout_regression_width_80() {
    use crate::tui::oil::components::StatusBar;

    let bar = StatusBar::new()
        .mode(ChatMode::Normal)
        .model("gpt-4o")
        .context(64000, 128000);
    assert_snapshot!(render_node_with_planner(&bar.emergency_view(), 80, 4));
}

#[test]
fn snapshot_status_bar_layout_regression_width_120() {
    use crate::tui::oil::components::StatusBar;

    let bar = StatusBar::new()
        .mode(ChatMode::Normal)
        .model("gpt-4o")
        .context(64000, 128000);
    assert_snapshot!(render_node_with_planner(&bar.emergency_view(), 120, 4));
}

#[test]
fn snapshot_status_bar_layout_regression_width_200() {
    use crate::tui::oil::components::StatusBar;

    let bar = StatusBar::new()
        .mode(ChatMode::Normal)
        .model("gpt-4o")
        .context(64000, 128000);
    assert_snapshot!(render_node_with_planner(&bar.emergency_view(), 200, 4));
}

#[test]
fn snapshot_status_bar_normal_mode_width_80() {
    use crate::tui::oil::components::StatusBar;

    let bar = StatusBar::new()
        .mode(ChatMode::Normal)
        .model("gpt-4o")
        .context(64000, 128000);
    assert_snapshot!(render_node_with_planner(&bar.emergency_view(), 80, 4));
}

#[test]
fn snapshot_status_bar_normal_mode_width_120() {
    use crate::tui::oil::components::StatusBar;

    let bar = StatusBar::new()
        .mode(ChatMode::Normal)
        .model("gpt-4o")
        .context(64000, 128000);
    assert_snapshot!(render_node_with_planner(&bar.emergency_view(), 120, 4));
}

#[test]
fn snapshot_status_bar_normal_mode_width_200() {
    use crate::tui::oil::components::StatusBar;

    let bar = StatusBar::new()
        .mode(ChatMode::Normal)
        .model("gpt-4o")
        .context(64000, 128000);
    assert_snapshot!(render_node_with_planner(&bar.emergency_view(), 200, 4));
}

#[test]
fn snapshot_notification_no_content_above() {
    let mut app = OilChatApp::default();
    app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
    )));
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_notification_overlays_streaming_content() {
    use crucible_core::types::{Notification, NotificationKind};

    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Hello".to_string()));
    app.on_message(ChatAppMsg::TextDelta(
        "Hi there! How can I help you today?".to_string(),
    ));

    app.add_notification(Notification::new(
        NotificationKind::Toast,
        "Test notification",
    ));
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_multiple_notifications_stacked() {
    use crucible_core::types::{Notification, NotificationKind};

    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Hello".to_string()));
    app.on_message(ChatAppMsg::TextDelta("Hi there!".to_string()));

    app.add_notification(Notification::new(NotificationKind::Toast, "First toast"));
    app.add_notification(Notification::new(NotificationKind::Toast, "Second toast"));
    app.add_notification(Notification::new(
        NotificationKind::Warning,
        "Context at 85%",
    ));
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_multi_turn_conversation() {
    let mut app = OilChatApp::default();

    // First exchange
    app.on_message(ChatAppMsg::UserMessage("What is Rust?".to_string()));
    app.on_message(ChatAppMsg::TextDelta(
        "Rust is a systems programming language.".to_string(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);

    // Second exchange
    app.on_message(ChatAppMsg::UserMessage(
        "What about memory safety?".to_string(),
    ));
    app.on_message(ChatAppMsg::TextDelta(
        "Rust ensures memory safety through its ownership system.".to_string(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);

    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_system_message() {
    let mut app = OilChatApp::default();

    for c in ":help".chars() {
        app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
            KeyCode::Char(c),
            KeyModifiers::NONE,
        )));
    }
    app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )));

    assert_snapshot!(render_app(&app));
}

mod composer_stability_snapshots;
mod interaction_modal_snapshots;
mod overlay_snapshots;

// =============================================================================
// Thinking Display Snapshot Tests
// =============================================================================

#[test]
fn snapshot_thinking_delta_during_stream() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Solve this puzzle".to_string()));
    app.on_message(ChatAppMsg::ThinkingDelta(
        "Let me think about this step by step...".to_string(),
    ));
    app.on_message(ChatAppMsg::ThinkingDelta(
        "\nFirst, I need to consider the constraints.".to_string(),
    ));
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_thinking_then_text_response() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("What is 2+2?".to_string()));
    app.on_message(ChatAppMsg::ThinkingDelta(
        "Simple arithmetic...".to_string(),
    ));
    app.on_message(ChatAppMsg::TextDelta("The answer is 4.".to_string()));
    app.on_message(ChatAppMsg::StreamComplete);
    assert_snapshot!(render_app(&app));
}

// =============================================================================
// Context Usage Display Snapshot Tests
// =============================================================================

#[test]
fn snapshot_context_usage_in_status_bar() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::ContextUsage {
        used: 1500,
        total: 8192,
    });
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_context_usage_near_limit() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::ContextUsage {
        used: 7800,
        total: 8192,
    });
    app.on_message(ChatAppMsg::UserMessage("Almost full context".to_string()));
    app.on_message(ChatAppMsg::TextDelta(
        "I see the context is nearly full.".to_string(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);
    assert_snapshot!(render_app(&app));
}

// =============================================================================
// Subagent Display Snapshot Tests
// =============================================================================

#[test]
fn snapshot_subagent_spawned() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Research this topic".to_string()));
    app.on_message(ChatAppMsg::TextDelta(
        "I'll spawn some subagents.".to_string(),
    ));
    app.on_message(ChatAppMsg::SubagentSpawned {
        id: "sub-1".to_string(),
        prompt: "Search for Rust memory safety patterns".to_string(),
    });
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_subagent_completed() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Research this".to_string()));
    app.on_message(ChatAppMsg::TextDelta(
        "Spawning research agents...".to_string(),
    ));
    app.on_message(ChatAppMsg::SubagentSpawned {
        id: "sub-1".to_string(),
        prompt: "Search for patterns".to_string(),
    });
    app.on_message(ChatAppMsg::SubagentCompleted {
        id: "sub-1".to_string(),
        summary: "Found 5 relevant patterns in the codebase".to_string(),
    });
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_subagent_failed() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Do something complex".to_string()));
    app.on_message(ChatAppMsg::SubagentSpawned {
        id: "sub-err".to_string(),
        prompt: "Analyze remote API".to_string(),
    });
    app.on_message(ChatAppMsg::SubagentFailed {
        id: "sub-err".to_string(),
        error: "Connection timeout after 30s".to_string(),
    });
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_multiple_subagents_parallel() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Broad research".to_string()));
    app.on_message(ChatAppMsg::TextDelta(
        "Launching parallel research...".to_string(),
    ));
    app.on_message(ChatAppMsg::SubagentSpawned {
        id: "sub-a".to_string(),
        prompt: "Search database layer".to_string(),
    });
    app.on_message(ChatAppMsg::SubagentSpawned {
        id: "sub-b".to_string(),
        prompt: "Search API layer".to_string(),
    });
    app.on_message(ChatAppMsg::SubagentCompleted {
        id: "sub-a".to_string(),
        summary: "Found 3 database modules".to_string(),
    });
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_delegation_spawned_with_target() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage(
        "Research auth patterns".to_string(),
    ));
    app.on_message(ChatAppMsg::TextDelta(
        "I'll delegate this to a specialized agent.".to_string(),
    ));
    app.on_message(ChatAppMsg::DelegationSpawned {
        id: "deleg-1".to_string(),
        prompt: "Research authentication patterns in Rust".to_string(),
        target_agent: Some("cursor".to_string()),
    });
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_delegation_spawned_without_target() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Do something".to_string()));
    app.on_message(ChatAppMsg::TextDelta(
        "Delegating to same agent...".to_string(),
    ));
    app.on_message(ChatAppMsg::DelegationSpawned {
        id: "deleg-2".to_string(),
        prompt: "Analyze the code".to_string(),
        target_agent: None,
    });
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_delegation_completed() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Research patterns".to_string()));
    app.on_message(ChatAppMsg::DelegationSpawned {
        id: "deleg-3".to_string(),
        prompt: "Find security patterns".to_string(),
        target_agent: Some("opencode".to_string()),
    });
    app.on_message(ChatAppMsg::DelegationCompleted {
        id: "deleg-3".to_string(),
        summary: "Found 3 security patterns in the codebase".to_string(),
    });
    assert_snapshot!(render_app(&app));
}

// =============================================================================
// Precognition Result Snapshot Tests
// =============================================================================

#[test]
fn snapshot_precognition_with_results() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::PrecognitionResult {
        notes_count: 5,
        notes: vec![
            PrecognitionNoteInfo {
                title: "Authentication Guide".to_string(),
                kiln_label: None,
            },
            PrecognitionNoteInfo {
                title: "OAuth2 Patterns".to_string(),
                kiln_label: None,
            },
            PrecognitionNoteInfo {
                title: "Security Best Practices".to_string(),
                kiln_label: Some("docs".to_string()),
            },
            PrecognitionNoteInfo {
                title: "Token Management".to_string(),
                kiln_label: None,
            },
            PrecognitionNoteInfo {
                title: "API Keys Reference".to_string(),
                kiln_label: Some("reference".to_string()),
            },
        ],
    });
    app.on_message(ChatAppMsg::UserMessage("Tell me about auth".to_string()));
    app.on_message(ChatAppMsg::TextDelta(
        "Based on your notes about authentication...".to_string(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);
    assert_snapshot!(render_app(&app));
}

// =============================================================================
// Multi-turn with Tools Snapshot Tests
// =============================================================================

#[test]
fn snapshot_multi_turn_with_tool_calls() {
    let mut app = OilChatApp::default();

    // Turn 1: user asks, agent uses tool
    app.on_message(ChatAppMsg::UserMessage("Read my config file".to_string()));
    app.on_message(ChatAppMsg::TextDelta(
        "Let me read that for you.".to_string(),
    ));
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".to_string(),
        args: r#"{"path":"config.toml"}"#.to_string(),
        call_id: Some("call-1".to_string()),
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "read_file".to_string(),
        delta: "[kiln]\npath = \"~/notes\"".to_string(),
        call_id: Some("call-1".to_string()),
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".to_string(),
        call_id: Some("call-1".to_string()),
    });
    app.on_message(ChatAppMsg::TextDelta(
        "\nYour config sets the kiln path to ~/notes.".to_string(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);

    // Turn 2: follow-up question
    app.on_message(ChatAppMsg::UserMessage("Change it to ~/vault".to_string()));
    app.on_message(ChatAppMsg::TextDelta("I'll update that now.".to_string()));
    app.on_message(ChatAppMsg::ToolCall {
        name: "edit_file".to_string(),
        args: r#"{"path":"config.toml","content":"[kiln]\npath = \"~/vault\""}"#.to_string(),
        call_id: Some("call-2".to_string()),
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "edit_file".to_string(),
        delta: "File updated successfully".to_string(),
        call_id: Some("call-2".to_string()),
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "edit_file".to_string(),
        call_id: Some("call-2".to_string()),
    });
    app.on_message(ChatAppMsg::TextDelta("\nDone! Config updated.".to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    assert_snapshot!(render_app(&app));
}

// =============================================================================
// Error During Stream Snapshot Tests
// =============================================================================

#[test]
fn snapshot_error_interrupts_streaming() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Tell me a story".to_string()));
    app.on_message(ChatAppMsg::TextDelta(
        "Once upon a time, there was a ".to_string(),
    ));
    app.on_message(ChatAppMsg::Error(
        "Connection lost: daemon unreachable".to_string(),
    ));
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_stream_cancelled_by_user() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Write a long essay".to_string()));
    app.on_message(ChatAppMsg::TextDelta(
        "Here is my comprehensive analysis of the topic...".to_string(),
    ));
    app.on_message(ChatAppMsg::StreamCancelled);
    assert_snapshot!(render_app(&app));
}

// =============================================================================
// Raw ANSI Snapshot Baselines
// =============================================================================

#[test]
fn snapshot_raw_empty_chat_view() {
    let app = OilChatApp::default();
    assert_snapshot!(render_app_raw(&app));
}

#[test]
fn snapshot_raw_user_and_assistant_exchange() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("What is 2+2?".to_string()));
    app.on_message(ChatAppMsg::TextDelta("The answer is ".to_string()));
    app.on_message(ChatAppMsg::TextDelta("4.".to_string()));
    app.on_message(ChatAppMsg::StreamComplete);
    assert_snapshot!(render_app_raw(&app));
}

#[test]
fn snapshot_raw_streaming_in_progress() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Tell me a story".to_string()));
    app.on_message(ChatAppMsg::TextDelta("Once upon a time".to_string()));
    assert_snapshot!(render_app_raw(&app));
}

#[test]
fn snapshot_raw_tool_call_pending() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Read a file".to_string()));
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".to_string(),
        args: r#"{"path":"README.md","offset":1,"limit":100}"#.to_string(),
        call_id: None,
    });
    assert_snapshot!(render_app_raw(&app));
}

#[test]
fn snapshot_raw_tool_call_complete() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Read a file".to_string()));
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".to_string(),
        args: r#"{"path":"README.md"}"#.to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "read_file".to_string(),
        delta: "# README\n\nThis is the content.\n".to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".to_string(),
        call_id: None,
    });
    assert_snapshot!(render_app_raw(&app));
}

#[test]
fn snapshot_raw_popup_open() {
    let mut app = OilChatApp::default();
    app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
        KeyCode::F(1),
        KeyModifiers::NONE,
    )));
    assert_snapshot!(render_app_raw(&app));
}

#[test]
fn snapshot_raw_status_bar_plan_mode() {
    let mut app = OilChatApp::default();
    app.set_mode(ChatMode::Plan);
    app.on_message(ChatAppMsg::ContextUsage {
        used: 5000,
        total: 128000,
    });
    assert_snapshot!(render_app_raw(&app));
}

#[test]
fn snapshot_raw_status_bar_auto_mode() {
    let mut app = OilChatApp::default();
    app.set_mode(ChatMode::Auto);
    app.on_message(ChatAppMsg::ContextUsage {
        used: 100000,
        total: 128000,
    });
    assert_snapshot!(render_app_raw(&app));
}

#[test]
fn snapshot_raw_error_displayed_as_notification() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Do something".to_string()));
    app.on_message(ChatAppMsg::Error("Connection failed: timeout".to_string()));
    assert_snapshot!(render_app_raw(&app));
}

#[test]
fn snapshot_raw_notification_no_content_above() {
    let mut app = OilChatApp::default();
    app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
    )));
    let plain = crucible_oil::ansi::strip_ansi(&render_app_raw(&app));
    let statusline = plain.lines().last().expect("statusline should exist");
    let notification_col = statusline
        .find("Ctrl+C")
        .expect("notification text should be present");
    assert!(
        notification_col >= 30,
        "Ctrl+C notification should be right-aligned (col >= 30), but starts at col {notification_col}. \
         Statusline: {statusline:?}"
    );
    assert_snapshot!(render_app_raw(&app));
}

#[test]
fn snapshot_raw_thinking_delta_during_stream() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Solve this puzzle".to_string()));
    app.on_message(ChatAppMsg::ThinkingDelta(
        "Let me think about this step by step...".to_string(),
    ));
    app.on_message(ChatAppMsg::ThinkingDelta(
        "\nFirst, I need to consider the constraints.".to_string(),
    ));
    assert_snapshot!(render_app_raw(&app));
}

#[test]
fn snapshot_raw_perm_modal_bash_command() {
    let mut app = OilChatApp::default();
    let request = crucible_core::interaction::InteractionRequest::Permission(
        crucible_core::interaction::PermRequest::bash(["npm", "install", "lodash"]),
    );
    app.open_interaction("perm-bash".to_string(), request);
    assert_snapshot!(render_app_raw(&app));
}

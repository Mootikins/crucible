//! Snapshot tests for ChatApp visual states

use crate::tui::oil::ansi::strip_ansi;
use crate::tui::oil::app::{App, ViewContext};
use crate::tui::oil::chat_app::{ChatAppMsg, ChatMode, OilChatApp};
use crate::tui::oil::focus::FocusContext;
use crate::tui::oil::planning::FramePlanner;
use crate::tui::oil::render::render_to_string;
use crate::tui::oil::test_harness::AppHarness;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
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
    app.on_message(ChatAppMsg::ToolResultComplete { name: "read_file".to_string(),
                call_id: None });
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
    app.on_message(ChatAppMsg::ToolResultComplete { name: "mcp_bash".to_string(),
                call_id: None });
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
    app.on_message(ChatAppMsg::ToolResultComplete { name: "mcp_read".to_string(),
                call_id: None });
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
    app.on_message(ChatAppMsg::ToolResultComplete { name: "bash".to_string(),
                call_id: None });

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
    app.on_message(ChatAppMsg::ToolResultComplete { name: "read_file".to_string(),
                call_id: None });

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
    app.on_message(ChatAppMsg::ToolResultComplete { name: "bash".to_string(),
                call_id: None });

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
    app.on_message(ChatAppMsg::ToolResultComplete { name: "bash".to_string(),
                call_id: None });

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
    app.on_message(ChatAppMsg::UserMessage(
        "Analyze the project".to_string(),
    ));

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
    app.on_message(ChatAppMsg::ToolResultComplete { name: "bash".to_string(),
                call_id: None });

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
    app.on_message(ChatAppMsg::ToolResultComplete { name: "read_file".to_string(),
                call_id: None });

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
    app.on_message(ChatAppMsg::UserMessage("Describe the architecture".to_string()));
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
    app.on_message(ChatAppMsg::UserMessage(
        "Steps to fix the bug".to_string(),
    ));

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
    app.on_message(ChatAppMsg::ToolResultComplete { name: "read_file".to_string(),
                call_id: None });

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
    app.on_message(ChatAppMsg::ToolResultComplete { name: "read_file".to_string(),
                call_id: None });

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
    app.on_message(ChatAppMsg::UserMessage(
        "Check the project".to_string(),
    ));

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
    app.on_message(ChatAppMsg::ToolResultComplete { name: "bash".to_string(),
                call_id: None });

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
    app.on_message(ChatAppMsg::UserMessage(
        "Analyze everything".to_string(),
    ));

    // First text
    app.on_message(ChatAppMsg::TextDelta(
        "Starting analysis.".to_string(),
    ));

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
    app.on_message(ChatAppMsg::ToolResultComplete { name: "bash".to_string(),
                call_id: None });

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
    app.on_message(ChatAppMsg::ToolResultComplete { name: "read_file".to_string(),
                call_id: None });

    // No StreamComplete — still streaming. Turn spinner should show.
    assert_snapshot!(render_app(&app));
}

/// Parallel tool calls: two tools with the same name issued before either gets results.
/// This tests that results are delivered to the correct tool, not mixed up.
#[test]
fn snapshot_parallel_tool_calls_same_name() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage(
        "Read both files".to_string(),
    ));

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
    app.on_message(ChatAppMsg::UserMessage(
        "Read all files".to_string(),
    ));

    // Initial text
    app.on_message(ChatAppMsg::TextDelta(
        "Let me read everything.".to_string(),
    ));

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
    app.on_message(ChatAppMsg::ToolResultComplete { name: "read_file".to_string(),
                call_id: None });

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
    app.on_message(ChatAppMsg::ToolResultComplete { name: "read_file".to_string(),
                call_id: None });

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
    app.on_message(ChatAppMsg::ToolResultComplete { name: "read_file".to_string(),
                call_id: None });

    app.on_message(ChatAppMsg::StreamComplete);
    assert_snapshot!(render_app(&app));
}

#[test]
fn snapshot_error_displayed() {
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

    // Trigger help command which adds system message
    for c in "/help".chars() {
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

mod composer_stability_snapshots {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn snapshot_popup_hidden_baseline() {
        let mut app = OilChatApp::default();
        app.set_workspace_files(vec![
            "src/main.rs".to_string(),
            "src/lib.rs".to_string(),
            "Cargo.toml".to_string(),
        ]);
        assert_snapshot!(render_app(&app));
    }

    #[test]
    fn snapshot_popup_visible_same_height() {
        let mut app = OilChatApp::default();
        app.set_workspace_files(vec![
            "src/main.rs".to_string(),
            "src/lib.rs".to_string(),
            "Cargo.toml".to_string(),
        ]);

        app.update(crate::tui::oil::event::Event::Key(key(KeyCode::Char('@'))));

        assert_snapshot!(render_app(&app));
    }

    #[test]
    fn snapshot_input_empty() {
        let app = OilChatApp::default();
        assert_snapshot!(render_app(&app));
    }

    #[test]
    fn snapshot_input_short_text() {
        let mut app = OilChatApp::default();
        for c in "Hello".chars() {
            app.update(crate::tui::oil::event::Event::Key(key(KeyCode::Char(c))));
        }
        assert_snapshot!(render_app(&app));
    }

    #[test]
    fn snapshot_input_long_text_clamped() {
        let mut app = OilChatApp::default();
        let long_text = "x".repeat(300);
        for c in long_text.chars() {
            app.update(crate::tui::oil::event::Event::Key(key(KeyCode::Char(c))));
        }
        assert_snapshot!(render_app(&app));
    }

    #[test]
    fn verify_input_height_grows_with_content() {
        use crate::tui::oil::chat_app::INPUT_MAX_CONTENT_LINES;

        let app_empty = OilChatApp::default();

        let mut app_long = OilChatApp::default();
        for c in "x".repeat(300).chars() {
            app_long.update(crate::tui::oil::event::Event::Key(key(KeyCode::Char(c))));
        }

        let empty_output = render_app(&app_empty);
        let long_output = render_app(&app_long);

        let empty_lines = empty_output.lines().count();
        let long_lines = long_output.lines().count();

        assert!(
            empty_lines < long_lines,
            "Long input ({} lines) should have more lines than empty ({} lines)",
            long_lines,
            empty_lines
        );

        let max_input_height = INPUT_MAX_CONTENT_LINES + 2;
        assert!(
            long_lines <= empty_lines + max_input_height,
            "Long input growth should be bounded by max content lines"
        );
    }
}

// =============================================================================
// Interaction Modal Snapshot Tests
// =============================================================================

mod interaction_modal_snapshots {
    use super::*;
    use crucible_core::interaction::{AskRequest, InteractionRequest, PermRequest};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// Snapshot test: AskRequest with 3 choices, first selected (default)
    #[test]
    fn snapshot_ask_modal_with_choices_first_selected() {
        let mut app = OilChatApp::default();
        let request = InteractionRequest::Ask(
            AskRequest::new("Which programming language do you prefer?").choices([
                "Rust",
                "Python",
                "TypeScript",
            ]),
        );
        app.open_interaction("ask-1".to_string(), request);

        assert_snapshot!(render_app(&app));
    }

    /// Snapshot test: AskRequest with 3 choices, second selected
    #[test]
    fn snapshot_ask_modal_with_choices_second_selected() {
        let mut app = OilChatApp::default();
        let request = InteractionRequest::Ask(
            AskRequest::new("Which programming language do you prefer?").choices([
                "Rust",
                "Python",
                "TypeScript",
            ]),
        );
        app.open_interaction("ask-2".to_string(), request);

        // Navigate down to select second option
        app.update(crate::tui::oil::event::Event::Key(key(KeyCode::Down)));

        assert_snapshot!(render_app(&app));
    }

    /// Snapshot test: AskRequest with allow_other showing "Other..." option
    #[test]
    fn snapshot_ask_modal_with_allow_other() {
        let mut app = OilChatApp::default();
        let request = InteractionRequest::Ask(
            AskRequest::new("Select your favorite or enter custom:")
                .choices(["Option A", "Option B"])
                .allow_other(),
        );
        app.open_interaction("ask-3".to_string(), request);

        assert_snapshot!(render_app(&app));
    }

    /// Snapshot test: AskRequest free-text only (no choices)
    #[test]
    fn snapshot_ask_modal_free_text_only() {
        let mut app = OilChatApp::default();
        let request =
            InteractionRequest::Ask(AskRequest::new("Enter your custom value:").allow_other());
        app.open_interaction("ask-4".to_string(), request);

        assert_snapshot!(render_app(&app));
    }

    /// Snapshot test: PermRequest for bash command
    #[test]
    fn snapshot_perm_modal_bash_command() {
        let mut app = OilChatApp::default();
        let request =
            InteractionRequest::Permission(PermRequest::bash(["npm", "install", "lodash"]));
        app.open_interaction("perm-bash".to_string(), request);

        assert_snapshot!(render_app(&app));
    }

    /// Snapshot test: PermRequest for file write
    #[test]
    fn snapshot_perm_modal_file_write() {
        let mut app = OilChatApp::default();
        let request = InteractionRequest::Permission(PermRequest::write([
            "home", "user", "project", "src", "main.rs",
        ]));
        app.open_interaction("perm-write".to_string(), request);

        assert_snapshot!(render_app(&app));
    }

    /// Snapshot test: PermRequest for file read
    #[test]
    fn snapshot_perm_modal_file_read() {
        let mut app = OilChatApp::default();
        let request = InteractionRequest::Permission(PermRequest::read(["etc", "hosts"]));
        app.open_interaction("perm-read".to_string(), request);

        assert_snapshot!(render_app(&app));
    }

    /// Snapshot test: PermRequest for tool execution
    #[test]
    fn snapshot_perm_modal_tool() {
        let mut app = OilChatApp::default();
        let request = InteractionRequest::Permission(PermRequest::tool(
            "semantic_search",
            serde_json::json!({"query": "rust memory safety", "limit": 10}),
        ));
        app.open_interaction("perm-tool".to_string(), request);

        assert_snapshot!(render_app(&app));
    }

    /// Snapshot test: AskRequest with many choices (scrolling)
    #[test]
    fn snapshot_ask_modal_many_choices() {
        let mut app = OilChatApp::default();
        let request = InteractionRequest::Ask(AskRequest::new("Select an option:").choices([
            "First option",
            "Second option",
            "Third option",
            "Fourth option",
            "Fifth option",
            "Sixth option",
            "Seventh option",
            "Eighth option",
        ]));
        app.open_interaction("ask-many".to_string(), request);

        assert_snapshot!(render_app(&app));
    }

    /// Snapshot test: AskRequest after navigating to last choice
    #[test]
    fn snapshot_ask_modal_last_selected() {
        let mut app = OilChatApp::default();
        let request = InteractionRequest::Ask(
            AskRequest::new("Pick one:").choices(["Alpha", "Beta", "Gamma", "Delta"]),
        );
        app.open_interaction("ask-last".to_string(), request);

        // Navigate to last option
        for _ in 0..3 {
            app.update(crate::tui::oil::event::Event::Key(key(KeyCode::Down)));
        }

        assert_snapshot!(render_app(&app));
    }

    // =========================================================================
    // Multi-select Mode Tests
    // =========================================================================

    #[test]
    fn snapshot_ask_modal_multi_select() {
        let mut app = OilChatApp::default();
        let request = InteractionRequest::Ask(
            AskRequest::new("Select all languages you know:")
                .choices(["Rust", "Python", "Go", "TypeScript"])
                .multi_select(),
        );
        app.open_interaction("ask-multi".to_string(), request);

        assert_snapshot!(render_app(&app));
    }

    #[test]
    fn snapshot_ask_modal_multi_select_with_selection() {
        let mut app = OilChatApp::default();
        let request = InteractionRequest::Ask(
            AskRequest::new("Select frameworks:")
                .choices(["React", "Vue", "Angular", "Svelte"])
                .multi_select(),
        );
        app.open_interaction("ask-multi-sel".to_string(), request);

        // Toggle first item with Space
        app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
            KeyCode::Char(' '),
            KeyModifiers::NONE,
        )));
        // Move down and toggle second
        app.update(crate::tui::oil::event::Event::Key(key(KeyCode::Down)));
        app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
            KeyCode::Char(' '),
            KeyModifiers::NONE,
        )));

        assert_snapshot!(render_app(&app));
    }

    // =========================================================================
    // Completion Flow Tests
    // =========================================================================

    #[test]
    fn snapshot_ask_modal_after_escape() {
        let mut app = OilChatApp::default();
        let request = InteractionRequest::Ask(AskRequest::new("Choose:").choices(["Yes", "No"]));
        app.open_interaction("ask-esc".to_string(), request);

        // Verify modal is visible
        assert!(app.interaction_visible());

        // Press Escape to cancel
        app.update(crate::tui::oil::event::Event::Key(key(KeyCode::Esc)));

        // Modal should be closed - snapshot shows regular view
        assert_snapshot!(render_app(&app));
    }

    #[test]
    fn snapshot_ask_modal_after_ctrl_c() {
        let mut app = OilChatApp::default();
        let request = InteractionRequest::Ask(AskRequest::new("Choose:").choices(["Yes", "No"]));
        app.open_interaction("ask-ctrl-c".to_string(), request);

        // Verify modal is visible
        assert!(app.interaction_visible());

        // Press Ctrl+C to cancel
        app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        )));

        // Modal should be closed - snapshot shows regular view
        assert_snapshot!(render_app(&app));
    }

    #[test]
    fn snapshot_perm_modal_after_allow() {
        let mut app = OilChatApp::default();
        let request = InteractionRequest::Permission(PermRequest::bash(["ls"]));
        app.open_interaction("perm-allow".to_string(), request);

        // Press 'y' to allow
        app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
            KeyCode::Char('y'),
            KeyModifiers::NONE,
        )));

        // Modal should be closed
        assert_snapshot!(render_app(&app));
    }

    #[test]
    fn snapshot_perm_modal_after_deny() {
        let mut app = OilChatApp::default();
        let request = InteractionRequest::Permission(PermRequest::bash(["rm", "-rf", "/"]));
        app.open_interaction("perm-deny".to_string(), request);

        // Press 'n' to deny
        app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
            KeyCode::Char('n'),
            KeyModifiers::NONE,
        )));

        // Modal should be closed
        assert_snapshot!(render_app(&app));
    }

    // =========================================================================
    // Edge Case Tests
    // =========================================================================

    #[test]
    fn snapshot_ask_modal_long_question_text() {
        let mut app = OilChatApp::default();
        let long_question = "This is a very long question that should test how the modal handles text overflow. It contains multiple sentences to ensure we're testing a realistic scenario where the agent asks a detailed question that might wrap across multiple lines in the terminal.";
        let request = InteractionRequest::Ask(
            AskRequest::new(long_question).choices(["Accept", "Reject", "Skip"]),
        );
        app.open_interaction("ask-long".to_string(), request);

        assert_snapshot!(render_app(&app));
    }

    #[test]
    fn snapshot_ask_modal_long_choice_text() {
        let mut app = OilChatApp::default();
        let request = InteractionRequest::Ask(
            AskRequest::new("Select option:").choices([
                "Short",
                "This is a much longer choice that might need to be truncated or wrapped depending on the terminal width",
                "Medium length option here",
            ]),
        );
        app.open_interaction("ask-long-choice".to_string(), request);

        assert_snapshot!(render_app(&app));
    }

    #[test]
    fn snapshot_ask_modal_unicode_content() {
        let mut app = OilChatApp::default();
        let request = InteractionRequest::Ask(
            AskRequest::new("Select your preferred emoji reaction:").choices([
                "👍 Thumbs up",
                "❤️ Heart",
                "🎉 Party",
                "🚀 Rocket",
                "🤔 Thinking",
            ]),
        );
        app.open_interaction("ask-unicode".to_string(), request);

        assert_snapshot!(render_app(&app));
    }

    #[test]
    fn snapshot_perm_modal_long_command() {
        let mut app = OilChatApp::default();
        let request = InteractionRequest::Permission(PermRequest::bash([
            "docker",
            "run",
            "--rm",
            "-it",
            "-v",
            "/home/user/project:/app",
            "-e",
            "DATABASE_URL=postgres://localhost/db",
            "-p",
            "8080:8080",
            "myimage:latest",
        ]));
        app.open_interaction("perm-long-cmd".to_string(), request);

        assert_snapshot!(render_app(&app));
    }

    #[test]
    fn snapshot_perm_modal_deeply_nested_path() {
        let mut app = OilChatApp::default();
        let request = InteractionRequest::Permission(PermRequest::write([
            "home",
            "user",
            "projects",
            "company",
            "team",
            "repository",
            "packages",
            "core",
            "src",
            "components",
            "Button.tsx",
        ]));
        app.open_interaction("perm-deep-path".to_string(), request);

        assert_snapshot!(render_app(&app));
    }
}

// =============================================================================
// Overlay System Snapshot Tests (Golden Reference Scenarios)
// =============================================================================

mod overlay_snapshots {
    use super::*;
    use crucible_core::types::Notification;

    /// Scenario 4: :messages drawer with notification history
    #[test]
    fn snapshot_messages_drawer_with_history() {
        let mut app = OilChatApp::default();

        app.add_notification(Notification::toast("Session saved"));
        app.add_notification(Notification::toast("Thinking display: on"));
        app.add_notification(Notification::progress(45, 100, "Indexing files"));
        app.add_notification(Notification::warning("Context at 85%"));

        app.show_messages();

        assert_snapshot!(render_app(&app));
    }

    /// :messages command opens drawer during streaming
    #[test]
    fn snapshot_messages_drawer_during_streaming() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("Hello".to_string()));
        app.on_message(ChatAppMsg::TextDelta("Hi there!".to_string()));

        app.add_notification(Notification::toast("Session saved"));
        app.add_notification(Notification::warning("Context at 85%"));

        app.show_messages();
        assert_snapshot!(render_app(&app));
    }

    /// Scenario 7: Recent warnings show as toast; counts show after expiry
    #[test]
    fn snapshot_statusline_warning_counts() {
        use crate::tui::oil::component::Component;
        use crate::tui::oil::components::status_bar::NotificationToastKind;
        use crate::tui::oil::components::StatusBar;

        let bar = StatusBar::new()
            .mode(crate::tui::oil::chat_app::ChatMode::Normal)
            .model("gpt-4o")
            .counts(vec![
                (NotificationToastKind::Warning, 3),
                (NotificationToastKind::Error, 1),
            ]);
        let focus = crate::tui::oil::focus::FocusContext::default();
        let ctx = crate::tui::oil::ViewContext::new(&focus);
        let node = bar.view(&ctx);
        let output = crate::tui::oil::render::render_to_plain_text(&node, 80);
        assert_snapshot!(output);
    }

    /// Scenario 1: Simple info toast on statusline (drawer closed)
    #[test]
    fn snapshot_statusline_info_toast() {
        let mut app = OilChatApp::default();
        app.add_notification(Notification::toast("Session saved"));
        app.hide_messages();
        assert_snapshot!(render_app(&app));
    }

    /// Scenario 3: Warning toast on statusline (drawer closed)
    #[test]
    fn snapshot_statusline_warning_toast() {
        let mut app = OilChatApp::default();
        app.add_notification(Notification::warning("Context at 85%"));
        app.hide_messages();
        assert_snapshot!(render_app(&app));
    }

    /// Drawer with conversation content above
    #[test]
    fn snapshot_messages_drawer_with_conversation() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("Hello".to_string()));
        app.on_message(ChatAppMsg::TextDelta("Hi there!".to_string()));
        app.on_message(ChatAppMsg::StreamComplete);

        app.add_notification(Notification::toast("Session saved"));
        app.add_notification(Notification::warning("Context at 85%"));

        app.show_messages();
        assert_snapshot!(render_app(&app));
    }

    /// Empty drawer (no notifications)
    #[test]
    fn snapshot_messages_drawer_empty() {
        let mut app = OilChatApp::default();
        app.show_messages();
        assert_snapshot!(render_app(&app));
    }

    #[test]
    fn snapshot_raw_drawer_with_history() {
        let mut app = OilChatApp::default();
        app.add_notification(Notification::toast("Session saved"));
        app.add_notification(Notification::toast("Thinking display: on"));
        app.add_notification(Notification::progress(45, 100, "Indexing files"));
        app.add_notification(Notification::warning("Context at 85%"));
        app.show_messages();
        assert_snapshot!(render_app_raw(&app));
    }

    #[test]
    fn snapshot_raw_statusline_info_toast() {
        let mut app = OilChatApp::default();
        app.add_notification(Notification::toast("Session saved"));
        app.hide_messages();
        assert_snapshot!(render_app_raw(&app));
    }

    #[test]
    fn snapshot_raw_statusline_warning_toast() {
        let mut app = OilChatApp::default();
        app.add_notification(Notification::warning("Context at 85%"));
        app.hide_messages();
        assert_snapshot!(render_app_raw(&app));
    }

    #[test]
    fn snapshot_raw_statusline_warning_counts() {
        use crate::tui::oil::component::Component;
        use crate::tui::oil::components::status_bar::NotificationToastKind;
        use crate::tui::oil::components::StatusBar;

        let bar = StatusBar::new()
            .mode(crate::tui::oil::chat_app::ChatMode::Normal)
            .model("gpt-4o")
            .counts(vec![
                (NotificationToastKind::Warning, 3),
                (NotificationToastKind::Error, 1),
            ]);
        let focus = crate::tui::oil::focus::FocusContext::default();
        let ctx = crate::tui::oil::ViewContext::new(&focus);
        let node = bar.view(&ctx);
        let output = crate::tui::oil::render::render_to_string(&node, 80);
        assert_snapshot!(output);
    }

    #[test]
    fn snapshot_raw_perm_bash() {
        let mut app = OilChatApp::default();
        let request = crucible_core::interaction::InteractionRequest::Permission(
            crucible_core::interaction::PermRequest::bash(["npm", "install", "lodash"]),
        );
        app.open_interaction("perm-bash".to_string(), request);
        assert_snapshot!(render_app_raw(&app));
    }
}

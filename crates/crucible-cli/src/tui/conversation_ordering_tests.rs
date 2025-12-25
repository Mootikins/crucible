//! Tests for conversation message ordering
//!
//! These tests verify that messages in the conversation view maintain
//! their chronological order regardless of message type.
//!
//! BUG UNDER TEST: Messages currently appear sorted by type instead of
//! chronological order - prose is grouped together, tool calls are grouped
//! separately, breaking conversational flow.

use super::conversation::{
    render_item_to_lines, ConversationItem, ConversationState, ConversationWidget, StatusKind,
    ToolCallDisplay, ToolStatus,
};
use super::content_block::ContentBlock;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

/// Helper to render conversation to a string for snapshot testing
fn render_conversation_to_string(state: &ConversationState, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|f| {
            let widget = ConversationWidget::new(state);
            f.render_widget(widget, f.area());
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let mut output = String::new();

    for y in 0..buffer.area().height {
        let line: String = (0..buffer.area().width)
            .map(|x| buffer.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "))
            .collect();
        // Trim trailing whitespace for cleaner snapshots
        output.push_str(line.trim_end());
        output.push('\n');
    }

    output
}

/// Helper to get rendered text order (simplified, just item types in order)
fn get_rendered_item_types(state: &ConversationState) -> Vec<&'static str> {
    state
        .items()
        .iter()
        .map(|item| match item {
            ConversationItem::UserMessage { .. } => "user",
            ConversationItem::AssistantMessage { .. } => "assistant",
            ConversationItem::Status(_) => "status",
            ConversationItem::ToolCall(_) => "tool",
        })
        .collect()
}

// =============================================================================
// Order Preservation Tests
// =============================================================================

#[test]
fn test_simple_user_assistant_order() {
    // Basic sanity check: user then assistant
    let mut state = ConversationState::new();
    state.push_user_message("Hello");
    state.push_assistant_message("Hi there!");

    let types = get_rendered_item_types(&state);
    assert_eq!(types, vec!["user", "assistant"]);
}

#[test]
fn test_interleaved_turns_preserve_order() {
    // Multiple turns should maintain order
    let mut state = ConversationState::new();
    state.push_user_message("Turn 1");
    state.push_assistant_message("Response 1");
    state.push_user_message("Turn 2");
    state.push_assistant_message("Response 2");
    state.push_user_message("Turn 3");
    state.push_assistant_message("Response 3");

    let types = get_rendered_item_types(&state);
    assert_eq!(
        types,
        vec![
            "user",
            "assistant",
            "user",
            "assistant",
            "user",
            "assistant"
        ]
    );
}

/// This test demonstrates the BUG.
///
/// In a real conversation, an assistant might:
/// 1. Say "Let me search for that"
/// 2. Call the grep tool
/// 3. Say "Found 5 results"
///
/// Currently, this renders as:
/// - All prose together: "Let me search for that" + "Found 5 results"
/// - Tool call: grep
///
/// It SHOULD render as:
/// - "Let me search for that"
/// - grep tool call
/// - "Found 5 results"
#[test]
fn test_tool_calls_interleaved_in_response() {
    let mut state = ConversationState::new();

    // User asks a question
    state.push_user_message("Search for foo in the codebase");

    // Assistant starts streaming
    state.start_assistant_streaming();

    // First, assistant says something
    state.append_or_create_prose("Let me search for that.\n");

    // Then calls a tool (currently this creates a SEPARATE item!)
    state.push_tool_running("grep");

    // Tool completes
    state.complete_tool("grep", Some("5 matches".into()));

    // Then assistant continues with more prose
    // BUG: This appends to the SAME assistant message, not after the tool
    state.append_or_create_prose("Found 5 results. Here they are:\n");

    // Complete streaming
    state.complete_streaming();

    // Check the order of items
    let types = get_rendered_item_types(&state);

    // EXPECTED: user -> assistant(prose1) -> tool -> assistant(prose2)
    // ACTUAL (bug): user -> assistant(prose1+prose2) -> tool
    //
    // This assertion will FAIL, demonstrating the bug
    assert_eq!(
        types,
        vec!["user", "assistant", "tool", "assistant"],
        "Tool call should be interleaved between assistant prose blocks. \
         Got {:?} - this means prose is grouped together, breaking conversation flow.",
        types
    );
}

/// Snapshot test for interleaved conversation rendering
#[test]
fn test_interleaved_conversation_snapshot() {
    let mut state = ConversationState::new();

    // Build a realistic conversation with interleaved content
    state.push_user_message("What files contain 'TODO'?");

    // Simulate streaming with tool call in the middle
    state.start_assistant_streaming();
    state.append_or_create_prose("I'll search for TODO comments.\n");
    state.push_tool_running("grep");
    state.complete_tool("grep", Some("Found 3 files".into()));
    state.append_or_create_prose("Found 3 files with TODO:\n- main.rs\n- lib.rs\n- test.rs\n");
    state.complete_streaming();

    let output = render_conversation_to_string(&state, 80, 30);

    // Verify the ORDER of content in the rendered output
    // The tool call should appear BETWEEN the two prose sections
    let prose1_pos = output.find("I'll search");
    let tool_pos = output.find("grep");
    let prose2_pos = output.find("Found 3 files");

    // All should be present
    assert!(
        prose1_pos.is_some(),
        "First prose should be in output: {}",
        output
    );
    assert!(tool_pos.is_some(), "Tool call should be in output: {}", output);
    assert!(
        prose2_pos.is_some(),
        "Second prose should be in output: {}",
        output
    );

    // Order should be: prose1 < tool < prose2
    // This assertion will FAIL, demonstrating the bug
    let prose1 = prose1_pos.unwrap();
    let tool = tool_pos.unwrap();
    let prose2 = prose2_pos.unwrap();

    assert!(
        prose1 < tool,
        "First prose should appear before tool call. \
         prose1 at {}, tool at {}. Output:\n{}",
        prose1,
        tool,
        output
    );

    assert!(
        tool < prose2,
        "Tool call should appear before second prose. \
         tool at {}, prose2 at {}. This is the BUG - prose is grouped together! Output:\n{}",
        tool,
        prose2,
        output
    );
}

/// Test that multiple tool calls maintain their order
#[test]
fn test_multiple_tools_preserve_order() {
    let mut state = ConversationState::new();

    state.push_user_message("Check the code");

    state.start_assistant_streaming();
    state.append_or_create_prose("Running checks:\n");

    // First tool
    state.push_tool_running("cargo check");
    state.complete_tool("cargo check", Some("OK".into()));
    state.append_or_create_prose("Check passed.\n");

    // Second tool
    state.push_tool_running("cargo test");
    state.complete_tool("cargo test", Some("3 tests".into()));
    state.append_or_create_prose("Tests passed.\n");

    // Third tool
    state.push_tool_running("cargo clippy");
    state.complete_tool("cargo clippy", Some("0 warnings".into()));
    state.append_or_create_prose("No warnings.\n");

    state.complete_streaming();

    let output = render_conversation_to_string(&state, 80, 40);

    // Find positions
    let check_pos = output.find("cargo check");
    let test_pos = output.find("cargo test");
    let clippy_pos = output.find("cargo clippy");

    assert!(check_pos.is_some(), "cargo check should appear");
    assert!(test_pos.is_some(), "cargo test should appear");
    assert!(clippy_pos.is_some(), "cargo clippy should appear");

    // Tools should appear in order
    assert!(
        check_pos.unwrap() < test_pos.unwrap(),
        "cargo check should be before cargo test"
    );
    assert!(
        test_pos.unwrap() < clippy_pos.unwrap(),
        "cargo test should be before cargo clippy"
    );
}

/// Test using insta snapshots for visual regression
#[test]
fn test_conversation_with_tools_insta_snapshot() {
    let mut state = ConversationState::new();

    // User message
    state.push_user_message("Help me understand the codebase");

    // Assistant with interleaved tool calls
    state.start_assistant_streaming();
    state.append_or_create_prose("I'll explore the project structure.\n");

    state.push_tool_running("ls");
    state.complete_tool("ls", Some("15 files".into()));

    state.append_or_create_prose("Found 15 files. Let me check the main module.\n");

    state.push_tool_running("cat main.rs");
    state.complete_tool("cat main.rs", Some("200 lines".into()));

    state.append_or_create_prose("The main module handles initialization.\n");
    state.complete_streaming();

    let output = render_conversation_to_string(&state, 80, 35);

    insta::assert_snapshot!("conversation_with_interleaved_tools", output);
}

// =============================================================================
// Regression Prevention Tests
// =============================================================================

/// After the fix, this test verifies the FIXED behavior
#[test]
fn test_fixed_interleaved_order() {
    let mut state = ConversationState::new();

    state.push_user_message("Question");
    state.start_assistant_streaming();
    state.append_or_create_prose("Before tool.\n");
    state.push_tool_running("tool1");
    state.complete_tool("tool1", None);
    state.append_or_create_prose("After tool.\n");
    state.complete_streaming();

    let output = render_conversation_to_string(&state, 80, 20);

    // In the FIXED version, "Before" should appear before "tool1"
    // which should appear before "After"
    let before_pos = output.find("Before tool");
    let tool_pos = output.find("tool1");
    let after_pos = output.find("After tool");

    // All present
    assert!(before_pos.is_some());
    assert!(tool_pos.is_some());
    assert!(after_pos.is_some());

    let before = before_pos.unwrap();
    let tool = tool_pos.unwrap();
    let after = after_pos.unwrap();

    // This is the EXPECTED behavior (will fail until bug is fixed)
    assert!(
        before < tool && tool < after,
        "Expected chronological order: before({}) < tool({}) < after({})\n\
         If tool < before: prose is being grouped (the bug)\n\
         Output:\n{}",
        before,
        tool,
        after,
        output
    );
}

#[cfg(test)]
mod snapshot_helpers {
    //! Additional helpers for snapshot testing

    use super::*;

    /// Render just the item order as a simple list for debugging
    pub fn debug_item_order(state: &ConversationState) -> String {
        let mut output = String::new();
        for (i, item) in state.items().iter().enumerate() {
            let desc = match item {
                ConversationItem::UserMessage { content } => {
                    format!("USER: {}", content.chars().take(30).collect::<String>())
                }
                ConversationItem::AssistantMessage { blocks, .. } => {
                    let text: String = blocks.iter().map(|b| b.text()).collect::<Vec<_>>().join("");
                    format!(
                        "ASST: {} ({} blocks)",
                        text.chars().take(30).collect::<String>(),
                        blocks.len()
                    )
                }
                ConversationItem::Status(s) => format!("STATUS: {:?}", s),
                ConversationItem::ToolCall(t) => {
                    format!("TOOL: {} ({:?})", t.name, t.status)
                }
            };
            output.push_str(&format!("[{}] {}\n", i, desc));
        }
        output
    }
}

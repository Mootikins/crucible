//! Tests for render.rs

use super::render::render;
use super::state::{ToolCallInfo, TuiState};
use super::streaming::StreamingBuffer;
use super::testing::{test_terminal, test_terminal_sized, TestStateBuilder};
use insta::assert_snapshot;
use ratatui::buffer::Buffer;

fn buffer_to_string(buffer: &Buffer) -> String {
    let mut result = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let cell = &buffer[(x, y)];
            result.push_str(cell.symbol());
        }
        result.push('\n');
    }
    result
}

fn render_state(state: &TuiState) -> String {
    let mut terminal = test_terminal();
    terminal.draw(|f| render(f, state)).unwrap();
    buffer_to_string(terminal.backend().buffer())
}

#[test]
fn render_plan_mode_empty() {
    let state = TestStateBuilder::new("plan").build();
    assert_snapshot!("render_plan_empty", render_state(&state));
}

#[test]
fn render_act_mode_empty() {
    let state = TestStateBuilder::new("act").build();
    assert_snapshot!("render_act_empty", render_state(&state));
}

#[test]
fn render_auto_mode_empty() {
    let state = TestStateBuilder::new("auto").build();
    assert_snapshot!("render_auto_empty", render_state(&state));
}

#[test]
fn render_unknown_mode() {
    let state = TestStateBuilder::new("custom").build();
    assert_snapshot!("render_unknown_mode", render_state(&state));
}

#[test]
fn render_with_input() {
    let state = TestStateBuilder::new("plan").with_input("/help").build();
    assert_snapshot!("render_with_input", render_state(&state));
}

#[test]
fn render_with_long_input() {
    let state = TestStateBuilder::new("plan")
        .with_input("This is a longer input that might test wrapping behavior")
        .build();
    assert_snapshot!("render_long_input", render_state(&state));
}

#[test]
fn render_with_cursor_at_start() {
    let state = TestStateBuilder::new("plan")
        .with_input_and_cursor("hello world", 0)
        .build();
    assert_snapshot!("render_cursor_start", render_state(&state));
}

#[test]
fn render_with_cursor_in_middle() {
    let state = TestStateBuilder::new("plan")
        .with_input_and_cursor("hello world", 5)
        .build();
    assert_snapshot!("render_cursor_middle", render_state(&state));
}

#[test]
fn render_with_streaming() {
    let state = TestStateBuilder::new("plan")
        .with_streaming("I am responding to your question...")
        .build();
    assert_snapshot!("render_streaming", render_state(&state));
}

#[test]
fn render_with_error() {
    let state = TestStateBuilder::new("plan")
        .with_error("Connection timeout")
        .build();
    assert_snapshot!("render_error", render_state(&state));
}

#[test]
fn render_with_pending_tools() {
    let mut state = TuiState::new("plan");
    state.pending_tools.push(ToolCallInfo {
        name: "read_file".to_string(),
        args: serde_json::json!({}),
        call_id: Some("1".to_string()),
        completed: false,
        result: None,
        error: None,
    });
    assert_snapshot!("render_pending_tools", render_state(&state));
}

#[test]
fn render_multiple_pending_tools() {
    let mut state = TuiState::new("plan");
    state.pending_tools.push(ToolCallInfo {
        name: "read_file".to_string(),
        args: serde_json::json!({}),
        call_id: Some("1".to_string()),
        completed: false,
        result: None,
        error: None,
    });
    state.pending_tools.push(ToolCallInfo {
        name: "grep".to_string(),
        args: serde_json::json!({}),
        call_id: Some("2".to_string()),
        completed: false,
        result: None,
        error: None,
    });
    assert_snapshot!("render_multiple_pending", render_state(&state));
}

#[test]
fn render_with_completed_tools() {
    let mut state = TuiState::new("plan");
    state.pending_tools.push(ToolCallInfo {
        name: "read_file".to_string(),
        args: serde_json::json!({}),
        call_id: Some("1".to_string()),
        completed: true,
        result: Some("done".to_string()),
        error: None,
    });
    assert_snapshot!("render_completed_tools", render_state(&state));
}

#[test]
fn render_narrow_terminal() {
    let state = TestStateBuilder::new("plan").with_input("input").build();
    let mut terminal = test_terminal_sized(40, 12);
    terminal.draw(|f| render(f, &state)).unwrap();
    assert_snapshot!(
        "render_narrow",
        buffer_to_string(terminal.backend().buffer())
    );
}

#[test]
fn render_unicode_input() {
    let state = TestStateBuilder::new("plan")
        .with_input("Hello 世界 🚀")
        .build();
    assert_snapshot!("render_unicode", render_state(&state));
}

#[test]
fn render_streaming_with_input() {
    let state = TestStateBuilder::new("act")
        .with_input("my question")
        .with_streaming("Processing your request...")
        .build();
    assert_snapshot!("render_streaming_with_input", render_state(&state));
}

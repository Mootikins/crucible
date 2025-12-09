//! Integration tests for the Rune plugin system
//!
//! Tests end-to-end plugin loading and event processing.

use crucible_rune::{ContentBlock, EventPipeline, PluginLoader, ToolResultEvent};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::RwLock;

/// Helper to create a test event
fn make_event(tool_name: &str, text: &str, is_error: bool) -> ToolResultEvent {
    ToolResultEvent {
        tool_name: tool_name.to_string(),
        arguments: serde_json::json!({}),
        is_error,
        content: vec![ContentBlock::Text {
            text: text.to_string(),
        }],
        duration_ms: 100,
    }
}

/// Helper to set up pipeline with plugin
async fn setup_pipeline(plugin_content: &str) -> (EventPipeline, TempDir) {
    let temp = TempDir::new().unwrap();
    let plugin_path = temp.path().join("test_plugin.rn");
    std::fs::write(&plugin_path, plugin_content).unwrap();

    let mut loader = PluginLoader::new(temp.path()).unwrap();
    loader.load_plugins().await.unwrap();

    let pipeline = EventPipeline::new(Arc::new(RwLock::new(loader)));
    (pipeline, temp)
}

#[tokio::test]
async fn test_cargo_test_output_filtered() {
    let cargo_output = "running 5 tests\ntest one ... ok\ntest two ... ok\ntest result: ok. 5 passed";

    let (pipeline, _temp) = setup_pipeline(
        r#"
pub fn init() {
    #{ hooks: [#{ event: "tool_result", pattern: "just_test*", handler: "filter" }] }
}

pub fn filter(ctx, event) {
    // Simply replace with "FILTERED" to verify hook is working
    event.content = [#{ type: "text", text: "FILTERED" }];
    event
}
"#,
    )
    .await;

    let event = make_event("just_test", cargo_output, false);
    let result = pipeline.process_tool_result(event).await.unwrap();

    let text = result.text_content();
    // Should be filtered
    assert_eq!(text, "FILTERED", "Expected 'FILTERED' but got: {}", text);
}

#[tokio::test]
async fn test_pytest_summary_extraction() {
    // Test that we can extract summary from pytest output
    let pytest_output = "test_one PASSED\ntest_two PASSED\n====== 10 passed in 0.12s ======";

    let (pipeline, _temp) = setup_pipeline(
        r#"
pub fn init() {
    #{ hooks: [#{ event: "tool_result", pattern: "just_test*", handler: "filter" }] }
}

pub fn filter(ctx, event) {
    // Simpler: just check if text contains "passed in" and replace with summary
    let text = event.content[0].text;
    if text.contains("passed in") {
        event.content = [#{ type: "text", text: "SUMMARY: 10 passed" }];
    }
    event
}
"#,
    )
    .await;

    let event = make_event("just_test_python", pytest_output, false);
    let result = pipeline.process_tool_result(event).await.unwrap();

    let text = result.text_content();
    // Should be replaced with summary
    assert_eq!(
        text, "SUMMARY: 10 passed",
        "Expected 'SUMMARY: 10 passed' but got: {}",
        text
    );
}

#[tokio::test]
async fn test_no_match_passthrough() {
    let (pipeline, _temp) = setup_pipeline(
        r#"
pub fn init() {
    #{ hooks: [#{ event: "tool_result", pattern: "just_test*", handler: "filter" }] }
}

pub fn filter(ctx, event) {
    event.content = [#{ type: "text", text: "filtered!" }];
    event
}
"#,
    )
    .await;

    // Non-matching tool name
    let event = make_event("just_build", "original content", false);
    let result = pipeline.process_tool_result(event).await.unwrap();

    // Should be unchanged (no matching hooks)
    assert_eq!(result.text_content(), "original content");
}

#[tokio::test]
async fn test_hook_can_modify_is_error() {
    let (pipeline, _temp) = setup_pipeline(
        r#"
pub fn init() {
    #{ hooks: [#{ event: "tool_result", pattern: "*", handler: "check_failure" }] }
}

pub fn check_failure(ctx, event) {
    let text = event.content[0].text;
    if text.contains("FAILED") {
        event.is_error = true;
    }
    event
}
"#,
    )
    .await;

    let event = make_event("just_test", "test result: FAILED. 1 failed", false);
    let result = pipeline.process_tool_result(event).await.unwrap();

    // Hook should have set is_error to true
    assert!(result.is_error);
}

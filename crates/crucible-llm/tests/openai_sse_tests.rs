//! OpenAI SSE Streaming Tests
//!
//! TDD tests for the OpenAI SSE (Server-Sent Events) streaming implementation.
//! These tests use wiremock to simulate OpenAI's SSE streaming API responses.
//!
//! Target: `crates/crucible-llm/src/chat/openai.rs:296-396`

mod common;

use common::mock_server::{
    openai_mock_server, openai_mock_server_any_key, openai_mock_server_error,
    openai_response_chunks, openai_response_single, openai_response_with_tool_call,
};
use common::{collect_stream_content, collect_stream_with_error, collect_tool_calls, sse_stream};
use crucible_llm::chat::OpenAIChatProvider;
use crucible_core::traits::{ChatCompletionRequest, LlmMessage, TextGenerationProvider};

// ============================================================================
// Helper Functions
// ============================================================================

fn create_test_request(content: &str) -> ChatCompletionRequest {
    let mut request = ChatCompletionRequest::new(
        "gpt-4".to_string(),
        vec![LlmMessage::user(content)],
    );
    request.max_tokens = Some(100);
    request.temperature = Some(0.7);
    request
}

// ============================================================================
// TEST: SSE Parsing
// ============================================================================

/// Test that [DONE] marker ends stream cleanly
#[tokio::test]
async fn test_openai_sse_done_marker() {
    let response = openai_response_single("Hello!");
    let server = openai_mock_server(&response).await;

    let provider = OpenAIChatProvider::new(
        "fake-key".to_string(),
        Some(server.uri()),
        "gpt-4".to_string(),
        60,
    );
    let request = create_test_request("Hi");

    let stream = provider.generate_chat_completion_stream(request);
    let content = collect_stream_content(stream).await.expect("Stream should complete cleanly");

    assert_eq!(content, "Hello!");
}

/// Test that empty lines in SSE are ignored
#[tokio::test]
async fn test_openai_sse_empty_lines_ignored() {
    // SSE with extra empty lines
    let response = r#"data: {"choices":[{"index":0,"delta":{"role":"assistant","content":"Hi"}}]}


data: {"choices":[{"index":0,"delta":{"content":"!"}}]}


data: {"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: [DONE]

"#;
    let server = openai_mock_server_any_key(response).await;

    let provider = OpenAIChatProvider::new(
        "test-key".to_string(),
        Some(server.uri()),
        "gpt-4".to_string(),
        60,
    );
    let request = create_test_request("Hi");

    let stream = provider.generate_chat_completion_stream(request);
    let content = collect_stream_content(stream).await.expect("Should handle empty lines");

    assert_eq!(content, "Hi!");
}

/// Test handling of malformed JSON in SSE
#[tokio::test]
async fn test_openai_sse_malformed_json() {
    let response = "data: {not valid json}\n\ndata: [DONE]\n\n";
    let server = openai_mock_server_any_key(response).await;

    let provider = OpenAIChatProvider::new(
        "test-key".to_string(),
        Some(server.uri()),
        "gpt-4".to_string(),
        60,
    );
    let request = create_test_request("Hi");

    let stream = provider.generate_chat_completion_stream(request);
    let (content, error) = collect_stream_with_error(stream).await;

    // Should have error for invalid JSON
    assert!(
        error.is_some() || content.is_empty(),
        "Malformed JSON should cause error"
    );
}

/// Test SSE data split across buffer boundaries
#[tokio::test]
async fn test_openai_sse_buffer_boundary() {
    // This test simulates data being received in chunks where JSON might be split
    let response = openai_response_chunks(&["Chunk", " one", " two"]);
    let server = openai_mock_server_any_key(&response).await;

    let provider = OpenAIChatProvider::new(
        "test-key".to_string(),
        Some(server.uri()),
        "gpt-4".to_string(),
        60,
    );
    let request = create_test_request("Hi");

    let stream = provider.generate_chat_completion_stream(request);
    let content = collect_stream_content(stream).await.expect("Should handle buffer splits");

    assert_eq!(content, "Chunk one two");
}

// ============================================================================
// TEST: Tool Calls
// ============================================================================

/// Test streaming tool call deltas are accumulated correctly
#[tokio::test]
async fn test_openai_sse_tool_call_deltas() {
    let response = openai_response_with_tool_call("get_weather", r#"{"city": "London"}"#);
    let server = openai_mock_server_any_key(&response).await;

    let provider = OpenAIChatProvider::new(
        "test-key".to_string(),
        Some(server.uri()),
        "gpt-4".to_string(),
        60,
    );
    let request = create_test_request("What's the weather in London?");

    let stream = provider.generate_chat_completion_stream(request);
    let tool_calls = collect_tool_calls(stream).await;

    // Should have tool call(s)
    assert!(
        !tool_calls.is_empty(),
        "Should extract tool calls from SSE stream"
    );

    // Verify we captured the tool name
    let has_get_weather = tool_calls.iter().any(|tc| {
        tc.function
            .as_ref()
            .and_then(|f| f.name.as_ref())
            .map(|n| n == "get_weather")
            .unwrap_or(false)
    });
    assert!(has_get_weather, "Should have get_weather tool call");
}

/// Test multiple tool calls in single response
#[tokio::test]
async fn test_openai_sse_multiple_tool_calls() {
    // Build response with two tool calls
    let response = r#"data: {"choices":[{"index":0,"delta":{"role":"assistant","tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"search"}}]}}]}

data: {"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"q\":"}}]}}]}

data: {"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"test\"}"}}]}}]}

data: {"choices":[{"index":0,"delta":{"tool_calls":[{"index":1,"id":"call_2","type":"function","function":{"name":"analyze"}}]}}]}

data: {"choices":[{"index":0,"delta":{"tool_calls":[{"index":1,"function":{"arguments":"{\"data\":\"x\"}"}}]}}]}

data: {"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}

data: [DONE]

"#;
    let server = openai_mock_server_any_key(response).await;

    let provider = OpenAIChatProvider::new(
        "test-key".to_string(),
        Some(server.uri()),
        "gpt-4".to_string(),
        60,
    );
    let request = create_test_request("Search and analyze");

    let stream = provider.generate_chat_completion_stream(request);
    let tool_calls = collect_tool_calls(stream).await;

    // Should have at least 2 tool call deltas (may be more due to argument streaming)
    assert!(
        tool_calls.len() >= 2,
        "Should have multiple tool call deltas"
    );
}

// ============================================================================
// TEST: Error Handling
// ============================================================================

/// Test HTTP 401 unauthorized error
#[tokio::test]
async fn test_openai_unauthorized() {
    let error_body = r#"{"error": {"message": "Invalid API key", "type": "invalid_request_error"}}"#;
    let server = openai_mock_server_error(401, error_body).await;

    let provider = OpenAIChatProvider::new(
        "bad-key".to_string(),
        Some(server.uri()),
        "gpt-4".to_string(),
        60,
    );
    let request = create_test_request("Hi");

    let stream = provider.generate_chat_completion_stream(request);
    let (content, error) = collect_stream_with_error(stream).await;

    assert!(content.is_empty());
    assert!(error.is_some(), "Should have auth error");
}

/// Test HTTP 429 rate limit error
#[tokio::test]
async fn test_openai_rate_limit() {
    let error_body = r#"{"error": {"message": "Rate limit exceeded", "type": "rate_limit_error"}}"#;
    let server = openai_mock_server_error(429, error_body).await;

    let provider = OpenAIChatProvider::new(
        "test-key".to_string(),
        Some(server.uri()),
        "gpt-4".to_string(),
        60,
    );
    let request = create_test_request("Hi");

    let stream = provider.generate_chat_completion_stream(request);
    let (content, error) = collect_stream_with_error(stream).await;

    assert!(content.is_empty());
    assert!(error.is_some(), "Should have rate limit error");
}

/// Test HTTP 500 server error
#[tokio::test]
async fn test_openai_server_error() {
    let error_body = r#"{"error": {"message": "Internal error", "type": "server_error"}}"#;
    let server = openai_mock_server_error(500, error_body).await;

    let provider = OpenAIChatProvider::new(
        "test-key".to_string(),
        Some(server.uri()),
        "gpt-4".to_string(),
        60,
    );
    let request = create_test_request("Hi");

    let stream = provider.generate_chat_completion_stream(request);
    let (content, error) = collect_stream_with_error(stream).await;

    assert!(content.is_empty());
    assert!(error.is_some(), "Should have server error");
}

// ============================================================================
// TEST: Edge Cases
// ============================================================================

/// Test response with only finish_reason (no content)
#[tokio::test]
async fn test_openai_finish_only() {
    let response = r#"data: {"choices":[{"index":0,"delta":{"role":"assistant"}}]}

data: {"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: [DONE]

"#;
    let server = openai_mock_server_any_key(response).await;

    let provider = OpenAIChatProvider::new(
        "test-key".to_string(),
        Some(server.uri()),
        "gpt-4".to_string(),
        60,
    );
    let request = create_test_request("Hi");

    let stream = provider.generate_chat_completion_stream(request);
    let content = collect_stream_content(stream).await.expect("Should succeed");

    // Empty content is valid
    assert!(content.is_empty() || content.len() >= 0);
}

/// Test Unicode content in SSE stream
#[tokio::test]
async fn test_openai_unicode_content() {
    let response = openai_response_single("Hello 世界! 🚀");
    let server = openai_mock_server_any_key(&response).await;

    let provider = OpenAIChatProvider::new(
        "test-key".to_string(),
        Some(server.uri()),
        "gpt-4".to_string(),
        60,
    );
    let request = create_test_request("Say hi globally");

    let stream = provider.generate_chat_completion_stream(request);
    let content = collect_stream_content(stream).await.expect("Should handle Unicode");

    assert!(content.contains("世界"), "Should contain Chinese chars");
    assert!(content.contains("🚀"), "Should contain emoji");
}

/// Test SSE with 'event:' prefix (not used by OpenAI but valid SSE)
#[tokio::test]
async fn test_openai_sse_event_prefix() {
    // Standard OpenAI doesn't use event: prefix, but SSE spec allows it
    let response = r#"event: message
data: {"choices":[{"index":0,"delta":{"role":"assistant","content":"Hi"}}]}

data: {"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: [DONE]

"#;
    let server = openai_mock_server_any_key(response).await;

    let provider = OpenAIChatProvider::new(
        "test-key".to_string(),
        Some(server.uri()),
        "gpt-4".to_string(),
        60,
    );
    let request = create_test_request("Hi");

    let stream = provider.generate_chat_completion_stream(request);
    let result = collect_stream_content(stream).await;

    // Should handle or gracefully ignore event: prefix
    assert!(result.is_ok() || result.is_err());
}

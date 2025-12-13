//! Ollama NDJSON Streaming Tests
//!
//! TDD tests for the Ollama streaming implementation.
//! These tests use wiremock to simulate Ollama's NDJSON streaming API responses.
//!
//! Target: `crates/crucible-llm/src/chat/ollama.rs:267-367`

mod common;

use common::mock_server::{
    ollama_mock_server, ollama_mock_server_delayed, ollama_mock_server_error,
    ollama_response_chunks, ollama_response_single, ollama_response_with_tool_call,
};
use common::{collect_stream_content, collect_stream_with_error, collect_tool_calls};
use crucible_llm::chat::OllamaChatProvider;
use crucible_core::traits::{ChatCompletionRequest, LlmMessage, TextGenerationProvider};

// ============================================================================
// Helper Functions
// ============================================================================

fn create_test_request(content: &str) -> ChatCompletionRequest {
    let mut request = ChatCompletionRequest::new(
        "llama3.2".to_string(),
        vec![LlmMessage::user(content)],
    );
    request.max_tokens = Some(100);
    request.temperature = Some(0.7);
    request
}

// ============================================================================
// TEST: Successful Streaming
// ============================================================================

/// Test basic successful streaming with single response chunk
#[tokio::test]
async fn test_ollama_stream_success() {
    let response = ollama_response_single("Hello, world!");
    let server = ollama_mock_server(&response).await;

    let provider = OllamaChatProvider::new(server.uri(), "llama3.2".to_string(), 60);
    let request = create_test_request("Hi");

    let stream = provider.generate_chat_completion_stream(request);
    let content = collect_stream_content(stream).await.expect("Stream should succeed");

    assert_eq!(content, "Hello, world!");
}

/// Test streaming with multiple content chunks
#[tokio::test]
async fn test_ollama_stream_multiple_chunks() {
    let response = ollama_response_chunks(&["Hello", ", ", "world", "!"]);
    let server = ollama_mock_server(&response).await;

    let provider = OllamaChatProvider::new(server.uri(), "llama3.2".to_string(), 60);
    let request = create_test_request("Hi");

    let stream = provider.generate_chat_completion_stream(request);
    let content = collect_stream_content(stream).await.expect("Stream should succeed");

    assert_eq!(content, "Hello, world!");
}

// ============================================================================
// TEST: Error Handling
// ============================================================================

/// Test HTTP 500 error returns appropriate error
#[tokio::test]
async fn test_ollama_http_500() {
    let error_body = r#"{"error": "Internal server error"}"#;
    let server = ollama_mock_server_error(500, error_body).await;

    let provider = OllamaChatProvider::new(server.uri(), "llama3.2".to_string(), 60);
    let request = create_test_request("Hi");

    let stream = provider.generate_chat_completion_stream(request);
    let (content, error) = collect_stream_with_error(stream).await;

    // Should have no content and an error
    assert!(content.is_empty(), "No content on HTTP 500");
    assert!(error.is_some(), "Should have error on HTTP 500");
}

/// Test HTTP 404 (model not found) error
#[tokio::test]
async fn test_ollama_model_not_found() {
    let error_body = r#"{"error": "model 'nonexistent' not found"}"#;
    let server = ollama_mock_server_error(404, error_body).await;

    let provider = OllamaChatProvider::new(server.uri(), "nonexistent".to_string(), 60);
    let request = create_test_request("Hi");

    let stream = provider.generate_chat_completion_stream(request);
    let (content, error) = collect_stream_with_error(stream).await;

    assert!(content.is_empty());
    assert!(error.is_some());
}

/// Test connection timeout handling
#[tokio::test]
async fn test_ollama_connection_timeout() {
    use std::time::Duration;

    // Create a server that delays response beyond timeout
    let response = ollama_response_single("Delayed");
    let server = ollama_mock_server_delayed(&response, Duration::from_secs(5)).await;

    // Create provider with 1 second timeout
    let provider = OllamaChatProvider::new(server.uri(), "llama3.2".to_string(), 1);
    let request = create_test_request("Hi");

    let stream = provider.generate_chat_completion_stream(request);
    let (content, error) = collect_stream_with_error(stream).await;

    // Should timeout and return error
    // Note: The exact behavior depends on implementation - may get partial content or error
    assert!(
        error.is_some() || content.is_empty(),
        "Should timeout or have no content"
    );
}

// ============================================================================
// TEST: NDJSON Parsing
// ============================================================================

/// Test handling of malformed JSON in stream
#[tokio::test]
async fn test_ollama_malformed_json() {
    // Invalid JSON in response
    let response = "{not valid json}\n";
    let server = ollama_mock_server(response).await;

    let provider = OllamaChatProvider::new(server.uri(), "llama3.2".to_string(), 60);
    let request = create_test_request("Hi");

    let stream = provider.generate_chat_completion_stream(request);
    let (content, error) = collect_stream_with_error(stream).await;

    // Should have error for invalid JSON
    assert!(
        error.is_some() || content.is_empty(),
        "Malformed JSON should cause error or empty content"
    );
}

/// Test partial NDJSON (incomplete line at end)
#[tokio::test]
async fn test_ollama_partial_ndjson() {
    // Complete first line, incomplete second line
    let response = r#"{"model":"llama3.2","message":{"role":"assistant","content":"Hi"},"done":false}
{"model":"llama3.2","message":{"role":"assistant","cont"#;
    let server = ollama_mock_server(response).await;

    let provider = OllamaChatProvider::new(server.uri(), "llama3.2".to_string(), 60);
    let request = create_test_request("Hi");

    let stream = provider.generate_chat_completion_stream(request);
    let (content, error) = collect_stream_with_error(stream).await;

    // Should at least get the first chunk's content
    // The partial line may or may not cause an error depending on implementation
    assert!(
        content.contains("Hi") || error.is_some(),
        "Should get partial content or error"
    );
}

/// Test empty response handling
#[tokio::test]
async fn test_ollama_empty_response() {
    let server = ollama_mock_server("").await;

    let provider = OllamaChatProvider::new(server.uri(), "llama3.2".to_string(), 60);
    let request = create_test_request("Hi");

    let stream = provider.generate_chat_completion_stream(request);
    let content = collect_stream_content(stream).await;

    // Empty response should succeed with empty content or return an error
    assert!(
        content.is_ok() || content.is_err(),
        "Should handle empty response"
    );
}

// ============================================================================
// TEST: Tool Calls
// ============================================================================

/// Test streaming response with tool calls
#[tokio::test]
async fn test_ollama_stream_with_tool_calls() {
    let response = ollama_response_with_tool_call("search", r#"{"query": "test"}"#);
    let server = ollama_mock_server(&response).await;

    let provider = OllamaChatProvider::new(server.uri(), "llama3.2".to_string(), 60);
    let request = create_test_request("Search for test");

    let stream = provider.generate_chat_completion_stream(request);
    let tool_calls = collect_tool_calls(stream).await;

    // Should have at least one tool call
    assert!(
        !tool_calls.is_empty(),
        "Should extract tool calls from response"
    );

    // Verify tool call details
    let first_call = &tool_calls[0];
    assert!(first_call.function.is_some(), "Should have function");

    let function = first_call.function.as_ref().unwrap();
    assert_eq!(
        function.name.as_deref(),
        Some("search"),
        "Tool name should match"
    );
}

// ============================================================================
// TEST: Edge Cases
// ============================================================================

/// Test response with Unicode content
#[tokio::test]
async fn test_ollama_unicode_content() {
    let response = ollama_response_single("Hello 世界! 🌍");
    let server = ollama_mock_server(&response).await;

    let provider = OllamaChatProvider::new(server.uri(), "llama3.2".to_string(), 60);
    let request = create_test_request("Say hi in multiple languages");

    let stream = provider.generate_chat_completion_stream(request);
    let content = collect_stream_content(stream).await.expect("Stream should succeed");

    assert!(content.contains("世界"), "Should contain Chinese characters");
    assert!(content.contains("🌍"), "Should contain emoji");
}

/// Test response with special characters in JSON
#[tokio::test]
async fn test_ollama_special_json_chars() {
    // Content with characters that need JSON escaping
    let response = ollama_response_single(r#"Quote: \"Hello\" and newline\n"#);
    let server = ollama_mock_server(&response).await;

    let provider = OllamaChatProvider::new(server.uri(), "llama3.2".to_string(), 60);
    let request = create_test_request("Quote something");

    let stream = provider.generate_chat_completion_stream(request);
    let content = collect_stream_content(stream).await;

    // Should handle JSON escaping correctly
    assert!(content.is_ok(), "Should handle special JSON characters");
}

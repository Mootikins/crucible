//! Shared test utilities for crucible-llm tests
//!
//! This module provides helpers for:
//! - Building NDJSON streams (Ollama format)
//! - Building SSE streams (OpenAI format)
//! - Mock HTTP servers for testing streaming endpoints
//! - Stream collection utilities

pub mod mock_server;

use crucible_core::traits::{ChatCompletionChunk, LlmError, LlmResult, ToolCallDelta};
use futures::stream::BoxStream;
use futures_util::StreamExt;

/// Build NDJSON stream content (Ollama format)
///
/// Each line is a complete JSON object separated by newlines.
/// Used by Ollama's streaming API.
///
/// # Example
/// ```
/// let stream = ndjson_stream(&[
///     r#"{"model":"llama3.2","message":{"content":"Hello"},"done":false}"#,
///     r#"{"model":"llama3.2","message":{"content":"!"},"done":true}"#,
/// ]);
/// ```
pub fn ndjson_stream(lines: &[&str]) -> String {
    lines
        .iter()
        .map(|line| format!("{}\n", line))
        .collect::<String>()
}

/// Build SSE stream content (OpenAI format)
///
/// Each event is prefixed with "data: " and followed by double newlines.
/// Automatically appends the "[DONE]" marker.
///
/// # Example
/// ```
/// let stream = sse_stream(&[
///     r#"{"choices":[{"delta":{"content":"Hi"}}]}"#,
///     r#"{"choices":[{"delta":{"content":"!"}}]}"#,
/// ]);
/// ```
pub fn sse_stream(events: &[&str]) -> String {
    let mut result = events
        .iter()
        .map(|e| format!("data: {}\n\n", e))
        .collect::<String>();
    result.push_str("data: [DONE]\n\n");
    result
}

/// Collect all text content from a ChatCompletionChunk stream
///
/// Aggregates the `delta.content` from all chunks into a single string.
/// Returns an error if any chunk in the stream is an error.
pub async fn collect_stream_content(
    mut stream: BoxStream<'_, LlmResult<ChatCompletionChunk>>,
) -> Result<String, LlmError> {
    let mut content = String::new();

    while let Some(result) = stream.next().await {
        let chunk = result?;
        if let Some(delta_content) = &chunk.delta.content {
            content.push_str(delta_content);
        }
    }

    Ok(content)
}

/// Collect all tool call deltas from a ChatCompletionChunk stream
///
/// Aggregates all `ToolCallDelta` from all chunks.
/// Ignores errors - use `collect_stream_content` if you need error handling.
pub async fn collect_tool_calls(
    mut stream: BoxStream<'_, LlmResult<ChatCompletionChunk>>,
) -> Vec<ToolCallDelta> {
    let mut tool_calls = Vec::new();

    while let Some(result) = stream.next().await {
        if let Ok(chunk) = result {
            if let Some(ref deltas) = chunk.delta.tool_calls {
                tool_calls.extend(deltas.clone());
            }
        }
    }

    tool_calls
}

/// Collect stream and return both content and any error that occurred
///
/// Useful for tests that expect an error partway through the stream.
pub async fn collect_stream_with_error(
    mut stream: BoxStream<'_, LlmResult<ChatCompletionChunk>>,
) -> (String, Option<LlmError>) {
    let mut content = String::new();
    let mut error = None;

    while let Some(result) = stream.next().await {
        match result {
            Ok(chunk) => {
                if let Some(delta_content) = &chunk.delta.content {
                    content.push_str(delta_content);
                }
            }
            Err(e) => {
                error = Some(e);
                break;
            }
        }
    }

    (content, error)
}

/// Assert that a stream yields the expected content
pub async fn assert_stream_yields_content(
    stream: BoxStream<'_, LlmResult<ChatCompletionChunk>>,
    expected: &str,
) {
    let content = collect_stream_content(stream)
        .await
        .expect("stream should succeed");
    assert_eq!(content, expected);
}

/// Assert that a stream contains at least one tool call with the given name
pub async fn assert_stream_has_tool_call(
    stream: BoxStream<'_, LlmResult<ChatCompletionChunk>>,
    expected_tool_name: &str,
) {
    let tool_calls = collect_tool_calls(stream).await;
    let has_tool = tool_calls.iter().any(|tc| {
        tc.function
            .as_ref()
            .and_then(|f| f.name.as_ref())
            .map(|n| n == expected_tool_name)
            .unwrap_or(false)
    });
    assert!(
        has_tool,
        "expected tool call '{}' not found in {:?}",
        expected_tool_name, tool_calls
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ndjson_stream_builder() {
        let stream = ndjson_stream(&[r#"{"done":false}"#, r#"{"done":true}"#]);
        assert_eq!(stream, "{\"done\":false}\n{\"done\":true}\n");
    }

    #[test]
    fn test_sse_stream_builder() {
        let stream = sse_stream(&[r#"{"choices":[]}"#]);
        assert_eq!(stream, "data: {\"choices\":[]}\n\ndata: [DONE]\n\n");
    }
}

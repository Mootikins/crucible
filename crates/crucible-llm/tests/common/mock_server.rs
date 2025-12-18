//! Mock HTTP server helpers for testing streaming LLM providers
//!
//! Uses wiremock to create mock servers that simulate Ollama and OpenAI APIs.

use wiremock::matchers::{body_json_schema, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Start a mock Ollama server that returns the given NDJSON response
///
/// The mock listens on POST /api/chat and returns the NDJSON body as a stream.
///
/// # Example
/// ```
/// let ndjson = r#"{"model":"llama3.2","message":{"content":"Hi"},"done":true}"#;
/// let server = ollama_mock_server(&format!("{}\n", ndjson)).await;
/// let provider = OllamaChatProvider::new(server.uri(), "llama3.2".into(), 60);
/// ```
pub async fn ollama_mock_server(ndjson_body: &str) -> MockServer {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(ndjson_body)
                .insert_header("content-type", "application/x-ndjson"),
        )
        .mount(&server)
        .await;

    server
}

/// Start a mock Ollama server that returns an HTTP error
pub async fn ollama_mock_server_error(status_code: u16, error_body: &str) -> MockServer {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(status_code).set_body_string(error_body))
        .mount(&server)
        .await;

    server
}

/// Start a mock Ollama server with a delayed response (for timeout testing)
pub async fn ollama_mock_server_delayed(
    ndjson_body: &str,
    delay: std::time::Duration,
) -> MockServer {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(ndjson_body)
                .set_delay(delay)
                .insert_header("content-type", "application/x-ndjson"),
        )
        .mount(&server)
        .await;

    server
}

/// Start a mock OpenAI server that returns the given SSE response
///
/// The mock listens on POST /chat/completions and returns the SSE body as a stream.
///
/// # Example
/// ```
/// let sse = r#"data: {"choices":[{"delta":{"content":"Hi"}}]}
///
/// data: [DONE]
/// "#;
/// let server = openai_mock_server(sse).await;
/// let provider = OpenAIChatProvider::new("fake-key".into(), Some(server.uri()), "gpt-4".into(), 60);
/// ```
pub async fn openai_mock_server(sse_body: &str) -> MockServer {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(header("Authorization", "Bearer fake-key"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_body)
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&server)
        .await;

    server
}

/// Start a mock OpenAI server that accepts any API key
pub async fn openai_mock_server_any_key(sse_body: &str) -> MockServer {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_body)
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&server)
        .await;

    server
}

/// Start a mock OpenAI server that returns an HTTP error
pub async fn openai_mock_server_error(status_code: u16, error_body: &str) -> MockServer {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(status_code).set_body_string(error_body))
        .mount(&server)
        .await;

    server
}

/// Build a minimal valid Ollama streaming response
///
/// Returns a single content chunk followed by a done marker.
pub fn ollama_response_single(content: &str) -> String {
    format!(
        r#"{{"model":"llama3.2","message":{{"role":"assistant","content":"{}"}},"done":false}}
{{"model":"llama3.2","message":{{"role":"assistant","content":""}},"done":true,"done_reason":"stop"}}
"#,
        content
    )
}

/// Build an Ollama streaming response with multiple content chunks
pub fn ollama_response_chunks(chunks: &[&str]) -> String {
    let mut result = String::new();
    for chunk in chunks {
        result.push_str(&format!(
            r#"{{"model":"llama3.2","message":{{"role":"assistant","content":"{}"}},"done":false}}"#,
            chunk
        ));
        result.push('\n');
    }
    result.push_str(
        r#"{"model":"llama3.2","message":{"role":"assistant","content":""},"done":true,"done_reason":"stop"}"#,
    );
    result.push('\n');
    result
}

/// Build an Ollama streaming response with a tool call
pub fn ollama_response_with_tool_call(tool_name: &str, arguments: &str) -> String {
    format!(
        r#"{{"model":"llama3.2","message":{{"role":"assistant","content":"","tool_calls":[{{"id":"call_123","function":{{"name":"{}","arguments":{}}}}}]}},"done":true,"done_reason":"tool_calls"}}
"#,
        tool_name, arguments
    )
}

/// Build a minimal valid OpenAI streaming response
///
/// Returns a single content chunk followed by a done marker.
pub fn openai_response_single(content: &str) -> String {
    format!(
        r#"data: {{"choices":[{{"index":0,"delta":{{"role":"assistant","content":"{}"}}}}]}}

data: {{"choices":[{{"index":0,"delta":{{}},"finish_reason":"stop"}}]}}

data: [DONE]

"#,
        content
    )
}

/// Build an OpenAI streaming response with multiple content chunks
pub fn openai_response_chunks(chunks: &[&str]) -> String {
    let mut result = String::new();
    for chunk in chunks {
        result.push_str(&format!(
            r#"data: {{"choices":[{{"index":0,"delta":{{"content":"{}"}}}}]}}

"#,
            chunk
        ));
    }
    result.push_str(
        r#"data: {"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: [DONE]

"#,
    );
    result
}

/// Build an OpenAI streaming response with tool call deltas
///
/// Tool calls are streamed in multiple chunks:
/// 1. First chunk with id and name
/// 2. Subsequent chunks with argument fragments
pub fn openai_response_with_tool_call(tool_name: &str, arguments: &str) -> String {
    // Split arguments for realistic streaming
    let arg_mid = arguments.len() / 2;
    let (arg1, arg2) = arguments.split_at(arg_mid);

    format!(
        r#"data: {{"choices":[{{"index":0,"delta":{{"role":"assistant","tool_calls":[{{"index":0,"id":"call_abc123","type":"function","function":{{"name":"{}"}}}}]}}}}]}}

data: {{"choices":[{{"index":0,"delta":{{"tool_calls":[{{"index":0,"function":{{"arguments":"{}"}}}}]}}}}]}}

data: {{"choices":[{{"index":0,"delta":{{"tool_calls":[{{"index":0,"function":{{"arguments":"{}"}}}}]}}}}]}}

data: {{"choices":[{{"index":0,"delta":{{}},"finish_reason":"tool_calls"}}]}}

data: [DONE]

"#,
        tool_name,
        arg1.replace('"', r#"\""#),
        arg2.replace('"', r#"\""#)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_response_single() {
        let response = ollama_response_single("Hello");
        assert!(response.contains("Hello"));
        assert!(response.contains("\"done\":true"));
    }

    #[test]
    fn test_openai_response_single() {
        let response = openai_response_single("World");
        assert!(response.contains("World"));
        assert!(response.contains("[DONE]"));
    }

    #[test]
    fn test_ollama_response_with_tool_call() {
        let response = ollama_response_with_tool_call("search", r#"{"query":"test"}"#);
        assert!(response.contains("search"));
        assert!(response.contains("query"));
    }
}

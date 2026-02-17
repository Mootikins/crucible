//! Display parity integration tests — verifies ACP streaming chunks preserve
//! tool call arguments, tool results, and handle missing token usage gracefully.

use crucible_acp::client::{ClientConfig, CrucibleAcpClient};
use crucible_acp::StreamingChunk;
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};

fn test_config(timeout_ms: Option<u64>) -> ClientConfig {
    ClientConfig {
        agent_path: PathBuf::from("mock-display-parity"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms,
        max_retries: Some(1),
    }
}

fn client_with_custom_transport(
    timeout_ms: Option<u64>,
) -> (
    CrucibleAcpClient,
    BufReader<tokio::io::ReadHalf<DuplexStream>>,
    tokio::io::WriteHalf<DuplexStream>,
) {
    let (client_to_agent_client, client_to_agent_agent) = tokio::io::duplex(65_536);
    let (agent_to_client_agent, agent_to_client_client) = tokio::io::duplex(65_536);

    let (_client_read_unused, client_write) = tokio::io::split(client_to_agent_client);
    let (agent_read, _agent_write_unused) = tokio::io::split(client_to_agent_agent);

    let (_agent_read_unused, agent_write) = tokio::io::split(agent_to_client_agent);
    let (client_read, _client_write_unused) = tokio::io::split(agent_to_client_client);

    let client = CrucibleAcpClient::with_transport(
        test_config(timeout_ms),
        Box::pin(client_write),
        Box::pin(BufReader::new(client_read)),
    );

    (client, BufReader::new(agent_read), agent_write)
}

fn make_prompt_request(session_id: &str, text: &str) -> agent_client_protocol::PromptRequest {
    serde_json::from_value(json!({
        "sessionId": session_id,
        "prompt": [{"type": "text", "text": text}],
        "_meta": null
    }))
    .expect("valid prompt request")
}

async fn write_json_line(
    writer: &mut tokio::io::WriteHalf<DuplexStream>,
    value: serde_json::Value,
) -> std::io::Result<()> {
    writer
        .write_all(format!("{}\n", serde_json::to_string(&value).unwrap()).as_bytes())
        .await?;
    writer.flush().await
}

fn tool_call_notification(
    session_id: &str,
    tool_call_id: &str,
    title: &str,
    raw_input: Option<serde_json::Value>,
) -> serde_json::Value {
    let mut update = json!({
        "sessionUpdate": "tool_call",
        "toolCallId": tool_call_id,
        "title": title,
        "status": "in_progress"
    });
    if let Some(input) = raw_input {
        update["rawInput"] = input;
    }
    json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "sessionId": session_id,
            "update": update
        }
    })
}

fn tool_call_update_completed(
    session_id: &str,
    tool_call_id: &str,
    raw_output: Option<serde_json::Value>,
) -> serde_json::Value {
    let mut update = json!({
        "sessionUpdate": "tool_call_update",
        "toolCallId": tool_call_id,
        "status": "completed"
    });
    if let Some(output) = raw_output {
        update["rawOutput"] = output;
    }
    json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "sessionId": session_id,
            "update": update
        }
    })
}

fn tool_call_update_failed(
    session_id: &str,
    tool_call_id: &str,
    raw_output: Option<serde_json::Value>,
) -> serde_json::Value {
    let mut update = json!({
        "sessionUpdate": "tool_call_update",
        "toolCallId": tool_call_id,
        "status": "failed"
    });
    if let Some(output) = raw_output {
        update["rawOutput"] = output;
    }
    json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "sessionId": session_id,
            "update": update
        }
    })
}

fn text_chunk(session_id: &str, text: &str) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "sessionId": session_id,
            "update": {
                "sessionUpdate": "agent_message_chunk",
                "content": {"type": "text", "text": text}
            }
        }
    })
}

fn final_response(request_id: u64) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "result": {"stopReason": "end_turn", "_meta": null}
    })
}

#[tokio::test]
async fn tool_start_with_arguments_emits_chunk_with_args() {
    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(500));

    let chunks: Arc<Mutex<Vec<StreamingChunk>>> = Arc::new(Mutex::new(Vec::new()));
    let chunks_cb = Arc::clone(&chunks);

    tokio::spawn(async move {
        let mut request_line = String::new();
        agent_reader.read_line(&mut request_line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&request_line).unwrap();
        let request_id = request["id"].as_u64().unwrap();

        write_json_line(
            &mut agent_writer,
            tool_call_notification(
                "ses-tool-args",
                "tool-42",
                "mcp__crucible__semantic_search",
                Some(json!({"query": "rust async patterns", "limit": 5})),
            ),
        )
        .await
        .unwrap();

        write_json_line(&mut agent_writer, final_response(request_id))
            .await
            .unwrap();
    });

    let request = make_prompt_request("ses-tool-args", "search something");
    let (_content, tool_calls, _response) = client
        .send_prompt_with_callback(
            request,
            Box::new(move |chunk| {
                chunks_cb.lock().unwrap().push(chunk);
                true
            }),
        )
        .await
        .expect("streaming should complete");

    let captured = chunks.lock().unwrap();
    let tool_start = captured
        .iter()
        .find(|c| matches!(c, StreamingChunk::ToolStart { .. }))
        .expect("should have received ToolStart chunk");

    match tool_start {
        StreamingChunk::ToolStart {
            name,
            id,
            arguments,
        } => {
            assert_eq!(name, "Semantic Search", "MCP prefix should be humanized");
            assert_eq!(id, "tool-42");
            let args = arguments.as_ref().expect("arguments should be Some");
            assert_eq!(args["query"], "rust async patterns");
            assert_eq!(args["limit"], 5);
        }
        _ => unreachable!(),
    }

    assert!(!tool_calls.is_empty(), "should have accumulated tool calls");
    let tc = &tool_calls[0];
    assert_eq!(tc.title, "mcp__crucible__semantic_search");
    assert!(tc.arguments.is_some());
    assert_eq!(
        tc.arguments.as_ref().unwrap()["query"],
        "rust async patterns"
    );
}

#[tokio::test]
async fn tool_start_without_arguments_has_none() {
    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(500));

    let chunks: Arc<Mutex<Vec<StreamingChunk>>> = Arc::new(Mutex::new(Vec::new()));
    let chunks_cb = Arc::clone(&chunks);

    tokio::spawn(async move {
        let mut request_line = String::new();
        agent_reader.read_line(&mut request_line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&request_line).unwrap();
        let request_id = request["id"].as_u64().unwrap();

        write_json_line(
            &mut agent_writer,
            tool_call_notification("ses-no-args", "tool-99", "list_models", None),
        )
        .await
        .unwrap();

        write_json_line(&mut agent_writer, final_response(request_id))
            .await
            .unwrap();
    });

    let request = make_prompt_request("ses-no-args", "list models");
    client
        .send_prompt_with_callback(
            request,
            Box::new(move |chunk| {
                chunks_cb.lock().unwrap().push(chunk);
                true
            }),
        )
        .await
        .expect("streaming should complete");

    let captured = chunks.lock().unwrap();
    let tool_start = captured
        .iter()
        .find(|c| matches!(c, StreamingChunk::ToolStart { .. }))
        .expect("should have ToolStart");

    match tool_start {
        StreamingChunk::ToolStart { arguments, .. } => {
            assert!(
                arguments.is_none(),
                "arguments should be None when not provided"
            );
        }
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn tool_start_complex_arguments_preserved() {
    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(500));

    let chunks: Arc<Mutex<Vec<StreamingChunk>>> = Arc::new(Mutex::new(Vec::new()));
    let chunks_cb = Arc::clone(&chunks);

    let complex_args = json!({
        "path": "/home/user/project/src/main.rs",
        "options": {
            "encoding": "utf-8",
            "line_range": [10, 50],
            "include_metadata": true
        },
        "tags": ["rust", "source"],
        "nested": {"deep": {"value": 42}}
    });
    let expected_args = complex_args.clone();

    tokio::spawn(async move {
        let mut request_line = String::new();
        agent_reader.read_line(&mut request_line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&request_line).unwrap();
        let request_id = request["id"].as_u64().unwrap();

        write_json_line(
            &mut agent_writer,
            tool_call_notification("ses-complex", "tool-c1", "read_file", Some(complex_args)),
        )
        .await
        .unwrap();

        write_json_line(&mut agent_writer, final_response(request_id))
            .await
            .unwrap();
    });

    let request = make_prompt_request("ses-complex", "read file");
    client
        .send_prompt_with_callback(
            request,
            Box::new(move |chunk| {
                chunks_cb.lock().unwrap().push(chunk);
                true
            }),
        )
        .await
        .expect("streaming should complete");

    let captured = chunks.lock().unwrap();
    let tool_start = captured
        .iter()
        .find(|c| matches!(c, StreamingChunk::ToolStart { .. }))
        .expect("should have ToolStart");

    match tool_start {
        StreamingChunk::ToolStart { arguments, .. } => {
            let args = arguments
                .as_ref()
                .expect("complex args should survive roundtrip");
            assert_eq!(
                *args, expected_args,
                "nested JSON should be fully preserved"
            );
        }
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn tool_end_with_result_emits_chunk() {
    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(500));

    let chunks: Arc<Mutex<Vec<StreamingChunk>>> = Arc::new(Mutex::new(Vec::new()));
    let chunks_cb = Arc::clone(&chunks);

    tokio::spawn(async move {
        let mut request_line = String::new();
        agent_reader.read_line(&mut request_line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&request_line).unwrap();
        let request_id = request["id"].as_u64().unwrap();

        write_json_line(
            &mut agent_writer,
            tool_call_notification(
                "ses-result",
                "tool-r1",
                "read_note",
                Some(json!({"path": "README.md"})),
            ),
        )
        .await
        .unwrap();

        write_json_line(
            &mut agent_writer,
            tool_call_update_completed(
                "ses-result",
                "tool-r1",
                Some(json!("# README\n\nThis is the readme content.")),
            ),
        )
        .await
        .unwrap();

        write_json_line(&mut agent_writer, final_response(request_id))
            .await
            .unwrap();
    });

    let request = make_prompt_request("ses-result", "read readme");
    client
        .send_prompt_with_callback(
            request,
            Box::new(move |chunk| {
                chunks_cb.lock().unwrap().push(chunk);
                true
            }),
        )
        .await
        .expect("streaming should complete");

    let captured = chunks.lock().unwrap();

    let tool_start = captured
        .iter()
        .find(|c| matches!(c, StreamingChunk::ToolStart { .. }));
    assert!(tool_start.is_some(), "should have ToolStart");

    let tool_end = captured
        .iter()
        .find(|c| matches!(c, StreamingChunk::ToolEnd { .. }))
        .expect("should have ToolEnd chunk");

    match tool_end {
        StreamingChunk::ToolEnd { id, result, error } => {
            assert_eq!(id, "tool-r1");
            assert!(result.is_some(), "completed tool should have result");
            assert!(
                result.as_ref().unwrap().contains("README"),
                "result should contain the tool output"
            );
            assert!(error.is_none(), "successful tool should have no error");
        }
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn tool_end_with_error_emits_error_field() {
    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(500));

    let chunks: Arc<Mutex<Vec<StreamingChunk>>> = Arc::new(Mutex::new(Vec::new()));
    let chunks_cb = Arc::clone(&chunks);

    tokio::spawn(async move {
        let mut request_line = String::new();
        agent_reader.read_line(&mut request_line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&request_line).unwrap();
        let request_id = request["id"].as_u64().unwrap();

        write_json_line(
            &mut agent_writer,
            tool_call_notification(
                "ses-error",
                "tool-e1",
                "write_file",
                Some(json!({"path": "/protected/file.txt", "content": "test"})),
            ),
        )
        .await
        .unwrap();

        write_json_line(
            &mut agent_writer,
            tool_call_update_failed(
                "ses-error",
                "tool-e1",
                Some(json!({"error": "permission denied: /protected/file.txt"})),
            ),
        )
        .await
        .unwrap();

        write_json_line(&mut agent_writer, final_response(request_id))
            .await
            .unwrap();
    });

    let request = make_prompt_request("ses-error", "write file");
    client
        .send_prompt_with_callback(
            request,
            Box::new(move |chunk| {
                chunks_cb.lock().unwrap().push(chunk);
                true
            }),
        )
        .await
        .expect("streaming should complete");

    let captured = chunks.lock().unwrap();
    let tool_end = captured
        .iter()
        .find(|c| matches!(c, StreamingChunk::ToolEnd { .. }))
        .expect("should have ToolEnd chunk for failed tool");

    match tool_end {
        StreamingChunk::ToolEnd { id, error, .. } => {
            assert_eq!(id, "tool-e1");
            assert!(error.is_some(), "failed tool should have error");
            assert!(
                error.as_ref().unwrap().contains("permission denied"),
                "error should contain the failure message"
            );
        }
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn tool_end_failed_without_output_has_generic_error() {
    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(500));

    let chunks: Arc<Mutex<Vec<StreamingChunk>>> = Arc::new(Mutex::new(Vec::new()));
    let chunks_cb = Arc::clone(&chunks);

    tokio::spawn(async move {
        let mut request_line = String::new();
        agent_reader.read_line(&mut request_line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&request_line).unwrap();
        let request_id = request["id"].as_u64().unwrap();

        write_json_line(
            &mut agent_writer,
            tool_call_notification("ses-fail-no-out", "tool-f1", "broken_tool", None),
        )
        .await
        .unwrap();

        write_json_line(
            &mut agent_writer,
            tool_call_update_failed("ses-fail-no-out", "tool-f1", None),
        )
        .await
        .unwrap();

        write_json_line(&mut agent_writer, final_response(request_id))
            .await
            .unwrap();
    });

    let request = make_prompt_request("ses-fail-no-out", "try broken");
    client
        .send_prompt_with_callback(
            request,
            Box::new(move |chunk| {
                chunks_cb.lock().unwrap().push(chunk);
                true
            }),
        )
        .await
        .expect("streaming should complete");

    let captured = chunks.lock().unwrap();
    let tool_end = captured
        .iter()
        .find(|c| matches!(c, StreamingChunk::ToolEnd { .. }))
        .expect("should have ToolEnd for failed tool");

    match tool_end {
        StreamingChunk::ToolEnd { error, .. } => {
            assert!(
                error.is_some(),
                "failed tool with no output should still report error"
            );
            assert!(
                error.as_ref().unwrap().contains("failed"),
                "should have generic failure message, got: {:?}",
                error
            );
        }
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn stream_without_usage_data_completes_gracefully() {
    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(500));

    tokio::spawn(async move {
        let mut request_line = String::new();
        agent_reader.read_line(&mut request_line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&request_line).unwrap();
        let request_id = request["id"].as_u64().unwrap();

        write_json_line(
            &mut agent_writer,
            text_chunk("ses-no-usage", "Hello from agent"),
        )
        .await
        .unwrap();

        write_json_line(&mut agent_writer, final_response(request_id))
            .await
            .unwrap();
    });

    let request = make_prompt_request("ses-no-usage", "say hello");
    let (content, tool_calls, response) = client
        .send_prompt_with_streaming(request)
        .await
        .expect("stream should complete without crash when no usage data");

    assert!(
        content.contains("Hello from agent"),
        "content should be accumulated"
    );
    assert!(tool_calls.is_empty());
    assert_eq!(
        response.stop_reason,
        agent_client_protocol::StopReason::EndTurn
    );
}

#[tokio::test]
async fn empty_stream_no_usage_no_chunks_completes() {
    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(500));

    tokio::spawn(async move {
        let mut request_line = String::new();
        agent_reader.read_line(&mut request_line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&request_line).unwrap();
        let request_id = request["id"].as_u64().unwrap();

        write_json_line(&mut agent_writer, final_response(request_id))
            .await
            .unwrap();
    });

    let request = make_prompt_request("ses-empty", "nothing");
    let (content, tool_calls, _response) = client
        .send_prompt_with_streaming(request)
        .await
        .expect("empty stream should complete without crash");

    assert!(content.is_empty(), "no chunks = empty content");
    assert!(tool_calls.is_empty());
}

#[tokio::test]
async fn full_flow_text_tool_result_text_via_callback() {
    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(500));

    let chunks: Arc<Mutex<Vec<StreamingChunk>>> = Arc::new(Mutex::new(Vec::new()));
    let chunks_cb = Arc::clone(&chunks);

    tokio::spawn(async move {
        let mut request_line = String::new();
        agent_reader.read_line(&mut request_line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&request_line).unwrap();
        let request_id = request["id"].as_u64().unwrap();

        write_json_line(
            &mut agent_writer,
            text_chunk("ses-full", "Let me search for that. "),
        )
        .await
        .unwrap();

        write_json_line(
            &mut agent_writer,
            tool_call_notification(
                "ses-full",
                "tool-s1",
                "mcp__crucible__semantic_search",
                Some(json!({"query": "async patterns"})),
            ),
        )
        .await
        .unwrap();

        write_json_line(
            &mut agent_writer,
            tool_call_update_completed(
                "ses-full",
                "tool-s1",
                Some(json!("Found 3 relevant notes about async patterns.")),
            ),
        )
        .await
        .unwrap();

        write_json_line(
            &mut agent_writer,
            text_chunk("ses-full", "Based on the results, here is your answer."),
        )
        .await
        .unwrap();

        write_json_line(&mut agent_writer, final_response(request_id))
            .await
            .unwrap();
    });

    let request = make_prompt_request("ses-full", "search async patterns");
    let (content, tool_calls, _response) = client
        .send_prompt_with_callback(
            request,
            Box::new(move |chunk| {
                chunks_cb.lock().unwrap().push(chunk);
                true
            }),
        )
        .await
        .expect("full flow should complete");

    let captured = chunks.lock().unwrap();

    let kinds: Vec<&str> = captured
        .iter()
        .map(|c| match c {
            StreamingChunk::Text(_) => "text",
            StreamingChunk::Thinking(_) => "thinking",
            StreamingChunk::ToolStart { .. } => "tool_start",
            StreamingChunk::ToolEnd { .. } => "tool_end",
        })
        .collect();

    assert_eq!(
        kinds,
        vec!["text", "tool_start", "tool_end", "text"],
        "chunks should arrive in order: text → tool_start → tool_end → text"
    );

    assert!(content.contains("Let me search"));
    assert!(content.contains("here is your answer"));

    assert_eq!(tool_calls.len(), 1);
    assert!(tool_calls[0]
        .arguments
        .as_ref()
        .unwrap()
        .get("query")
        .is_some());
}

#[test]
fn streaming_chunk_variants_roundtrip_via_json() {
    fn roundtrip(chunk: &StreamingChunk) -> StreamingChunk {
        let serialized = match chunk {
            StreamingChunk::Text(text) => json!({"kind": "text", "text": text}),
            StreamingChunk::Thinking(text) => json!({"kind": "thinking", "text": text}),
            StreamingChunk::ToolStart {
                name,
                id,
                arguments,
            } => json!({"kind": "tool_start", "name": name, "id": id, "arguments": arguments}),
            StreamingChunk::ToolEnd { id, result, error } => {
                json!({"kind": "tool_end", "id": id, "result": result, "error": error})
            }
        };

        let kind = serialized["kind"].as_str().unwrap();
        match kind {
            "text" => StreamingChunk::Text(serialized["text"].as_str().unwrap().to_string()),
            "thinking" => {
                StreamingChunk::Thinking(serialized["text"].as_str().unwrap().to_string())
            }
            "tool_start" => StreamingChunk::ToolStart {
                name: serialized["name"].as_str().unwrap().to_string(),
                id: serialized["id"].as_str().unwrap().to_string(),
                arguments: serialized
                    .get("arguments")
                    .cloned()
                    .filter(|v| !v.is_null()),
            },
            "tool_end" => StreamingChunk::ToolEnd {
                id: serialized["id"].as_str().unwrap().to_string(),
                result: serialized
                    .get("result")
                    .and_then(|v| v.as_str().map(str::to_string)),
                error: serialized
                    .get("error")
                    .and_then(|v| v.as_str().map(str::to_string)),
            },
            _ => panic!("unknown kind"),
        }
    }

    let fixtures = vec![
        StreamingChunk::Text("hello world".to_string()),
        StreamingChunk::Thinking("let me reason about this".to_string()),
        StreamingChunk::ToolStart {
            name: "semantic_search".to_string(),
            id: "tool-1".to_string(),
            arguments: Some(json!({"query": "rust", "limit": 10})),
        },
        StreamingChunk::ToolStart {
            name: "list_models".to_string(),
            id: "tool-2".to_string(),
            arguments: None,
        },
        StreamingChunk::ToolEnd {
            id: "tool-1".to_string(),
            result: Some("found 5 results".to_string()),
            error: None,
        },
        StreamingChunk::ToolEnd {
            id: "tool-3".to_string(),
            result: None,
            error: Some("timeout".to_string()),
        },
    ];

    for chunk in &fixtures {
        let rt = roundtrip(chunk);
        match (chunk, &rt) {
            (StreamingChunk::Text(a), StreamingChunk::Text(b)) => assert_eq!(a, b),
            (StreamingChunk::Thinking(a), StreamingChunk::Thinking(b)) => assert_eq!(a, b),
            (
                StreamingChunk::ToolStart {
                    name: a_n,
                    id: a_i,
                    arguments: a_a,
                },
                StreamingChunk::ToolStart {
                    name: b_n,
                    id: b_i,
                    arguments: b_a,
                },
            ) => {
                assert_eq!(a_n, b_n);
                assert_eq!(a_i, b_i);
                assert_eq!(a_a, b_a);
            }
            (
                StreamingChunk::ToolEnd {
                    id: a_i,
                    result: a_r,
                    error: a_e,
                },
                StreamingChunk::ToolEnd {
                    id: b_i,
                    result: b_r,
                    error: b_e,
                },
            ) => {
                assert_eq!(a_i, b_i);
                assert_eq!(a_r, b_r);
                assert_eq!(a_e, b_e);
            }
            _ => panic!("variant mismatch after roundtrip"),
        }
    }
}

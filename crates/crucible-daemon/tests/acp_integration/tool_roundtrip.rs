//! Tool round-trip integration tests — verifies that tool call notifications
//! flow correctly through the ACP streaming pipeline and that tool results
//! are captured in the accumulated output.

use crucible_daemon::acp::client::{ClientConfig, CrucibleAcpClient};
use crucible_daemon::acp::StreamingChunk;
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};

fn test_config(timeout_ms: Option<u64>) -> ClientConfig {
    ClientConfig {
        agent_path: PathBuf::from("mock-tool-roundtrip"),
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

/// Verifies the full tool round-trip: agent calls read_file, the client receives
/// ToolStart and ToolEnd chunks with the correct tool name, arguments, and result.
#[tokio::test]
async fn test_acp_tool_roundtrip_read_file() {
    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(5000));

    let chunks: Arc<Mutex<Vec<StreamingChunk>>> = Arc::new(Mutex::new(Vec::new()));
    let chunks_cb = Arc::clone(&chunks);

    let session_id = "ses-roundtrip-read";

    tokio::spawn(async move {
        // Agent reads the prompt request
        let mut request_line = String::new();
        agent_reader.read_line(&mut request_line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&request_line).unwrap();
        let request_id = request["id"].as_u64().unwrap();

        // Agent emits initial text
        write_json_line(
            &mut agent_writer,
            text_chunk(session_id, "Let me read that file for you. "),
        )
        .await
        .unwrap();

        // Agent calls read_file tool
        write_json_line(
            &mut agent_writer,
            tool_call_notification(
                session_id,
                "tc-read-1",
                "read_file",
                Some(json!({"path": "/tmp/test.md"})),
            ),
        )
        .await
        .unwrap();

        // Tool completes with file content
        write_json_line(
            &mut agent_writer,
            tool_call_update_completed(
                session_id,
                "tc-read-1",
                Some(json!("# Test File\n\nThis is the content of the file.")),
            ),
        )
        .await
        .unwrap();

        // Agent emits post-tool text
        write_json_line(
            &mut agent_writer,
            text_chunk(session_id, "The file contains a heading and a paragraph."),
        )
        .await
        .unwrap();

        // Final response
        write_json_line(&mut agent_writer, final_response(request_id))
            .await
            .unwrap();
    });

    let request = make_prompt_request(session_id, "read /tmp/test.md");
    let (content, tool_calls, response) = client
        .send_prompt_with_callback(
            request,
            Box::new(move |chunk| {
                chunks_cb.lock().unwrap().push(chunk);
                true
            }),
        )
        .await
        .expect("tool roundtrip should complete");

    // Verify chunk ordering: text -> tool_start -> tool_end -> text
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
        "chunks should arrive in order: text -> tool_start -> tool_end -> text"
    );

    // Verify ToolStart has correct name, id, and arguments
    let tool_start = captured
        .iter()
        .find(|c| matches!(c, StreamingChunk::ToolStart { .. }))
        .expect("should have ToolStart chunk");

    match tool_start {
        StreamingChunk::ToolStart {
            name,
            id,
            arguments,
            ..
        } => {
            assert_eq!(name, "Read File", "tool name should be humanized");
            assert_eq!(id, "tc-read-1");
            let args = arguments.as_ref().expect("arguments should be present");
            assert_eq!(args["path"], "/tmp/test.md");
        }
        _ => unreachable!(),
    }

    // Verify ToolEnd has result content
    let tool_end = captured
        .iter()
        .find(|c| matches!(c, StreamingChunk::ToolEnd { .. }))
        .expect("should have ToolEnd chunk");

    match tool_end {
        StreamingChunk::ToolEnd { id, result, error } => {
            assert_eq!(id, "tc-read-1");
            let result_text = result.as_ref().expect("completed tool should have result");
            assert!(
                result_text.contains("Test File"),
                "result should contain file content"
            );
            assert!(error.is_none(), "successful tool should have no error");
        }
        _ => unreachable!(),
    }

    // Verify accumulated content includes text chunks
    assert!(content.contains("Let me read that file"));
    assert!(content.contains("heading and a paragraph"));

    // Verify tool_calls accumulator
    assert_eq!(tool_calls.len(), 1, "should have one tool call");
    assert_eq!(tool_calls[0].title, "read_file");
    assert!(tool_calls[0].arguments.is_some());
    assert_eq!(
        tool_calls[0].arguments.as_ref().unwrap()["path"],
        "/tmp/test.md"
    );

    // Verify stop reason
    assert_eq!(
        response.stop_reason,
        agent_client_protocol::StopReason::EndTurn
    );
}

/// Verifies that multiple sequential tool calls in a single turn are all captured
/// and arrive in the correct order.
#[tokio::test]
async fn test_acp_tool_roundtrip_multiple_tools() {
    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(5000));

    let chunks: Arc<Mutex<Vec<StreamingChunk>>> = Arc::new(Mutex::new(Vec::new()));
    let chunks_cb = Arc::clone(&chunks);

    let session_id = "ses-roundtrip-multi";

    tokio::spawn(async move {
        let mut request_line = String::new();
        agent_reader.read_line(&mut request_line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&request_line).unwrap();
        let request_id = request["id"].as_u64().unwrap();

        // First tool call: semantic_search
        write_json_line(
            &mut agent_writer,
            tool_call_notification(
                session_id,
                "tc-search-1",
                "mcp__crucible__semantic_search",
                Some(json!({"query": "async patterns", "limit": 3})),
            ),
        )
        .await
        .unwrap();

        write_json_line(
            &mut agent_writer,
            tool_call_update_completed(
                session_id,
                "tc-search-1",
                Some(json!("Found 3 notes about async patterns.")),
            ),
        )
        .await
        .unwrap();

        // Text between tools
        write_json_line(
            &mut agent_writer,
            text_chunk(session_id, "Let me also check the config. "),
        )
        .await
        .unwrap();

        // Second tool call: read_file
        write_json_line(
            &mut agent_writer,
            tool_call_notification(
                session_id,
                "tc-read-2",
                "read_file",
                Some(json!({"path": "/home/user/config.toml"})),
            ),
        )
        .await
        .unwrap();

        write_json_line(
            &mut agent_writer,
            tool_call_update_completed(
                session_id,
                "tc-read-2",
                Some(json!("[settings]\ntheme = \"dark\"")),
            ),
        )
        .await
        .unwrap();

        // Final text
        write_json_line(
            &mut agent_writer,
            text_chunk(session_id, "Done reviewing both sources."),
        )
        .await
        .unwrap();

        write_json_line(&mut agent_writer, final_response(request_id))
            .await
            .unwrap();
    });

    let request = make_prompt_request(session_id, "search and read config");
    let (content, tool_calls, _response) = client
        .send_prompt_with_callback(
            request,
            Box::new(move |chunk| {
                chunks_cb.lock().unwrap().push(chunk);
                true
            }),
        )
        .await
        .expect("multi-tool roundtrip should complete");

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
        vec![
            "tool_start",
            "tool_end",
            "text",
            "tool_start",
            "tool_end",
            "text"
        ],
        "two tool calls with interleaved text"
    );

    // Verify both tool calls captured
    assert_eq!(tool_calls.len(), 2, "should have two tool calls");
    assert_eq!(tool_calls[0].title, "mcp__crucible__semantic_search");
    assert_eq!(tool_calls[1].title, "read_file");

    // Verify content accumulates text from between and after tools
    assert!(content.contains("check the config"));
    assert!(content.contains("Done reviewing"));
}

/// Verifies tool round-trip with an in-process MCP host: the agent calls a tool,
/// the MCP server has real tools available, and the client receives correctly
/// structured ToolStart/ToolEnd chunks.
///
/// This uses the custom transport pattern (not a real agent process) combined
/// with a real MCP host to verify the tool listing works end-to-end.
#[tokio::test]
async fn test_acp_tool_roundtrip_with_mcp_server() {
    use crucible_core::enrichment::EmbeddingProvider;
    use crucible_core::traits::KnowledgeRepository;
    use crucible_daemon::test_support::{MockEmbeddingProvider, MockKnowledgeRepository};
    use crucible_daemon::InProcessMcpHost;
    use std::sync::Arc;
    use tempfile::TempDir;

    // Set up a temp kiln with a test note
    let temp = TempDir::new().unwrap();
    let note_path = temp.path().join("test-note.md");
    std::fs::write(
        &note_path,
        "---\ntitle: Test Note\ntags: [rust, async]\n---\n\n# Test Note\n\nThis is a test note for tool roundtrip.",
    )
    .unwrap();

    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

    let host = match InProcessMcpHost::start(
        temp.path().to_path_buf(),
        knowledge_repo,
        embedding_provider,
        None,
    )
    .await
    {
        Ok(host) => host,
        Err(e) => {
            let err_str = format!("{}", e);
            if err_str.contains("Operation not permitted") {
                eprintln!("SKIP: MCP host cannot bind in sandbox: {}", e);
                return;
            }
            panic!("Failed to start MCP host: {:?}", e);
        }
    };

    let mcp_url = host.mcp_url();

    // Verify the MCP server is running and has tools
    let http_client = reqwest::Client::new();

    // Initialize MCP session
    let init_resp = http_client
        .post(&mcp_url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test-roundtrip","version":"0.1.0"}}}"#)
        .send()
        .await
        .expect("MCP initialize should succeed");

    assert!(
        init_resp.status().is_success(),
        "MCP init failed: {}",
        init_resp.status()
    );

    let session_id_header = init_resp
        .headers()
        .get("mcp-session-id")
        .expect("should have session id")
        .to_str()
        .unwrap()
        .to_string();

    // Send initialized notification
    let _notif = http_client
        .post(&mcp_url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("Mcp-Session-Id", &session_id_header)
        .body(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
        .send()
        .await
        .expect("initialized notification should succeed");

    // List tools to verify they exist
    let tools_resp = http_client
        .post(&mcp_url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("Mcp-Session-Id", &session_id_header)
        .body(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#)
        .send()
        .await
        .expect("tools/list should succeed");

    let body = tools_resp.text().await.unwrap();

    // Parse SSE format
    let json_str = body
        .lines()
        .find(|line| line.starts_with("data: {"))
        .and_then(|line| line.strip_prefix("data: "))
        .expect("should find data line with JSON");

    let parsed: serde_json::Value = serde_json::from_str(json_str).expect("should be valid JSON");
    let tools = parsed["result"]["tools"]
        .as_array()
        .expect("should have tools array");

    let tool_names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();

    // Verify list_notes is among the available tools
    assert!(
        tool_names.contains(&"list_notes"),
        "MCP server should expose list_notes tool, got: {:?}",
        tool_names
    );

    // Now test the ACP client side: create a custom transport client and simulate
    // an agent that calls a tool. The tool call here is simulated (the agent side
    // sends tool_call notifications), but the MCP server is real and verified above.
    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(5000));

    let chunks: Arc<Mutex<Vec<StreamingChunk>>> = Arc::new(Mutex::new(Vec::new()));
    let chunks_cb = Arc::clone(&chunks);

    let acp_session_id = "ses-mcp-roundtrip";

    tokio::spawn(async move {
        let mut request_line = String::new();
        agent_reader.read_line(&mut request_line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&request_line).unwrap();
        let request_id = request["id"].as_u64().unwrap();

        // Agent calls list_notes via MCP
        write_json_line(
            &mut agent_writer,
            tool_call_notification(
                acp_session_id,
                "tc-list-1",
                "mcp__crucible__list_notes",
                Some(json!({"limit": 10})),
            ),
        )
        .await
        .unwrap();

        // Simulate tool result (in a real scenario the MCP server would execute this)
        write_json_line(
            &mut agent_writer,
            tool_call_update_completed(
                acp_session_id,
                "tc-list-1",
                Some(json!("Notes found:\n- test-note.md")),
            ),
        )
        .await
        .unwrap();

        write_json_line(
            &mut agent_writer,
            text_chunk(acp_session_id, "I found the test note in your kiln."),
        )
        .await
        .unwrap();

        write_json_line(&mut agent_writer, final_response(request_id))
            .await
            .unwrap();
    });

    let request = make_prompt_request(acp_session_id, "list my notes");
    let (content, tool_calls, _response) = client
        .send_prompt_with_callback(
            request,
            Box::new(move |chunk| {
                chunks_cb.lock().unwrap().push(chunk);
                true
            }),
        )
        .await
        .expect("MCP tool roundtrip should complete");

    {
        let captured = chunks.lock().unwrap();

        // Verify ToolStart arrived with correct MCP-prefixed tool name
        let tool_start = captured
            .iter()
            .find(|c| matches!(c, StreamingChunk::ToolStart { .. }))
            .expect("should have ToolStart chunk");

        match tool_start {
            StreamingChunk::ToolStart {
                name,
                id,
                arguments,
                ..
            } => {
                // MCP-prefixed names get humanized
                assert_eq!(name, "List Notes", "MCP tool name should be humanized");
                assert_eq!(id, "tc-list-1");
                let args = arguments.as_ref().expect("arguments should be present");
                assert_eq!(args["limit"], 10);
            }
            _ => unreachable!(),
        }

        // Verify ToolEnd has the simulated result
        let tool_end = captured
            .iter()
            .find(|c| matches!(c, StreamingChunk::ToolEnd { .. }))
            .expect("should have ToolEnd chunk");

        match tool_end {
            StreamingChunk::ToolEnd { id, result, error } => {
                assert_eq!(id, "tc-list-1");
                let result_text = result.as_ref().expect("should have result");
                assert!(
                    result_text.contains("test-note"),
                    "result should mention the test note"
                );
                assert!(error.is_none());
            }
            _ => unreachable!(),
        }
    }

    // Verify accumulated state
    assert!(content.contains("found the test note"));
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].title, "mcp__crucible__list_notes");

    host.shutdown().await;
}

/// Verifies that a tool call followed by agent text referencing the result
/// produces correct content accumulation — the text after a tool should
/// appear in the final content string.
#[tokio::test]
async fn test_acp_tool_roundtrip_content_after_tool() {
    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(5000));

    let session_id = "ses-roundtrip-after";

    tokio::spawn(async move {
        let mut request_line = String::new();
        agent_reader.read_line(&mut request_line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&request_line).unwrap();
        let request_id = request["id"].as_u64().unwrap();

        // Tool call with no preceding text
        write_json_line(
            &mut agent_writer,
            tool_call_notification(
                session_id,
                "tc-grep-1",
                "grep",
                Some(json!({"pattern": "fn main", "path": "/src"})),
            ),
        )
        .await
        .unwrap();

        write_json_line(
            &mut agent_writer,
            tool_call_update_completed(
                session_id,
                "tc-grep-1",
                Some(json!("src/main.rs:1:fn main() {")),
            ),
        )
        .await
        .unwrap();

        // Text referencing the tool result
        write_json_line(
            &mut agent_writer,
            text_chunk(
                session_id,
                "The main function is defined at line 1 of src/main.rs.",
            ),
        )
        .await
        .unwrap();

        write_json_line(&mut agent_writer, final_response(request_id))
            .await
            .unwrap();
    });

    let request = make_prompt_request(session_id, "find main function");
    let (content, tool_calls, _response) = client
        .send_prompt_with_streaming(request)
        .await
        .expect("content-after-tool roundtrip should complete");

    assert!(
        content.contains("main function is defined at line 1"),
        "content after tool call should be in accumulated output, got: {}",
        content
    );
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].title, "grep");
}

use crate::support::{MockStdioAgentConfig, ThreadedMockAgent};
use agent_client_protocol::PromptRequest;
use crucible_acp::client::{ClientConfig, CrucibleAcpClient};
use crucible_acp::discovery::{clear_agent_cache, discover_agent};
use crucible_acp::{StreamConfig, StreamHandler, StreamingChunk};
use crucible_config::AcpConfig;
use once_cell::sync::Lazy;
use serde_json::json;
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};
use tokio::sync::{oneshot, Barrier, Mutex};

const MAX_SUBAGENT_OUTPUT: usize = 10 * 1024 * 1024;
static AGENT_CACHE_TEST_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

fn test_config(timeout_ms: Option<u64>) -> ClientConfig {
    ClientConfig {
        agent_path: PathBuf::from("mock-threaded-agent"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms,
        max_retries: Some(1),
    }
}

fn make_prompt_request(session_id: &str, text: &str) -> PromptRequest {
    serde_json::from_value(json!({
        "sessionId": session_id,
        "prompt": [{"type": "text", "text": text}],
        "_meta": null
    }))
    .expect("valid prompt request")
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

async fn write_json_line(
    writer: &mut tokio::io::WriteHalf<DuplexStream>,
    value: serde_json::Value,
) -> std::io::Result<()> {
    writer
        .write_all(format!("{}\n", serde_json::to_string(&value).unwrap()).as_bytes())
        .await?;
    writer.flush().await
}

fn session_update(session_id: &str, text: &str) -> serde_json::Value {
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

#[derive(Debug, Clone)]
struct EnvVarGuard {
    key: &'static str,
    old: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: String) -> Self {
        let old = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self { key, old }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(old) = self.old.clone() {
            std::env::set_var(self.key, old);
        } else {
            std::env::remove_var(self.key);
        }
    }
}

fn serialize_chunk(chunk: &StreamingChunk) -> serde_json::Value {
    match chunk {
        StreamingChunk::Text(text) => json!({"kind": "text", "text": text}),
        StreamingChunk::Thinking(text) => json!({"kind": "thinking", "text": text}),
        StreamingChunk::ToolStart { name, id } => {
            json!({"kind": "tool_start", "name": name, "id": id})
        }
        StreamingChunk::ToolEnd { id } => json!({"kind": "tool_end", "id": id}),
    }
}

fn deserialize_chunk(value: &serde_json::Value) -> StreamingChunk {
    let kind = value["kind"].as_str().expect("kind field is required");
    match kind {
        "text" => StreamingChunk::Text(value["text"].as_str().unwrap().to_string()),
        "thinking" => StreamingChunk::Thinking(value["text"].as_str().unwrap().to_string()),
        "tool_start" => StreamingChunk::ToolStart {
            name: value["name"].as_str().unwrap().to_string(),
            id: value["id"].as_str().unwrap().to_string(),
        },
        "tool_end" => StreamingChunk::ToolEnd {
            id: value["id"].as_str().unwrap().to_string(),
        },
        other => panic!("unknown chunk kind: {other}"),
    }
}

fn assert_chunk_eq(left: &StreamingChunk, right: &StreamingChunk) {
    match (left, right) {
        (StreamingChunk::Text(a), StreamingChunk::Text(b)) => assert_eq!(a, b),
        (StreamingChunk::Thinking(a), StreamingChunk::Thinking(b)) => assert_eq!(a, b),
        (
            StreamingChunk::ToolStart { name: a_name, id: a_id },
            StreamingChunk::ToolStart { name: b_name, id: b_id },
        ) => {
            assert_eq!(a_name, b_name);
            assert_eq!(a_id, b_id);
        }
        (StreamingChunk::ToolEnd { id: a }, StreamingChunk::ToolEnd { id: b }) => {
            assert_eq!(a, b)
        }
        _ => panic!("chunk variants differ: left={left:?} right={right:?}"),
    }
}

#[tokio::test]
async fn concurrent_dual_sessions_isolated_no_cross_contamination() {
    let (mut client_a, _handle_a) = ThreadedMockAgent::spawn_with_client(MockStdioAgentConfig::opencode());
    let (mut client_b, _handle_b) = ThreadedMockAgent::spawn_with_client(MockStdioAgentConfig::opencode());

    let (session_a, session_b) = tokio::join!(client_a.connect_with_handshake(), client_b.connect_with_handshake());

    let session_a = session_a.expect("session A should connect");
    let session_b = session_b.expect("session B should connect");

    assert_ne!(session_a.id(), session_b.id(), "sessions must be distinct");
    assert!(session_a.id().starts_with("mock-session-"));
    assert!(session_b.id().starts_with("mock-session-"));
}

#[tokio::test]
async fn concurrent_agent_cache_isolation_with_clear_boundaries() {
    let _guard = AGENT_CACHE_TEST_LOCK.lock().await;
    clear_agent_cache();

    let temp = TempDir::new().expect("temp dir");
    let fake_agent_path = temp.path().join("opencode");
    std::fs::write(
        &fake_agent_path,
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then\n  echo fake-opencode-1.0\n  exit 0\nfi\nexit 0\n",
    )
    .expect("write fake agent script");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&fake_agent_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&fake_agent_path, perms).unwrap();
    }

    let old_path = std::env::var("PATH").unwrap_or_default();
    let merged = format!("{}:{}", temp.path().display(), old_path);
    let _path_guard = EnvVarGuard::set("PATH", merged);

    let config = AcpConfig::default();
    let discovered = discover_agent(None, &config)
        .await
        .expect("discover should succeed with fake PATH");
    assert_eq!(discovered.name, "opencode");
    assert_eq!(discovered.command, "opencode");

    clear_agent_cache();
    let discovered_again = discover_agent(None, &config)
        .await
        .expect("discover should still succeed after clear");
    assert_eq!(discovered_again.name, "opencode");

    clear_agent_cache();
}

#[tokio::test]
async fn stream_edge_chunk_ordering_preserved_per_stream_with_parallel_streams() {
    let (mut client_a, mut agent_a_reader, mut agent_a_writer) = client_with_custom_transport(Some(300));
    let (mut client_b, mut agent_b_reader, mut agent_b_writer) = client_with_custom_transport(Some(300));
    let barrier = std::sync::Arc::new(Barrier::new(3));

    let barrier_a = barrier.clone();
    let agent_task_a = tokio::spawn(async move {
        let mut request_line = String::new();
        agent_a_reader.read_line(&mut request_line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&request_line).unwrap();
        let request_id = request["id"].as_u64().unwrap();

        barrier_a.wait().await;
        for chunk in ["A-1", "A-2", "A-3"] {
            write_json_line(&mut agent_a_writer, session_update("session-a", chunk))
                .await
                .unwrap();
        }
        write_json_line(&mut agent_a_writer, final_response(request_id))
            .await
            .unwrap();
    });

    let barrier_b = barrier.clone();
    let agent_task_b = tokio::spawn(async move {
        let mut request_line = String::new();
        agent_b_reader.read_line(&mut request_line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&request_line).unwrap();
        let request_id = request["id"].as_u64().unwrap();

        barrier_b.wait().await;
        for chunk in ["B-1", "B-2", "B-3"] {
            write_json_line(&mut agent_b_writer, session_update("session-b", chunk))
                .await
                .unwrap();
        }
        write_json_line(&mut agent_b_writer, final_response(request_id))
            .await
            .unwrap();
    });

    let seen_a = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let seen_b = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));

    let seen_a_cb = seen_a.clone();
    let stream_a = tokio::spawn(async move {
        let request = make_prompt_request("session-a", "stream A");
        client_a
            .send_prompt_with_callback(
                request,
                Box::new(move |chunk| {
                    if let StreamingChunk::Text(text) = chunk {
                        seen_a_cb.lock().unwrap().push(text);
                    }
                    true
                }),
            )
            .await
            .unwrap()
            .0
    });

    let seen_b_cb = seen_b.clone();
    let stream_b = tokio::spawn(async move {
        let request = make_prompt_request("session-b", "stream B");
        client_b
            .send_prompt_with_callback(
                request,
                Box::new(move |chunk| {
                    if let StreamingChunk::Text(text) = chunk {
                        seen_b_cb.lock().unwrap().push(text);
                    }
                    true
                }),
            )
            .await
            .unwrap()
            .0
    });

    barrier.wait().await;

    let (content_a, content_b) = tokio::join!(stream_a, stream_b);
    let content_a = content_a.unwrap();
    let content_b = content_b.unwrap();

    assert_eq!(content_a, "A-1A-2A-3");
    assert_eq!(content_b, "B-1B-2B-3");
    assert_eq!(&*seen_a.lock().unwrap(), &["A-1", "A-2", "A-3"]);
    assert_eq!(&*seen_b.lock().unwrap(), &["B-1", "B-2", "B-3"]);

    let (_a, _b) = tokio::join!(agent_task_a, agent_task_b);
}

#[tokio::test]
async fn stream_edge_cancel_mid_stream_aborts_and_closes_transport() {
    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(300));
    let (first_chunk_tx, first_chunk_rx) = oneshot::channel::<()>();
    let (continue_tx, continue_rx) = oneshot::channel::<()>();
    let (cleanup_tx, cleanup_rx) = oneshot::channel::<bool>();

    tokio::spawn(async move {
        let mut request_line = String::new();
        agent_reader.read_line(&mut request_line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&request_line).unwrap();

        let request_id = request["id"].as_u64().unwrap();
        write_json_line(&mut agent_writer, session_update("cancel-session", "first"))
            .await
            .unwrap();
        let _ = first_chunk_tx.send(());

        let _ = continue_rx.await;

        let second_write_err = write_json_line(
            &mut agent_writer,
            session_update("cancel-session", "second-after-cancel"),
        )
        .await
        .is_err();

        let final_write_err = write_json_line(&mut agent_writer, final_response(request_id))
            .await
            .is_err();

        let _ = cleanup_tx.send(second_write_err || final_write_err);
    });

    let stream_task = tokio::spawn(async move {
        let request = make_prompt_request("cancel-session", "cancel me");
        client
            .send_prompt_with_callback(
                request,
                Box::new(|_| true),
            )
            .await
    });

    first_chunk_rx.await.expect("first chunk should arrive");
    stream_task.abort();
    let _ = continue_tx.send(());

    let transport_closed = cleanup_rx
        .await
        .expect("cleanup signal should be sent by agent");
    assert!(transport_closed, "aborted stream should close transport and fail agent writes");
}

#[test]
fn stream_edge_stream_config_respects_show_thoughts_toggle() {
    let with_thoughts = StreamHandler::new(StreamConfig {
        show_thoughts: true,
        show_tool_calls: true,
        use_colors: false,
    });
    let without_thoughts = StreamHandler::new(StreamConfig {
        show_thoughts: false,
        show_tool_calls: true,
        use_colors: false,
    });

    assert!(with_thoughts
        .format_thought_chunk("reasoning")
        .unwrap()
        .is_some());
    assert!(without_thoughts
        .format_thought_chunk("reasoning")
        .unwrap()
        .is_none());
}

#[test]
fn stream_edge_stream_config_respects_show_tool_calls_toggle() {
    let with_tools = StreamHandler::new(StreamConfig {
        show_thoughts: true,
        show_tool_calls: true,
        use_colors: false,
    });
    let without_tools = StreamHandler::new(StreamConfig {
        show_thoughts: true,
        show_tool_calls: false,
        use_colors: false,
    });

    let params = json!({"path": "demo.md"});
    assert!(with_tools.format_tool_call("read_note", &params).unwrap().is_some());
    assert!(without_tools
        .format_tool_call("read_note", &params)
        .unwrap()
        .is_none());
}

#[test]
fn stream_edge_streaming_chunk_round_trip_variants() {
    let fixtures = vec![
        StreamingChunk::Text("text chunk".to_string()),
        StreamingChunk::Thinking("thinking chunk".to_string()),
        StreamingChunk::ToolStart {
            name: "read_note".to_string(),
            id: "tool-1".to_string(),
        },
        StreamingChunk::ToolEnd {
            id: "tool-1".to_string(),
        },
    ];

    for chunk in fixtures {
        let encoded = serialize_chunk(&chunk);
        let decoded = deserialize_chunk(&encoded);
        assert_chunk_eq(&chunk, &decoded);
    }
}

#[tokio::test]
async fn stream_edge_large_response_near_max_output_is_accumulated() {
    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(1_000));

    let large_text = "x".repeat(MAX_SUBAGENT_OUTPUT - 4096);
    let expected_len = large_text.len();

    tokio::spawn(async move {
        let mut request_line = String::new();
        agent_reader.read_line(&mut request_line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&request_line).unwrap();
        let request_id = request["id"].as_u64().unwrap();

        write_json_line(
            &mut agent_writer,
            session_update("large-session", &large_text),
        )
        .await
        .unwrap();
        write_json_line(&mut agent_writer, final_response(request_id))
            .await
            .unwrap();
    });

    let request = make_prompt_request("large-session", "big stream");
    let (content, tool_calls, _response) = client
        .send_prompt_with_streaming(request)
        .await
        .expect("large streaming response should succeed");

    assert_eq!(content.len(), expected_len);
    assert!(content.starts_with('x'));
    assert!(content.ends_with('x'));
    assert!(tool_calls.is_empty());
}

#[tokio::test]
async fn stream_edge_empty_response_returns_empty_content() {
    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(200));

    tokio::spawn(async move {
        let mut request_line = String::new();
        agent_reader.read_line(&mut request_line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&request_line).unwrap();
        let request_id = request["id"].as_u64().unwrap();

        write_json_line(&mut agent_writer, final_response(request_id))
            .await
            .unwrap();
    });

    let request = make_prompt_request("empty-session", "respond with nothing");
    let (content, tool_calls, _response) = client
        .send_prompt_with_streaming(request)
        .await
        .expect("empty response should still complete");

    assert!(content.is_empty(), "no chunks should produce empty content");
    assert!(tool_calls.is_empty());
}

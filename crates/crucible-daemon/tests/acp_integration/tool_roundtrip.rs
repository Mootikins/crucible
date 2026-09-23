//! Tool round-trip integration tests — verifies that tool call notifications
//! flow correctly through the ACP streaming pipeline and that tool results
//! are captured in the accumulated output.

use crate::support::mcp_http::{mcp_http_open_session, mcp_http_request};
use crate::support::mock_agent::{make_prompt_request, tool_call, tool_call_update};
use crate::support::parity::capture_chunks;
use crate::support::{connect, logged, prompt_with, MockScript, Step};
use crucible_daemon::acp::StreamingChunk;
use serde_json::json;
use std::sync::Arc;

/// Verifies the full tool round-trip: agent calls read_file, the client receives
/// ToolStart and ToolEnd chunks with the correct tool name, arguments, and result.
#[tokio::test]
async fn test_acp_tool_roundtrip_read_file() {
    let (chunks, callback) = capture_chunks();

    let session_id = "ses-roundtrip-read";

    let (client, _agent) = connect(
        MockScript {
            turn: vec![
                // Agent emits initial text
                Step::Text("Let me read that file for you. ".into()),
                // Agent calls read_file tool
                tool_call(
                    "tc-read-1",
                    "read_file",
                    Some(json!({"path": "/tmp/test.md"})),
                ),
                // Tool completes with file content
                tool_call_update(
                    "tc-read-1",
                    "completed",
                    Some(json!("# Test File\n\nThis is the content of the file.")),
                ),
                // Agent emits post-tool text
                Step::Text("The file contains a heading and a paragraph.".into()),
            ],
            ..MockScript::default()
        },
        Some(5000),
        None,
    )
    .await;

    let request = make_prompt_request(session_id, "read /tmp/test.md");
    let (_summary, response) = prompt_with(&client, request, callback)
        .await
        .expect("tool roundtrip should complete");
    let content = crate::support::parity::text_of(&chunks.lock().unwrap());

    // Verify chunk ordering: text -> tool_start -> tool_end -> text
    let captured = chunks.lock().unwrap();
    let kinds: Vec<&str> = captured
        .iter()
        .map(crate::support::parity::chunk_kind)
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
        StreamingChunk::ToolStart { id, call } => {
            assert_eq!(
                call.tool, "tool",
                "a title is prose, so an unnamed call gets the fallback name"
            );
            assert_eq!(call.paths, ["/tmp/test.md"]);
            assert_eq!(id, "tc-read-1");
            let args = call
                .raw
                .as_ref()
                .and_then(|raw| raw.raw_input.as_ref())
                .expect("arguments should be present");
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
        StreamingChunk::ToolEnd {
            id, result, error, ..
        } => {
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

    // Verify the announced call
    let tool_start = captured
        .iter()
        .find(|c| matches!(c, StreamingChunk::ToolStart { .. }))
        .expect("should have ToolStart chunk");
    match tool_start {
        StreamingChunk::ToolStart { call, .. } => {
            assert_eq!(call.tool, "tool");
            let arguments = call
                .raw
                .as_ref()
                .and_then(|raw| raw.raw_input.as_ref())
                .expect("the call carries arguments");
            assert_eq!(arguments["path"], "/tmp/test.md");
        }
        _ => unreachable!(),
    }

    // Verify stop reason
    assert_eq!(
        response.stop_reason,
        agent_client_protocol::schema::v1::StopReason::EndTurn
    );
}

/// Verifies that multiple sequential tool calls in a single turn are all captured
/// and arrive in the correct order.
#[tokio::test]
async fn test_acp_tool_roundtrip_multiple_tools() {
    let (chunks, callback) = capture_chunks();

    let session_id = "ses-roundtrip-multi";

    let (client, _agent) = connect(
        MockScript {
            turn: vec![
                // First tool call: semantic_search
                tool_call(
                    "tc-search-1",
                    "mcp__crucible__semantic_search",
                    Some(json!({"query": "async patterns", "limit": 3})),
                ),
                tool_call_update(
                    "tc-search-1",
                    "completed",
                    Some(json!("Found 3 notes about async patterns.")),
                ),
                // Text between tools
                Step::Text("Let me also check the config. ".into()),
                // Second tool call: read_file
                tool_call(
                    "tc-read-2",
                    "read_file",
                    Some(json!({"path": "/home/user/config.toml"})),
                ),
                tool_call_update(
                    "tc-read-2",
                    "completed",
                    Some(json!("[settings]\ntheme = \"dark\"")),
                ),
                // Final text
                Step::Text("Done reviewing both sources.".into()),
            ],
            ..MockScript::default()
        },
        Some(5000),
        None,
    )
    .await;

    let request = make_prompt_request(session_id, "search and read config");
    let (summary, _response) = prompt_with(&client, request, callback)
        .await
        .expect("multi-tool roundtrip should complete");
    let content = crate::support::parity::text_of(&chunks.lock().unwrap());

    let captured = chunks.lock().unwrap();
    let kinds: Vec<&str> = captured
        .iter()
        .map(crate::support::parity::chunk_kind)
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
    assert!(summary.announced_any, "should have two tool calls");
    assert_eq!(
        crate::support::parity::tool_names_of(&captured),
        vec!["semantic_search", "tool"]
    );

    // Verify content accumulates text from between and after tools
    assert!(content.contains("check the config"));
    assert!(content.contains("Done reviewing"));
}

/// The test first asks the real in-process MCP host for the `list_notes`
/// answer. The mock agent then takes the MCP url from the `session/new`
/// frame that the client sent, and calls `list_notes` on the host. It also
/// sends the host's answer as the raw output of its tool call. The `ToolEnd`
/// chunk must carry exactly that answer.
#[tokio::test]
async fn test_acp_tool_result_from_the_real_mcp_host_reaches_tool_end() {
    use crucible_core::enrichment::EmbeddingProvider;
    use crucible_core::traits::KnowledgeRepository;
    use crucible_daemon::test_support::{MockEmbeddingProvider, MockKnowledgeRepository};
    use crucible_daemon::InProcessMcpHost;
    use tempfile::TempDir;

    let temp = TempDir::new().unwrap();
    std::fs::write(
        temp.path().join("test-note.md"),
        "---\ntitle: Test Note\ntags: [rust, async]\n---\n\n# Test Note\n\nThis is a test note for tool roundtrip.",
    )
    .unwrap();

    let knowledge_repo = Arc::new(MockKnowledgeRepository::new()) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider::new()) as Arc<dyn EmbeddingProvider>;

    let host = InProcessMcpHost::start(
        temp.path().to_path_buf(),
        temp.path().to_path_buf(),
        knowledge_repo,
        embedding_provider,
        None,
        crucible_daemon::tools::containment::RootSet::Ambient,
    )
    .await
    .expect("the in-process MCP host binds to localhost");

    let url = host.mcp_url();
    let http = reqwest::Client::new();
    let mcp_session = mcp_http_open_session(&http, &url).await;
    let reply = mcp_http_request(
        &http,
        &url,
        &mcp_session,
        2,
        "tools/call",
        json!({"name": "list_notes", "arguments": {}}),
    )
    .await;
    let tool_output = reply["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("list_notes answered with no text: {reply}"))
        .to_string();

    let log_dir = TempDir::new().unwrap();
    let log = log_dir.path().join("agent.jsonl");
    let (mut client, _agent) = connect(
        MockScript {
            turn: vec![
                tool_call("tc-list-1", "mcp__crucible__list_notes", Some(json!({}))),
                Step::McpCall {
                    tool: "list_notes".into(),
                    args: json!({}),
                },
                tool_call_update("tc-list-1", "completed", Some(json!(tool_output))),
            ],
            log: Some(log.clone()),
            ..MockScript::default()
        },
        Some(5000),
        None,
    )
    .await;

    let session = client
        .handshake(Some(&url), None)
        .await
        .expect("the mock agent completes the handshake");

    let (chunks, callback) = capture_chunks();
    let turn = prompt_with(
        &client,
        make_prompt_request(session.id(), "list my notes"),
        callback,
    )
    .await;
    let (summary, _response) = turn.expect("MCP tool roundtrip should complete");

    assert!(
        tool_output.contains("test-note"),
        "the real MCP host lists the kiln's note, got: {tool_output}"
    );

    // The agent reached the host through the url that `session/new` offered,
    // and the host gave it the same answer.
    let agent_reply = logged(&log, "mcp/result");
    let agent_reply: serde_json::Value = agent_reply
        .first()
        .and_then(serde_json::Value::as_str)
        .and_then(|text| serde_json::from_str(text).ok())
        .unwrap_or_else(|| panic!("the agent's MCP call failed: {agent_reply:?}"));
    assert_eq!(
        agent_reply["result"]["content"][0]["text"],
        tool_output.as_str(),
        "the agent's own MCP call must reach the same host"
    );

    let captured = chunks.lock().unwrap().clone();
    let tool_end = captured
        .iter()
        .find_map(|c| match c {
            StreamingChunk::ToolEnd {
                id, result, error, ..
            } => Some((id, result, error)),
            _ => None,
        })
        .expect("should have ToolEnd chunk");
    assert_eq!(tool_end.0, "tc-list-1");
    assert_eq!(
        tool_end.1.as_deref(),
        Some(tool_output.as_str()),
        "ToolEnd must carry the MCP host's answer unchanged"
    );
    assert!(tool_end.2.is_none());

    assert!(summary.announced_any);
    assert_eq!(
        crate::support::parity::tool_names_of(&captured),
        vec!["list_notes"]
    );

    host.shutdown().await;
}

/// Verifies that a tool call followed by agent text referencing the result
/// produces correct content accumulation — the text after a tool should
/// appear in the final content string.
#[tokio::test]
async fn test_acp_tool_roundtrip_content_after_tool() {
    let session_id = "ses-roundtrip-after";

    let (client, _agent) = connect(
        MockScript {
            turn: vec![
                // Tool call with no preceding text
                tool_call(
                    "tc-grep-1",
                    "grep",
                    Some(json!({"pattern": "fn main", "path": "/src"})),
                ),
                tool_call_update(
                    "tc-grep-1",
                    "completed",
                    Some(json!("src/main.rs:1:fn main() {")),
                ),
                // Text referencing the tool result
                Step::Text("The main function is defined at line 1 of src/main.rs.".into()),
            ],
            ..MockScript::default()
        },
        Some(5000),
        None,
    )
    .await;

    let request = make_prompt_request(session_id, "find main function");
    let (chunks, callback) = capture_chunks();
    let (summary, _response) = prompt_with(&client, request, callback)
        .await
        .expect("content-after-tool roundtrip should complete");
    let content = crate::support::parity::text_of(&chunks.lock().unwrap());

    assert!(
        content.contains("main function is defined at line 1"),
        "content after tool call should be in accumulated output, got: {}",
        content
    );
    assert!(summary.announced_any);
    assert_eq!(
        crate::support::parity::tool_names_of(&chunks.lock().unwrap()),
        vec!["tool"]
    );
}

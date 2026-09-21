//! Tool round-trip integration tests — verifies that tool call notifications
//! flow correctly through the ACP streaming pipeline and that tool results
//! are captured in the accumulated output.

use crate::scripted_agent::{
    client_with_custom_transport, final_response, make_prompt_request, mcp_http_open_session,
    mcp_http_request, read_frame, read_request_id, text_chunk, tool_call_notification,
    tool_call_update_completed, write_json_line,
};
use crucible_daemon::acp::StreamingChunk;
use serde_json::json;
use std::sync::{Arc, Mutex};

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
        let request_id = read_request_id(&mut agent_reader).await;

        // Agent emits initial text
        write_json_line(
            &mut agent_writer,
            text_chunk(session_id, "Let me read that file for you. "),
        )
        .await;

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
        .await;

        // Tool completes with file content
        write_json_line(
            &mut agent_writer,
            tool_call_update_completed(
                session_id,
                "tc-read-1",
                Some(json!("# Test File\n\nThis is the content of the file.")),
            ),
        )
        .await;

        // Agent emits post-tool text
        write_json_line(
            &mut agent_writer,
            text_chunk(session_id, "The file contains a heading and a paragraph."),
        )
        .await;

        // Final response
        write_json_line(&mut agent_writer, final_response(request_id)).await;
    });

    let request = make_prompt_request(session_id, "read /tmp/test.md");
    let (_summary, response) = client
        .send_prompt_with_callback(
            request,
            Box::new(move |chunk| {
                chunks_cb.lock().unwrap().push(chunk);
                true
            }),
        )
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
        StreamingChunk::ToolStart {
            name, arguments, ..
        } => {
            assert_eq!(name, "Read File");
            let arguments = arguments.as_ref().expect("the call carries arguments");
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
    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(5000));

    let chunks: Arc<Mutex<Vec<StreamingChunk>>> = Arc::new(Mutex::new(Vec::new()));
    let chunks_cb = Arc::clone(&chunks);

    let session_id = "ses-roundtrip-multi";

    tokio::spawn(async move {
        let request_id = read_request_id(&mut agent_reader).await;

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
        .await;

        write_json_line(
            &mut agent_writer,
            tool_call_update_completed(
                session_id,
                "tc-search-1",
                Some(json!("Found 3 notes about async patterns.")),
            ),
        )
        .await;

        // Text between tools
        write_json_line(
            &mut agent_writer,
            text_chunk(session_id, "Let me also check the config. "),
        )
        .await;

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
        .await;

        write_json_line(
            &mut agent_writer,
            tool_call_update_completed(
                session_id,
                "tc-read-2",
                Some(json!("[settings]\ntheme = \"dark\"")),
            ),
        )
        .await;

        // Final text
        write_json_line(
            &mut agent_writer,
            text_chunk(session_id, "Done reviewing both sources."),
        )
        .await;

        write_json_line(&mut agent_writer, final_response(request_id)).await;
    });

    let request = make_prompt_request(session_id, "search and read config");
    let (summary, _response) = client
        .send_prompt_with_callback(
            request,
            Box::new(move |chunk| {
                chunks_cb.lock().unwrap().push(chunk);
                true
            }),
        )
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
        vec!["Semantic Search", "Read File"]
    );

    // Verify content accumulates text from between and after tools
    assert!(content.contains("check the config"));
    assert!(content.contains("Done reviewing"));
}

/// The scripted agent takes the MCP url from the `session/new` frame the
/// client sent, calls `list_notes` on the real in-process MCP host, and
/// relays the host's answer as the tool call's raw output. The `ToolEnd`
/// chunk must carry exactly that answer, so the whole path is real except
/// the agent's own decision to call the tool.
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

    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(5000));
    let acp_session_id = "ses-mcp-roundtrip";

    let agent = tokio::spawn(async move {
        let init = read_frame(&mut agent_reader).await;
        assert_eq!(init["method"], "initialize");
        write_json_line(
            &mut agent_writer,
            json!({
                "jsonrpc": "2.0",
                "id": init["id"],
                "result": {
                    "protocolVersion": 1,
                    "agentCapabilities": {"mcpCapabilities": {"http": true, "sse": false}},
                    "authMethods": []
                }
            }),
        )
        .await;

        let new_session = read_frame(&mut agent_reader).await;
        assert_eq!(new_session["method"], "session/new");
        let mcp_url = new_session["params"]["mcpServers"][0]["url"]
            .as_str()
            .unwrap_or_else(|| panic!("session/new offers no HTTP MCP url: {new_session}"))
            .to_string();
        write_json_line(
            &mut agent_writer,
            json!({
                "jsonrpc": "2.0",
                "id": new_session["id"],
                "result": {"sessionId": acp_session_id}
            }),
        )
        .await;

        let prompt_request_id = read_request_id(&mut agent_reader).await;
        write_json_line(
            &mut agent_writer,
            tool_call_notification(
                acp_session_id,
                "tc-list-1",
                "mcp__crucible__list_notes",
                Some(json!({})),
            ),
        )
        .await;

        let http = reqwest::Client::new();
        let mcp_session = mcp_http_open_session(&http, &mcp_url).await;
        let reply = mcp_http_request(
            &http,
            &mcp_url,
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

        write_json_line(
            &mut agent_writer,
            tool_call_update_completed(acp_session_id, "tc-list-1", Some(json!(tool_output))),
        )
        .await;
        write_json_line(&mut agent_writer, final_response(prompt_request_id)).await;
        tool_output
    });

    client
        .connect_with_best_mcp(Some(&host.mcp_url()))
        .await
        .expect("the scripted agent completes the handshake");

    let (chunks, callback) = crate::support::parity::capture_chunks();
    let turn = client
        .send_prompt_with_callback(
            make_prompt_request(acp_session_id, "list my notes"),
            callback,
        )
        .await;
    let tool_output = agent.await.expect("the scripted agent finished its turn");
    let (summary, _response) = turn.expect("MCP tool roundtrip should complete");

    assert!(
        tool_output.contains("test-note"),
        "the real MCP host lists the kiln's note, got: {tool_output}"
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
        vec!["List Notes"]
    );

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
        let request_id = read_request_id(&mut agent_reader).await;

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
        .await;

        write_json_line(
            &mut agent_writer,
            tool_call_update_completed(
                session_id,
                "tc-grep-1",
                Some(json!("src/main.rs:1:fn main() {")),
            ),
        )
        .await;

        // Text referencing the tool result
        write_json_line(
            &mut agent_writer,
            text_chunk(
                session_id,
                "The main function is defined at line 1 of src/main.rs.",
            ),
        )
        .await;

        write_json_line(&mut agent_writer, final_response(request_id)).await;
    });

    let request = make_prompt_request(session_id, "find main function");
    let (chunks, callback) = crate::support::parity::capture_chunks();
    let (summary, _response) = client
        .send_prompt_with_callback(request, callback)
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
        vec!["Grep"]
    );
}

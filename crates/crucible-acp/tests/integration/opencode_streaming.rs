//! Integration tests for OpenCode ACP streaming behavior.
//!
//! These tests require the `opencode` binary to be installed and available in PATH.
//! They are ignored by default and can be run with:
//!
//! ```bash
//! cargo test -p crucible-acp --test integration -- --ignored opencode
//! ```
//!
//! To run with output visible:
//! ```bash
//! cargo test -p crucible-acp --test integration -- --ignored --nocapture opencode
//! ```

use agent_client_protocol::{ContentBlock, PromptRequest, SessionId};
use crucible_acp::client::{ClientConfig, CrucibleAcpClient};
use std::path::PathBuf;
use std::process::Command;

/// Helper to check if opencode binary is available
fn opencode_available() -> Option<PathBuf> {
    which::which("opencode").ok()
}

/// Test that OpenCode ACP returns content for a simple prompt.
///
/// This test reproduces an issue where:
/// - Python client receives: available_commands_update, agent_message_chunk, final_response
/// - Rust client receives: available_commands_update, final_response (missing chunk!)
///
/// Run with: `cargo test -p crucible-acp -- --ignored test_opencode_streaming_returns_content`
#[tokio::test]
#[ignore = "requires opencode binary - run with --ignored"]
async fn test_opencode_streaming_returns_content() {
    // Skip gracefully if opencode not installed
    let opencode_path = match opencode_available() {
        Some(path) => {
            eprintln!("Found opencode at: {:?}", path);
            path
        }
        None => {
            eprintln!("SKIP: 'opencode' binary not found in PATH");
            eprintln!("Install opencode or ensure it's in your PATH to run this test");
            return;
        }
    };

    // Verify opencode can run
    let version_check = Command::new(&opencode_path)
        .arg("--version")
        .output()
        .expect("Failed to run opencode --version");

    if !version_check.status.success() {
        eprintln!(
            "SKIP: opencode --version failed: {}",
            String::from_utf8_lossy(&version_check.stderr)
        );
        return;
    }
    eprintln!(
        "OpenCode version: {}",
        String::from_utf8_lossy(&version_check.stdout).trim()
    );

    // Create ACP client
    let config = ClientConfig {
        agent_path: opencode_path,
        agent_args: Some(vec!["acp".to_string()]),
        working_dir: Some(PathBuf::from("/tmp")),
        env_vars: None,
        timeout_ms: Some(30_000),
        max_retries: Some(1),
    };

    let mut client = CrucibleAcpClient::new(config);

    // Connect with handshake
    eprintln!("Connecting to OpenCode...");
    let session = client
        .connect_with_handshake()
        .await
        .expect("Failed to connect to opencode");

    eprintln!("Session established: {}", session.id());

    // Send a simple prompt
    let prompt_request = PromptRequest {
        session_id: SessionId::from(session.id().to_string()),
        prompt: vec![ContentBlock::from("say hello".to_string())],
        meta: None,
    };

    eprintln!("Sending prompt: 'say hello'");

    // This is where the bug manifests - we should get content back
    let result = client.send_prompt_with_streaming(prompt_request).await;

    match result {
        Ok((content, tool_calls, response)) => {
            eprintln!("--- RESULT ---");
            eprintln!("Content length: {} chars", content.len());
            eprintln!("Content: '{}'", content);
            eprintln!("Tool calls: {}", tool_calls.len());
            eprintln!("Stop reason: {:?}", response.stop_reason);

            // THE BUG: content is empty when it should contain "hello" or similar
            assert!(
                !content.is_empty(),
                "BUG: OpenCode returned empty content. \
                 Expected agent_message_chunk with text content. \
                 This indicates the streaming loop is missing the chunk notification."
            );
        }
        Err(e) => {
            panic!("Failed to get streaming response: {}", e);
        }
    }
}

/// Test that verifies the exact line sequence from OpenCode.
///
/// Expected sequence after session/prompt:
/// 1. available_commands_update notification
/// 2. agent_message_chunk notification (with content!)
/// 3. Final response with stopReason
///
/// Run with: `cargo test -p crucible-acp -- --ignored test_opencode_line_sequence`
#[tokio::test]
#[ignore = "requires opencode binary - run with --ignored"]
async fn test_opencode_line_sequence() {
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};

    let opencode_path = match opencode_available() {
        Some(path) => path,
        None => {
            eprintln!("SKIP: 'opencode' binary not found");
            return;
        }
    };

    // Spawn opencode in ACP mode
    let mut child = Command::new(&opencode_path)
        .arg("acp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn opencode");

    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    // Helper to send JSON-RPC
    let send = |stdin: &mut std::process::ChildStdin, msg: &serde_json::Value| {
        let data = serde_json::to_string(msg).unwrap() + "\n";
        stdin.write_all(data.as_bytes()).unwrap();
        stdin.flush().unwrap();
    };

    // Helper to read line
    let read_line = |reader: &mut BufReader<std::process::ChildStdout>| -> String {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        line.trim().to_string()
    };

    // 1. Initialize
    send(
        &mut stdin,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {"protocolVersion": 1, "clientCapabilities": {}}
        }),
    );
    let _init_response = read_line(&mut reader);
    eprintln!("Initialize: OK");

    // 2. Create session
    send(
        &mut stdin,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "session/new",
            "params": {"cwd": "/tmp", "mcpServers": []}
        }),
    );
    let session_response = read_line(&mut reader);
    let session_json: serde_json::Value = serde_json::from_str(&session_response).unwrap();
    let session_id = session_json["result"]["sessionId"].as_str().unwrap();
    eprintln!("Session created: {}", session_id);

    // 3. Send prompt
    send(
        &mut stdin,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "session/prompt",
            "params": {
                "sessionId": session_id,
                "prompt": [{"type": "text", "text": "say hello"}]
            }
        }),
    );

    // 4. Read ALL lines from response
    eprintln!("\n--- Lines after session/prompt ---");
    let mut lines = Vec::new();
    let mut found_final = false;
    let mut found_chunk = false;

    // Read with timeout
    let start = std::time::Instant::now();
    while start.elapsed() < std::time::Duration::from_secs(30) && !found_final {
        let line = read_line(&mut reader);
        if line.is_empty() {
            std::thread::sleep(std::time::Duration::from_millis(100));
            continue;
        }

        eprintln!("Line {}: {} bytes", lines.len(), line.len());

        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
            if let Some(method) = json.get("method").and_then(|m| m.as_str()) {
                let update_type = json["params"]["update"]["sessionUpdate"]
                    .as_str()
                    .unwrap_or("?");
                eprintln!("  Type: notification, sessionUpdate={}", update_type);

                if update_type == "agent_message_chunk" {
                    found_chunk = true;
                    let content = &json["params"]["update"]["content"];
                    eprintln!("  Content: {:?}", content);
                }
            } else if json.get("result").is_some() {
                let stop = json["result"]["stopReason"].as_str().unwrap_or("?");
                eprintln!("  Type: response, stopReason={}", stop);
                found_final = true;
            }
        }

        lines.push(line);
    }

    // Cleanup
    let _ = child.kill();

    // Assertions
    eprintln!("\n--- Summary ---");
    eprintln!("Total lines: {}", lines.len());
    eprintln!("Found agent_message_chunk: {}", found_chunk);
    eprintln!("Found final response: {}", found_final);

    assert!(
        found_chunk,
        "BUG: No agent_message_chunk received. \
         OpenCode should send content before the final response."
    );
}

/// Test using SSE MCP path (same as cru chat uses)
///
/// This test verifies if the SSE MCP connection path has different behavior
/// compared to the stdio MCP path used in the direct handshake test.
///
/// Run with: `cargo test -p crucible-acp -- --ignored test_opencode_with_sse_mcp`
#[tokio::test]
#[ignore = "requires opencode binary - run with --ignored"]
async fn test_opencode_with_sse_mcp() {
    use crucible_acp::InProcessMcpHost;
    use crucible_core::enrichment::EmbeddingProvider;
    use crucible_core::traits::KnowledgeRepository;
    use std::sync::Arc;
    use tempfile::TempDir;

    // Mock implementations for testing
    struct MockKnowledgeRepository;
    struct MockEmbeddingProvider;

    #[async_trait::async_trait]
    impl KnowledgeRepository for MockKnowledgeRepository {
        async fn get_note_by_name(
            &self,
            _name: &str,
        ) -> crucible_core::Result<Option<crucible_core::parser::ParsedNote>> {
            Ok(None)
        }

        async fn list_notes(
            &self,
            _path: Option<&str>,
        ) -> crucible_core::Result<Vec<crucible_core::traits::knowledge::NoteInfo>> {
            Ok(vec![])
        }

        async fn search_vectors(
            &self,
            _vector: Vec<f32>,
        ) -> crucible_core::Result<Vec<crucible_core::types::SearchResult>> {
            Ok(vec![])
        }
    }

    #[async_trait::async_trait]
    impl EmbeddingProvider for MockEmbeddingProvider {
        async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
            Ok(vec![0.1; 384])
        }

        async fn embed_batch(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
            Ok(vec![vec![0.1; 384]; texts.len()])
        }

        fn model_name(&self) -> &str {
            "mock-model"
        }

        fn dimensions(&self) -> usize {
            384
        }
    }

    // Skip if opencode not installed
    let opencode_path = match opencode_available() {
        Some(path) => {
            eprintln!("Found opencode at: {:?}", path);
            path
        }
        None => {
            eprintln!("SKIP: 'opencode' binary not found in PATH");
            return;
        }
    };

    // Create temp directory for MCP host
    let temp = TempDir::new().expect("Failed to create temp dir");

    // Start in-process MCP host (like cru chat does)
    eprintln!("Starting in-process MCP host...");
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

    let mcp_host = match InProcessMcpHost::start(
        temp.path().to_path_buf(),
        knowledge_repo,
        embedding_provider,
    )
    .await
    {
        Ok(host) => host,
        Err(e) => {
            eprintln!("SKIP: Failed to start MCP host (permission denied?): {}", e);
            return;
        }
    };

    let sse_url = mcp_host.sse_url();
    eprintln!("MCP host running at: {}", sse_url);

    // Create ACP client
    let config = ClientConfig {
        agent_path: opencode_path,
        agent_args: Some(vec!["acp".to_string()]),
        working_dir: Some(PathBuf::from("/tmp")),
        env_vars: None,
        timeout_ms: Some(30_000),
        max_retries: Some(1),
    };

    let mut client = CrucibleAcpClient::new(config);

    // Connect with SSE MCP (this is what cru chat uses!)
    eprintln!("Connecting to OpenCode with SSE MCP...");
    let session = client
        .connect_with_sse_mcp(&sse_url)
        .await
        .expect("Failed to connect to opencode with SSE MCP");

    eprintln!("Session established: {}", session.id());

    // Send a simple prompt
    let prompt_request = PromptRequest {
        session_id: SessionId::from(session.id().to_string()),
        prompt: vec![ContentBlock::from("say hello".to_string())],
        meta: None,
    };

    eprintln!("Sending prompt: 'say hello'");

    // This is where the SSE path bug might manifest
    let result = client.send_prompt_with_streaming(prompt_request).await;

    match result {
        Ok((content, tool_calls, response)) => {
            eprintln!("--- SSE MCP RESULT ---");
            eprintln!("Content length: {} chars", content.len());
            eprintln!("Content: '{}'", content);
            eprintln!("Tool calls: {}", tool_calls.len());
            eprintln!("Stop reason: {:?}", response.stop_reason);

            // If SSE path is broken, content will be empty
            assert!(
                !content.is_empty(),
                "BUG: SSE MCP path returned empty content. \
                 The connect_with_sse_mcp path is broken compared to connect_with_handshake."
            );
        }
        Err(e) => {
            panic!("Failed to get streaming response via SSE MCP: {}", e);
        }
    }

    // Cleanup
    mcp_host.shutdown();
}

/// Test with NO MCP servers configured
///
/// This tests if the issue is specifically with SSE MCP configuration
/// or if it's something else about how we create sessions.
///
/// Run with: `cargo test -p crucible-acp -- --ignored test_opencode_no_mcp`
#[tokio::test]
#[ignore = "requires opencode binary - run with --ignored"]
async fn test_opencode_no_mcp() {
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};

    let opencode_path = match opencode_available() {
        Some(path) => {
            eprintln!("Found opencode at: {:?}", path);
            path
        }
        None => {
            eprintln!("SKIP: 'opencode' binary not found");
            return;
        }
    };

    // Spawn opencode in ACP mode
    let mut child = Command::new(&opencode_path)
        .arg("acp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn opencode");

    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    // Helper functions
    let send = |stdin: &mut std::process::ChildStdin, msg: &serde_json::Value| {
        let data = serde_json::to_string(msg).unwrap() + "\n";
        stdin.write_all(data.as_bytes()).unwrap();
        stdin.flush().unwrap();
    };

    let read_line = |reader: &mut BufReader<std::process::ChildStdout>| -> String {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        line.trim().to_string()
    };

    // 1. Initialize
    send(&mut stdin, &serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {"protocolVersion": 1, "clientCapabilities": {}}
    }));
    let _init_response = read_line(&mut reader);
    eprintln!("Initialize: OK");

    // 2. Create session with NO MCP servers (should work!)
    send(&mut stdin, &serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "session/new",
        "params": {"cwd": "/tmp", "mcpServers": []}
    }));
    let session_response = read_line(&mut reader);
    let session_json: serde_json::Value = serde_json::from_str(&session_response).unwrap();
    let session_id = session_json["result"]["sessionId"].as_str().unwrap();
    eprintln!("Session (no MCP): {}", session_id);

    // 3. Send prompt
    send(&mut stdin, &serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "session/prompt",
        "params": {
            "sessionId": session_id,
            "prompt": [{"type": "text", "text": "say hello"}]
        }
    }));

    // 4. Read response
    eprintln!("\n--- NO MCP Response ---");
    let mut found_chunk = false;
    let mut content = String::new();
    let start = std::time::Instant::now();

    while start.elapsed() < std::time::Duration::from_secs(30) {
        let line = read_line(&mut reader);
        if line.is_empty() {
            std::thread::sleep(std::time::Duration::from_millis(100));
            continue;
        }

        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
            let update_type = json["params"]["update"]["sessionUpdate"].as_str();

            if update_type == Some("agent_message_chunk") {
                found_chunk = true;
                if let Some(text) = json["params"]["update"]["content"]["text"].as_str() {
                    content.push_str(text);
                    eprintln!("Chunk content: '{}'", text);
                }
            } else if json.get("result").is_some() {
                eprintln!("Final response at +{:.3}s", start.elapsed().as_secs_f64());
                break;
            }
        }
    }

    let _ = child.kill();

    eprintln!("Found chunk: {}", found_chunk);
    eprintln!("Content: '{}'", content);

    // This should pass if no MCP = works
    assert!(found_chunk, "No MCP should work and return content");
    assert!(!content.is_empty(), "Should have received content");
}

/// Test with SSE MCP configured via raw JSON-RPC
///
/// This tests if OpenCode itself has issues with SSE MCP configuration
/// independent of our client library.
///
/// Run with: `cargo test -p crucible-acp -- --ignored test_opencode_raw_sse_mcp`
#[tokio::test]
#[ignore = "requires opencode binary - run with --ignored"]
async fn test_opencode_raw_sse_mcp() {
    use crucible_acp::InProcessMcpHost;
    use crucible_core::enrichment::EmbeddingProvider;
    use crucible_core::traits::KnowledgeRepository;
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};
    use std::sync::Arc;
    use tempfile::TempDir;

    // Mock implementations
    struct MockKnowledgeRepository;
    struct MockEmbeddingProvider;

    #[async_trait::async_trait]
    impl KnowledgeRepository for MockKnowledgeRepository {
        async fn get_note_by_name(
            &self,
            _name: &str,
        ) -> crucible_core::Result<Option<crucible_core::parser::ParsedNote>> {
            Ok(None)
        }
        async fn list_notes(
            &self,
            _path: Option<&str>,
        ) -> crucible_core::Result<Vec<crucible_core::traits::knowledge::NoteInfo>> {
            Ok(vec![])
        }
        async fn search_vectors(
            &self,
            _vector: Vec<f32>,
        ) -> crucible_core::Result<Vec<crucible_core::types::SearchResult>> {
            Ok(vec![])
        }
    }

    #[async_trait::async_trait]
    impl EmbeddingProvider for MockEmbeddingProvider {
        async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
            Ok(vec![0.1; 384])
        }
        async fn embed_batch(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
            Ok(vec![vec![0.1; 384]; texts.len()])
        }
        fn model_name(&self) -> &str {
            "mock-model"
        }
        fn dimensions(&self) -> usize {
            384
        }
    }

    let opencode_path = match opencode_available() {
        Some(path) => {
            eprintln!("Found opencode at: {:?}", path);
            path
        }
        None => {
            eprintln!("SKIP: 'opencode' binary not found");
            return;
        }
    };

    // Start SSE MCP host
    let temp = TempDir::new().expect("Failed to create temp dir");
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

    let mcp_host = match InProcessMcpHost::start(
        temp.path().to_path_buf(),
        knowledge_repo,
        embedding_provider,
    )
    .await
    {
        Ok(host) => host,
        Err(e) => {
            eprintln!("SKIP: Failed to start MCP host: {}", e);
            return;
        }
    };

    let sse_url = mcp_host.sse_url();
    eprintln!("MCP host at: {}", sse_url);

    // Spawn opencode
    let mut child = Command::new(&opencode_path)
        .arg("acp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn opencode");

    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    let send = |stdin: &mut std::process::ChildStdin, msg: &serde_json::Value| {
        let data = serde_json::to_string(msg).unwrap() + "\n";
        stdin.write_all(data.as_bytes()).unwrap();
        stdin.flush().unwrap();
    };

    let read_line = |reader: &mut BufReader<std::process::ChildStdout>| -> String {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        line.trim().to_string()
    };

    // Initialize
    send(&mut stdin, &serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {"protocolVersion": 1, "clientCapabilities": {}}
    }));
    let _init = read_line(&mut reader);
    eprintln!("Initialize: OK");

    // Try different SSE MCP formats to find what OpenCode expects

    // Format 1: headers as empty object (not array)
    let format1 = serde_json::json!({
        "type": "sse",
        "name": "crucible",
        "url": sse_url,
        "headers": {}
    });

    // Format 2: PascalCase type
    let format2 = serde_json::json!({
        "type": "Sse",
        "name": "crucible",
        "url": sse_url,
        "headers": []
    });

    // Format 3: headers as empty array (ACP spec)
    let format3 = serde_json::json!({
        "type": "sse",
        "name": "crucible",
        "url": sse_url,
        "headers": []
    });

    // Format 4: headers with a dummy entry
    let format4 = serde_json::json!({
        "type": "sse",
        "name": "crucible",
        "url": sse_url,
        "headers": [{"name": "X-Test", "value": "test"}]
    });

    // Try format 3 first (empty array - ACP spec)
    let session_params = serde_json::json!({
        "cwd": "/tmp",
        "mcpServers": [format3.clone()]
    });
    eprintln!("Format 3 (empty array): {}", serde_json::to_string(&session_params).unwrap());

    send(&mut stdin, &serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "session/new",
        "params": session_params
    }));

    let session_response = read_line(&mut reader);
    eprintln!("Session response: {}", &session_response[..session_response.len().min(200)]);

    let session_json: serde_json::Value = serde_json::from_str(&session_response).unwrap();
    let session_id = session_json["result"]["sessionId"].as_str().unwrap();
    eprintln!("Session (SSE MCP): {}", session_id);

    // Send prompt
    send(&mut stdin, &serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "session/prompt",
        "params": {
            "sessionId": session_id,
            "prompt": [{"type": "text", "text": "say hello"}]
        }
    }));

    // Read response
    eprintln!("\n--- RAW SSE MCP Response ---");
    let mut found_chunk = false;
    let mut content = String::new();
    let start = std::time::Instant::now();

    while start.elapsed() < std::time::Duration::from_secs(30) {
        let line = read_line(&mut reader);
        if line.is_empty() {
            std::thread::sleep(std::time::Duration::from_millis(100));
            continue;
        }

        eprintln!("[+{:.3}s] Line: {} bytes", start.elapsed().as_secs_f64(), line.len());

        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
            let update_type = json["params"]["update"]["sessionUpdate"].as_str();

            if update_type == Some("agent_message_chunk") {
                found_chunk = true;
                if let Some(text) = json["params"]["update"]["content"]["text"].as_str() {
                    content.push_str(text);
                    eprintln!("  Chunk: '{}'", text);
                }
            } else if json.get("result").is_some() {
                eprintln!("  Final response");
                break;
            } else if let Some(ut) = update_type {
                eprintln!("  Update: {}", ut);
            }
        }
    }

    let _ = child.kill();
    mcp_host.shutdown();

    eprintln!("\nFound chunk: {}", found_chunk);
    eprintln!("Content: '{}'", content);

    // If SSE MCP is the problem in OpenCode itself, this will fail
    assert!(
        found_chunk,
        "RAW SSE MCP test failed - OpenCode doesn't return content with SSE MCP"
    );
}

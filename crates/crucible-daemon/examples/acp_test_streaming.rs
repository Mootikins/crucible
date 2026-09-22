// Quick test to verify streaming works end-to-end
use std::path::PathBuf;

#[tokio::main]
async fn main() {
    // Setup basic logging (set RUST_LOG=debug for more detail)
    tracing_subscriber::fmt::init();

    // Get workspace root
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    let mock_agent_path = workspace_root.join("target/debug/crucible-mock-agent");

    println!("Using mock agent at: {}", mock_agent_path.display());

    // Create client config
    let client_config = crucible_daemon::acp::client::ClientConfig {
        agent_path: mock_agent_path,
        agent_args: Some(vec!["--behavior".to_string(), "streaming".to_string()]),
        timeout_ms: Some(10000),
        ..Default::default()
    };

    // Connect and handshake
    println!("\n=== Connecting and performing handshake ===");
    let mut client = crucible_daemon::acp::CrucibleAcpClient::spawn(client_config, "mock", None)
        .await
        .expect("Spawn failed");
    let session = client
        .handshake(None, None)
        .await
        .expect("Handshake failed");

    println!("✅ Handshake successful! Session ID: {}", session.id());

    // Send a prompt with streaming
    println!("\n=== Sending prompt with streaming ===");
    use agent_client_protocol::schema::v1::PromptRequest;

    let prompt_request: PromptRequest = serde_json::from_value(serde_json::json!({
        "sessionId": session.id().to_string(),
        "prompt": [{"text": "What is 2+2?"}],
        "_meta": null
    }))
    .expect("Failed to create PromptRequest");

    let (out, mut chunks) = tokio::sync::mpsc::unbounded_channel();
    let result = client.prompt(prompt_request, &out).await;

    match result {
        Ok((summary, response)) => {
            let mut content = String::new();
            while let Ok(chunk) = chunks.try_recv() {
                if let crucible_daemon::acp::StreamingChunk::Text(text) = chunk {
                    content.push_str(&text);
                }
            }
            println!("\n✅ Streaming successful!");
            println!("Accumulated content: '{}'", content);
            println!("Announced a tool call: {}", summary.announced_any);
            println!("Stop reason: {:?}", response.stop_reason);

            // Verify we got the expected content
            assert_eq!(content, "The answer is 4", "Content mismatch!");
            println!("\n🎉 TEST PASSED! Streaming works correctly!");
        }
        Err(e) => {
            eprintln!("\n❌ Streaming failed: {:?}", e);
            std::process::exit(1);
        }
    }
}

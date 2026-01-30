//! End-to-end integration tests for Rig agent with real LLM infrastructure.
//!
//! These tests require a running LLM endpoint and are ignored by default.
//! Run with: `cargo test -p crucible-rig --test rig_e2e_integration -- --ignored`

use crucible_core::traits::chat::{AgentHandle, ChatChunk};
use crucible_rig::{build_agent_with_tools, AgentConfig, RigAgentHandle};
use futures::StreamExt;
use rig::providers::openai;
use tempfile::TempDir;

/// Helper to create an OpenAI-compatible client for llama.cpp
fn create_test_client() -> openai::CompletionsClient {
    openai::CompletionsClient::builder()
        .api_key("not-needed")
        .base_url("https://llama.krohnos.io/v1")
        .build()
        .expect("Failed to create client")
}

/// Helper to create a test agent config
fn create_test_config() -> AgentConfig {
    AgentConfig::new(
        "qwen3-4b-instruct-2507-q8_0",
        "You are a helpful assistant. Be concise.",
    )
}

#[tokio::test]
#[ignore = "requires llama.krohnos.io endpoint"]
async fn test_rig_agent_basic_prompt() {
    let client = create_test_client();
    let config = create_test_config();
    let temp_dir = TempDir::new().unwrap();

    let (agent, _ws_ctx) =
        build_agent_with_tools(&config, &client, temp_dir.path(), vec![]).expect("Failed to build agent");
    let mut handle = RigAgentHandle::new(agent);

    // Send a simple message and collect response
    let mut stream =
        handle.send_message_stream("What is 2+2? Answer with just the number.".to_string());
    let mut response = String::new();
    let mut got_done = false;

    while let Some(result) = stream.next().await {
        match result {
            Ok(chunk) => {
                response.push_str(&chunk.delta);
                if chunk.done {
                    got_done = true;
                    break;
                }
            }
            Err(e) => panic!("Stream error: {}", e),
        }
    }

    assert!(got_done, "Should receive done signal");
    assert!(!response.is_empty(), "Should have response content");
    // The response should contain "4" somewhere
    assert!(
        response.contains("4"),
        "Expected '4' in response, got: {}",
        response
    );
}

#[tokio::test]
#[ignore = "requires llama.krohnos.io endpoint"]
async fn test_rig_agent_with_read_file_tool() {
    let client = create_test_client();
    let config = AgentConfig::new(
        "qwen3-4b-instruct-2507-q8_0",
        "You are a helpful assistant with access to file tools. Use the read_file tool when asked to read files.",
    );

    let temp_dir = TempDir::new().unwrap();

    // Create a test file
    let test_file = temp_dir.path().join("test.txt");
    std::fs::write(&test_file, "Hello from the test file!").expect("Failed to write test file");

    let (agent, _ws_ctx) =
        build_agent_with_tools(&config, &client, temp_dir.path(), vec![]).expect("Failed to build agent");
    let mut handle = RigAgentHandle::new(agent);

    // Ask the agent to read the file
    let mut stream =
        handle.send_message_stream("Read the file test.txt and tell me what it says.".to_string());
    let mut response = String::new();
    let mut tool_calls = Vec::new();

    while let Some(result) = stream.next().await {
        match result {
            Ok(chunk) => {
                response.push_str(&chunk.delta);
                if let Some(ref tcs) = chunk.tool_calls {
                    tool_calls.extend(tcs.clone());
                }
                if chunk.done {
                    break;
                }
            }
            Err(e) => panic!("Stream error: {}", e),
        }
    }

    // Should have called the read_file tool
    assert!(
        !tool_calls.is_empty() || response.contains("Hello from the test file"),
        "Expected tool call or file content in response. Got: {}",
        response
    );
}

#[tokio::test]
#[ignore = "requires llama.krohnos.io endpoint"]
async fn test_rig_agent_streaming_multiple_chunks() {
    let client = create_test_client();
    let config = create_test_config();
    let temp_dir = TempDir::new().unwrap();

    let (agent, _ws_ctx) =
        build_agent_with_tools(&config, &client, temp_dir.path(), vec![]).expect("Failed to build agent");
    let mut handle = RigAgentHandle::new(agent);

    // Ask for a slightly longer response
    let mut stream =
        handle.send_message_stream("Count from 1 to 5, each number on a new line.".to_string());

    let mut chunks: Vec<ChatChunk> = Vec::new();

    while let Some(result) = stream.next().await {
        match result {
            Ok(chunk) => {
                let done = chunk.done;
                chunks.push(chunk);
                if done {
                    break;
                }
            }
            Err(e) => panic!("Stream error: {}", e),
        }
    }

    // Should have received multiple chunks (streaming)
    assert!(
        chunks.len() > 1,
        "Expected multiple streaming chunks, got {}",
        chunks.len()
    );

    // Combine all deltas
    let full_response: String = chunks.iter().map(|c| c.delta.as_str()).collect();

    // Should contain numbers 1-5
    for n in 1..=5 {
        assert!(
            full_response.contains(&n.to_string()),
            "Expected {} in response: {}",
            n,
            full_response
        );
    }
}

#[tokio::test]
#[ignore = "requires llama.krohnos.io endpoint"]
async fn test_rig_agent_history_preserved() {
    let client = create_test_client();
    let config = create_test_config();
    let temp_dir = TempDir::new().unwrap();

    let (agent, _ws_ctx) =
        build_agent_with_tools(&config, &client, temp_dir.path(), vec![]).expect("Failed to build agent");
    let mut handle = RigAgentHandle::new(agent);

    // First message - introduce a fact
    let mut stream = handle.send_message_stream(
        "Remember this: my favorite color is blue. Just say 'OK'.".to_string(),
    );
    while let Some(result) = stream.next().await {
        if result.is_ok() && result.as_ref().unwrap().done {
            break;
        }
    }

    // Check history has the user message
    let history = handle.get_history().await;
    assert!(!history.is_empty(), "History should not be empty");

    // Second message - ask about the fact
    let mut stream2 = handle
        .send_message_stream("What is my favorite color? Answer with just the color.".to_string());
    let mut response = String::new();
    while let Some(result) = stream2.next().await {
        match result {
            Ok(chunk) => {
                response.push_str(&chunk.delta);
                if chunk.done {
                    break;
                }
            }
            Err(e) => panic!("Stream error: {}", e),
        }
    }

    // Should remember "blue"
    assert!(
        response.to_lowercase().contains("blue"),
        "Expected 'blue' in response (history test): {}",
        response
    );
}

//! Model Contract Adherence Tests
//!
//! These tests verify that different LLM backends conform to expected contracts:
//! - Tool call format correctness
//! - Response structure
//! - Streaming behavior
//! - Error handling
//!
//! # Running Tests
//!
//! ```bash
//! # Test with local Ollama endpoint
//! OLLAMA_BASE_URL=http://localhost:11434 cargo test -p crucible-rig --test model_contract_tests -- --ignored --nocapture
//!
//! # Test with llama.cpp endpoint
//! LLAMA_BASE_URL=https://llama.krohnos.io/v1 cargo test -p crucible-rig --test model_contract_tests -- --ignored --nocapture
//!
//! # Test specific model size
//! TEST_MODEL=qwen3-4b-instruct-2507-q8_0 cargo test -p crucible-rig --test model_contract_tests small -- --ignored
//! ```
//!
//! # Model Size Categories
//!
//! - **Small** (< 4B params): Fast, may struggle with complex tool calls
//! - **Medium** (4-13B params): Good balance of speed and capability
//! - **Large** (> 13B params): Best accuracy, slower
//!
//! # Contracts Tested
//!
//! 1. **Basic Response**: Model produces non-empty, coherent response
//! 2. **Tool Declaration**: Model receives tool definitions correctly
//! 3. **Tool Invocation**: Model calls tools with valid JSON arguments
//! 4. **Multi-Turn**: Model maintains context across turns
//! 5. **Streaming**: Chunks arrive in order, done signal received

use crucible_core::traits::chat::{AgentHandle, ChatChunk};
use crucible_rig::{build_agent_with_tools, AgentConfig, RigAgentHandle};
use futures::StreamExt;
use rig::providers::openai;
use serde_json::Value as JsonValue;
use std::time::{Duration, Instant};
use tempfile::TempDir;

// =============================================================================
// Test Configuration
// =============================================================================

/// Model configuration for testing
struct ModelTestConfig {
    name: &'static str,
    model_id: String,
    base_url: String,
    timeout: Duration,
    expected_tool_success_rate: f64,
}

impl ModelTestConfig {
    fn from_env() -> Self {
        let model_id = std::env::var("TEST_MODEL")
            .unwrap_or_else(|_| "qwen3-4b-instruct-2507-q8_0".to_string());

        let base_url = std::env::var("LLAMA_BASE_URL")
            .or_else(|_| std::env::var("OLLAMA_BASE_URL").map(|u| format!("{}/v1", u)))
            .unwrap_or_else(|_| "https://llama.krohnos.io/v1".to_string());

        let timeout_secs: u64 = std::env::var("TEST_TIMEOUT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(120);

        Self {
            name: "test-model",
            model_id,
            base_url,
            timeout: Duration::from_secs(timeout_secs),
            expected_tool_success_rate: 0.8,
        }
    }

    fn small_local() -> Self {
        Self {
            name: "small",
            model_id: "qwen3-4b-instruct-2507-q8_0".to_string(),
            base_url: "https://llama.krohnos.io/v1".to_string(),
            timeout: Duration::from_secs(60),
            expected_tool_success_rate: 0.7,
        }
    }

    fn medium_local() -> Self {
        Self {
            name: "medium",
            model_id: "qwen3-8b-instruct-q6_k".to_string(),
            base_url: "https://llama.krohnos.io/v1".to_string(),
            timeout: Duration::from_secs(120),
            expected_tool_success_rate: 0.85,
        }
    }

    fn create_client(&self) -> openai::CompletionsClient {
        openai::CompletionsClient::builder()
            .api_key("not-needed")
            .base_url(&self.base_url)
            .build()
            .expect("Failed to create client")
    }
}

// =============================================================================
// Contract: Basic Response
// =============================================================================

/// Contract: Model produces non-empty response within timeout
async fn test_basic_response_contract(config: &ModelTestConfig) -> Result<(), String> {
    let client = config.create_client();
    let agent_config =
        AgentConfig::new(&config.model_id, "You are a helpful assistant. Be concise.");
    let temp_dir = TempDir::new().map_err(|e| e.to_string())?;

    let (agent, _ws_ctx) = build_agent_with_tools(&agent_config, &client, temp_dir.path(), vec![])
        .map_err(|e| format!("Failed to build agent: {}", e))?;
    let mut handle = RigAgentHandle::new(agent);

    let start = Instant::now();
    let mut stream = handle.send_message_stream("Say hello.".to_string());
    let mut response = String::new();
    let mut got_done = false;

    while let Some(result) = stream.next().await {
        if start.elapsed() > config.timeout {
            return Err(format!("Timeout after {:?}", config.timeout));
        }

        match result {
            Ok(chunk) => {
                response.push_str(&chunk.delta);
                if chunk.done {
                    got_done = true;
                    break;
                }
            }
            Err(e) => return Err(format!("Stream error: {}", e)),
        }
    }

    if !got_done {
        return Err("Did not receive done signal".to_string());
    }

    if response.is_empty() {
        return Err("Response was empty".to_string());
    }

    if response.len() < 2 {
        return Err(format!("Response too short: '{}'", response));
    }

    Ok(())
}

#[tokio::test]
#[ignore = "requires LLM endpoint"]
async fn contract_basic_response() {
    let config = ModelTestConfig::from_env();
    println!("Testing basic response contract with {}", config.model_id);

    match test_basic_response_contract(&config).await {
        Ok(()) => println!("✓ Basic response contract passed"),
        Err(e) => panic!("✗ Basic response contract failed: {}", e),
    }
}

// =============================================================================
// Contract: Tool Invocation
// =============================================================================

/// Contract: Model can invoke tools with valid JSON arguments
async fn test_tool_invocation_contract(config: &ModelTestConfig) -> Result<(), String> {
    let client = config.create_client();
    let agent_config = AgentConfig::new(
        &config.model_id,
        "You are a helpful assistant with file tools. When asked to read a file, use the read_file tool.",
    );
    let temp_dir = TempDir::new().map_err(|e| e.to_string())?;

    // Create a test file
    let test_file = temp_dir.path().join("test.txt");
    std::fs::write(&test_file, "Contract test content").map_err(|e| e.to_string())?;

    let (agent, _ws_ctx) = build_agent_with_tools(&agent_config, &client, temp_dir.path(), vec![])
        .map_err(|e| format!("Failed to build agent: {}", e))?;
    let mut handle = RigAgentHandle::new(agent);

    let start = Instant::now();
    let mut stream = handle.send_message_stream("Read the file test.txt".to_string());
    let mut tool_calls: Vec<(String, JsonValue)> = Vec::new();
    let mut got_done = false;

    while let Some(result) = stream.next().await {
        if start.elapsed() > config.timeout {
            return Err(format!("Timeout after {:?}", config.timeout));
        }

        match result {
            Ok(chunk) => {
                if let Some(ref tcs) = chunk.tool_calls {
                    for tc in tcs {
                        if let Some(args) = tc.arguments.clone() {
                            tool_calls.push((tc.name.clone(), args));
                        }
                    }
                }
                if chunk.done {
                    got_done = true;
                    break;
                }
            }
            Err(e) => return Err(format!("Stream error: {}", e)),
        }
    }

    if !got_done {
        return Err("Did not receive done signal".to_string());
    }

    if tool_calls.is_empty() {
        return Err("No tool calls made".to_string());
    }

    // Validate tool call structure
    for (name, args) in &tool_calls {
        if name.is_empty() {
            return Err("Tool call has empty name".to_string());
        }

        // Args should be a JSON object
        if !args.is_object() {
            return Err(format!("Tool args not an object: {:?}", args));
        }
    }

    // Check if read_file was called
    let has_read_file = tool_calls.iter().any(|(name, _)| name == "read_file");
    if !has_read_file {
        return Err(format!(
            "Expected read_file tool, got: {:?}",
            tool_calls.iter().map(|(n, _)| n).collect::<Vec<_>>()
        ));
    }

    Ok(())
}

#[tokio::test]
#[ignore = "requires LLM endpoint"]
async fn contract_tool_invocation() {
    let config = ModelTestConfig::from_env();
    println!("Testing tool invocation contract with {}", config.model_id);

    match test_tool_invocation_contract(&config).await {
        Ok(()) => println!("✓ Tool invocation contract passed"),
        Err(e) => panic!("✗ Tool invocation contract failed: {}", e),
    }
}

// =============================================================================
// Contract: Streaming Order
// =============================================================================

/// Contract: Streaming chunks arrive and done signal is last
async fn test_streaming_order_contract(config: &ModelTestConfig) -> Result<(), String> {
    let client = config.create_client();
    let agent_config = AgentConfig::new(&config.model_id, "You are a helpful assistant.");
    let temp_dir = TempDir::new().map_err(|e| e.to_string())?;

    let (agent, _ws_ctx) = build_agent_with_tools(&agent_config, &client, temp_dir.path(), vec![])
        .map_err(|e| format!("Failed to build agent: {}", e))?;
    let mut handle = RigAgentHandle::new(agent);

    let start = Instant::now();
    let mut stream = handle.send_message_stream("Count from 1 to 5.".to_string());
    let mut chunks: Vec<ChatChunk> = Vec::new();

    while let Some(result) = stream.next().await {
        if start.elapsed() > config.timeout {
            return Err(format!("Timeout after {:?}", config.timeout));
        }

        match result {
            Ok(chunk) => {
                chunks.push(chunk.clone());
                if chunk.done {
                    break;
                }
            }
            Err(e) => return Err(format!("Stream error: {}", e)),
        }
    }

    if chunks.is_empty() {
        return Err("No chunks received".to_string());
    }

    // Last chunk should be done
    let last = chunks.last().unwrap();
    if !last.done {
        return Err("Last chunk should have done=true".to_string());
    }

    // No chunk before last should have done=true
    for (i, chunk) in chunks.iter().enumerate() {
        if i < chunks.len() - 1 && chunk.done {
            return Err(format!("Chunk {} has done=true but is not last", i));
        }
    }

    // Should have at least 2 chunks (some content + done)
    if chunks.len() < 2 {
        return Err(format!("Expected >= 2 chunks, got {}", chunks.len()));
    }

    Ok(())
}

#[tokio::test]
#[ignore = "requires LLM endpoint"]
async fn contract_streaming_order() {
    let config = ModelTestConfig::from_env();
    println!("Testing streaming order contract with {}", config.model_id);

    match test_streaming_order_contract(&config).await {
        Ok(()) => println!("✓ Streaming order contract passed"),
        Err(e) => panic!("✗ Streaming order contract failed: {}", e),
    }
}

// =============================================================================
// Contract: Multi-Turn Context
// =============================================================================

/// Contract: Model maintains context across multiple turns
async fn test_multi_turn_context_contract(config: &ModelTestConfig) -> Result<(), String> {
    let client = config.create_client();
    let agent_config = AgentConfig::new(
        &config.model_id,
        "You are a helpful assistant. Remember what the user tells you.",
    );
    let temp_dir = TempDir::new().map_err(|e| e.to_string())?;

    let (agent, _ws_ctx) = build_agent_with_tools(&agent_config, &client, temp_dir.path(), vec![])
        .map_err(|e| format!("Failed to build agent: {}", e))?;
    let mut handle = RigAgentHandle::new(agent);

    // Turn 1: Tell the model something specific
    let mut stream = handle.send_message_stream("My name is TestUser. Remember that.".to_string());
    while let Some(result) = stream.next().await {
        match result {
            Ok(chunk) if chunk.done => break,
            Ok(_) => continue,
            Err(e) => return Err(format!("Turn 1 error: {}", e)),
        }
    }

    // Turn 2: Ask about what we said
    let mut stream = handle.send_message_stream("What is my name?".to_string());
    let mut response = String::new();

    while let Some(result) = stream.next().await {
        match result {
            Ok(chunk) => {
                response.push_str(&chunk.delta);
                if chunk.done {
                    break;
                }
            }
            Err(e) => return Err(format!("Turn 2 error: {}", e)),
        }
    }

    // Response should contain the name
    if !response.to_lowercase().contains("testuser") {
        return Err(format!(
            "Model did not remember name. Response: '{}'",
            response
        ));
    }

    Ok(())
}

#[tokio::test]
#[ignore = "requires LLM endpoint"]
async fn contract_multi_turn_context() {
    let config = ModelTestConfig::from_env();
    println!(
        "Testing multi-turn context contract with {}",
        config.model_id
    );

    match test_multi_turn_context_contract(&config).await {
        Ok(()) => println!("✓ Multi-turn context contract passed"),
        Err(e) => panic!("✗ Multi-turn context contract failed: {}", e),
    }
}

// =============================================================================
// Full Contract Suite
// =============================================================================

/// Run all contracts against a model configuration
async fn run_contract_suite(config: &ModelTestConfig) -> (usize, usize, Vec<String>) {
    let mut passed = 0;
    let mut failed = 0;
    let mut failures = Vec::new();

    println!(
        "\n=== Contract Suite: {} ({}) ===\n",
        config.name, config.model_id
    );

    // Basic response
    print!("  Basic response... ");
    match test_basic_response_contract(config).await {
        Ok(()) => {
            println!("✓");
            passed += 1;
        }
        Err(e) => {
            println!("✗ {}", e);
            failed += 1;
            failures.push(format!("basic_response: {}", e));
        }
    }

    // Streaming order
    print!("  Streaming order... ");
    match test_streaming_order_contract(config).await {
        Ok(()) => {
            println!("✓");
            passed += 1;
        }
        Err(e) => {
            println!("✗ {}", e);
            failed += 1;
            failures.push(format!("streaming_order: {}", e));
        }
    }

    // Tool invocation
    print!("  Tool invocation... ");
    match test_tool_invocation_contract(config).await {
        Ok(()) => {
            println!("✓");
            passed += 1;
        }
        Err(e) => {
            println!("✗ {}", e);
            failed += 1;
            failures.push(format!("tool_invocation: {}", e));
        }
    }

    // Multi-turn context
    print!("  Multi-turn context... ");
    match test_multi_turn_context_contract(config).await {
        Ok(()) => {
            println!("✓");
            passed += 1;
        }
        Err(e) => {
            println!("✗ {}", e);
            failed += 1;
            failures.push(format!("multi_turn_context: {}", e));
        }
    }

    println!("\n  Summary: {} passed, {} failed", passed, failed);

    (passed, failed, failures)
}

#[tokio::test]
#[ignore = "requires LLM endpoint - runs full suite"]
async fn contract_suite_small_model() {
    let config = ModelTestConfig::small_local();
    let (passed, failed, failures) = run_contract_suite(&config).await;

    if !failures.is_empty() {
        println!("\nFailures:");
        for f in &failures {
            println!("  - {}", f);
        }
    }

    let success_rate = passed as f64 / (passed + failed) as f64;
    assert!(
        success_rate >= config.expected_tool_success_rate,
        "Expected >= {:.0}% success rate, got {:.0}%",
        config.expected_tool_success_rate * 100.0,
        success_rate * 100.0
    );
}

#[tokio::test]
#[ignore = "requires LLM endpoint - runs full suite"]
async fn contract_suite_medium_model() {
    let config = ModelTestConfig::medium_local();
    let (passed, failed, failures) = run_contract_suite(&config).await;

    if !failures.is_empty() {
        println!("\nFailures:");
        for f in &failures {
            println!("  - {}", f);
        }
    }

    let success_rate = passed as f64 / (passed + failed) as f64;
    assert!(
        success_rate >= config.expected_tool_success_rate,
        "Expected >= {:.0}% success rate, got {:.0}%",
        config.expected_tool_success_rate * 100.0,
        success_rate * 100.0
    );
}

#[tokio::test]
#[ignore = "requires LLM endpoint - runs full suite"]
async fn contract_suite_env_model() {
    let config = ModelTestConfig::from_env();
    let (passed, failed, failures) = run_contract_suite(&config).await;

    if !failures.is_empty() {
        println!("\nFailures:");
        for f in &failures {
            println!("  - {}", f);
        }
    }

    // For env-specified model, we just report results
    let total = passed + failed;
    println!(
        "\nFinal: {}/{} contracts passed ({:.0}%)",
        passed,
        total,
        (passed as f64 / total as f64) * 100.0
    );
}

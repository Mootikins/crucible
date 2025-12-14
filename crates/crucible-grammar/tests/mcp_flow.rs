//! Integration tests for MCP call/response flows
//!
//! Tests complete tool calling flows:
//! 1. User prompt → Model generates tool call (JSON)
//! 2. Parse tool call → Execute tool
//! 3. Tool result → Model generates response
//!
//! Run with: cargo test -p crucible-grammar --test mcp_flow -- --ignored --nocapture

use crucible_grammar::mcp::{
    execute_tool_call, parse_tool_call, tools_to_grammar, tools_to_system_prompt, CliTools,
    ToolCallParams,
};
use crucible_grammar::{ChatMessage, CompletionRequest, LlamaClient};
use rmcp::model::{CallToolResult, Content, RawContent, RawTextContent, Tool};
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;

/// Test configuration
struct FlowTestConfig {
    endpoint: String,
    model: String,
    max_tokens: u32,
}

impl Default for FlowTestConfig {
    fn default() -> Self {
        Self {
            endpoint: std::env::var("LLAMA_ENDPOINT")
                .unwrap_or_else(|_| "https://llama.krohnos.io".to_string()),
            model: std::env::var("LLAMA_MODEL")
                .unwrap_or_else(|_| "granite-micro-3b-q6_k".to_string()),
            max_tokens: 150,
        }
    }
}

/// Result of a complete flow
#[derive(Debug)]
struct FlowResult {
    user_prompt: String,
    tool_call_raw: String,
    tool_call: Option<ToolCallParams>,
    tool_result: Option<CallToolResult>,
    latency_ms: u64,
    tokens: u32,
    success: bool,
}

/// Run a complete MCP flow
async fn run_mcp_flow(
    client: &LlamaClient,
    config: &FlowTestConfig,
    tools: &[Tool],
    user_prompt: &str,
    use_grammar: bool,
) -> Result<FlowResult, Box<dyn std::error::Error>> {
    let system_prompt = tools_to_system_prompt(tools);

    let messages = vec![
        ChatMessage::system(&system_prompt),
        ChatMessage::user(user_prompt),
    ];

    let mut request = CompletionRequest::new(&config.model, messages)
        .with_max_tokens(config.max_tokens)
        .with_temperature(0.0)
        .without_thinking();

    if use_grammar {
        let grammar = tools_to_grammar(tools);
        request = request.with_grammar(&grammar);
    }

    let (response, duration) = client.complete(request).await?;
    let tool_call_raw = LlamaClient::extract_content(&response)
        .unwrap_or("")
        .to_string();

    // Parse the tool call
    let tool_call = parse_tool_call(&tool_call_raw);

    // Execute if parsed successfully
    let tool_result = if let Some(ref tc) = tool_call {
        Some(execute_tool_call(tc).await)
    } else {
        None
    };

    let success = tool_call.is_some()
        && tool_result
            .as_ref()
            .map(|r| r.is_error != Some(true))
            .unwrap_or(false);

    Ok(FlowResult {
        user_prompt: user_prompt.to_string(),
        tool_call_raw,
        tool_call,
        tool_result,
        latency_ms: duration.as_millis() as u64,
        tokens: response.usage.completion_tokens,
        success,
    })
}

/// Print flow result
fn print_flow_result(result: &FlowResult, format_name: &str) {
    let status = if result.success { "✓" } else { "✗" };
    println!("\n{} {} Flow", status, format_name);
    println!("  Prompt: {}", result.user_prompt);
    println!("  Raw output: {}", result.tool_call_raw);

    if let Some(ref tc) = result.tool_call {
        println!(
            "  Parsed: {} with {}",
            tc.name,
            serde_json::to_string(&tc.arguments).unwrap_or_default()
        );
    } else {
        println!("  Parsed: FAILED");
    }

    if let Some(ref tr) = result.tool_result {
        let content_preview: String = tr
            .content
            .iter()
            .filter_map(|c| match &c.raw {
                RawContent::Text(t) => Some(t.text.chars().take(100).collect::<String>()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(", ");
        let error_status = if tr.is_error == Some(true) {
            " (ERROR)"
        } else {
            ""
        };
        println!("  Result{}: {}...", error_status, content_preview);
    }

    println!("  Tokens: {}, Latency: {}ms", result.tokens, result.latency_ms);
}

// ===== Tests =====

#[tokio::test]
#[ignore = "Requires llama server"]
async fn test_mcp_search_flow() {
    let config = FlowTestConfig::default();
    let client = LlamaClient::new(&config.endpoint).with_timeout(Duration::from_secs(60));
    let tools = CliTools::all();

    let result = run_mcp_flow(
        &client,
        &config,
        &tools,
        "Search for TODO comments in the source code",
        true,
    )
    .await
    .expect("Flow should complete");

    print_flow_result(&result, "MCP JSON");

    assert!(result.tool_call.is_some(), "Should parse tool call");
    let tc = result.tool_call.unwrap();
    assert_eq!(tc.name, "rg", "Should select rg tool");
}

#[tokio::test]
#[ignore = "Requires llama server"]
async fn test_mcp_read_file_flow() {
    let config = FlowTestConfig::default();
    let client = LlamaClient::new(&config.endpoint).with_timeout(Duration::from_secs(60));
    let tools = CliTools::all();

    let result = run_mcp_flow(&client, &config, &tools, "Read the Cargo.toml file", true)
        .await
        .expect("Flow should complete");

    print_flow_result(&result, "MCP JSON");

    assert!(result.tool_call.is_some(), "Should parse tool call");
    let tc = result.tool_call.unwrap();
    assert_eq!(tc.name, "cat", "Should select cat tool");
}

/// Configuration for a single test variant
#[derive(Clone)]
struct TestVariant {
    name: &'static str,
    tools: Vec<Tool>,
    use_grammar: bool,
}

#[tokio::test]
#[ignore = "Requires llama server - run with: cargo test -p crucible-grammar --test mcp_flow -- --ignored --nocapture"]
async fn test_full_matrix() {
    let config = FlowTestConfig::default();
    let client = LlamaClient::new(&config.endpoint).with_timeout(Duration::from_secs(60));

    println!("\n{}", "=".repeat(80));
    println!(" FULL 2x2 MATRIX: Schema Type × Grammar Constraint - {}", config.model);
    println!("{}", "=".repeat(80));

    let test_cases = [
        "Search for TODO comments in the source code",
        "Find all Rust files in the project",
        "Read the Cargo.toml file",
        "List the contents of the src directory",
    ];

    // Full 2x2 matrix
    let variants = [
        TestVariant {
            name: "Structured+Grammar",
            tools: CliTools::all(),
            use_grammar: true,
        },
        TestVariant {
            name: "Structured-NoGrammar",
            tools: CliTools::all(),
            use_grammar: false,
        },
        TestVariant {
            name: "Passthrough+Grammar",
            tools: CliTools::all_passthrough(),
            use_grammar: true,
        },
        TestVariant {
            name: "Passthrough-NoGrammar",
            tools: CliTools::all_passthrough(),
            use_grammar: false,
        },
    ];

    // Results per variant
    let mut all_results: Vec<(&str, Vec<FlowResult>)> = Vec::new();

    for variant in &variants {
        let mut results = Vec::new();

        println!("\n{}", "=".repeat(80));
        println!(" {} ", variant.name);
        println!("{}", "=".repeat(80));

        for prompt in &test_cases {
            println!("\n{}", "-".repeat(60));

            match run_mcp_flow(&client, &config, &variant.tools, prompt, variant.use_grammar).await {
                Ok(result) => {
                    print_flow_result(&result, variant.name);
                    results.push(result);
                }
                Err(e) => {
                    println!("✗ {} - Error: {}", variant.name, e);
                }
            }
        }

        all_results.push((variant.name, results));
    }

    // Summary matrix
    println!("\n{}", "=".repeat(80));
    println!(" SUMMARY MATRIX");
    println!("{}", "=".repeat(80));
    println!("{:25} | {:>8} | {:>12} | {:>8}", "Variant", "Success", "Avg Tokens", "Parsed");
    println!("{}", "-".repeat(80));

    for (name, results) in &all_results {
        let success = results.iter().filter(|r| r.success).count();
        let parsed = results.iter().filter(|r| r.tool_call.is_some()).count();
        let total_tokens: u32 = results.iter().map(|r| r.tokens).sum();
        let avg_tokens = if results.is_empty() { 0.0 } else { total_tokens as f64 / results.len() as f64 };

        println!(
            "{:25} | {:>5}/{:<2} | {:>10.1} | {:>5}/{:<2}",
            name, success, results.len(), avg_tokens, parsed, results.len()
        );
    }

    // Per-prompt breakdown
    println!("\n{}", "=".repeat(80));
    println!(" PER-PROMPT BREAKDOWN");
    println!("{}", "=".repeat(80));

    for (i, prompt) in test_cases.iter().enumerate() {
        println!("\n{}. {}", i + 1, prompt);
        println!("{:25} | {:>6} | {:>6} | Output", "Variant", "Status", "Tokens");
        println!("{}", "-".repeat(80));

        for (name, results) in &all_results {
            if let Some(r) = results.get(i) {
                let status = if r.success { "✓" } else if r.tool_call.is_some() { "~" } else { "✗" };
                let output_preview: String = r.tool_call_raw.chars().take(40).collect();
                println!("{:25} | {:>6} | {:>6} | {}", name, status, r.tokens, output_preview);
            }
        }
    }

    println!("\n{}", "=".repeat(80));
    println!("Legend: ✓=success, ~=parsed but execution failed, ✗=parse failed");
    println!("{}", "=".repeat(80));
}

#[tokio::test]
#[ignore = "Requires llama server"]
async fn test_multi_turn_flow() {
    let config = FlowTestConfig::default();
    let client = LlamaClient::new(&config.endpoint).with_timeout(Duration::from_secs(60));
    let tools = CliTools::all();

    println!("\n{}", "=".repeat(70));
    println!(" Multi-Turn MCP Flow");
    println!("{}", "=".repeat(70));

    // Turn 1: Search for something
    println!("\n--- Turn 1: Search ---");
    let system_prompt = tools_to_system_prompt(&tools);
    let grammar = tools_to_grammar(&tools);

    let messages1 = vec![
        ChatMessage::system(&system_prompt),
        ChatMessage::user("Find all files containing 'test' in the filename"),
    ];

    let request1 = CompletionRequest::new(&config.model, messages1.clone())
        .with_max_tokens(config.max_tokens)
        .with_temperature(0.0)
        .with_grammar(&grammar)
        .without_thinking();

    let (response1, _) = client.complete(request1).await.expect("Request 1 failed");
    let output1 = LlamaClient::extract_content(&response1).unwrap_or("");
    println!("Model output: {}", output1);

    let tool_call1 = parse_tool_call(output1).expect("Should parse first tool call");
    println!("Tool call: {} {:?}", tool_call1.name, tool_call1.arguments);

    let result1 = execute_tool_call(&tool_call1).await;
    let result1_text = result1
        .content
        .iter()
        .filter_map(|c| match &c.raw {
            RawContent::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    println!("Tool result (truncated): {}...", &result1_text[..result1_text.len().min(200)]);

    // Turn 2: Follow up based on result
    println!("\n--- Turn 2: Follow-up ---");

    // Build conversation with tool result
    let mut messages2 = messages1.clone();
    messages2.push(ChatMessage::assistant(output1));
    messages2.push(ChatMessage::user(&format!(
        "Tool result:\n{}\n\nNow read the first test file from that list.",
        &result1_text[..result1_text.len().min(500)]
    )));

    let request2 = CompletionRequest::new(&config.model, messages2)
        .with_max_tokens(config.max_tokens)
        .with_temperature(0.0)
        .with_grammar(&grammar)
        .without_thinking();

    let (response2, _) = client.complete(request2).await.expect("Request 2 failed");
    let output2 = LlamaClient::extract_content(&response2).unwrap_or("");
    println!("Model output: {}", output2);

    if let Some(tool_call2) = parse_tool_call(output2) {
        println!("Tool call: {} {:?}", tool_call2.name, tool_call2.arguments);
        assert_eq!(
            tool_call2.name, "cat",
            "Second turn should use cat to read file"
        );
    }

    println!("{}", "=".repeat(70));
}

// ===== Unit tests =====

#[test]
fn test_tools_system_prompt_contains_all_tools() {
    let tools = CliTools::all();
    let prompt = tools_to_system_prompt(&tools);

    for tool in &tools {
        assert!(
            prompt.contains(&format!("## {}", tool.name)),
            "Prompt should contain tool {}",
            tool.name
        );
    }
}

#[test]
fn test_grammar_constrains_tool_names() {
    let tools = CliTools::all();
    let grammar = tools_to_grammar(&tools);

    // Should contain all tool names as alternatives
    assert!(grammar.contains(r#""rg""#));
    assert!(grammar.contains(r#""fd""#));
    assert!(grammar.contains(r#""cat""#));
    assert!(grammar.contains(r#""ls""#));

    // Should be OR'd together
    assert!(grammar.contains(" | "));
}

#[tokio::test]
async fn test_execute_tool_rg() {
    let params = ToolCallParams {
        name: "rg".to_string(),
        arguments: json!({"pattern": "test", "path": "."}),
    };

    let result = execute_tool_call(&params).await;
    // Should not error (even if no matches found)
    assert!(
        result.is_error.is_none() || result.is_error == Some(false),
        "rg should not error"
    );
}

#[tokio::test]
async fn test_execute_tool_fd() {
    let params = ToolCallParams {
        name: "fd".to_string(),
        arguments: json!({"pattern": ".", "path": "."}),
    };

    let result = execute_tool_call(&params).await;
    assert!(
        result.is_error.is_none() || result.is_error == Some(false),
        "fd should not error"
    );
    assert!(!result.content.is_empty(), "fd should return content");
}

#[tokio::test]
async fn test_execute_unknown_tool() {
    let params = ToolCallParams {
        name: "unknown_tool".to_string(),
        arguments: json!({}),
    };

    let result = execute_tool_call(&params).await;
    assert_eq!(result.is_error, Some(true), "Unknown tool should error");
}

//! Tests for schema detail levels and system prompt variations
//!
//! Compares:
//! 1. Schema detail: Minimal vs Standard vs Detailed
//! 2. System prompt: Minimal vs Standard vs Detailed vs JsonFocused
//! 3. Different model sizes: 3B, 8B, 14B
//!
//! Run with: cargo test -p crucible-grammar --test schema_detail -- --ignored --nocapture

use crucible_grammar::mcp::parse_tool_call;
use crucible_grammar::tools::{
    get_tools_by_detail, estimate_tool_tokens, DetailedUnixTools, MinimalUnixTools,
    SchemaDetail, SystemPromptStyle, UnixEnhancedTools,
};
use crucible_grammar::{ChatMessage, CompletionRequest, LlamaClient};
use std::collections::HashMap;
use std::time::Duration;

/// Test configuration with model selection
struct TestConfig {
    endpoint: String,
    model: String,
    max_tokens: u32,
}

impl TestConfig {
    fn with_model(model: &str) -> Self {
        Self {
            endpoint: std::env::var("LLAMA_ENDPOINT")
                .unwrap_or_else(|_| "https://llama.krohnos.io".to_string()),
            model: model.to_string(),
            max_tokens: 150,
        }
    }
}

/// Result of a schema/prompt test
#[derive(Debug)]
struct TestResult {
    schema_detail: SchemaDetail,
    prompt_style: SystemPromptStyle,
    user_prompt: String,
    output: String,
    tool_name: Option<String>,
    correct_tool: bool,
    tokens: u32,
    latency_ms: u64,
    prompt_tokens: usize,
}

/// Expected tool for each task
fn expected_tool(task: &str) -> &'static str {
    match task {
        "read" => "cat",
        "search" => "rg",
        "list" => "ls",
        "find" => "fd",
        _ => "unknown",
    }
}

/// Run a single test
async fn run_test(
    client: &LlamaClient,
    config: &TestConfig,
    schema_detail: SchemaDetail,
    prompt_style: SystemPromptStyle,
    user_prompt: &str,
    task: &str,
) -> Result<TestResult, Box<dyn std::error::Error>> {
    let tools = get_tools_by_detail(schema_detail);
    let system_prompt = prompt_style.generate(&tools);
    let prompt_tokens = estimate_tool_tokens(&tools) + system_prompt.len() / 4;

    let messages = vec![
        ChatMessage::system(&system_prompt),
        ChatMessage::user(user_prompt),
    ];

    let request = CompletionRequest::new(&config.model, messages)
        .with_max_tokens(config.max_tokens)
        .with_temperature(0.0)
        .without_thinking();

    let (response, duration) = client.complete(request).await?;
    let output = LlamaClient::extract_content(&response)
        .unwrap_or("")
        .to_string();

    let tool_call = parse_tool_call(&output);
    let tool_name = tool_call.as_ref().map(|tc| tc.name.clone());
    let expected = expected_tool(task);
    let correct_tool = tool_name.as_ref().map(|n| n == expected).unwrap_or(false);

    Ok(TestResult {
        schema_detail,
        prompt_style,
        user_prompt: user_prompt.to_string(),
        output,
        tool_name,
        correct_tool,
        tokens: response.usage.completion_tokens,
        latency_ms: duration.as_millis() as u64,
        prompt_tokens,
    })
}

// =============================================================================
// TESTS
// =============================================================================

#[tokio::test]
#[ignore = "Requires llama server - run with: cargo test -p crucible-grammar --test schema_detail -- --ignored --nocapture"]
async fn test_schema_detail_3b() {
    run_schema_detail_tests("granite-micro-3b-q6_k").await;
}

#[tokio::test]
#[ignore = "Requires llama server - run with: cargo test -p crucible-grammar --test schema_detail -- --ignored --nocapture"]
async fn test_schema_detail_8b() {
    run_schema_detail_tests("deepseek-r1-0528-qwen3-8b-q8_0").await;
}

#[tokio::test]
#[ignore = "Requires llama server - run with: cargo test -p crucible-grammar --test schema_detail -- --ignored --nocapture"]
async fn test_schema_detail_14b() {
    run_schema_detail_tests("qwen3-14b-ud-q8_k_xl").await;
}

async fn run_schema_detail_tests(model: &str) {
    let config = TestConfig::with_model(model);
    let client = LlamaClient::new(&config.endpoint)
        .with_timeout(Duration::from_secs(120));

    println!("\n{}", "=".repeat(100));
    println!(" SCHEMA DETAIL TEST - {}", model);
    println!("{}", "=".repeat(100));

    // Test cases
    let test_cases = [
        ("Read the contents of README.md", "read"),
        ("Search for 'TODO' in the source code", "search"),
        ("List the files in the current directory", "list"),
        ("Find all Rust files in the project", "find"),
    ];

    let schema_details = [SchemaDetail::Minimal, SchemaDetail::Standard, SchemaDetail::Detailed];
    let prompt_styles = [SystemPromptStyle::Standard]; // Focus on schema for now

    // Print schema token estimates
    println!("\nSchema Token Estimates:");
    for detail in &schema_details {
        let tools = get_tools_by_detail(*detail);
        let tokens = estimate_tool_tokens(&tools);
        println!("  {:10}: ~{} tokens", detail.name(), tokens);
    }

    // Results storage
    let mut results: HashMap<SchemaDetail, Vec<TestResult>> = HashMap::new();
    for detail in &schema_details {
        results.insert(*detail, Vec::new());
    }

    // Run tests
    for (prompt, task) in &test_cases {
        println!("\n{}", "-".repeat(100));
        println!("Prompt: {}", prompt);
        println!("Expected: {}", expected_tool(task));
        println!("{}", "-".repeat(100));

        for detail in &schema_details {
            for style in &prompt_styles {
                match run_test(&client, &config, *detail, *style, prompt, task).await {
                    Ok(result) => {
                        let status = if result.correct_tool { "✓" } else { "✗" };
                        let tool = result.tool_name.as_deref().unwrap_or("NONE");

                        println!(
                            "  {} {:10} {:12} | {:>5} tok | {:>4}ms | tool={:<6} | {}",
                            status,
                            detail.name(),
                            style.name(),
                            result.tokens,
                            result.latency_ms,
                            tool,
                            &result.output[..result.output.len().min(40)]
                        );

                        results.get_mut(detail).unwrap().push(result);
                    }
                    Err(e) => {
                        println!("  ✗ {:10} {:12} | ERROR: {}", detail.name(), style.name(), e);
                    }
                }
            }
        }
    }

    // Summary
    println!("\n{}", "=".repeat(100));
    println!(" SUMMARY - {}", model);
    println!("{}", "=".repeat(100));
    println!(
        "{:10} | {:>10} | {:>12} | {:>12} | {:>10}",
        "Schema", "Correct", "Avg Tokens", "Avg Latency", "Prompt Tok"
    );
    println!("{}", "-".repeat(100));

    for detail in &schema_details {
        let res = results.get(detail).unwrap();
        let correct = res.iter().filter(|r| r.correct_tool).count();
        let total = res.len();
        let avg_tokens = res.iter().map(|r| r.tokens).sum::<u32>() as f64 / total.max(1) as f64;
        let avg_latency = res.iter().map(|r| r.latency_ms).sum::<u64>() as f64 / total.max(1) as f64;
        let prompt_tok = res.first().map(|r| r.prompt_tokens).unwrap_or(0);

        println!(
            "{:10} | {:>7}/{:<2} | {:>10.1} | {:>10.0}ms | {:>10}",
            detail.name(),
            correct,
            total,
            avg_tokens,
            avg_latency,
            prompt_tok
        );
    }

    println!("{}", "=".repeat(100));
}

#[tokio::test]
#[ignore = "Requires llama server - run with: cargo test -p crucible-grammar --test schema_detail -- --ignored --nocapture"]
async fn test_prompt_styles_14b() {
    run_prompt_style_tests("qwen3-14b-ud-q8_k_xl").await;
}

#[tokio::test]
#[ignore = "Requires llama server - run with: cargo test -p crucible-grammar --test schema_detail -- --ignored --nocapture"]
async fn test_prompt_styles_8b() {
    run_prompt_style_tests("deepseek-r1-0528-qwen3-8b-q8_0").await;
}

async fn run_prompt_style_tests(model: &str) {
    let config = TestConfig::with_model(model);
    let client = LlamaClient::new(&config.endpoint)
        .with_timeout(Duration::from_secs(120));

    println!("\n{}", "=".repeat(100));
    println!(" SYSTEM PROMPT STYLE TEST - {}", model);
    println!("{}", "=".repeat(100));

    let test_cases = [
        ("Read the contents of README.md", "read"),
        ("Search for 'TODO' in the source code", "search"),
        ("List the files in the current directory", "list"),
        ("Find all Rust files in the project", "find"),
    ];

    let prompt_styles = [
        SystemPromptStyle::Minimal,
        SystemPromptStyle::Standard,
        SystemPromptStyle::Detailed,
        SystemPromptStyle::JsonFocused,
    ];

    // Use standard schema for all
    let schema_detail = SchemaDetail::Standard;

    // Results storage
    let mut results: HashMap<SystemPromptStyle, Vec<TestResult>> = HashMap::new();
    for style in &prompt_styles {
        results.insert(*style, Vec::new());
    }

    // Run tests
    for (prompt, task) in &test_cases {
        println!("\n{}", "-".repeat(100));
        println!("Prompt: {}", prompt);
        println!("Expected: {}", expected_tool(task));
        println!("{}", "-".repeat(100));

        for style in &prompt_styles {
            match run_test(&client, &config, schema_detail, *style, prompt, task).await {
                Ok(result) => {
                    let status = if result.correct_tool { "✓" } else { "✗" };
                    let tool = result.tool_name.as_deref().unwrap_or("NONE");

                    println!(
                        "  {} {:12} | {:>5} tok | {:>4}ms | tool={:<6} | {}",
                        status,
                        style.name(),
                        result.tokens,
                        result.latency_ms,
                        tool,
                        &result.output[..result.output.len().min(50)]
                    );

                    results.get_mut(style).unwrap().push(result);
                }
                Err(e) => {
                    println!("  ✗ {:12} | ERROR: {}", style.name(), e);
                }
            }
        }
    }

    // Summary
    println!("\n{}", "=".repeat(100));
    println!(" SUMMARY - {}", model);
    println!("{}", "=".repeat(100));
    println!(
        "{:12} | {:>10} | {:>12} | {:>12}",
        "Style", "Correct", "Avg Tokens", "Avg Latency"
    );
    println!("{}", "-".repeat(100));

    for style in &prompt_styles {
        let res = results.get(style).unwrap();
        let correct = res.iter().filter(|r| r.correct_tool).count();
        let total = res.len();
        let avg_tokens = res.iter().map(|r| r.tokens).sum::<u32>() as f64 / total.max(1) as f64;
        let avg_latency = res.iter().map(|r| r.latency_ms).sum::<u64>() as f64 / total.max(1) as f64;

        println!(
            "{:12} | {:>7}/{:<2} | {:>10.1} | {:>10.0}ms",
            style.name(),
            correct,
            total,
            avg_tokens,
            avg_latency
        );
    }

    println!("{}", "=".repeat(100));
}

#[tokio::test]
#[ignore = "Requires llama server - run with: cargo test -p crucible-grammar --test schema_detail -- --ignored --nocapture"]
async fn test_full_matrix_14b() {
    run_full_matrix("qwen3-14b-ud-q8_k_xl").await;
}

async fn run_full_matrix(model: &str) {
    let config = TestConfig::with_model(model);
    let client = LlamaClient::new(&config.endpoint)
        .with_timeout(Duration::from_secs(180));

    println!("\n{}", "=".repeat(120));
    println!(" FULL MATRIX: Schema Detail × Prompt Style - {}", model);
    println!("{}", "=".repeat(120));

    let test_cases = [
        ("Read the contents of README.md", "read"),
        ("Search for 'TODO' in the source code", "search"),
        ("List the files in the current directory", "list"),
        ("Find all Rust files in the project", "find"),
    ];

    let schema_details = [SchemaDetail::Minimal, SchemaDetail::Standard, SchemaDetail::Detailed];
    let prompt_styles = [
        SystemPromptStyle::Minimal,
        SystemPromptStyle::Standard,
        SystemPromptStyle::JsonFocused,
    ];

    // Results: (schema, prompt_style) -> results
    let mut results: HashMap<(SchemaDetail, SystemPromptStyle), Vec<TestResult>> = HashMap::new();

    for detail in &schema_details {
        for style in &prompt_styles {
            results.insert((*detail, *style), Vec::new());
        }
    }

    // Run all combinations
    for (prompt, task) in &test_cases {
        println!("\n{}", "-".repeat(120));
        println!("Prompt: {} | Expected: {}", prompt, expected_tool(task));
        println!("{}", "-".repeat(120));

        for detail in &schema_details {
            for style in &prompt_styles {
                match run_test(&client, &config, *detail, *style, prompt, task).await {
                    Ok(result) => {
                        let status = if result.correct_tool { "✓" } else { "✗" };
                        let tool = result.tool_name.as_deref().unwrap_or("NONE");

                        println!(
                            "  {} {:10}+{:12} | {:>4}tok {:>4}ms | {:<6} | {}",
                            status,
                            detail.name(),
                            style.name(),
                            result.tokens,
                            result.latency_ms,
                            tool,
                            &result.output[..result.output.len().min(35)]
                        );

                        results.get_mut(&(*detail, *style)).unwrap().push(result);
                    }
                    Err(e) => {
                        println!("  ✗ {:10}+{:12} | ERROR: {}", detail.name(), style.name(), e);
                    }
                }
            }
        }
    }

    // Matrix summary
    println!("\n{}", "=".repeat(120));
    println!(" ACCURACY MATRIX - {}", model);
    println!("{}", "=".repeat(120));

    // Header row
    print!("{:>12} |", "");
    for style in &prompt_styles {
        print!(" {:>12} |", style.name());
    }
    println!();
    println!("{}", "-".repeat(120));

    // Data rows
    for detail in &schema_details {
        print!("{:>12} |", detail.name());
        for style in &prompt_styles {
            let res = results.get(&(*detail, *style)).unwrap();
            let correct = res.iter().filter(|r| r.correct_tool).count();
            let total = res.len();
            print!(" {:>9}/{:<2} |", correct, total);
        }
        println!();
    }

    println!("{}", "=".repeat(120));

    // Token efficiency matrix
    println!("\n AVERAGE TOKENS");
    println!("{}", "-".repeat(120));
    print!("{:>12} |", "");
    for style in &prompt_styles {
        print!(" {:>12} |", style.name());
    }
    println!();
    println!("{}", "-".repeat(120));

    for detail in &schema_details {
        print!("{:>12} |", detail.name());
        for style in &prompt_styles {
            let res = results.get(&(*detail, *style)).unwrap();
            let avg = res.iter().map(|r| r.tokens).sum::<u32>() as f64 / res.len().max(1) as f64;
            print!(" {:>12.1} |", avg);
        }
        println!();
    }

    println!("{}", "=".repeat(120));
}

#[test]
fn test_print_schema_sizes() {
    println!("\n{}", "=".repeat(80));
    println!(" SCHEMA SIZE COMPARISON");
    println!("{}", "=".repeat(80));

    let variants = [
        ("Minimal", MinimalUnixTools::all()),
        ("Standard", UnixEnhancedTools::all()),
        ("Detailed", DetailedUnixTools::all()),
    ];

    for (name, tools) in &variants {
        println!("\n## {}", name);
        let mut total_chars = 0;

        for tool in tools {
            let schema_json = serde_json::to_string(&tool.input_schema).unwrap();
            let desc_len = tool.description.as_ref().map(|d| d.len()).unwrap_or(0);
            let tool_chars = tool.name.len() + desc_len + schema_json.len();
            total_chars += tool_chars;

            println!(
                "  {}: {} chars (name={}, desc={}, schema={})",
                tool.name,
                tool_chars,
                tool.name.len(),
                desc_len,
                schema_json.len()
            );
        }

        println!("  TOTAL: {} chars (~{} tokens)", total_chars, total_chars / 4);
    }

    println!("\n{}", "=".repeat(80));
}

#[test]
fn test_print_prompt_sizes() {
    println!("\n{}", "=".repeat(80));
    println!(" SYSTEM PROMPT SIZE COMPARISON");
    println!("{}", "=".repeat(80));

    let tools = UnixEnhancedTools::all();
    let styles = [
        SystemPromptStyle::Minimal,
        SystemPromptStyle::Standard,
        SystemPromptStyle::Detailed,
        SystemPromptStyle::JsonFocused,
    ];

    for style in &styles {
        let prompt = style.generate(&tools);
        println!("\n## {} ({} chars, ~{} tokens)", style.name(), prompt.len(), prompt.len() / 4);
        println!("{}", "-".repeat(60));
        // Print first 500 chars
        println!("{}...", &prompt[..prompt.len().min(500)]);
    }

    println!("\n{}", "=".repeat(80));
}

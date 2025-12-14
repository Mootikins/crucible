//! A/B tests comparing General vs Unix tool naming
//!
//! Tests three tool set variants:
//! 1. General: Read, Search, List, Find
//! 2. UnixRaw: cat, rg, ls, fd (no limits)
//! 3. UnixEnhanced: cat, rg, ls, fd (with limits)
//!
//! Run with: cargo test -p crucible-grammar --test tool_variants -- --ignored --nocapture

use crucible_grammar::mcp::{parse_tool_call, tools_to_system_prompt, ToolCallParams};
use crucible_grammar::tools::{
    analyze_variants, estimate_tool_tokens, get_tools, GeneralTools, ToolSetVariant,
    UnixEnhancedTools, UnixRawTools,
};
use crucible_grammar::{ChatMessage, CompletionRequest, LlamaClient};
use std::collections::HashMap;
use std::time::Duration;

/// Test configuration
struct TestConfig {
    endpoint: String,
    model: String,
    max_tokens: u32,
    /// Whether the model is a "thinking" model that always reasons
    is_thinking_model: bool,
}

impl Default for TestConfig {
    fn default() -> Self {
        let model = std::env::var("LLAMA_MODEL")
            .unwrap_or_else(|_| "granite-micro-3b-q6_k".to_string());
        let is_thinking = model.contains("thinking");
        Self {
            endpoint: std::env::var("LLAMA_ENDPOINT")
                .unwrap_or_else(|_| "https://llama.krohnos.io".to_string()),
            // Thinking models need more tokens for reasoning + response
            max_tokens: if is_thinking { 500 } else { 100 },
            is_thinking_model: is_thinking,
            model,
        }
    }
}

/// Result of a single tool call test
#[derive(Debug)]
struct ToolTestResult {
    variant: ToolSetVariant,
    prompt: String,
    output: String,
    tool_call: Option<ToolCallParams>,
    correct_tool: bool,
    tokens: u32,
    latency_ms: u64,
}

/// Map expected tool names across variants
fn expected_tool(variant: ToolSetVariant, task: &str) -> &'static str {
    match (variant, task) {
        (ToolSetVariant::General, "read") => "Read",
        (ToolSetVariant::General, "search") => "Search",
        (ToolSetVariant::General, "list") => "List",
        (ToolSetVariant::General, "find") => "Find",

        (ToolSetVariant::UnixRaw | ToolSetVariant::UnixEnhanced, "read") => "cat",
        (ToolSetVariant::UnixRaw | ToolSetVariant::UnixEnhanced, "search") => "rg",
        (ToolSetVariant::UnixRaw | ToolSetVariant::UnixEnhanced, "list") => "ls",
        (ToolSetVariant::UnixRaw | ToolSetVariant::UnixEnhanced, "find") => "fd",

        (ToolSetVariant::SemanticUnix, "read") => "Cat",
        (ToolSetVariant::SemanticUnix, "search") => "Ripgrep",
        (ToolSetVariant::SemanticUnix, "list") => "Ls",
        (ToolSetVariant::SemanticUnix, "find") => "FdFind",

        _ => "unknown",
    }
}

/// Run a test with a specific tool variant
async fn run_variant_test(
    client: &LlamaClient,
    config: &TestConfig,
    variant: ToolSetVariant,
    prompt: &str,
    task: &str,
) -> Result<ToolTestResult, Box<dyn std::error::Error>> {
    let tools = get_tools(variant);
    let system_prompt = tools_to_system_prompt(&tools);

    let messages = vec![
        ChatMessage::system(&system_prompt),
        ChatMessage::user(prompt),
    ];

    // Build request - only disable thinking for non-thinking models
    let mut request = CompletionRequest::new(&config.model, messages)
        .with_max_tokens(config.max_tokens)
        .with_temperature(0.0);

    // For non-thinking models, explicitly disable thinking to prevent
    // <think> tokens from interfering with output
    if !config.is_thinking_model {
        request = request.without_thinking();
    }

    let (response, duration) = client.complete(request).await?;

    // For thinking models, content is separate from reasoning_content
    let output = LlamaClient::extract_content(&response)
        .unwrap_or("")
        .to_string();

    let tool_call = parse_tool_call(&output);
    let expected = expected_tool(variant, task);
    let correct_tool = tool_call
        .as_ref()
        .map(|tc| tc.name == expected)
        .unwrap_or(false);

    Ok(ToolTestResult {
        variant,
        prompt: prompt.to_string(),
        output,
        tool_call,
        correct_tool,
        tokens: response.usage.completion_tokens,
        latency_ms: duration.as_millis() as u64,
    })
}

// =============================================================================
// TESTS
// =============================================================================

#[tokio::test]
#[ignore = "Requires llama server - run with: cargo test -p crucible-grammar --test tool_variants -- --ignored --nocapture"]
async fn test_tool_variants_ab() {
    let config = TestConfig::default();
    let client = LlamaClient::new(&config.endpoint).with_timeout(Duration::from_secs(60));

    println!("\n{}", "=".repeat(90));
    println!(" TOOL VARIANT A/B TEST - {}", config.model);
    println!("{}", "=".repeat(90));

    // Test cases: (prompt, task_type)
    let test_cases = [
        ("Read the contents of README.md", "read"),
        ("Search for 'TODO' in the source code", "search"),
        ("List the files in the current directory", "list"),
        ("Find all Rust files in the project", "find"),
        ("Show me what's in Cargo.toml", "read"),
        ("Look for 'error' in the codebase", "search"),
    ];

    let variants = [
        ToolSetVariant::General,
        ToolSetVariant::UnixRaw,
        ToolSetVariant::UnixEnhanced,
        ToolSetVariant::SemanticUnix,
    ];

    // Print schema token estimates
    println!("\nSchema Token Estimates:");
    for variant in &variants {
        let tools = get_tools(*variant);
        let tokens = estimate_tool_tokens(&tools);
        println!("  {:15}: ~{} tokens", variant.name(), tokens);
    }

    // Results storage
    let mut all_results: HashMap<ToolSetVariant, Vec<ToolTestResult>> = HashMap::new();
    for variant in &variants {
        all_results.insert(*variant, Vec::new());
    }

    // Run tests
    for (prompt, task) in &test_cases {
        println!("\n{}", "-".repeat(90));
        println!("Prompt: {}", prompt);
        println!("Task: {} (expected tools: Read/cat, Search/rg, List/ls, Find/fd)", task);
        println!("{}", "-".repeat(90));

        for variant in &variants {
            match run_variant_test(&client, &config, *variant, prompt, task).await {
                Ok(result) => {
                    let status = if result.correct_tool { "✓" } else { "✗" };
                    let tool_name = result
                        .tool_call
                        .as_ref()
                        .map(|tc| tc.name.as_str())
                        .unwrap_or("NONE");

                    println!(
                        "  {} {:15} | {:>6} tok | {:>4}ms | tool={:<10} | {}",
                        status,
                        variant.name(),
                        result.tokens,
                        result.latency_ms,
                        tool_name,
                        &result.output[..result.output.len().min(40)]
                    );

                    all_results.get_mut(variant).unwrap().push(result);
                }
                Err(e) => {
                    println!("  ✗ {:15} | ERROR: {}", variant.name(), e);
                }
            }
        }
    }

    // Summary
    println!("\n{}", "=".repeat(90));
    println!(" SUMMARY");
    println!("{}", "=".repeat(90));
    println!(
        "{:15} | {:>10} | {:>12} | {:>12} | {:>8}",
        "Variant", "Correct", "Avg Tokens", "Avg Latency", "Schema"
    );
    println!("{}", "-".repeat(90));

    for variant in &variants {
        let results = all_results.get(variant).unwrap();
        let correct = results.iter().filter(|r| r.correct_tool).count();
        let total = results.len();
        let avg_tokens = results.iter().map(|r| r.tokens).sum::<u32>() as f64 / total as f64;
        let avg_latency = results.iter().map(|r| r.latency_ms).sum::<u64>() as f64 / total as f64;
        let schema_tokens = estimate_tool_tokens(&get_tools(*variant));

        println!(
            "{:15} | {:>7}/{:<2} | {:>10.1} | {:>10.0}ms | {:>8}",
            variant.name(),
            correct,
            total,
            avg_tokens,
            avg_latency,
            schema_tokens
        );
    }

    // Per-task breakdown
    println!("\n{}", "=".repeat(90));
    println!(" PER-TASK ACCURACY");
    println!("{}", "=".repeat(90));

    let tasks = ["read", "search", "list", "find"];
    print!("{:15}", "Variant");
    for task in &tasks {
        print!(" | {:>10}", task);
    }
    println!();
    println!("{}", "-".repeat(90));

    for variant in &variants {
        let results = all_results.get(variant).unwrap();
        print!("{:15}", variant.name());

        for task in &tasks {
            let task_results: Vec<_> = test_cases
                .iter()
                .enumerate()
                .filter(|(_, (_, t))| t == task)
                .filter_map(|(i, _)| results.get(i))
                .collect();

            let correct = task_results.iter().filter(|r| r.correct_tool).count();
            let total = task_results.len();
            print!(" | {:>7}/{:<2}", correct, total);
        }
        println!();
    }

    println!("{}", "=".repeat(90));
}

#[test]
fn test_print_analysis() {
    println!("\n{}", "=".repeat(80));
    println!(" TOOL VARIANT ANALYSIS");
    println!("{}", "=".repeat(80));

    for analysis in analyze_variants() {
        println!("\n## {}", analysis.variant.name());
        println!("\nPros:");
        for pro in &analysis.pros {
            println!("  + {}", pro);
        }
        println!("\nCons:");
        for con in &analysis.cons {
            println!("  - {}", con);
        }
        println!("\nBest for:");
        for use_case in &analysis.best_for {
            println!("  → {}", use_case);
        }
    }

    println!("\n{}", "=".repeat(80));
}

#[test]
fn test_schema_sizes() {
    println!("\n{}", "=".repeat(60));
    println!(" SCHEMA SIZE COMPARISON");
    println!("{}", "=".repeat(60));

    let variants = [
        ("General", GeneralTools::all()),
        ("UnixRaw", UnixRawTools::all()),
        ("UnixEnhanced", UnixEnhancedTools::all()),
    ];

    for (name, tools) in &variants {
        println!("\n## {}", name);

        let mut total_chars = 0;
        for tool in tools {
            let schema_json = serde_json::to_string_pretty(&tool.input_schema).unwrap();
            let desc_len = tool.description.as_ref().map(|d| d.len()).unwrap_or(0);
            total_chars += tool.name.len() + desc_len + schema_json.len();

            println!(
                "  {}: {} chars (name={}, desc={}, schema={})",
                tool.name,
                tool.name.len() + desc_len + schema_json.len(),
                tool.name.len(),
                desc_len,
                schema_json.len()
            );
        }

        println!("  TOTAL: {} chars (~{} tokens)", total_chars, total_chars / 4);
    }

    println!("\n{}", "=".repeat(60));
}

#[tokio::test]
#[ignore = "Requires llama server"]
async fn test_ambiguous_prompts() {
    let config = TestConfig::default();
    let client = LlamaClient::new(&config.endpoint).with_timeout(Duration::from_secs(60));

    println!("\n{}", "=".repeat(80));
    println!(" AMBIGUOUS PROMPT TEST");
    println!("{}", "=".repeat(80));
    println!("Testing how different tool names handle ambiguous requests...\n");

    // Ambiguous prompts that could map to multiple tools
    let ambiguous = [
        ("Show me the config file", "Could be Read/cat or List/ls"),
        ("What's in the src folder?", "Could be List/ls or Read/cat"),
        ("Find errors in the code", "Could be Search/rg or Find/fd"),
        ("Get the test files", "Could be Find/fd or List/ls"),
    ];

    let variants = [ToolSetVariant::General, ToolSetVariant::UnixRaw];

    for (prompt, note) in &ambiguous {
        println!("{}", "-".repeat(80));
        println!("Prompt: {}", prompt);
        println!("Note: {}", note);

        for variant in &variants {
            let tools = get_tools(*variant);
            let system_prompt = tools_to_system_prompt(&tools);

            let messages = vec![
                ChatMessage::system(&system_prompt),
                ChatMessage::user(*prompt),
            ];

            let mut request = CompletionRequest::new(&config.model, messages)
                .with_max_tokens(config.max_tokens)
                .with_temperature(0.0);

            if !config.is_thinking_model {
                request = request.without_thinking();
            }

            if let Ok((response, _)) = client.complete(request).await {
                let output = LlamaClient::extract_content(&response).unwrap_or("");
                if let Some(tc) = parse_tool_call(output) {
                    println!("  {:15} → {}", variant.name(), tc.name);
                } else {
                    println!("  {:15} → PARSE FAILED: {}", variant.name(), &output[..output.len().min(50)]);
                }
            }
        }
    }

    println!("{}", "=".repeat(80));
}

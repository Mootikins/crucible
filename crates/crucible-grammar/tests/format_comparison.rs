//! Integration tests comparing CLI tool call formats
//!
//! Tests three formats:
//! 1. Structured: `rg(pattern="TODO", path="src")`
//! 2. Passthrough: `rg(args="-n TODO src/")`
//! 3. Raw CLI: `rg -n TODO src/`
//!
//! Run with: cargo test -p crucible-grammar --test format_comparison

use crucible_grammar::{
    ChatMessage, CompletionRequest, Grammar, LlamaClient, ParsedToolCall, Score, Scorer,
};
use std::collections::HashMap;
use std::time::Duration;

/// Test configuration
struct FormatTestConfig {
    endpoint: String,
    model: String,
    max_tokens: u32,
}

impl Default for FormatTestConfig {
    fn default() -> Self {
        Self {
            endpoint: std::env::var("LLAMA_ENDPOINT")
                .unwrap_or_else(|_| "https://llama.krohnos.io".to_string()),
            model: std::env::var("LLAMA_MODEL")
                .unwrap_or_else(|_| "granite-micro-3b-q6_k".to_string()),
            max_tokens: 80,
        }
    }
}

/// System prompts for different formats
mod prompts {
    /// Structured format: tool(param="value")
    pub const STRUCTURED: &str = r#"You are a tool-calling assistant. Available tools:
- rg(pattern, path?): Search file contents
- fd(pattern, path?): Find files by name
- cat(path): Read file contents
- ls(path): List directory

Output ONLY the tool call in format: tool(param="value")"#;

    /// Passthrough format: tool(args="...")
    pub const PASSTHROUGH: &str = r#"You are a tool-calling assistant. Available tools:
- rg(args): ripgrep - search file contents
- fd(args): fd-find - find files by name
- cat(args): read file contents
- ls(args): list directory

Output ONLY the tool call in format: tool(args="command args")"#;

    /// Raw CLI format: command args
    pub const RAW_CLI: &str = r#"Commands: rg (search content), fd (find files), cat (read), ls (list).
Output ONLY the command."#;
}

/// Grammars for different formats
mod grammars {
    pub const STRUCTURED: &str = r#"root ::= tool
tool ::= rg | fd | cat | ls

rg ::= "rg(pattern=\"" pattern "\"" rg-opts? ")"
rg-opts ::= ", path=\"" path "\""

fd ::= "fd(pattern=\"" pattern "\"" fd-opts? ")"
fd-opts ::= ", path=\"" path "\""

cat ::= "cat(path=\"" path "\")"
ls ::= "ls(path=\"" path "\")"

pattern ::= [^"]+
path ::= [a-zA-Z0-9_./-]+"#;

    pub const PASSTHROUGH: &str = r#"root ::= tool "(" "args=\"" args "\"" ")"
tool ::= "rg" | "fd" | "cat" | "ls"
args ::= [^"]+"#;

    pub const RAW_CLI: &str = r#"root ::= command " " args
command ::= "rg" | "fd" | "cat" | "ls"
args ::= [^\n]+"#;
}

/// Result of a single format test
#[derive(Debug)]
struct FormatTestResult {
    format_name: String,
    output: String,
    parsed: Option<ParsedToolCall>,
    score: Score,
    tokens: u32,
    latency_ms: u64,
}

/// Run a test with a specific format
async fn run_format_test(
    client: &LlamaClient,
    config: &FormatTestConfig,
    format_name: &str,
    system_prompt: &str,
    grammar: Option<&str>,
    user_prompt: &str,
    expected_tool: &str,
    expected_params: HashMap<String, serde_json::Value>,
) -> Result<FormatTestResult, Box<dyn std::error::Error>> {
    let messages = vec![
        ChatMessage::system(system_prompt),
        ChatMessage::user(user_prompt),
    ];

    let mut request = CompletionRequest::new(&config.model, messages)
        .with_max_tokens(config.max_tokens)
        .with_temperature(0.0)
        .without_thinking();

    if let Some(g) = grammar {
        request = request.with_grammar(g);
    }

    let (response, duration) = client.complete(request).await?;
    let output = LlamaClient::extract_content(&response)
        .unwrap_or("")
        .to_string();

    let parsed = Scorer::parse(&output);
    let expected = crucible_grammar::scoring::ExpectedToolCall {
        tool: expected_tool.to_string(),
        params: expected_params,
    };
    let score = Scorer::score_unified(&output, &expected);

    Ok(FormatTestResult {
        format_name: format_name.to_string(),
        output,
        parsed,
        score,
        tokens: response.usage.completion_tokens,
        latency_ms: duration.as_millis() as u64,
    })
}

/// Compare all three formats for a given prompt
async fn compare_formats(
    client: &LlamaClient,
    config: &FormatTestConfig,
    user_prompt: &str,
    expected_tool: &str,
    expected_params: HashMap<String, serde_json::Value>,
) -> Vec<FormatTestResult> {
    let formats = [
        ("structured", prompts::STRUCTURED, Some(grammars::STRUCTURED)),
        (
            "passthrough",
            prompts::PASSTHROUGH,
            Some(grammars::PASSTHROUGH),
        ),
        ("raw_cli", prompts::RAW_CLI, Some(grammars::RAW_CLI)),
    ];

    let mut results = Vec::new();

    for (name, system, grammar) in formats {
        match run_format_test(
            client,
            config,
            name,
            system,
            grammar,
            user_prompt,
            expected_tool,
            expected_params.clone(),
        )
        .await
        {
            Ok(result) => results.push(result),
            Err(e) => eprintln!("Error testing {}: {}", name, e),
        }
    }

    results
}

/// Print comparison results
fn print_comparison(prompt: &str, results: &[FormatTestResult]) {
    println!("\n{}", "=".repeat(60));
    println!("Prompt: {}", prompt);
    println!("{}", "-".repeat(60));

    for r in results {
        let status = if r.score.tool_correct && r.score.param_accuracy >= 0.5 {
            "✓"
        } else if r.score.parsed {
            "~"
        } else {
            "✗"
        };

        println!(
            "{} {:12} | {:3} tok | {:4}ms | {}",
            status, r.format_name, r.tokens, r.latency_ms, r.output
        );
    }
}

// ===== Tests =====

#[tokio::test]
#[ignore = "Requires llama server"]
async fn test_search_todo_formats() {
    let config = FormatTestConfig::default();
    let client = LlamaClient::new(&config.endpoint).with_timeout(Duration::from_secs(60));

    let params: HashMap<String, serde_json::Value> =
        [("pattern".to_string(), serde_json::json!("TODO"))]
            .into_iter()
            .collect();

    let results = compare_formats(
        &client,
        &config,
        "Search for TODO comments in the source code",
        "rg",
        params,
    )
    .await;

    print_comparison("Search for TODO comments in the source code", &results);

    // At least one format should succeed
    assert!(
        results.iter().any(|r| r.score.tool_correct),
        "No format produced correct tool"
    );
}

#[tokio::test]
#[ignore = "Requires llama server"]
async fn test_find_rust_files_formats() {
    let config = FormatTestConfig::default();
    let client = LlamaClient::new(&config.endpoint).with_timeout(Duration::from_secs(60));

    let params: HashMap<String, serde_json::Value> =
        [("pattern".to_string(), serde_json::json!(".rs"))]
            .into_iter()
            .collect();

    let results = compare_formats(&client, &config, "Find all .rs files in the project", "fd", params)
        .await;

    print_comparison("Find all .rs files in the project", &results);

    assert!(
        results.iter().any(|r| r.score.tool_correct),
        "No format produced correct tool"
    );
}

#[tokio::test]
#[ignore = "Requires llama server"]
async fn test_read_readme_formats() {
    let config = FormatTestConfig::default();
    let client = LlamaClient::new(&config.endpoint).with_timeout(Duration::from_secs(60));

    let params: HashMap<String, serde_json::Value> =
        [("path".to_string(), serde_json::json!("README.md"))]
            .into_iter()
            .collect();

    let results = compare_formats(&client, &config, "Read the README.md file", "read", params).await;

    print_comparison("Read the README.md file", &results);

    // For raw CLI, "cat" maps to "read" in scoring
    assert!(
        results.iter().any(|r| r.score.tool_correct),
        "No format produced correct tool"
    );
}

/// Run all comparison tests and print summary
#[tokio::test]
#[ignore = "Requires llama server - run with: cargo test -p crucible-grammar --test format_comparison -- --ignored --nocapture"]
async fn test_full_format_comparison() {
    let config = FormatTestConfig::default();
    let client = LlamaClient::new(&config.endpoint).with_timeout(Duration::from_secs(60));

    println!("\n{}", "=".repeat(70));
    println!(" CLI Tool Format Comparison - {} ", config.model);
    println!("{}", "=".repeat(70));

    let test_cases = [
        (
            "Search for TODO comments",
            "rg",
            vec![("pattern", "TODO")],
        ),
        (
            "Find all .rs files in the project",
            "fd",
            vec![("pattern", ".rs")],
        ),
        ("Read the README.md file", "read", vec![("path", "README.md")]),
        ("List files in the current directory", "ls", vec![("path", ".")]),
        (
            "Search for 'impl Scorer' in rust files",
            "rg",
            vec![("pattern", "impl Scorer")],
        ),
    ];

    let mut summary: HashMap<String, (usize, usize, u32)> = HashMap::new(); // (correct, total, total_tokens)

    for (prompt, tool, params) in test_cases {
        let params: HashMap<String, serde_json::Value> = params
            .into_iter()
            .map(|(k, v)| (k.to_string(), serde_json::json!(v)))
            .collect();

        let results = compare_formats(&client, &config, prompt, tool, params).await;
        print_comparison(prompt, &results);

        for r in &results {
            let entry = summary.entry(r.format_name.clone()).or_insert((0, 0, 0));
            entry.1 += 1;
            entry.2 += r.tokens;
            if r.score.tool_correct && r.score.param_accuracy >= 0.5 {
                entry.0 += 1;
            }
        }
    }

    println!("\n{}", "=".repeat(70));
    println!(" SUMMARY ");
    println!("{}", "-".repeat(70));
    println!("{:12} | {:>8} | {:>12} | {:>10}", "Format", "Accuracy", "Avg Tokens", "Score");
    println!("{}", "-".repeat(70));

    for (format, (correct, total, tokens)) in &summary {
        let accuracy = *correct as f64 / *total as f64 * 100.0;
        let avg_tokens = *tokens as f64 / *total as f64;
        println!(
            "{:12} | {:6.1}% | {:10.1} | {}/{}",
            format, accuracy, avg_tokens, correct, total
        );
    }
    println!("{}\n", "=".repeat(70));
}

// ===== Unit tests for scoring =====

#[test]
fn test_score_structured_format() {
    let expected = crucible_grammar::scoring::ExpectedToolCall {
        tool: "rg".to_string(),
        params: [("pattern".to_string(), serde_json::json!("TODO"))]
            .into_iter()
            .collect(),
    };

    let score = Scorer::score_unified(r#"rg(pattern="TODO")"#, &expected);
    assert!(score.parsed);
    assert!(score.tool_correct);
    assert_eq!(score.param_accuracy, 1.0);
}

#[test]
fn test_score_passthrough_format() {
    let expected = crucible_grammar::scoring::ExpectedToolCall {
        tool: "rg".to_string(),
        params: [("pattern".to_string(), serde_json::json!("TODO"))]
            .into_iter()
            .collect(),
    };

    let score = Scorer::score_unified(r#"rg(args="-n TODO src/")"#, &expected);
    assert!(score.parsed);
    assert!(score.tool_correct);
    // "TODO" appears in args, so param_accuracy should be 1.0
    assert_eq!(score.param_accuracy, 1.0);
}

#[test]
fn test_score_raw_cli_format() {
    let expected = crucible_grammar::scoring::ExpectedToolCall {
        tool: "rg".to_string(),
        params: [("pattern".to_string(), serde_json::json!("TODO"))]
            .into_iter()
            .collect(),
    };

    let score = Scorer::score_unified("rg -n TODO src/", &expected);
    assert!(score.parsed);
    assert!(score.tool_correct);
    assert_eq!(score.param_accuracy, 1.0);
}

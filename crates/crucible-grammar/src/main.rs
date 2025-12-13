//! Grammar-constrained generation test runner

use clap::{Parser, ValueEnum};
use crucible_grammar::{
    grammar::{self, Grammar},
    harness::{ChatTemplate, HarnessConfig, Mode, TestHarness, TestSuite},
};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum TemplateArg {
    /// Qwen3/ChatML format
    Qwen3,
    /// Llama 3 format
    Llama3,
    /// GPT-OSS format with channel tags
    GptOss,
    /// DeepSeek R1 format
    DeepseekR1,
}

impl From<TemplateArg> for ChatTemplate {
    fn from(arg: TemplateArg) -> Self {
        match arg {
            TemplateArg::Qwen3 => ChatTemplate::Qwen3,
            TemplateArg::Llama3 => ChatTemplate::Llama3,
            TemplateArg::GptOss => ChatTemplate::GptOss,
            TemplateArg::DeepseekR1 => ChatTemplate::DeepSeekR1,
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "crucible-grammar")]
#[command(about = "Test grammar-constrained generation for tool calling")]
struct Args {
    /// API endpoint
    #[arg(long, default_value = "https://llama.krohnos.io")]
    endpoint: String,

    /// Model to use
    #[arg(long, default_value = "qwen3-14b-ud-q8_k_xl")]
    model: String,

    /// Chat template format for text completions
    #[arg(long, value_enum, default_value = "qwen3")]
    template: TemplateArg,

    /// Grammar file (GBNF)
    #[arg(long)]
    grammar: Option<PathBuf>,

    /// Use built-in L0+L1 tool grammar
    #[arg(long)]
    builtin_grammar: bool,

    /// Use built-in grammar that allows <think>...</think> blocks
    #[arg(long)]
    thinking_grammar: bool,

    /// Test suite file (TOML)
    #[arg(long)]
    suite: Option<PathBuf>,

    /// Output results as JSON
    #[arg(long)]
    json: bool,

    /// Run only constrained mode
    #[arg(long)]
    constrained_only: bool,

    /// Run only unconstrained mode
    #[arg(long)]
    unconstrained_only: bool,

    /// Quick test with a single prompt
    #[arg(long)]
    quick: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Load grammar
    let grammar = if let Some(path) = &args.grammar {
        Some(Grammar::from_file(path)?)
    } else if args.thinking_grammar {
        Some(grammar::presets::l0_l1_tools_with_thinking())
    } else if args.builtin_grammar {
        Some(grammar::presets::l0_l1_tools())
    } else {
        None
    };

    // Whether to allow thinking (don't disable for thinking grammar)
    let allow_thinking = args.thinking_grammar;

    // Build config
    // Use higher token limit for thinking mode (needs room for reasoning + tool call)
    let max_tokens = if allow_thinking { 512 } else { 128 };

    let config = HarnessConfig {
        endpoint: args.endpoint,
        model: args.model.clone(),
        grammar,
        allow_thinking,
        max_tokens,
        chat_template: args.template.into(),
        ..Default::default()
    };

    let harness = TestHarness::new(config);

    // Quick test mode
    if let Some(prompt) = args.quick {
        println!("Quick test: {}", prompt);
        println!("Model: {}", args.model);
        println!();

        let case = crucible_grammar::harness::TestCase {
            name: "quick".to_string(),
            prompt,
            system: None,
            expected: crucible_grammar::scoring::ExpectedToolCall {
                tool: "unknown".to_string(),
                params: Default::default(),
            },
        };

        if !args.unconstrained_only {
            println!("=== Constrained ===");
            match harness.run_test(&case, Mode::Constrained).await {
                Ok(result) => {
                    if let Some(thinking) = &result.thinking {
                        println!("Thinking: {}", thinking);
                        println!();
                    }
                    println!("Output: {}", result.output);
                    println!("Latency: {}ms", result.latency_ms);
                }
                Err(e) => println!("Error: {}", e),
            }
            println!();
        }

        if !args.constrained_only {
            println!("=== Unconstrained ===");
            match harness.run_test(&case, Mode::Unconstrained).await {
                Ok(result) => {
                    println!("Output: {}", result.output);
                    println!("Latency: {}ms", result.latency_ms);
                }
                Err(e) => println!("Error: {}", e),
            }
        }

        return Ok(());
    }

    // Suite mode
    let suite = if let Some(path) = &args.suite {
        TestSuite::from_file(path)?
    } else {
        // Default test suite
        TestSuite {
            name: "default".to_string(),
            cases: vec![
                crucible_grammar::harness::TestCase {
                    name: "read_readme".to_string(),
                    prompt: "Read the README.md file".to_string(),
                    system: None,
                    expected: crucible_grammar::scoring::ExpectedToolCall {
                        tool: "read".to_string(),
                        params: [("path".to_string(), serde_json::json!("README.md"))]
                            .into_iter()
                            .collect(),
                    },
                },
                crucible_grammar::harness::TestCase {
                    name: "search_todo".to_string(),
                    prompt: "Search for TODO comments in the src directory".to_string(),
                    system: None,
                    expected: crucible_grammar::scoring::ExpectedToolCall {
                        tool: "rg".to_string(),
                        params: [
                            ("pattern".to_string(), serde_json::json!("TODO")),
                            ("path".to_string(), serde_json::json!("src")),
                        ]
                        .into_iter()
                        .collect(),
                    },
                },
                crucible_grammar::harness::TestCase {
                    name: "git_status".to_string(),
                    prompt: "Show the git status".to_string(),
                    system: None,
                    expected: crucible_grammar::scoring::ExpectedToolCall {
                        tool: "git".to_string(),
                        params: [("args".to_string(), serde_json::json!("status"))]
                            .into_iter()
                            .collect(),
                    },
                },
            ],
        }
    };

    println!("Running test suite: {}", suite.name);
    println!("Model: {}", args.model);
    println!("Cases: {}", suite.cases.len());
    println!();

    let results = harness.run_suite(&suite).await?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&results)?);
    } else {
        // Print results table
        println!(
            "{:<20} {:<12} {:<8} {:<8} {:<8} {:<8}",
            "Case", "Mode", "Parsed", "Tool", "Params", "Latency"
        );
        println!("{}", "-".repeat(72));

        for result in &results {
            println!(
                "{:<20} {:<12} {:<8} {:<8} {:<8.2} {:<8}ms",
                result.case,
                format!("{:?}", result.mode).to_lowercase(),
                if result.score.parsed { "✓" } else { "✗" },
                if result.score.tool_correct { "✓" } else { "✗" },
                result.score.param_accuracy,
                result.latency_ms
            );
        }

        println!();
        println!("=== Summary ===");
        let summary = TestHarness::summarize(&results);
        for (mode, agg) in &summary {
            println!(
                "{:?}: parse={:.0}% tool={:.0}% params={:.0}% overall={:.2}",
                mode,
                agg.parse_rate * 100.0,
                agg.tool_accuracy * 100.0,
                agg.param_accuracy * 100.0,
                agg.overall
            );
        }
    }

    Ok(())
}

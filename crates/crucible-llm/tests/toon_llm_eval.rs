//! TOON LLM Evaluation Tests
//!
//! Integration tests that evaluate how well LLMs can read/write TOON format.
//!
//! ## Running Tests
//!
//! These tests require a running Ollama instance and are marked `#[ignore]`.
//!
//! ```bash
//! # Run with default settings (localhost:11434, qwen3:8b)
//! cargo test --package crucible-llm --test toon_llm_eval -- --ignored --nocapture
//!
//! # With custom endpoint
//! OLLAMA_BASE_URL=https://your-ollama.example.com \
//! TOON_EVAL_MODEL=llama3.2:8b \
//! cargo test --package crucible-llm --test toon_llm_eval -- --ignored --nocapture
//! ```

#[path = "toon_eval/mod.rs"]
mod toon_eval;

use serde::{Deserialize, Serialize};
use std::env;
use std::path::PathBuf;
use std::time::Duration;
use toon_eval::{
    all_fixtures, build_prompt, build_query_prompt, fixtures_by_complexity, query_fixtures,
    validate_json_output, validate_toon_output, Complexity, ConversionDirection, EvalReport,
    PromptConfig, QueryFixture, TestResult,
};

/// Get Ollama base URL from environment
fn ollama_base_url() -> String {
    env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://localhost:11434".to_string())
}

/// Get model to test from environment
fn eval_model() -> String {
    env::var("TOON_EVAL_MODEL").unwrap_or_else(|_| "qwen3:8b".to_string())
}

/// Ollama chat request
#[derive(Debug, Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
    options: OllamaOptions,
}

#[derive(Debug, Serialize)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct OllamaOptions {
    temperature: f32,
    num_predict: i32,
}

/// Ollama chat response
#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    message: OllamaResponseMessage,
}

#[derive(Debug, Deserialize)]
struct OllamaResponseMessage {
    content: String,
}

/// Call Ollama chat API
async fn call_ollama(prompt: &str) -> Result<String, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let url = format!("{}/api/chat", ollama_base_url());

    let request = OllamaChatRequest {
        model: eval_model(),
        messages: vec![OllamaMessage {
            role: "user".to_string(),
            content: prompt.to_string(),
        }],
        stream: false,
        options: OllamaOptions {
            temperature: 0.1, // Low temperature for consistent output
            num_predict: 2048,
        },
    };

    let response = client
        .post(&url)
        .json(&request)
        .timeout(Duration::from_secs(120))
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Ollama API error ({}): {}", status, body).into());
    }

    let chat_response: OllamaChatResponse = response.json().await?;
    Ok(chat_response.message.content)
}

/// Convert JSON to TOON for test input
fn json_to_toon_for_input(json: &serde_json::Value) -> String {
    toon_format::encode_default(json).unwrap_or_else(|_| "ERROR".to_string())
}

// =============================================================================
// Unit Tests (no LLM required)
// =============================================================================

#[test]
fn test_fixtures_are_valid_json() {
    let fixtures = all_fixtures();
    assert!(!fixtures.is_empty(), "Should have fixtures");

    for fixture in fixtures {
        // Just ensure the JSON is valid (it's constructed with json! macro)
        let json_str = serde_json::to_string(&fixture.json).unwrap();
        assert!(
            !json_str.is_empty(),
            "Fixture {} should serialize",
            fixture.id
        );
    }
}

#[test]
fn test_toon_format_roundtrip() {
    // Verify toon-format crate works correctly
    let json = serde_json::json!({"name": "Ada", "age": 30});
    let toon = toon_format::encode_default(&json).unwrap();
    let decoded: serde_json::Value = toon_format::decode_default(&toon).unwrap();
    assert_eq!(json, decoded);
}

#[test]
fn test_prompt_building() {
    let prompt = build_prompt(
        &PromptConfig::FewShot(2),
        ConversionDirection::JsonToToon,
        r#"{"test": 1}"#,
    );

    assert!(prompt.contains("Example: Simple Object"));
    assert!(prompt.contains("Example: Nested Object"));
    assert!(prompt.contains("Convert the following JSON to TOON"));
    assert!(prompt.contains(r#"{"test": 1}"#));
}

// =============================================================================
// LLM Evaluation Tests (require Ollama)
// =============================================================================

/// Run a single conversion test
async fn run_conversion_test(
    fixture_id: &str,
    json: &serde_json::Value,
    direction: ConversionDirection,
    config: &PromptConfig,
) -> TestResult {
    let input = match direction {
        ConversionDirection::JsonToToon => serde_json::to_string_pretty(json).unwrap(),
        ConversionDirection::ToonToJson => json_to_toon_for_input(json),
    };

    let prompt = build_prompt(config, direction, &input);

    let response = match call_ollama(&prompt).await {
        Ok(r) => r,
        Err(e) => {
            return TestResult {
                fixture_id: fixture_id.to_string(),
                direction,
                config: config.clone(),
                validation: toon_eval::ValidationResult::failure(
                    vec![toon_eval::ToonError::InvalidSyntax(format!(
                        "API error: {}",
                        e
                    ))],
                    format!("Error: {}", e),
                ),
            };
        }
    };

    let validation = match direction {
        ConversionDirection::JsonToToon => validate_toon_output(&response, json),
        ConversionDirection::ToonToJson => validate_json_output(&response, json),
    };

    TestResult {
        fixture_id: fixture_id.to_string(),
        direction,
        config: config.clone(),
        validation,
    }
}

/// Full evaluation across all fixtures and configurations
#[tokio::test]
#[ignore] // Requires Ollama
async fn test_full_toon_evaluation() {
    println!("\n========================================");
    println!("TOON LLM Evaluation");
    println!("Model: {}", eval_model());
    println!("Endpoint: {}", ollama_base_url());
    println!("========================================\n");

    let mut report = EvalReport::new(eval_model(), ollama_base_url());
    let fixtures = all_fixtures();
    let configs = PromptConfig::all_standard();

    let total_tests = fixtures.len() * configs.len() * 2; // Both directions
    let mut completed = 0;

    // Test JSON → TOON (writing TOON)
    println!("Testing JSON → TOON...\n");
    for config in &configs {
        println!("  Config: {}", config);
        for fixture in &fixtures {
            let result = run_conversion_test(
                fixture.id,
                &fixture.json,
                ConversionDirection::JsonToToon,
                config,
            )
            .await;

            let status = if result.validation.success {
                "✓"
            } else {
                "✗"
            };
            println!("    {} {}", status, fixture.id);

            report.add_result(result);
            completed += 1;
        }
        println!();
    }

    // Test TOON → JSON (reading TOON)
    println!("Testing TOON → JSON...\n");
    for config in &configs {
        println!("  Config: {}", config);
        for fixture in &fixtures {
            let result = run_conversion_test(
                fixture.id,
                &fixture.json,
                ConversionDirection::ToonToJson,
                config,
            )
            .await;

            let status = if result.validation.success {
                "✓"
            } else {
                "✗"
            };
            println!("    {} {}", status, fixture.id);

            report.add_result(result);
            completed += 1;
        }
        println!();
    }

    println!("Completed {}/{} tests", completed, total_tests);

    // Save report
    let report_path =
        PathBuf::from(env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string()))
            .join("toon_eval_report.md");

    if let Err(e) = report.save(&report_path) {
        eprintln!("Failed to save report: {}", e);
    } else {
        println!("\nReport saved to: {}", report_path.display());
    }

    // Print summary
    let aggregated = report.aggregate();
    println!("\n========================================");
    println!("SUMMARY");
    println!("========================================\n");

    println!("| Direction | Config | Pass | Fail | Rate |");
    println!("|-----------|--------|------|------|------|");

    let mut keys: Vec<_> = aggregated.keys().collect();
    keys.sort_by(|a, b| a.0.to_string().cmp(&b.0.to_string()).then(a.1.cmp(&b.1)));

    for key in keys {
        let stats = &aggregated[key];
        println!(
            "| {} | {} | {} | {} | {:.1}% |",
            key.0,
            key.1,
            stats.passed,
            stats.failed,
            stats.pass_rate()
        );
    }
}

/// Quick smoke test with just primitives
#[tokio::test]
#[ignore] // Requires Ollama
async fn test_toon_primitives_only() {
    println!("\nQuick TOON evaluation (primitives only)...\n");

    let fixtures = fixtures_by_complexity(Complexity::Primitives);
    let configs = vec![
        PromptConfig::ZeroShot,
        PromptConfig::FewShot(1),
        PromptConfig::Full,
    ];

    let mut report = EvalReport::new(eval_model(), ollama_base_url());

    for config in &configs {
        println!("Config: {}", config);

        for fixture in &fixtures {
            // Test both directions
            for direction in [
                ConversionDirection::JsonToToon,
                ConversionDirection::ToonToJson,
            ] {
                let result =
                    run_conversion_test(fixture.id, &fixture.json, direction, config).await;

                let status = if result.validation.success {
                    "✓"
                } else {
                    "✗"
                };
                println!("  {} {} ({})", status, fixture.id, direction);

                report.add_result(result);
            }
        }
        println!();
    }

    // Print quick summary
    let aggregated = report.aggregate();
    for (key, stats) in &aggregated {
        println!(
            "{} / {}: {}/{} ({:.1}%)",
            key.0,
            key.1,
            stats.passed,
            stats.passed + stats.failed,
            stats.pass_rate()
        );
    }
}

/// Test tabular arrays specifically (TOON's strength)
#[tokio::test]
#[ignore] // Requires Ollama
async fn test_toon_tabular_arrays() {
    println!("\nTOON tabular array evaluation...\n");

    let fixtures = fixtures_by_complexity(Complexity::TabularArrays);
    let configs = vec![
        PromptConfig::ZeroShot,
        PromptConfig::FewShot(3), // Include tabular example
        PromptConfig::Full,
    ];

    for config in &configs {
        println!("Config: {}", config);

        for fixture in &fixtures {
            let result = run_conversion_test(
                fixture.id,
                &fixture.json,
                ConversionDirection::JsonToToon,
                &config,
            )
            .await;

            let status = if result.validation.success {
                "✓"
            } else {
                "✗"
            };
            print!("  {} {}", status, fixture.id);

            if !result.validation.success {
                let errors: Vec<_> = result
                    .validation
                    .errors
                    .iter()
                    .map(|e| e.to_string())
                    .collect();
                print!(" - {}", errors.join(", "));
            }
            println!();

            // Show what the LLM produced vs expected
            if !result.validation.success {
                println!("    Expected TOON:");
                if let Some(expected) = fixture.expected_toon {
                    for line in expected.lines() {
                        println!("      {}", line);
                    }
                } else {
                    println!("      (no canonical form)");
                }
                println!("    Got:");
                for line in result.validation.raw_response.lines().take(5) {
                    println!("      {}", line);
                }
                println!();
            }
        }
        println!();
    }
}

/// Test TOON → JSON only (expected to work well)
#[tokio::test]
#[ignore] // Requires Ollama
async fn test_toon_reading_only() {
    println!("\nTOON reading evaluation (TOON → JSON)...\n");

    let fixtures = all_fixtures();

    // Just test zero-shot - if it can't read TOON zero-shot, something's wrong
    let config = PromptConfig::ZeroShot;

    let mut passed = 0;
    let mut failed = 0;

    for fixture in &fixtures {
        let result = run_conversion_test(
            fixture.id,
            &fixture.json,
            ConversionDirection::ToonToJson,
            &config,
        )
        .await;

        if result.validation.success {
            passed += 1;
            println!("  ✓ {}", fixture.id);
        } else {
            failed += 1;
            println!("  ✗ {} - {:?}", fixture.id, result.validation.errors);
        }
    }

    println!(
        "\nResults: {}/{} passed ({:.1}%)",
        passed,
        passed + failed,
        (passed as f64 / (passed + failed) as f64) * 100.0
    );

    // We expect high success rate for reading
    assert!(
        passed as f64 / (passed + failed) as f64 > 0.7,
        "Expected >70% success rate for TOON reading, got {:.1}%",
        (passed as f64 / (passed + failed) as f64) * 100.0
    );
}

/// Test how example count affects TOON → JSON comprehension
#[tokio::test]
#[ignore] // Requires Ollama
async fn test_toon_reading_example_variations() {
    println!("\nTOON reading with varying examples...\n");
    println!("Model: {}", eval_model());
    println!();

    let fixtures = all_fixtures();
    let configs = PromptConfig::example_variations();

    let mut report = EvalReport::new(eval_model(), ollama_base_url());

    for config in &configs {
        println!("Config: {}", config);
        let mut passed = 0;
        let mut failed = 0;

        for fixture in &fixtures {
            let result = run_conversion_test(
                fixture.id,
                &fixture.json,
                ConversionDirection::ToonToJson,
                config,
            )
            .await;

            if result.validation.success {
                passed += 1;
            } else {
                failed += 1;
            }
            report.add_result(result);
        }

        let rate = (passed as f64 / (passed + failed) as f64) * 100.0;
        println!("  {}/{} passed ({:.1}%)\n", passed, passed + failed, rate);
    }

    // Save report
    let report_path =
        PathBuf::from(env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string()))
            .join("toon_reading_variations_report.md");

    if let Err(e) = report.save(&report_path) {
        eprintln!("Failed to save report: {}", e);
    } else {
        println!("Report saved to: {}", report_path.display());
    }

    // Print summary table
    let aggregated = report.aggregate();
    println!("\n| Config | Pass | Fail | Rate |");
    println!("|--------|------|------|------|");

    let mut keys: Vec<_> = aggregated.keys().collect();
    keys.sort_by(|a, b| a.1.cmp(&b.1));

    for key in keys {
        let stats = &aggregated[key];
        println!(
            "| {} | {} | {} | {:.1}% |",
            key.1,
            stats.passed,
            stats.failed,
            stats.pass_rate()
        );
    }
}

/// Test how example count affects JSON → TOON writing
#[tokio::test]
#[ignore] // Requires Ollama
async fn test_toon_writing_example_variations() {
    println!("\nTOON writing with varying examples (JSON → TOON)...\n");
    println!("Model: {}", eval_model());
    println!();

    let fixtures = all_fixtures();
    let configs = PromptConfig::example_variations();

    let mut report = EvalReport::new(eval_model(), ollama_base_url());

    for config in &configs {
        println!("Config: {}", config);
        let mut passed = 0;
        let mut failed = 0;

        for fixture in &fixtures {
            let result = run_conversion_test(
                fixture.id,
                &fixture.json,
                ConversionDirection::JsonToToon,
                config,
            )
            .await;

            if result.validation.success {
                passed += 1;
            } else {
                failed += 1;
            }
            report.add_result(result);
        }

        let rate = (passed as f64 / (passed + failed) as f64) * 100.0;
        println!("  {}/{} passed ({:.1}%)\n", passed, passed + failed, rate);
    }

    // Print summary table
    let aggregated = report.aggregate();
    println!("\n| Config | Pass | Fail | Rate |");
    println!("|--------|------|------|------|");

    let mut keys: Vec<_> = aggregated.keys().collect();
    keys.sort_by(|a, b| a.1.cmp(&b.1));

    for key in keys {
        let stats = &aggregated[key];
        println!(
            "| {} | {} | {} | {:.1}% |",
            key.1,
            stats.passed,
            stats.failed,
            stats.pass_rate()
        );
    }
}

// =============================================================================
// Query Comprehension Tests
// =============================================================================

/// Check if an LLM response contains expected answer content
fn check_query_answer(response: &str, expected: &[&str], match_all: bool) -> bool {
    let response_lower = response.to_lowercase();
    if match_all {
        expected
            .iter()
            .all(|e| response_lower.contains(&e.to_lowercase()))
    } else {
        expected
            .iter()
            .any(|e| response_lower.contains(&e.to_lowercase()))
    }
}

/// Test TOON query comprehension - can LLMs answer questions about TOON data?
#[tokio::test]
#[ignore] // Requires Ollama
async fn test_toon_query_comprehension() {
    println!("\nTOON query comprehension evaluation...\n");
    println!("Model: {}", eval_model());
    println!("Endpoint: {}", ollama_base_url());
    println!();

    let fixtures = query_fixtures();
    let configs = PromptConfig::quick(); // ZeroShot, FewShot(1), FewShot(2), Full

    let mut results_by_config: std::collections::HashMap<String, (usize, usize)> =
        std::collections::HashMap::new();

    for config in &configs {
        println!("Config: {}", config);
        let mut passed = 0;
        let mut failed = 0;

        for fixture in &fixtures {
            println!("  Fixture: {} - {}", fixture.id, fixture.description);

            for question in &fixture.questions {
                let prompt = build_query_prompt(&config, fixture.toon, question.question);

                let response = match call_ollama(&prompt).await {
                    Ok(r) => r,
                    Err(e) => {
                        println!("    ✗ Q: {} - API error: {}", question.question, e);
                        failed += 1;
                        continue;
                    }
                };

                let success = check_query_answer(&response, &question.expected, question.match_all);

                if success {
                    passed += 1;
                    println!("    ✓ Q: {}", question.question);
                } else {
                    failed += 1;
                    println!("    ✗ Q: {}", question.question);
                    println!("      Expected: {:?}", question.expected);
                    // Show first line of response
                    let first_line = response.lines().next().unwrap_or("(empty)");
                    let truncated = if first_line.len() > 80 {
                        format!("{}...", &first_line[..80])
                    } else {
                        first_line.to_string()
                    };
                    println!("      Got: {}", truncated);
                }
            }
        }

        let rate = if passed + failed > 0 {
            (passed as f64 / (passed + failed) as f64) * 100.0
        } else {
            0.0
        };
        println!("\n  Total: {}/{} ({:.1}%)\n", passed, passed + failed, rate);

        results_by_config.insert(config.to_string(), (passed, failed));
    }

    // Print summary table
    println!("\n========================================");
    println!("QUERY COMPREHENSION SUMMARY");
    println!("========================================\n");
    println!("| Config | Pass | Fail | Rate |");
    println!("|--------|------|------|------|");

    let mut configs_sorted: Vec<_> = results_by_config.keys().collect();
    configs_sorted.sort();

    for config in configs_sorted {
        let (passed, failed) = results_by_config[config];
        let rate = if passed + failed > 0 {
            (passed as f64 / (passed + failed) as f64) * 100.0
        } else {
            0.0
        };
        println!("| {} | {} | {} | {:.1}% |", config, passed, failed, rate);
    }
}

/// Quick query comprehension test with zero-shot only
#[tokio::test]
#[ignore] // Requires Ollama
async fn test_toon_query_zero_shot() {
    println!("\nTOON query comprehension (zero-shot only)...\n");
    println!("Model: {}", eval_model());
    println!();

    let fixtures = query_fixtures();
    let config = PromptConfig::ZeroShot;

    let mut passed = 0;
    let mut failed = 0;

    for fixture in &fixtures {
        println!("Fixture: {} - {}", fixture.id, fixture.description);

        for question in &fixture.questions {
            let prompt = build_query_prompt(&config, fixture.toon, question.question);

            let response = match call_ollama(&prompt).await {
                Ok(r) => r,
                Err(e) => {
                    println!("  ✗ {} - API error: {}", question.question, e);
                    failed += 1;
                    continue;
                }
            };

            let success = check_query_answer(&response, &question.expected, question.match_all);

            if success {
                passed += 1;
                println!("  ✓ {}", question.question);
            } else {
                failed += 1;
                println!("  ✗ {}", question.question);
                // Show truncated response
                let response_short: String = response.chars().take(100).collect();
                println!("    Got: {}...", response_short.replace('\n', " "));
            }
        }
        println!();
    }

    let rate = (passed as f64 / (passed + failed) as f64) * 100.0;
    println!("Results: {}/{} ({:.1}%)", passed, passed + failed, rate);

    // Query comprehension should be easier than full JSON conversion
    // We expect at least 50% success zero-shot
    assert!(
        rate >= 50.0,
        "Expected >=50% query comprehension zero-shot, got {:.1}%",
        rate
    );
}

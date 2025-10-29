//! TDD RED Phase Test: JSON Output Formatting in Semantic Search
//!
//! This test file contains failing tests that demonstrate JSON output formatting issues
//! in semantic search functionality. The tests expose current problems where semantic
//! search returns error messages instead of valid JSON, has inconsistent JSON structure,
//! or doesn't properly handle error responses in JSON format.
//!
//! **Current Issues to Demonstrate:**
//! 1. Semantic search may return error messages instead of valid JSON
//! 2. JSON structure may not match expected format for different output types
//! 3. Error responses may not be properly formatted as JSON
//! 4. JSON fields may not contain meaningful search result data
//!
//! **Test Objectives (RED Phase):**
//! 1. Write failing tests that demonstrate JSON formatting problems
//! 2. Test JSON validity, structure, and content accuracy
//! 3. Test error handling in JSON format
//! 4. Drive implementation of proper JSON response formatting

/// Helper to create a temporary config file for integration tests
///
/// Since Phase 2.0 removed environment variable configuration, integration tests
/// that spawn the CLI binary need to pass configuration via --config flag.
fn create_temp_config(kiln_path: &PathBuf) -> Result<tempfile::NamedTempFile> {
    let config_content = format!(
        r#"[kiln]
path = "{}"
embedding_url = "http://localhost:11434"

[embedding]
provider = "fastembed"
model = "BAAI/bge-small-en-v1.5"

[embedding.fastembed]
cache_dir = "/home/moot/crucible/crates/crucible-llm/.fastembed_cache"
show_download = true

[network]
timeout_secs = 30
pool_size = 10
max_retries = 3

[llm]
chat_model = "llama3.2"
temperature = 0.7
max_tokens = 2048
"#,
        kiln_path.display(),
    );

    let temp_file = tempfile::NamedTempFile::new()?;
    std::fs::write(temp_file.path(), config_content)?;
    Ok(temp_file)
}

/// Helper function to get CLI binary path
fn cli_binary_path() -> PathBuf {
    let base_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| {
        std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .to_string()
    });

    let debug_path = PathBuf::from(&base_dir).join("../../target/debug/cru");
    let release_path = PathBuf::from(&base_dir).join("../../target/release/cru");

    if debug_path.exists() {
        debug_path
    } else if release_path.exists() {
        release_path
    } else {
        panic!("cru binary not found. Run 'cargo build -p crucible-cli' first.");
    }
}

/// Helper to run CLI semantic search command with JSON output
async fn run_semantic_search_json(
    kiln_path: &PathBuf,
    query: &str,
    additional_args: Vec<&str>,
) -> Result<String> {
    let binary_path = cli_binary_path();

    // Create temporary config file (Phase 2.0: no env var support)
    let config_file = create_temp_config(kiln_path)?;

    let mut cmd = Command::new(binary_path);

    // Base command with config file
    cmd.arg("--config")
        .arg(config_file.path())
        .arg("semantic")
        .arg(query)
        .arg("--format")
        .arg("json");

    // Add additional arguments
    for arg in additional_args {
        cmd.arg(arg);
    }

    let output = cmd.output().await?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // Return combined output (include stderr for error cases)
    let combined_output = if !stderr.is_empty() {
        format!("{}{}", stderr, stdout)
    } else {
        stdout
    };

    // Extract JSON from output if mixed with other text
    let json_output = extract_json_from_output(&combined_output);

    Ok(json_output)
}

/// Extract JSON from output that may contain other text
fn extract_json_from_output(output: &str) -> String {
    // Look for JSON object in the output
    let lines: Vec<&str> = output.lines().collect();

    // Find the first line that starts with '{' (beginning of JSON object)
    let json_start = lines
        .iter()
        .position(|line| line.trim_start().starts_with('{'));

    if let Some(start_idx) = json_start {
        // Extract from the JSON start to the end
        let json_lines: Vec<&str> = lines[start_idx..].to_vec();
        json_lines.join("\n")
    } else {
        // If no JSON found, return the original output
        output.to_string()
    }
}

/// Helper to create a test kiln with sample semantic content
async fn create_test_kiln() -> Result<(TempDir, PathBuf)> {
    let temp_dir = TempDir::new()?;
    let kiln_path = temp_dir.path().to_path_buf();

    // Create .obsidian directory for Obsidian kiln
    fs::create_dir_all(kiln_path.join(".obsidian"))?;

    // Create test markdown files with rich semantic content
    let test_files = vec![
        (
            "machine-learning-fundamentals.md",
            r#"# Machine Learning Fundamentals

Machine learning is a subset of artificial intelligence that focuses on algorithms that can learn from data.

## Core Concepts
- Neural networks and deep learning
- Supervised and unsupervised learning
- Feature engineering and model selection

## Applications
Machine learning is used in various domains including computer vision, natural language processing, and predictive analytics.
"#,
        ),
        (
            "rust-programming-guide.md",
            r#"# Rust Programming Guide

Rust is a systems programming language that guarantees memory safety without using a garbage collector.

## Key Features
- Ownership and borrowing system
- Pattern matching
- Zero-cost abstractions

## Use Cases
Rust is excellent for systems programming, web assembly, and performance-critical applications.
"#,
        ),
        (
            "database-systems-overview.md",
            r#"# Database Systems Overview

Database management systems provide structured ways to store and retrieve data efficiently.

## Types of Databases
- Relational databases (SQL)
- NoSQL databases
- Vector databases for similarity search
- Graph databases for networked data

## Modern Trends
Vector databases enable efficient semantic search using embeddings and similarity algorithms.
"#,
        ),
        (
            "ai-research-papers.md",
            r#"# AI Research Papers

Recent advances in artificial intelligence research have transformed multiple fields.

## Transformer Models
- Attention mechanisms in neural networks
- Large language models and their applications
- Transfer learning and fine-tuning

## Applications
Natural language processing, computer vision, and reinforcement learning have seen significant improvements with modern AI techniques.
"#,
        ),
    ];

    for (filename, content) in test_files {
        let file_path = kiln_path.join(filename);
        fs::write(file_path, content)?;
    }

    Ok((temp_dir, kiln_path))
}

/// Helper to use the existing test-kiln for more realistic testing
fn get_test_kiln_path() -> PathBuf {
    PathBuf::from("/home/moot/crucible/tests/test-kiln")
}

#[cfg(test)]
mod semantic_search_json_output_tdd_tests {
    use super::*;

    #[tokio::test]
    /// Test that semantic search returns valid JSON structure when --format json is used
    ///
    /// This test should FAIL until JSON output formatting is properly implemented.
    /// It demonstrates the current issue where semantic search may return error messages
    /// instead of valid JSON.
    async fn test_semantic_search_json_output_is_valid() -> Result<()> {
        println!("🧪 TDD RED Phase: Testing semantic search JSON output validity");

        // Use the existing test-kiln for realistic content
        let kiln_path = get_test_kiln_path();
        println!("📁 Using test-kiln: {}", kiln_path.display());

        // Test basic semantic search with JSON output
        let result = run_semantic_search_json(&kiln_path, "machine learning", vec![]).await?;
        println!("📄 Raw output length: {} characters", result.len());

        // Try to parse as JSON - this should fail if the output isn't valid JSON
        let parsed_result = match serde_json::from_str::<Value>(&result) {
            Ok(parsed) => {
                println!("✅ Output is valid JSON");
                parsed
            }
            Err(e) => {
                println!("❌ TDD FAILURE: Output is not valid JSON");
                println!("   JSON parsing error: {}", e);
                println!("   Raw output: {}", result);

                // This failure demonstrates the JSON formatting problem
                panic!("RED PHASE: Semantic search should return valid JSON, but got parsing error: {}", e);
            }
        };

        // Verify basic JSON structure
        if let Some(query_field) = parsed_result.get("query") {
            println!("✅ JSON contains 'query' field: {:?}", query_field);
        } else {
            println!("❌ TDD FAILURE: JSON missing required 'query' field");
            panic!("RED PHASE: JSON response should contain 'query' field");
        }

        if let Some(results_field) = parsed_result.get("results") {
            if results_field.is_array() {
                println!("✅ JSON contains 'results' array");
            } else {
                println!("❌ TDD FAILURE: 'results' field is not an array");
                panic!("RED PHASE: JSON 'results' field should be an array");
            }
        } else {
            println!("❌ TDD FAILURE: JSON missing required 'results' field");
            panic!("RED PHASE: JSON response should contain 'results' field");
        }

        println!("✅ JSON structure validation passed");
        Ok(())
    }

    #[tokio::test]
    /// Test that JSON contains actual search results, not error messages
    ///
    /// This test should FAIL until semantic search returns real search results
    /// in JSON format instead of error messages or mock data.
    async fn test_semantic_search_json_contains_search_results() -> Result<()> {
        println!("🧪 TDD RED Phase: Testing semantic search JSON content accuracy");

        let kiln_path = get_test_kiln_path();
        println!("📁 Using test-kiln: {}", kiln_path.display());

        // Test semantic search that should find relevant results
        let result =
            run_semantic_search_json(&kiln_path, "artificial intelligence", vec!["--top-k", "5"])
                .await?;
        println!("📄 Raw output: {}", result);

        // First check if it's valid JSON
        let parsed_result = match serde_json::from_str::<Value>(&result) {
            Ok(parsed) => parsed,
            Err(e) => {
                println!("❌ TDD FAILURE: Cannot test content - output is not valid JSON");
                println!("   JSON parsing error: {}", e);
                panic!(
                    "RED PHASE: Cannot test JSON content if output isn't valid JSON: {}",
                    e
                );
            }
        };

        // Check if JSON contains actual search results instead of error messages
        if let Some(results) = parsed_result.get("results").and_then(|r| r.as_array()) {
            println!("📊 Found {} results in JSON", results.len());

            if results.is_empty() {
                println!("⚠️  No results found - this might be expected for some queries");
            } else {
                // Check if results contain meaningful data
                let mut has_real_files = false;
                let mut has_error_messages = false;

                for (i, result) in results.iter().enumerate() {
                    println!("📄 Result {}: {:?}", i, result);

                    // Check for real file content
                    if let Some(id) = result.get("id").and_then(|id| id.as_str()) {
                        if id.ends_with(".md") || id.contains("test-kiln") {
                            has_real_files = true;
                            println!("✅ Found real file: {}", id);
                        }
                    }

                    // Check for error messages in results
                    let result_str = result.to_string();
                    if result_str.to_lowercase().contains("error")
                        || result_str.to_lowercase().contains("failed")
                        || result_str.to_lowercase().contains("not found")
                    {
                        has_error_messages = true;
                        println!("❌ Found error message in results: {}", result_str);
                    }
                }

                if has_error_messages {
                    println!(
                        "❌ TDD FAILURE: JSON contains error messages instead of search results"
                    );
                    panic!("RED PHASE: JSON should contain search results, not error messages");
                }

                if !has_real_files && !results.is_empty() {
                    println!("❌ TDD FAILURE: JSON doesn't contain real file references");
                    panic!("RED PHASE: JSON results should reference real files from the kiln");
                }
            }
        } else {
            println!("❌ TDD FAILURE: JSON missing 'results' array");
            panic!("RED PHASE: JSON should contain 'results' array with search results");
        }

        println!("✅ JSON content validation passed");
        Ok(())
    }

    #[tokio::test]
    /// Test that error responses are properly formatted as JSON
    ///
    /// This test should FAIL until error responses in semantic search are
    /// properly formatted as JSON instead of plain text error messages.
    async fn test_semantic_search_json_error_formatting() -> Result<()> {
        println!("🧪 TDD RED Phase: Testing semantic search JSON error formatting");

        // Create an empty kiln directory to trigger errors
        let temp_kiln = TempDir::new()?;
        let kiln_path = temp_kiln.path().to_path_buf();

        // Create .obsidian directory but no markdown files
        fs::create_dir_all(kiln_path.join(".obsidian"))?;

        println!(
            "📁 Using empty kiln to trigger errors: {}",
            kiln_path.display()
        );

        // Test semantic search that should fail due to no content
        let result = run_semantic_search_json(&kiln_path, "test query", vec![]).await?;
        println!("📄 Raw output: {}", result);

        // Check if error output is properly formatted as JSON
        let is_json = result.trim_start().starts_with('{') || result.trim_start().starts_with('[');

        if is_json {
            println!("✅ Error response is formatted as JSON");

            // Try to parse the error JSON
            match serde_json::from_str::<Value>(&result) {
                Ok(parsed_error) => {
                    println!("✅ Error JSON is valid");

                    // Check for expected error fields
                    if let Some(error_field) = parsed_error.get("error") {
                        println!("✅ JSON contains 'error' field: {:?}", error_field);
                    } else if let Some(message_field) = parsed_error.get("message") {
                        println!("✅ JSON contains 'message' field: {:?}", message_field);
                    } else {
                        println!("⚠️  JSON doesn't contain obvious error fields");
                    }
                }
                Err(e) => {
                    println!("❌ TDD FAILURE: Error response looks like JSON but isn't valid");
                    println!("   JSON parsing error: {}", e);
                    panic!("RED PHASE: Error responses should be valid JSON: {}", e);
                }
            }
        } else {
            println!("❌ TDD FAILURE: Error response is not formatted as JSON");
            println!(
                "   Error output starts with: {:?}",
                &result[..result.len().min(50)]
            );
            panic!("RED PHASE: Error responses should be formatted as JSON, got plain text");
        }

        println!("✅ Error formatting validation completed");
        Ok(())
    }

    #[tokio::test]
    /// Test that JSON has all expected fields with correct types
    ///
    /// This test should FAIL until the JSON structure includes all required
    /// fields with the correct data types for semantic search results.
    async fn test_semantic_search_json_field_validation() -> Result<()> {
        println!("🧪 TDD RED Phase: Testing semantic search JSON field validation");

        let kiln_path = get_test_kiln_path();
        println!("📁 Using test-kiln: {}", kiln_path.display());

        // Test semantic search with comprehensive output
        let result =
            run_semantic_search_json(&kiln_path, "database systems", vec!["--top-k", "3"]).await?;
        println!("📄 Raw output length: {} characters", result.len());

        // Parse JSON
        let parsed_result = match serde_json::from_str::<Value>(&result) {
            Ok(parsed) => {
                println!("✅ JSON is valid");
                parsed
            }
            Err(e) => {
                println!("❌ TDD FAILURE: Cannot validate fields - invalid JSON");
                panic!(
                    "RED PHASE: JSON field validation requires valid JSON: {}",
                    e
                );
            }
        };

        // Expected fields and their types
        let expected_fields = vec![
            ("query", "string"),
            ("total_results", "number"),
            ("results", "array"),
        ];

        let mut missing_fields: Vec<String> = Vec::new();
        let mut wrong_types: Vec<(String, String, String)> = Vec::new();

        // Check each expected field
        for (field_name, expected_type) in expected_fields {
            match parsed_result.get(field_name) {
                Some(value) => {
                    let actual_type = match value {
                        Value::String(_) => "string",
                        Value::Number(_) => "number",
                        Value::Array(_) => "array",
                        Value::Object(_) => "object",
                        Value::Bool(_) => "boolean",
                        Value::Null => "null",
                    };

                    if actual_type == expected_type {
                        println!(
                            "✅ Field '{}' has correct type: {}",
                            field_name, actual_type
                        );
                    } else {
                        println!(
                            "❌ Field '{}' has wrong type: expected {}, got {}",
                            field_name, expected_type, actual_type
                        );
                        wrong_types.push((
                            field_name.to_string(),
                            expected_type.to_string(),
                            actual_type.to_string(),
                        ));
                    }
                }
                None => {
                    println!("❌ Missing required field: {}", field_name);
                    missing_fields.push(field_name.to_string());
                }
            }
        }

        // Validate result object structure if results array exists
        if let Some(results) = parsed_result.get("results").and_then(|r| r.as_array()) {
            if !results.is_empty() {
                println!(
                    "📊 Validating structure of {} result objects",
                    results.len()
                );

                // Expected fields in each result object
                let result_fields = vec![
                    ("id", "string"),
                    ("title", "string"),
                    ("content_preview", "string"),
                    ("score", "number"),
                ];

                for (i, result_obj) in results.iter().enumerate() {
                    if let Some(result_object) = result_obj.as_object() {
                        for (field_name, expected_type) in &result_fields {
                            match result_object.get(*field_name) {
                                Some(value) => {
                                    let actual_type = match value {
                                        Value::String(_) => "string",
                                        Value::Number(_) => "number",
                                        Value::Array(_) => "array",
                                        Value::Object(_) => "object",
                                        Value::Bool(_) => "boolean",
                                        Value::Null => "null",
                                    };

                                    if actual_type == *expected_type {
                                        println!(
                                            "  ✅ Result[{}].{}: {}",
                                            i, field_name, actual_type
                                        );
                                    } else {
                                        println!(
                                            "  ❌ Result[{}].{}: expected {}, got {}",
                                            i, field_name, expected_type, actual_type
                                        );
                                        wrong_types.push((
                                            format!("result[{}].{}", i, field_name),
                                            expected_type.to_string(),
                                            actual_type.to_string(),
                                        ));
                                    }
                                }
                                None => {
                                    println!("  ❌ Result[{}] missing field: {}", i, field_name);
                                    missing_fields.push(format!("result[{}].{}", i, field_name));
                                }
                            }
                        }
                    } else {
                        println!("  ❌ Result[{}] is not an object", i);
                        wrong_types.push((
                            format!("result[{}]", i),
                            "object".to_string(),
                            "other".to_string(),
                        ));
                    }
                }
            }
        }

        // Fail if there are any issues
        if !missing_fields.is_empty() || !wrong_types.is_empty() {
            println!("\n❌ TDD FAILURE: JSON field validation failed");

            if !missing_fields.is_empty() {
                println!("   Missing fields: {:?}", missing_fields);
            }

            if !wrong_types.is_empty() {
                println!("   Wrong types: {:?}", wrong_types);
            }

            panic!("RED PHASE: JSON structure needs to be fixed with proper fields and types");
        }

        println!("✅ All JSON field validations passed");
        Ok(())
    }

    #[tokio::test]
    /// Test JSON output consistency across different queries and parameters
    ///
    /// This test should FAIL until JSON output format is consistent across
    /// different semantic search queries and parameter combinations.
    async fn test_semantic_search_json_consistency_across_queries() -> Result<()> {
        println!("🧪 TDD RED Phase: Testing semantic search JSON consistency across queries");

        let kiln_path = get_test_kiln_path();
        println!("📁 Using test-kiln: {}", kiln_path.display());

        let test_queries = vec![
            ("machine learning", vec![]),
            ("artificial intelligence", vec!["--top-k", "2"]),
            ("database systems", vec!["--top-k", "5"]),
        ];

        let mut json_structures = Vec::new();

        for (query, args) in test_queries {
            println!("\n🔍 Testing query: '{}' with args: {:?}", query, args);

            let result = run_semantic_search_json(&kiln_path, query, args).await?;

            // Extract structure (field names) without values
            if let Ok(parsed) = serde_json::from_str::<Value>(&result) {
                let structure = extract_json_structure(&parsed);
                json_structures.push((query, structure));
                println!("✅ Query '{}' returned valid JSON", query);
            } else {
                println!("❌ Query '{}' returned invalid JSON", query);
                panic!("RED PHASE: All queries should return valid JSON for consistency");
            }
        }

        // Compare structures
        if json_structures.len() > 1 {
            let first_structure = &json_structures[0].1;

            for (query, structure) in json_structures.iter().skip(1) {
                if structure != first_structure {
                    println!("\n❌ TDD FAILURE: JSON structure inconsistency detected");
                    println!("   First query structure: {:?}", first_structure);
                    println!("   Query '{}' structure: {:?}", query, structure);
                    panic!("RED PHASE: JSON output format should be consistent across all queries");
                }
            }

            println!("✅ JSON structure is consistent across all queries");
        }

        Ok(())
    }
}

/// Helper function to extract JSON structure (field names and types) for comparison
fn extract_json_structure(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut structure = serde_json::Map::new();
            for (key, val) in map {
                structure.insert(key.clone(), extract_json_structure(val));
            }
            Value::Object(structure)
        }
        Value::Array(arr) => {
            if !arr.is_empty() {
                // Use structure of first element as representative
                Value::Array(vec![extract_json_structure(&arr[0])])
            } else {
                Value::Array(vec![])
            }
        }
        Value::String(_) => Value::String("string".to_string()),
        Value::Number(_) => Value::String("number".to_string()),
        Value::Bool(_) => Value::String("boolean".to_string()),
        Value::Null => Value::String("null".to_string()),
    }
}
use anyhow::Result;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::process::Command;

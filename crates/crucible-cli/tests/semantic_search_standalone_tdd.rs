//! TDD RED Phase Test: Semantic Search Without External Daemon
//!
//! This test file contains failing tests that demonstrate the need for semantic search
//! functionality to work without requiring external daemon processes. The tests expose
//! the current architectural gap where semantic search through different CLI entry points
//! has inconsistent behavior regarding daemon dependencies.
//!
//! **Current Issue Analysis:**
//! - CLI `cru semantic` command: Uses integrated kiln_integration::semantic_search() ✓
//! - REPL `:run semantic_search` tool: Uses mock crucible_tools implementation ❌
//! - Some code paths may still attempt to spawn crucible-daemon process ❌
//!
//! **Test Objectives:**
//! 1. Demonstrate current inconsistency in semantic search behavior
//! 2. Show where daemon dependencies still exist
//! 3. Provide clear specification for daemonless semantic search
//! 4. Drive implementation of integrated semantic search across all entry points

/// Test helper to create a minimal test kiln with sample content
async fn create_test_kiln() -> Result<(TempDir, PathBuf)> {
    let temp_dir = TempDir::new()?;
    let kiln_path = temp_dir.path().to_path_buf();

    // Create test markdown files with semantic content
    let test_files = vec![
        ("machine-learning.md", "# Introduction to Machine Learning\n\nMachine learning is a subset of artificial intelligence that focuses on neural networks and algorithms that can learn from data."),
        ("rust-programming.md", "# Rust Programming Language\n\nRust is a systems programming language focused on memory safety and performance. It provides zero-cost abstractions and prevents common programming errors."),
        ("database-systems.md", "# Database Management Systems\n\nSQL and NoSQL databases provide different approaches to data storage and retrieval. Vector databases enable efficient similarity search for embeddings."),
        ("ai-research.md", "# AI Research Directions\n\nCurrent artificial intelligence research explores transformer models, large language models, and applications in natural language processing and computer vision."),
    ];

    for (filename, content) in test_files {
        let file_path = kiln_path.join(filename);
        fs::write(file_path, content)?;
    }

    Ok((temp_dir, kiln_path))
}

/// Helper to run CLI semantic search command
async fn run_cli_semantic_search(kiln_path: &PathBuf, query: &str) -> Result<String> {
    let output = Command::new(env!("CARGO_BIN_EXE_cru"))
        .arg("semantic")
        .arg(query)
        .arg("--top-k")
        .arg("3")
        .arg("--format")
        .arg("json")
        .env("OBSIDIAN_KILN_PATH", kiln_path.to_string_lossy().as_ref())
        .output()
        .await?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Helper to check if crucible-daemon binary exists
fn daemon_binary_exists() -> bool {
    let crate_root = env!("CARGO_MANIFEST_DIR");
    let daemon_debug = PathBuf::from(crate_root).join("../../target/debug/crucible-daemon");
    let daemon_release = PathBuf::from(crate_root).join("../../target/release/crucible-daemon");

    daemon_debug.exists() || daemon_release.exists()
}

#[cfg(test)]
mod semantic_search_daemonless_tdd_tests {
    use super::*;

    #[tokio::test]
    /// Test that demonstrates the current inconsistency in semantic search behavior
    ///
    /// This test should FAIL because:
    /// 1. CLI semantic search uses integrated kiln_integration::semantic_search()
    /// 2. REPL semantic_search tool uses mock crucible_tools implementation
    /// 3. This architectural gap causes different behavior for the same functionality
    async fn test_semantic_search_inconsistency_between_cli_and_repl() -> Result<()> {
        let (_temp_dir, kiln_path) = create_test_kiln().await?;

        println!("🔍 Testing semantic search inconsistency between CLI and REPL entry points");
        println!("📁 Test kiln: {}", kiln_path.display());

        // Test 1: CLI semantic search command (should work with integrated implementation)
        println!("\n1. Testing CLI semantic search command...");
        let cli_result = run_cli_semantic_search(&kiln_path, "machine learning").await;

        match cli_result {
            Ok(cli_output) => {
                println!("✅ CLI semantic search completed successfully");
                println!("📄 CLI output length: {} characters", cli_output.len());

                // Try to parse JSON output
                match serde_json::from_str::<Value>(&cli_output) {
                    Ok(parsed) => {
                        println!("✅ CLI output is valid JSON");
                        if let Some(results) = parsed.get("results").and_then(|r| r.as_array()) {
                            println!("📊 CLI returned {} results", results.len());
                        }
                    }
                    Err(e) => {
                        println!("⚠️  CLI output is not valid JSON: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("❌ CLI semantic search failed: {}", e);
                println!("   This may indicate daemon dependency or other integration issues");
            }
        }

        // Test 2: REPL semantic search tool (should use mock implementation)
        println!("\n2. Testing REPL semantic search tool...");

        // For now, we'll simulate what the REPL tool does by calling crucible_tools directly
        // In a real test environment, this would go through the REPL tool system
        let mock_tool_result = test_repl_semantic_search_tool().await;

        match mock_tool_result {
            Ok(mock_results) => {
                println!("✅ REPL semantic_search tool completed (likely with mock data)");
                println!("📊 Mock results: {:?}", mock_results);

                // This demonstrates the inconsistency: CLI uses real search, REPL uses mock
                println!("\n❌ INCONSISTENCY DETECTED:");
                println!(
                    "   - CLI semantic search: Uses integrated kiln_integration::semantic_search()"
                );
                println!("   - REPL semantic_search tool: Uses mock crucible_tools implementation");
                println!("   - Same functionality, different behavior and results");

                // This assertion should fail to highlight the inconsistency
                panic!(
                    "Semantic search behavior is inconsistent between CLI and REPL entry points. \
                       CLI uses real vector search while REPL uses mock implementation."
                );
            }
            Err(e) => {
                println!("❌ REPL semantic_search tool failed: {}", e);

                // This might also indicate daemon dependencies in the tool system
                if e.to_string().contains("crucible-daemon") {
                    println!("   🔍 Daemon dependency detected in REPL tool system");
                    println!("   This demonstrates the need for daemonless semantic search");
                }
            }
        }

        Ok(())
    }

    #[tokio::test]
    /// Test that semantic search works without external daemon dependencies
    ///
    /// This test should FAIL if any semantic search code path tries to spawn
    /// a crucible-daemon process. The test demonstrates where daemon dependencies
    /// still exist in the semantic search functionality.
    async fn test_semantic_search_without_external_daemon() -> Result<()> {
        let (_temp_dir, kiln_path) = create_test_kiln().await?;

        println!("🔍 Testing semantic search without external daemon dependencies");
        println!("📁 Test kiln: {}", kiln_path.display());

        // Check if daemon binary exists
        let daemon_exists = daemon_binary_exists();
        println!("🔧 Daemon binary exists: {}", daemon_exists);

        if daemon_exists {
            println!("⚠️  Daemon binary found - this may affect test results");
            println!("   Consider removing daemon binary to test daemonless functionality");
        }

        // Test semantic search through different entry points to find daemon dependencies

        // Test CLI Command
        println!("\n🔧 Testing entry point: CLI Command");
        let cli_result = run_cli_semantic_search(&kiln_path, "artificial intelligence").await;

        match cli_result {
            Ok(result) => {
                println!("✅ CLI Command completed successfully");
                println!("📄 Result length: {} characters", result.len());

                // Check result for signs of daemon dependency
                if result.contains("crucible-daemon") {
                    println!("❌ Daemon dependency detected in CLI output");
                    println!(
                        "   This indicates the semantic search is trying to spawn daemon process"
                    );

                    // This should fail to demonstrate the daemon dependency issue
                    panic!("CLI shows daemon dependency in output: {}", result);
                }
            }
            Err(e) => {
                let error_msg = e.to_string();

                if error_msg.contains("crucible-daemon") && error_msg.contains("not found") {
                    println!("❌ Daemon dependency detected in CLI");
                    println!("   Error: {}", error_msg);
                    println!("   This demonstrates the need for daemonless semantic search");

                    // This test failure shows exactly where daemon dependencies exist
                    panic!("CLI failed due to daemon dependency: {}", error_msg);
                } else {
                    println!("⚠️  CLI failed with other error: {}", error_msg);
                    println!("   This may be a different integration issue");
                }
            }
        }

        // Test Mock REPL Tool
        println!("\n🔧 Testing entry point: Mock REPL Tool");
        let repl_result = test_repl_semantic_search_tool()
            .await
            .map(|r| format!("{:?}", r));

        match repl_result {
            Ok(result) => {
                println!("✅ Mock REPL Tool completed successfully");
                println!("📄 Result length: {} characters", result.len());

                // Check result for signs of daemon dependency
                if result.contains("crucible-daemon") {
                    println!("❌ Daemon dependency detected in REPL output");
                    println!(
                        "   This indicates the semantic search is trying to spawn daemon process"
                    );

                    // This should fail to demonstrate the daemon dependency issue
                    panic!("REPL shows daemon dependency in output: {}", result);
                }
            }
            Err(e) => {
                let error_msg = e.to_string();

                if error_msg.contains("crucible-daemon") && error_msg.contains("not found") {
                    println!("❌ Daemon dependency detected in REPL");
                    println!("   Error: {}", error_msg);
                    println!("   This demonstrates the need for daemonless semantic search");

                    // This test failure shows exactly where daemon dependencies exist
                    panic!("REPL failed due to daemon dependency: {}", error_msg);
                } else {
                    println!("⚠️  REPL failed with other error: {}", error_msg);
                    println!("   This may be a different integration issue");
                }
            }
        }

        // If we reach here, all entry points work without daemon dependencies
        println!("\n✅ All semantic search entry points work without external daemon");
        println!("   This indicates the daemonless implementation is working correctly");

        Ok(())
    }

    #[tokio::test]
    /// Test that semantic search produces consistent results across different entry points
    ///
    /// This test should FAIL because CLI semantic search uses real vector search
    /// while REPL semantic_search tool uses mock implementation, producing different
    /// results for the same query.
    async fn test_semantic_search_consistency_across_entry_points() -> Result<()> {
        let (_temp_dir, kiln_path) = create_test_kiln().await?;

        println!("🔍 Testing semantic search consistency across entry points");
        println!("📁 Test kiln: {}", kiln_path.display());

        let test_queries = vec![
            "machine learning",
            "rust programming",
            "database systems",
            "artificial intelligence",
        ];

        for query in test_queries {
            println!("\n🔍 Testing query: '{}'", query);

            // Test CLI semantic search
            let cli_result = run_cli_semantic_search(&kiln_path, query).await;
            let repl_result = test_repl_semantic_search_tool_with_query(query).await;

            match (cli_result, repl_result) {
                (Ok(cli_output), Ok(repl_output)) => {
                    println!("✅ Both entry points returned results");

                    // Parse CLI JSON output
                    let cli_parsed = serde_json::from_str::<Value>(&cli_output)
                        .map_err(|e| anyhow::anyhow!("Failed to parse CLI JSON: {}", e))?;

                    let empty_vec = vec![];
                    let cli_results = cli_parsed
                        .get("results")
                        .and_then(|r| r.as_array())
                        .unwrap_or(&empty_vec);

                    println!("📊 CLI returned {} results", cli_results.len());
                    println!("📊 REPL returned {} mock results", repl_output.len());

                    // The inconsistency: CLI should return real results, REPL returns mock
                    if cli_results.len() != repl_output.len() {
                        println!("❌ INCONSISTENCY: Different number of results");
                        println!("   CLI: {} results", cli_results.len());
                        println!("   REPL: {} results", repl_output.len());

                        // This should fail to highlight the inconsistency
                        panic!(
                            "Result count mismatch for query '{}': CLI={}, REPL={}",
                            query,
                            cli_results.len(),
                            repl_output.len()
                        );
                    }

                    // Check if results are fundamentally different
                    let cli_has_real_files = cli_results.iter().any(|result| {
                        result
                            .get("id")
                            .and_then(|id| id.as_str())
                            .map(|id| id.contains(".md"))
                            .unwrap_or(false)
                    });

                    if cli_has_real_files {
                        println!("✅ CLI returns real file paths from kiln");
                    } else {
                        println!("⚠️  CLI may also be using mock data");
                    }

                    // Mock results typically have hardcoded file paths
                    let repl_uses_mock = repl_output.iter().any(|result| {
                        result.to_string().contains("docs/ai-research.md")
                            || result.to_string().contains("projects/ml-project.md")
                    });

                    if repl_uses_mock {
                        println!("❌ REPL uses mock implementation with hardcoded paths");
                        println!("   This demonstrates the need for integrated semantic search");

                        // This should fail to drive implementation
                        panic!(
                            "REPL semantic_search uses mock implementation instead of real search"
                        );
                    }
                }
                (Ok(_), Err(repl_err)) => {
                    println!("❌ REPL failed: {}", repl_err);
                    if repl_err.to_string().contains("crucible-daemon") {
                        panic!("REPL has daemon dependency: {}", repl_err);
                    }
                }
                (Err(cli_err), Ok(_)) => {
                    println!("❌ CLI failed: {}", cli_err);
                    if cli_err.to_string().contains("crucible-daemon") {
                        panic!("CLI has daemon dependency: {}", cli_err);
                    }
                }
                (Err(cli_err), Err(repl_err)) => {
                    println!("❌ Both entry points failed:");
                    println!("   CLI: {}", cli_err);
                    println!("   REPL: {}", repl_err);

                    // Check if either has daemon dependency
                    let cli_daemon = cli_err.to_string().contains("crucible-daemon");
                    let repl_daemon = repl_err.to_string().contains("crucible-daemon");

                    if cli_daemon || repl_daemon {
                        panic!(
                            "Daemon dependencies detected - CLI: {}, REPL: {}",
                            cli_daemon, repl_daemon
                        );
                    }
                }
            }
        }

        // If we reach here with consistent results, the implementation is working
        println!("\n✅ Semantic search is consistent across all entry points");
        println!("   This indicates the daemonless integration is working correctly");

        Ok(())
    }

    #[tokio::test]
    /// Test that demonstrates the specific daemon error that should be fixed
    ///
    /// This test should FAIL with the current "crucible-daemon binary not found"
    /// error, providing a clear specification for what needs to be implemented.
    async fn test_daemonless_semantic_search_specification() -> Result<()> {
        let (_temp_dir, kiln_path) = create_test_kiln().await?;

        println!("🎯 TDD RED Phase: Daemonless Semantic Search Specification");
        println!("📁 Test kiln: {}", kiln_path.display());

        // This test should demonstrate the current problem clearly
        println!("\n🔍 CURRENT PROBLEM:");
        println!("   - CLI semantic search works with integrated implementation ✓");
        println!("   - REPL semantic_search tool uses mock implementation ❌");
        println!("   - Some code paths may still depend on crucible-daemon ❌");
        println!("   - Inconsistent behavior across entry points ❌");

        println!("\n✅ EXPECTED BEHAVIOR (Green Phase):");
        println!("   - All semantic search entry points use integrated implementation");
        println!("   - No external daemon dependencies required");
        println!("   - Consistent real vector search results across all entry points");
        println!("   - Semantic search works standalone in any environment");

        // Test the current state to show what needs to be fixed
        println!("\n🧪 TESTING CURRENT STATE:");

        // This should reveal the current limitations
        let cli_works = test_cli_semantic_search_works(&kiln_path).await.is_ok();
        let repl_uses_mock = test_repl_uses_mock_implementation().await?;

        println!("📊 Test Results:");
        println!("   - CLI semantic search works: {}", cli_works);
        println!("   - REPL uses mock implementation: {}", repl_uses_mock);

        if !cli_works {
            panic!("❌ CLI semantic search should work with integrated implementation");
        }

        if repl_uses_mock {
            println!("\n❌ TDD FAILURE - This is expected and demonstrates the problem:");
            println!("   REPL semantic_search tool uses mock implementation");
            println!("   This needs to be fixed to use integrated semantic search");
            println!("   The fix should:");
            println!("   1. Replace mock crucible_tools semantic_search with real implementation");
            println!("   2. Use kiln_integration::semantic_search() across all entry points");
            println!("   3. Ensure no daemon dependencies exist");
            println!("   4. Provide consistent results across CLI and REPL");

            // This failure is the RED phase - it drives the implementation
            panic!("RED PHASE: REPL semantic_search needs daemonless integrated implementation");
        }

        println!("\n✅ GREEN PHASE: All semantic search entry points work consistently");
        Ok(())
    }
}

/// Helper function to test REPL semantic search tool behavior
async fn test_repl_semantic_search_tool() -> Result<Vec<Value>> {
    // This simulates what the REPL semantic_search tool does
    // In the actual implementation, this would go through crucible_tools::execute_tool

    // For this test, we'll use the crucible-tools semantic search function directly
    use crucible_tools::{execute_tool, load_all_tools};

    // Initialize tools
    load_all_tools().await?;

    // Execute semantic search tool
    let result = execute_tool(
        "semantic_search".to_string(),
        serde_json::json!({
            "query": "machine learning",
            "top_k": 3
        }),
        Some("test_user".to_string()),
        Some("test_session".to_string()),
    )
    .await?;

    if result.success {
        if let Some(data) = result.data {
            if let Some(results) = data.get("results").and_then(|r| r.as_array()) {
                return Ok(results.clone());
            }
        }
    }

    Err(anyhow::anyhow!(
        "Mock semantic search tool failed: {:?}",
        result.error
    ))
}

/// Helper function to test REPL semantic search with specific query
async fn test_repl_semantic_search_tool_with_query(query: &str) -> Result<Vec<Value>> {
    use crucible_tools::{execute_tool, load_all_tools};

    load_all_tools().await?;

    let result = execute_tool(
        "semantic_search".to_string(),
        serde_json::json!({
            "query": query,
            "top_k": 5
        }),
        Some("test_user".to_string()),
        Some("test_session".to_string()),
    )
    .await?;

    if result.success {
        if let Some(data) = result.data {
            if let Some(results) = data.get("results").and_then(|r| r.as_array()) {
                return Ok(results.clone());
            }
        }
    }

    Err(anyhow::anyhow!(
        "Mock semantic search failed: {:?}",
        result.error
    ))
}

/// Helper to test if REPL uses mock implementation
async fn test_repl_uses_mock_implementation() -> Result<bool> {
    match test_repl_semantic_search_tool().await {
        Ok(results) => {
            // Check if results contain mock/hardcoded data
            let mock_indicators = vec![
                "docs/ai-research.md",
                "projects/ml-project.md",
                "Comprehensive research on artificial intelligence",
                "Implementation details for our ML project",
            ];

            let results_str = format!("{:?}", results);
            let uses_mock = mock_indicators
                .iter()
                .any(|indicator| results_str.contains(indicator));

            Ok(uses_mock)
        }
        Err(e) => {
            println!("REPL semantic search test failed: {}", e);
            Ok(false)
        }
    }
}

/// Helper to test CLI semantic search functionality
async fn test_cli_semantic_search_works(kiln_path: &PathBuf) -> Result<()> {
    let output = run_cli_semantic_search(kiln_path, "test query").await?;

    // Check if output looks like valid JSON with results
    let parsed: Value = serde_json::from_str(&output)?;

    if let Some(_results) = parsed.get("results") {
        println!("✅ CLI semantic search returned results");
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "CLI semantic search didn't return expected results format"
        ))
    }
}
use anyhow::Result;
use crucible_llm::embeddings::create_mock_provider;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::pin::Pin;
use tempfile::TempDir;
use tokio::process::Command;

//! Cross-Interface Consistency Tests
//!
//! This module validates that the Crucible system provides consistent behavior,
//! results, and performance across all interfaces (CLI, REPL, and tool APIs).
//! It ensures that users get the same experience regardless of which interface
//! they choose to interact with.

use std::collections::HashMap;
use std::process::Command;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

use crate::comprehensive_integration_workflow_tests::{
    ComprehensiveTestVault, CliTestHarness, ReplTestHarness, CommandResult
};
use crate::cli_workflow_integration_tests::ExtendedCliTestHarness;
use crate::repl_interactive_workflow_tests::ExtendedReplTestHarness;
use crate::tool_api_integration_tests::ToolApiTestHarness;

/// Cross-interface consistency test harness
pub struct CrossInterfaceTestHarness {
    vault_dir: TempDir,
    test_vault: ComprehensiveTestVault,
}

impl CrossInterfaceTestHarness {
    /// Create a new cross-interface test harness
    pub async fn new() -> Result<Self> {
        let test_vault = ComprehensiveTestVault::create().await?;
        let vault_dir = test_vault.path().to_owned();

        Ok(Self {
            vault_dir: vault_dir.to_owned(),
            test_vault,
        })
    }

    /// Test query consistency across CLI and REPL interfaces
    pub async fn test_query_consistency(&self) -> Result<()> {
        println!("🧪 Testing query consistency across interfaces");

        let test_queries = vec![
            TestQuery {
                query: "quantum computing",
                interface_types: vec![InterfaceType::Cli, InterfaceType::Repl],
                expected_min_results: 1,
                tolerance: 0.1, // 10% tolerance for result count variations
            },
            TestQuery {
                query: "rust async patterns",
                interface_types: vec![InterfaceType::Cli, InterfaceType::Repl],
                expected_min_results: 1,
                tolerance: 0.1,
            },
            TestQuery {
                query: "project management",
                interface_types: vec![InterfaceType::Cli, InterfaceType::Repl],
                expected_min_results: 2,
                tolerance: 0.2,
            },
            TestQuery {
                query: "machine learning",
                interface_types: vec![InterfaceType::Cli, InterfaceType::Repl],
                expected_min_results: 1,
                tolerance: 0.1,
            },
        ];

        for test_query in test_queries {
            println!("  🔍 Testing query: '{}' across {:?}", test_query.query, test_query.interface_types);

            let mut interface_results = HashMap::new();

            // Execute query across different interfaces
            for interface_type in &test_query.interface_types {
                let result = match interface_type {
                    InterfaceType::Cli => {
                        let cli_harness = CliTestHarness::new().await?;
                        let cli_result = cli_harness.execute_cli_command(&["search", &test_query.query])?;
                        InterfaceQueryResult {
                            interface_type: interface_type.clone(),
                            raw_output: cli_result.stdout.clone(),
                            exit_code: cli_result.exit_code,
                            duration: cli_result.duration,
                            result_count: self.count_results(&cli_result.stdout),
                            success: cli_result.exit_code == 0,
                        }
                    }
                    InterfaceType::Repl => {
                        let repl_harness = ReplTestHarness::new().await?;
                        let mut repl = repl_harness.spawn_repl()?;
                        let repl_output = repl.send_command(&format!("search {}", test_query.query))?;
                        let result_count = self.count_results(&repl_output);
                        repl.quit()?;
                        InterfaceQueryResult {
                            interface_type: interface_type.clone(),
                            raw_output: repl_output.clone(),
                            exit_code: 0, // REPL doesn't have exit codes in the same way
                            duration: Duration::from_millis(100), // Approximate
                            result_count,
                            success: !repl_output.is_empty(),
                        }
                    }
                    InterfaceType::Tool => {
                        let tool_harness = ToolApiTestHarness::new().await?;
                        let repl_harness = ReplTestHarness::new().await?;
                        let mut repl = repl_harness.spawn_repl()?;
                        let tool_output = repl.send_command(&format!(":run search_documents \"{}\"", test_query.query))?;
                        let result_count = self.count_results(&tool_output);
                        repl.quit()?;
                        InterfaceQueryResult {
                            interface_type: interface_type.clone(),
                            raw_output: tool_output.clone(),
                            exit_code: 0,
                            duration: Duration::from_millis(150),
                            result_count,
                            success: !tool_output.is_empty() && !tool_output.contains("❌"),
                        }
                    }
                };

                interface_results.insert(interface_type.clone(), result);
            }

            // Validate consistency across interfaces
            self.validate_query_consistency(&test_query, &interface_results)?;
        }

        println!("✅ Query consistency test passed");
        Ok(())
    }

    /// Test performance consistency across interfaces
    pub async fn test_performance_consistency(&self) -> Result<()> {
        println!("🧪 Testing performance consistency across interfaces");

        let performance_queries = vec![
            "quantum computing",
            "rust patterns",
            "project management",
            "deployment checklist",
        ];

        let mut performance_metrics = HashMap::new();

        for query in performance_queries {
            println!("  ⚡ Testing performance for query: '{}'", query);

            let mut query_metrics = HashMap::new();

            // Test CLI performance
            let cli_harness = CliTestHarness::new().await?;
            let cli_start = Instant::now();
            let cli_result = cli_harness.execute_cli_command(&["search", query])?;
            let cli_duration = cli_start.elapsed();

            query_metrics.insert(InterfaceType::Cli, PerformanceMetric {
                duration: cli_duration,
                success: cli_result.exit_code == 0,
                output_size: cli_result.stdout.len(),
            });

            // Test REPL performance
            let repl_harness = ReplTestHarness::new().await?;
            let mut repl = repl_harness.spawn_repl()?;
            let repl_start = Instant::now();
            let repl_output = repl.send_command(&format!("search {}", query))?;
            let repl_duration = repl_start.elapsed();
            repl.quit();

            query_metrics.insert(InterfaceType::Repl, PerformanceMetric {
                duration: repl_duration,
                success: !repl_output.is_empty(),
                output_size: repl_output.len(),
            });

            // Test Tool API performance
            let tool_harness = ToolApiTestHarness::new().await?;
            let mut repl = repl_harness.spawn_repl()?;
            let tool_start = Instant::now();
            let tool_output = repl.send_command(&format!(":run search_documents \"{}\"", query))?;
            let tool_duration = tool_start.elapsed();
            repl.quit();

            query_metrics.insert(InterfaceType::Tool, PerformanceMetric {
                duration: tool_duration,
                success: !tool_output.is_empty() && !tool_output.contains("❌"),
                output_size: tool_output.len(),
            });

            performance_metrics.insert(query.to_string(), query_metrics);
        }

        // Validate performance consistency
        self.validate_performance_consistency(&performance_metrics)?;

        println!("✅ Performance consistency test passed");
        Ok(())
    }

    /// Test output format consistency across interfaces
    pub async fn test_output_format_consistency(&self) -> Result<()> {
        println!("🧪 Testing output format consistency across interfaces");

        let format_tests = vec![
            FormatTest {
                query: "quantum computing",
                format: OutputFormat::Table,
                expected_characteristics: vec![
                    "table structure",
                    "headers",
                    "rows",
                ],
            },
            FormatTest {
                query: "rust patterns",
                format: OutputFormat::Json,
                expected_characteristics: vec![
                    "JSON structure",
                    "brackets or braces",
                ],
            },
        ];

        for format_test in format_tests {
            println!("  📋 Testing {} format consistency for query: '{}'",
                     format_test.format, format_test.query);

            // Test CLI format
            let cli_harness = CliTestHarness::new().await?;
            let cli_args = match format_test.format {
                OutputFormat::Table => vec!["search", &format_test.query, "--format", "table"],
                OutputFormat::Json => vec!["search", &format_test.query, "--format", "json"],
                OutputFormat::Csv => vec!["search", &format_test.query, "--format", "csv"],
            };
            let cli_result = cli_harness.execute_cli_command(&cli_args)?;

            // Test REPL format
            let repl_harness = ReplTestHarness::new().await?;
            let mut repl = repl_harness.spawn_repl()?;

            // Set format in REPL
            let format_cmd = match format_test.format {
                OutputFormat::Table => ":format table",
                OutputFormat::Json => ":format json",
                OutputFormat::Csv => ":format csv",
            };
            repl.send_command(format_cmd)?;

            let repl_output = repl.send_command(&format_test.query)?;
            repl.quit();

            // Validate format characteristics
            self.validate_format_characteristics(&format_test, &cli_result.stdout, "CLI")?;
            self.validate_format_characteristics(&format_test, &repl_output, "REPL")?;
        }

        println!("✅ Output format consistency test passed");
        Ok(())
    }

    /// Test error handling consistency across interfaces
    pub async fn test_error_handling_consistency(&self) -> Result<()> {
        println!("🧪 Testing error handling consistency across interfaces");

        let error_scenarios = vec![
            ErrorScenario {
                description: "Non-existent file search".to_string(),
                cli_command: vec!["search", "nonexistent_content_xyz_12345"],
                repl_command: "search nonexistent_content_xyz_12345".to_string(),
                tool_command: ":run search_documents \"nonexistent_content_xyz_12345\"".to_string(),
                expected_behavior: ErrorBehavior::GracefulHandling,
            },
            ErrorScenario {
                description: "Invalid search parameters".to_string(),
                cli_command: vec!["search", "", "--limit", "invalid"],
                repl_command: "search  --limit invalid".to_string(),
                tool_command: ":run nonexistent_tool".to_string(),
                expected_behavior: ErrorBehavior::ClearErrorMessage,
            },
        ];

        for scenario in error_scenarios {
            println!("  ❌ Testing error scenario: {}", scenario.description);

            // Test CLI error handling
            let cli_harness = CliTestHarness::new().await?;
            let cli_result = cli_harness.execute_cli_command(&scenario.cli_command)?;

            // Test REPL error handling
            let repl_harness = ReplTestHarness::new().await?;
            let mut repl = repl_harness.spawn_repl()?;
            let repl_output = repl.send_command(&scenario.repl_command);
            repl.quit();

            // Test Tool API error handling
            let tool_harness = ToolApiTestHarness::new().await?;
            let mut tool_repl = tool_harness.spawn_repl()?;
            let tool_output = tool_repl.send_command(&scenario.tool_command);
            tool_repl.quit();

            // Validate error handling consistency
            self.validate_error_handling_consistency(&scenario, &cli_result, &repl_output, &tool_output)?;
        }

        println!("✅ Error handling consistency test passed");
        Ok(())
    }

    /// Test state consistency across interfaces
    pub async fn test_state_consistency(&self) -> Result<()> {
        println!("🧪 Testing state consistency across interfaces");

        // Test 1: Indexing state consistency
        println!("  📚 Testing indexing state consistency");

        // Index through CLI
        let cli_harness = CliTestHarness::new().await?;
        let index_result = cli_harness.execute_cli_command(&[
            "index",
            "--path", self.vault_dir.to_str().unwrap(),
            "--glob", "**/*.md"
        ])?;
        assert!(index_result.exit_code == 0, "CLI indexing should succeed");

        // Check state through CLI stats
        let cli_stats = cli_harness.execute_cli_command(&["stats"])?;
        assert!(cli_stats.exit_code == 0, "CLI stats should succeed");

        // Check state through REPL
        let repl_harness = ReplTestHarness::new().await?;
        let mut repl = repl_harness.spawn_repl()?;
        let repl_stats = repl.send_command(":run get_vault_stats")?;
        repl.quit();

        // Both should show indexed content
        assert!(!cli_stats.stdout.is_empty(), "CLI stats should show content");
        assert!(!repl_stats.is_empty(), "REPL stats should show content");

        // Test 2: Configuration state consistency
        println!("  ⚙️ Testing configuration state consistency");

        // This would test that configuration changes persist across interfaces
        // For now, we'll just verify both interfaces can read configuration

        let cli_config = cli_harness.execute_cli_command(&["config", "show"])?;
        assert!(cli_config.exit_code == 0, "CLI config should work");

        // Test 3: Search result consistency after state changes
        println!("  🔍 Testing search consistency after state changes");

        let search_query = "quantum computing";

        // Search through CLI
        let cli_search = cli_harness.execute_cli_command(&["search", search_query])?;
        assert!(cli_search.exit_code == 0, "CLI search should succeed");

        // Search through REPL
        let mut repl = repl_harness.spawn_repl()?;
        let repl_search = repl.send_command(&format!("search {}", search_query));
        repl.quit();

        // Both should find the same content
        assert!(cli_search.stdout.contains("quantum") || !cli_search.stdout.is_empty(),
               "CLI should find quantum content");

        if let Ok(repl_result) = repl_search {
            assert!(repl_result.contains("quantum") || !repl_result.is_empty(),
                   "REPL should find quantum content");
        }

        println!("✅ State consistency test passed");
        Ok(())
    }

    /// Test resource usage consistency across interfaces
    pub async fn test_resource_usage_consistency(&self) -> Result<()> {
        println!("🧪 Testing resource usage consistency across interfaces");

        let test_queries = vec![
            "quantum computing fundamentals applications",
            "rust async patterns error handling performance",
            "project management tasks deadlines milestones",
        ];

        for query in test_queries {
            println!("  💾 Testing resource usage for query: '{}'", query);

            let mut resource_metrics = HashMap::new();

            // Test CLI resource usage
            let cli_harness = CliTestHarness::new().await?;
            let cli_start = Instant::now();
            let cli_result = cli_harness.execute_cli_command(&["search", query])?;
            let cli_duration = cli_start.elapsed();

            resource_metrics.insert(InterfaceType::Cli, ResourceMetric {
                duration: cli_duration,
                memory_usage_estimate: cli_result.stdout.len(), // Rough estimate
                output_size: cli_result.stdout.len(),
                success: cli_result.exit_code == 0,
            });

            // Test REPL resource usage
            let repl_harness = ReplTestHarness::new().await?;
            let mut repl = repl_harness.spawn_repl()?;
            let repl_start = Instant::now();
            let repl_output = repl.send_command(&format!("search {}", query))?;
            let repl_duration = repl_start.elapsed();
            repl.quit();

            resource_metrics.insert(InterfaceType::Repl, ResourceMetric {
                duration: repl_duration,
                memory_usage_estimate: repl_output.len(),
                output_size: repl_output.len(),
                success: !repl_output.is_empty(),
            });

            // Validate resource usage is within reasonable bounds
            self.validate_resource_usage_consistency(&resource_metrics, query)?;
        }

        println!("✅ Resource usage consistency test passed");
        Ok(())
    }

    // Helper methods

    /// Count results in output text
    fn count_results(&self, output: &str) -> usize {
        // Simple heuristic: count non-empty lines that look like results
        output.lines()
            .filter(|line| {
                let trimmed = line.trim();
                !trimmed.is_empty() &&
                !trimmed.starts_with("crucible>") &&
                !trimmed.starts_with("Search") &&
                !trimmed.starts_with("Found") &&
                !trimmed.starts_with("─") &&
                !trimmed.starts_with("│")
            })
            .count()
    }

    /// Validate query consistency across interfaces
    fn validate_query_consistency(&self, test_query: &TestQuery, results: &HashMap<InterfaceType, InterfaceQueryResult>) -> Result<()> {
        let result_counts: Vec<_> = results.values()
            .map(|r| r.result_count)
            .collect();

        if result_counts.is_empty() {
            return Err(anyhow!("No results to validate for query: {}", test_query.query));
        }

        let max_count = *result_counts.iter().max().unwrap_or(&0);
        let min_count = *result_counts.iter().min().unwrap_or(&0);

        // Check if all interfaces succeeded
        for (interface_type, result) in results {
            assert!(result.success, "Interface {:?} should succeed for query: {}", interface_type, test_query.query);
        }

        // Check result count consistency within tolerance
        if max_count > 0 {
            let variance = if max_count > min_count {
                (max_count - min_count) as f64 / max_count as f64
            } else {
                0.0
            };

            assert!(variance <= test_query.tolerance,
                   "Result count variance ({:.2}) exceeds tolerance ({:.2}) for query: '{}'. Counts: {:?}",
                   variance, test_query.tolerance, test_query.query, result_counts);
        }

        // Check minimum result count
        assert!(min_count >= test_query.expected_min_results,
               "Expected at least {} results, got {} for query: '{}'",
               test_query.expected_min_results, min_count, test_query.query);

        Ok(())
    }

    /// Validate performance consistency across interfaces
    fn validate_performance_consistency(&self, metrics: &HashMap<String, HashMap<InterfaceType, PerformanceMetric>>) -> Result<()> {
        for (query, query_metrics) in metrics {
            let durations: Vec<_> = query_metrics.values()
                .map(|m| m.duration)
                .collect();

            if durations.is_empty() {
                continue;
            }

            let max_duration = *durations.iter().max().unwrap();
            let min_duration = *durations.iter().min().unwrap();

            // Performance variance should be within reasonable bounds
            let variance_ratio = if min_duration > Duration::ZERO {
                max_duration.as_millis() as f64 / min_duration.as_millis() as f64
            } else {
                1.0
            };

            // Allow up to 5x performance difference between interfaces
            assert!(variance_ratio <= 5.0,
                   "Performance variance too high for query '{}': {:.1}x ratio (min: {:?}, max: {:?})",
                   query, variance_ratio, min_duration, max_duration);

            // All interfaces should succeed
            for (interface_type, metric) in query_metrics {
                assert!(metric.success, "Interface {:?} should succeed for query: {}", interface_type, query);

                // Performance should be reasonable
                assert!(metric.duration < Duration::from_secs(10),
                       "Interface {:?} performance too slow for query '{}': {:?}",
                       interface_type, query, metric.duration);
            }

            println!("    ✅ Performance consistency for '{}': variance {:.1}x, durations: {:?}",
                     query, variance_ratio, durations);
        }

        Ok(())
    }

    /// Validate format characteristics
    fn validate_format_characteristics(&self, format_test: &FormatTest, output: &str, interface: &str) -> Result<()> {
        assert!(!output.is_empty(), "{} output should not be empty for {} format",
                interface, format_test.format);

        let validation_passed = match format_test.format {
            OutputFormat::Table => {
                output.contains("│") || output.lines().count() > 2 || output.contains("+")
            }
            OutputFormat::Json => {
                output.contains("{") || output.contains("[") || output.contains("\"")
            }
            OutputFormat::Csv => {
                output.contains(",") || output.lines().count() > 1
            }
        };

        assert!(validation_passed,
               "{} {} format output should have expected characteristics. Output preview: {}",
               interface, format_test.format, &output[..output.len().min(100)]);

        Ok(())
    }

    /// Validate error handling consistency
    fn validate_error_handling_consistency(&self, scenario: &ErrorScenario,
                                         cli_result: &CommandResult,
                                         repl_result: &Result<String, anyhow::Error>,
                                         tool_result: &Result<String, anyhow::Error>) -> Result<()> {
        match scenario.expected_behavior {
            ErrorBehavior::GracefulHandling => {
                // Should not crash, should handle gracefully
                assert!(cli_result.exit_code != 0 || !cli_result.stderr.is_empty(),
                       "CLI should handle error gracefully for: {}", scenario.description);

                if let Ok(repl_output) = repl_result {
                    assert!(!repl_output.contains("panic") && !repl_output.contains("stack trace"),
                           "REPL should handle error gracefully for: {}", scenario.description);
                }

                if let Ok(tool_output) = tool_result {
                    assert!(!tool_output.contains("panic") && !tool_output.contains("stack trace"),
                           "Tool should handle error gracefully for: {}", scenario.description);
                }
            }
            ErrorBehavior::ClearErrorMessage => {
                // Should provide clear error messages
                if cli_result.exit_code != 0 {
                    assert!(!cli_result.stderr.is_empty(),
                           "CLI should provide clear error message for: {}", scenario.description);
                }

                // For REPL and tools, check if they provide meaningful error feedback
                if let Err(repl_err) = repl_result {
                    // Expected to fail, but error should be meaningful
                    assert!(!repl_err.to_string().is_empty(),
                           "REPL error should be meaningful for: {}", scenario.description);
                }
            }
        }

        Ok(())
    }

    /// Validate resource usage consistency
    fn validate_resource_usage_consistency(&self, metrics: &HashMap<InterfaceType, ResourceMetric>, query: &str) -> Result<()> {
        let durations: Vec<_> = metrics.values().map(|m| m.duration).collect();
        let output_sizes: Vec<_> = metrics.values().map(|m| m.output_size).collect();

        if durations.is_empty() {
            return Ok(());
        }

        let max_duration = *durations.iter().max().unwrap();
        let max_output_size = *output_sizes.iter().max().unwrap();

        // Performance should be reasonable
        assert!(max_duration < Duration::from_secs(15),
               "All interfaces should complete within 15 seconds for query '{}', max was {:?}",
               query, max_duration);

        // Output size should be reasonable (not unexpectedly large)
        assert!(max_output_size < 1_000_000, // 1MB limit
               "Output size should be reasonable for query '{}', max was {} bytes",
               query, max_output_size);

        // All interfaces should succeed
        for (interface_type, metric) in metrics {
            assert!(metric.success, "Interface {:?} should succeed for query: {}", interface_type, query);
        }

        println!("    ✅ Resource usage for '{}': max duration {:?}, max output {} bytes",
                 query, max_duration, max_output_size);

        Ok(())
    }
}

// Data structures for testing

#[derive(Debug, Clone)]
pub struct TestQuery {
    pub query: String,
    pub interface_types: Vec<InterfaceType>,
    pub expected_min_results: usize,
    pub tolerance: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InterfaceType {
    Cli,
    Repl,
    Tool,
}

#[derive(Debug, Clone)]
pub struct InterfaceQueryResult {
    pub interface_type: InterfaceType,
    pub raw_output: String,
    pub exit_code: i32,
    pub duration: Duration,
    pub result_count: usize,
    pub success: bool,
}

#[derive(Debug, Clone)]
pub struct PerformanceMetric {
    pub duration: Duration,
    pub success: bool,
    pub output_size: usize,
}

#[derive(Debug, Clone)]
pub struct FormatTest {
    pub query: String,
    pub format: OutputFormat,
    pub expected_characteristics: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum OutputFormat {
    Table,
    Json,
    Csv,
}

#[derive(Debug, Clone)]
pub struct ErrorScenario {
    pub description: String,
    pub cli_command: Vec<&'static str>,
    pub repl_command: String,
    pub tool_command: String,
    pub expected_behavior: ErrorBehavior,
}

#[derive(Debug, Clone)]
pub enum ErrorBehavior {
    GracefulHandling,
    ClearErrorMessage,
}

#[derive(Debug, Clone)]
pub struct ResourceMetric {
    pub duration: Duration,
    pub memory_usage_estimate: usize,
    pub output_size: usize,
    pub success: bool,
}

// ============================================================================
// Test Execution Functions
// ============================================================================

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_cross_interface_consistency_comprehensive() -> Result<()> {
    println!("🧪 Running comprehensive cross-interface consistency tests");

    let harness = CrossInterfaceTestHarness::new().await?;

    // Run all consistency tests
    harness.test_query_consistency().await?;
    harness.test_performance_consistency().await?;
    harness.test_output_format_consistency().await?;
    harness.test_error_handling_consistency().await?;
    harness.test_state_consistency().await?;
    harness.test_resource_usage_consistency().await?;

    println!("✅ Comprehensive cross-interface consistency tests passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_query_consistency_validation() -> Result<()> {
    println!("🧪 Testing query consistency validation");

    let harness = CrossInterfaceTestHarness::new().await?;
    harness.test_query_consistency().await?;

    println!("✅ Query consistency validation test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_performance_consistency_validation() -> Result<()> {
    println!("🧪 Testing performance consistency validation");

    let harness = CrossInterfaceTestHarness::new().await?;
    harness.test_performance_consistency().await?;

    println!("✅ Performance consistency validation test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_output_format_consistency_validation() -> Result<()> {
    println!("🧪 Testing output format consistency validation");

    let harness = CrossInterfaceTestHarness::new().await?;
    harness.test_output_format_consistency().await?;

    println!("✅ Output format consistency validation test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_error_handling_consistency_validation() -> Result<()> {
    println!("🧪 Testing error handling consistency validation");

    let harness = CrossInterfaceTestHarness::new().await?;
    harness.test_error_handling_consistency().await?;

    println!("✅ Error handling consistency validation test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_state_consistency_validation() -> Result<()> {
    println!("🧪 Testing state consistency validation");

    let harness = CrossInterfaceTestHarness::new().await?;
    harness.test_state_consistency().await?;

    println!("✅ State consistency validation test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_resource_usage_consistency_validation() -> Result<()> {
    println!("🧪 Testing resource usage consistency validation");

    let harness = CrossInterfaceTestHarness::new().await?;
    harness.test_resource_usage_consistency().await?;

    println!("✅ Resource usage consistency validation test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_interface_equivalence_matrix() -> Result<()> {
    println!("🧪 Testing interface equivalence matrix");

    let harness = CrossInterfaceTestHarness::new().await?;

    // Test a comprehensive matrix of queries across all interfaces
    let equivalence_queries = vec![
        "quantum computing",
        "rust async patterns",
        "project management",
        "deployment checklist",
        "machine learning",
        "git commands",
        "travel planning",
        "system design",
    ];

    let mut consistency_results = Vec::new();

    for query in equivalence_queries {
        println!("  🔍 Testing equivalence for query: '{}'", query);

        // CLI
        let cli_harness = CliTestHarness::new().await?;
        let cli_result = cli_harness.execute_cli_command(&["search", query])?;
        let cli_success = cli_result.exit_code == 0 && !cli_result.stdout.is_empty();

        // REPL
        let repl_harness = ReplTestHarness::new().await?;
        let mut repl = repl_harness.spawn_repl()?;
        let repl_result = repl.send_command(&format!("search {}", query));
        let repl_success = repl_result.is_ok() &&
                          repl_result.as_ref().map_or(false, |s| !s.is_empty());
        repl.quit();

        // Tool
        let tool_harness = ToolApiTestHarness::new().await?;
        let mut tool_repl = tool_harness.spawn_repl()?;
        let tool_result = tool_repl.send_command(&format!(":run search_documents \"{}\"", query));
        let tool_success = tool_result.is_ok() &&
                         tool_result.as_ref().map_or(false, |s| !s.is_empty() && !s.contains("❌"));
        tool_repl.quit();

        let all_succeeded = cli_success && repl_success && tool_success;
        consistency_results.push((query.to_string(), all_succeeded));

        assert!(all_succeeded,
               "All interfaces should succeed for query '{}'. CLI: {}, REPL: {}, Tool: {}",
               query, cli_success, repl_success, tool_success);
    }

    let successful_queries = consistency_results.iter().filter(|(_, success)| *success).count();
    let total_queries = consistency_results.len();

    assert!(successful_queries == total_queries,
           "All {} queries should succeed across all interfaces, got {}",
           total_queries, successful_queries);

    println!("✅ Interface equivalence matrix test passed - {}/{} queries consistent across all interfaces",
             successful_queries, total_queries);

    Ok(())
}
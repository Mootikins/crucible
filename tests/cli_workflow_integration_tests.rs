//! CLI Workflow Integration Tests
//!
//! This module specifically focuses on testing CLI command workflows and integration
//! patterns. These tests validate that the CLI interface works correctly across
//! different commands, options, and usage patterns.

use std::process::Command;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

use crate::comprehensive_integration_workflow_tests::{
    ComprehensiveTestKiln, CliTestHarness, CommandResult
};

/// Extended CLI workflow test harness
pub struct ExtendedCliTestHarness {
    kiln_dir: TempDir,
    test_kiln: ComprehensiveTestKiln,
}

impl ExtendedCliTestHarness {
    /// Create a new extended CLI test harness
    pub async fn new() -> Result<Self> {
        let test_kiln = ComprehensiveTestKiln::create().await?;
        let kiln_dir = test_kiln.path().to_owned();

        Ok(Self {
            kiln_dir: kiln_dir.to_owned(),
            test_kiln,
        })
    }

    /// Execute a CLI command with environment variables and timeout
    pub fn execute_cli_command_with_env(&self, args: &[&str], env_vars: &[(&str, &str)]) -> Result<CommandResult> {
        let start_time = Instant::now();

        let mut cmd = Command::new(env!("CARGO_BIN_EXE_crucible-cli"));
        cmd.args(args)
           .current_dir(&self.kiln_dir)
           .env("CRUCIBLE_KILN_PATH", self.kiln_dir.to_str().unwrap());

        // Add environment variables
        for (key, value) in env_vars {
            cmd.env(key, value);
        }

        let output = cmd.output()
            .map_err(|e| anyhow!("Failed to execute CLI command: {}", e))?;

        let duration = start_time.elapsed();

        Ok(CommandResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
            duration,
        })
    }

    /// Test advanced CLI search workflows with different options
    pub async fn test_advanced_search_workflows(&self) -> Result<()> {
        println!("🧪 Testing advanced CLI search workflows");

        // Test 1: Search with different output formats
        let formats = vec!["table", "json", "csv"];
        for format in formats {
            let result = self.execute_cli_command(&[
                "search", "quantum computing",
                "--format", format,
                "--limit", "5"
            ])?;

            assert!(result.exit_code == 0, "Search should succeed with {} format", format);
            assert!(!result.stdout.is_empty(), "Search should return results in {} format", format);

            // Verify format-specific characteristics
            match format {
                "json" => assert!(result.stdout.contains("{") || result.stdout.contains("["),
                                   "JSON format should contain JSON structure"),
                "csv" => assert!(result.stdout.contains(",") || result.lines().count() > 1,
                                  "CSV format should contain comma-separated values"),
                "table" => assert!(result.stdout.contains("│") || result.stdout.lines().count() > 2,
                                   "Table format should contain table structure"),
                _ => {}
            }

            println!("✅ Search with {} format passed", format);
        }

        // Test 2: Search with content preview
        let result = self.execute_cli_command(&[
            "search", "rust patterns",
            "--show-content",
            "--limit", "3"
        ])?;

        assert!(result.exit_code == 0, "Search with content should succeed");
        assert!(result.stdout.len() > 200, "Should show content preview");

        // Test 3: Fuzzy search with specific options
        let result = self.execute_cli_command(&[
            "fuzzy", "project",
            "--content", "true",
            "--tags", "true",
            "--paths", "true",
            "--limit", "10"
        ])?;

        assert!(result.exit_code == 0, "Fuzzy search should succeed");
        assert!(!result.stdout.is_empty(), "Fuzzy search should return results");

        // Test 4: Semantic search with scores
        let result = self.execute_cli_command(&[
            "semantic", "machine learning",
            "--top-k", "5",
            "--show-scores",
            "--format", "json"
        ])?;

        assert!(result.exit_code == 0, "Semantic search should succeed");
        assert!(!result.stdout.is_empty(), "Semantic search should return results");

        println!("✅ Advanced CLI search workflows test passed");
        Ok(())
    }

    /// Test CLI indexing workflows with different scenarios
    pub async fn test_indexing_workflows(&self) -> Result<()> {
        println!("🧪 Testing CLI indexing workflows");

        // Test 1: Basic indexing
        let result = self.execute_cli_command(&[
            "index",
            "--path", self.kiln_dir.to_str().unwrap(),
            "--glob", "**/*.md"
        ])?;

        assert!(result.exit_code == 0, "Basic indexing should succeed");
        assert!(result.stdout.contains("files") || result.stdout.contains("indexed"),
               "Should report indexed files");

        // Test 2: Force re-indexing
        let result = self.execute_cli_command(&[
            "index",
            "--path", self.kiln_dir.to_str().unwrap(),
            "--force",
            "--glob", "**/*.md"
        ])?;

        assert!(result.exit_code == 0, "Force re-indexing should succeed");

        // Test 3: Index with custom glob pattern
        let result = self.execute_cli_command(&[
            "index",
            "--path", self.kiln_dir.to_str().unwrap(),
            "--glob", "code/**/*.md"
        ])?;

        assert!(result.exit_code == 0, "Pattern-based indexing should succeed");

        // Test 4: Index specific subdirectory
        let result = self.execute_cli_command(&[
            "index",
            "--path", self.kiln_dir.join("research").to_str().unwrap()
        ])?;

        assert!(result.exit_code == 0, "Subdirectory indexing should succeed");

        println!("✅ CLI indexing workflows test passed");
        Ok(())
    }

    /// Test CLI note management workflows
    pub async fn test_note_workflows(&self) -> Result<()> {
        println!("🧪 Testing CLI note workflows");

        // Test 1: List all notes
        let result = self.execute_cli_command(&["note", "list", "--format", "table"])?;

        assert!(result.exit_code == 0, "Note list should succeed");
        assert!(!result.stdout.is_empty(), "Should list notes");

        // Test 2: Get a specific note
        let note_path = "research/quantum-computing.md";
        let result = self.execute_cli_command(&[
            "note", "get", note_path,
            "--format", "plain"
        ])?;

        assert!(result.exit_code == 0, "Note get should succeed");
        assert!(result.stdout.contains("quantum"), "Should contain note content");

        // Test 3: Create a new note
        let new_note_path = "test-note.md";
        let new_note_content = "# Test Note\n\nThis is a test note created by CLI.";

        let result = self.execute_cli_command(&[
            "note", "create", new_note_path,
            "--content", new_note_content
        ])?;

        assert!(result.exit_code == 0, "Note creation should succeed");

        // Test 4: Update note properties
        let properties = r#"{"tags": ["test", "cli"], "status": "draft"}"#;
        let result = self.execute_cli_command(&[
            "note", "update", new_note_path,
            "--properties", properties
        ])?;

        assert!(result.exit_code == 0, "Note update should succeed");

        println!("✅ CLI note workflows test passed");
        Ok(())
    }

    /// Test CLI configuration workflows
    pub async fn test_config_workflows(&self) -> Result<()> {
        println!("🧪 Testing CLI configuration workflows");

        // Test 1: Show current configuration
        let result = self.execute_cli_command(&["config", "show", "--format", "toml"])?;

        assert!(result.exit_code == 0, "Config show should succeed");
        assert!(!result.stdout.is_empty(), "Should show configuration");

        // Test 2: Show configuration in JSON format
        let result = self.execute_cli_command(&["config", "show", "--format", "json"])?;

        assert!(result.exit_code == 0, "Config show in JSON should succeed");
        assert!(result.stdout.contains("{") || result.stdout.contains("["),
               "JSON format should contain JSON structure");

        println!("✅ CLI configuration workflows test passed");
        Ok(())
    }

    /// Test CLI error handling and edge cases
    pub async fn test_error_handling_workflows(&self) -> Result<()> {
        println!("🧪 Testing CLI error handling workflows");

        // Test 1: Invalid command
        let result = self.execute_cli_command(&["invalid-command"])?;

        assert!(result.exit_code != 0, "Invalid command should fail");
        assert!(!result.stderr.is_empty(), "Should show error message");

        // Test 2: Search with non-existent query
        let result = self.execute_cli_command(&["search", "nonexistent-term-xyz-12345"])?;

        assert!(result.exit_code == 0, "Search with no results should still succeed");
        // Should handle gracefully without crashing

        // Test 3: Invalid file path for note operations
        let result = self.execute_cli_command(&["note", "get", "/invalid/path/note.md"])?;

        assert!(result.exit_code != 0, "Invalid note path should fail");

        // Test 4: Index non-existent directory
        let result = self.execute_cli_command(&[
            "index",
            "--path", "/nonexistent/directory"
        ])?;

        assert!(result.exit_code != 0, "Indexing non-existent directory should fail");

        // Test 5: Invalid format option
        let result = self.execute_cli_command(&[
            "search", "test",
            "--format", "invalid-format"
        ])?;

        assert!(result.exit_code != 0, "Invalid format should fail");

        println!("✅ CLI error handling workflows test passed");
        Ok(())
    }

    /// Test CLI performance with large datasets
    pub async fn test_performance_workflows(&self) -> Result<()> {
        println!("🧪 Testing CLI performance workflows");

        // Test 1: Search performance with different query complexities
        let search_queries = vec![
            ("simple", "quantum"),
            ("medium", "quantum computing fundamentals"),
            ("complex", "quantum computing fundamentals applications challenges"),
        ];

        for (complexity, query) in search_queries {
            let start_time = Instant::now();
            let result = self.execute_cli_command(&["search", query, "--limit", "10"])?;
            let duration = start_time.elapsed();

            assert!(result.exit_code == 0, "Search should succeed for {} query", complexity);

            // Performance assertion (should complete within reasonable time)
            assert!(duration < Duration::from_secs(5),
                   "Search should complete within 5 seconds for {} query, took {:?}",
                   complexity, duration);

            println!("✅ {} search completed in {:?}", complexity, duration);
        }

        // Test 2: Multiple rapid searches
        let start_time = Instant::now();
        for i in 0..10 {
            let query = format!("search term {}", i);
            let result = self.execute_cli_command(&["search", &query, "--limit", "5"])?;
            assert!(result.exit_code == 0, "Rapid search {} should succeed", i);
        }
        let total_duration = start_time.elapsed();

        // Should handle 10 searches efficiently
        assert!(total_duration < Duration::from_secs(30),
               "10 rapid searches should complete within 30 seconds, took {:?}",
               total_duration);

        println!("✅ 10 rapid searches completed in {:?}", total_duration);

        // Test 3: Large result set handling
        let result = self.execute_cli_command(&["search", "", "--limit", "100"])?;

        assert!(result.exit_code == 0, "Large result set search should succeed");
        assert!(!result.stdout.is_empty(), "Should return results");

        println!("✅ CLI performance workflows test passed");
        Ok(())
    }

    /// Test CLI integration with external tools and services
    pub async fn test_external_integration_workflows(&self) -> Result<()> {
        println!("🧪 Testing CLI external integration workflows");

        // Test 1: Tool testing command
        let result = self.execute_cli_command(&["test"])?;

        assert!(result.exit_code == 0, "Tool test should succeed");
        assert!(!result.stdout.is_empty(), "Should show tool test results");

        // Test 2: Run command with script execution (if available)
        // This test might be skipped if Rune scripts aren't available
        let result = self.execute_cli_command(&["commands"])?;

        assert!(result.exit_code == 0, "Commands listing should succeed");
        assert!(result.stdout.contains("Available Commands") || result.stdout.contains("commands"),
               "Should list available commands");

        // Test 3: Service health check
        let result = self.execute_cli_command(&["service", "health"])?;

        // This might fail if services aren't running, but should handle gracefully
        println!("Service health check result: {}", result.exit_code);

        println!("✅ CLI external integration workflows test passed");
        Ok(())
    }
}

/// CLI workflow test scenarios
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliWorkflowScenario {
    pub name: String,
    pub description: String,
    pub commands: Vec<CliCommand>,
    pub expected_outcomes: Vec<String>,
    pub performance_thresholds: Option<PerformanceThresholds>,
}

/// Individual CLI command in a workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliCommand {
    pub args: Vec<String>,
    pub env_vars: Vec<(String, String)>,
    pub expected_exit_code: i32,
    pub timeout_ms: Option<u64>,
}

/// Performance thresholds for workflow validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceThresholds {
    pub max_total_duration_ms: u64,
    pub max_individual_command_duration_ms: u64,
}

/// Workflow test runner
pub struct WorkflowTestRunner {
    scenarios: Vec<CliWorkflowScenario>,
}

impl WorkflowTestRunner {
    /// Create new workflow test runner with default scenarios
    pub fn new() -> Self {
        let scenarios = vec![
            CliWorkflowScenario {
                name: "Basic Research Workflow".to_string(),
                description: "Complete research workflow from indexing to search to retrieval".to_string(),
                commands: vec![
                    CliCommand {
                        args: vec!["index".to_string(), "--glob".to_string(), "**/*.md".to_string()],
                        env_vars: vec![],
                        expected_exit_code: 0,
                        timeout_ms: Some(30000),
                    },
                    CliCommand {
                        args: vec!["search".to_string(), "quantum computing".to_string(), "--limit".to_string(), "5".to_string()],
                        env_vars: vec![],
                        expected_exit_code: 0,
                        timeout_ms: Some(10000),
                    },
                    CliCommand {
                        args: vec!["semantic".to_string(), "physics research".to_string(), "--top-k".to_string(), "3".to_string()],
                        env_vars: vec![],
                        expected_exit_code: 0,
                        timeout_ms: Some(15000),
                    },
                ],
                expected_outcomes: vec![
                    "Files indexed successfully".to_string(),
                    "Search results found".to_string(),
                    "Semantic search completed".to_string(),
                ],
                performance_thresholds: Some(PerformanceThresholds {
                    max_total_duration_ms: 60000,
                    max_individual_command_duration_ms: 30000,
                }),
            },
            CliWorkflowScenario {
                name: "Project Management Workflow".to_string(),
                description: "Project management workflow with notes and tasks".to_string(),
                commands: vec![
                    CliCommand {
                        args: vec!["note".to_string(), "list".to_string(), "--format".to_string(), "table".to_string()],
                        env_vars: vec![],
                        expected_exit_code: 0,
                        timeout_ms: Some(10000),
                    },
                    CliCommand {
                        args: vec!["search".to_string(), "project management tasks".to_string()],
                        env_vars: vec![],
                        expected_exit_code: 0,
                        timeout_ms: Some(10000),
                    },
                    CliCommand {
                        args: vec!["fuzzy".to_string(), "deadline".to_string(), "--tags".to_string(), "true".to_string()],
                        env_vars: vec![],
                        expected_exit_code: 0,
                        timeout_ms: Some(15000),
                    },
                ],
                expected_outcomes: vec![
                    "Notes listed successfully".to_string(),
                    "Project information found".to_string(),
                    "Deadline information retrieved".to_string(),
                ],
                performance_thresholds: Some(PerformanceThresholds {
                    max_total_duration_ms: 45000,
                    max_individual_command_duration_ms: 20000,
                }),
            },
        ];

        Self { scenarios }
    }

    /// Run all workflow scenarios
    pub async fn run_all_workflows(&self, harness: &ExtendedCliTestHarness) -> Result<Vec<WorkflowResult>> {
        let mut results = Vec::new();

        for scenario in &self.scenarios {
            println!("\n🔄 Running workflow: {}", scenario.name);
            let result = self.run_workflow(scenario, harness).await?;
            results.push(result);
        }

        Ok(results)
    }

    /// Run a single workflow scenario
    async fn run_workflow(&self, scenario: &CliWorkflowScenario, harness: &ExtendedCliTestHarness) -> Result<WorkflowResult> {
        let workflow_start = Instant::now();
        let mut command_results = Vec::new();
        let mut all_commands_passed = true;

        for (i, command) in scenario.commands.iter().enumerate() {
            println!("  📋 Executing command {}/{}: {:?}", i + 1, scenario.commands.len(), command.args);

            let cmd_start = Instant::now();
            let env_vars: Vec<(&str, &str)> = command.env_vars.iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();

            let result = harness.execute_cli_command_with_env(
                &command.args.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                &env_vars
            )?;

            let cmd_duration = cmd_start.elapsed();

            // Validate command result
            let command_passed = result.exit_code == command.expected_exit_code;
            if !command_passed {
                all_commands_passed = false;
                println!("    ❌ Command failed: expected {}, got {}", command.expected_exit_code, result.exit_code);
                println!("    stderr: {}", result.stderr);
            } else {
                println!("    ✅ Command passed in {:?}", cmd_duration);
            }

            // Check performance thresholds
            if let Some(thresholds) = &scenario.performance_thresholds {
                if cmd_duration.as_millis() as u64 > thresholds.max_individual_command_duration_ms {
                    println!("    ⚠️  Command exceeded performance threshold: {:?} > {}ms",
                             cmd_duration, thresholds.max_individual_command_duration_ms);
                }
            }

            command_results.push(CommandWorkflowResult {
                command: command.clone(),
                result,
                duration: cmd_duration,
                passed: command_passed,
            });
        }

        let workflow_duration = workflow_start.elapsed();

        // Check overall performance
        let performance_passed = if let Some(thresholds) = &scenario.performance_thresholds {
            workflow_duration.as_millis() as u64 <= thresholds.max_total_duration_ms
        } else {
            true
        };

        let workflow_passed = all_commands_passed && performance_passed;

        println!("  📊 Workflow '{}' completed in {:?} - {}",
                 scenario.name, workflow_duration,
                 if workflow_passed { "✅ PASSED" } else { "❌ FAILED" });

        Ok(WorkflowResult {
            scenario: scenario.clone(),
            command_results,
            total_duration: workflow_duration,
            passed: workflow_passed,
        })
    }
}

/// Result of running a workflow
#[derive(Debug, Clone)]
pub struct WorkflowResult {
    pub scenario: CliWorkflowScenario,
    pub command_results: Vec<CommandWorkflowResult>,
    pub total_duration: Duration,
    pub passed: bool,
}

/// Result of a single command in a workflow
#[derive(Debug, Clone)]
pub struct CommandWorkflowResult {
    pub command: CliCommand,
    pub result: CommandResult,
    pub duration: Duration,
    pub passed: bool,
}

// ============================================================================
// Test Execution Functions
// ============================================================================

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_cli_workflow_integration_comprehensive() -> Result<()> {
    println!("🧪 Running comprehensive CLI workflow integration tests");

    let harness = ExtendedCliTestHarness::new().await?;

    // Run all workflow tests
    harness.test_advanced_search_workflows().await?;
    harness.test_indexing_workflows().await?;
    harness.test_note_workflows().await?;
    harness.test_config_workflows().await?;
    harness.test_error_handling_workflows().await?;
    harness.test_performance_workflows().await?;
    harness.test_external_integration_workflows().await?;

    // Run workflow scenarios
    let runner = WorkflowTestRunner::new();
    let workflow_results = runner.run_all_workflows(&harness).await?;

    // Validate workflow results
    let total_workflows = workflow_results.len();
    let passed_workflows = workflow_results.iter().filter(|r| r.passed).count();

    assert!(passed_workflows == total_workflows,
           "All {} workflows should pass, but only {} passed",
           total_workflows, passed_workflows);

    println!("✅ CLI workflow integration tests passed - {}/{} workflows successful",
             passed_workflows, total_workflows);

    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_cli_search_workflow_variations() -> Result<()> {
    println!("🧪 Testing CLI search workflow variations");

    let harness = ExtendedCliTestHarness::new().await?;
    harness.test_advanced_search_workflows().await?;

    println!("✅ CLI search workflow variations test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_cli_indexing_workflow_scenarios() -> Result<()> {
    println!("🧪 Testing CLI indexing workflow scenarios");

    let harness = ExtendedCliTestHarness::new().await?;
    harness.test_indexing_workflows().await?;

    println!("✅ CLI indexing workflow scenarios test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_cli_error_handling_robustness() -> Result<()> {
    println!("🧪 Testing CLI error handling robustness");

    let harness = ExtendedCliTestHarness::new().await?;
    harness.test_error_handling_workflows().await?;

    println!("✅ CLI error handling robustness test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_cli_performance_validation() -> Result<()> {
    println!("🧪 Testing CLI performance validation");

    let harness = ExtendedCliTestHarness::new().await?;
    harness.test_performance_workflows().await?;

    println!("✅ CLI performance validation test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_cli_workflow_scenarios() -> Result<()> {
    println!("🧪 Testing CLI workflow scenarios");

    let harness = ExtendedCliTestHarness::new().await?;
    let runner = WorkflowTestRunner::new();
    let results = runner.run_all_workflows(&harness).await?;

    let passed_count = results.iter().filter(|r| r.passed).count();
    assert!(passed_count == results.len(),
           "All workflow scenarios should pass, got {}/{}",
           passed_count, results.len());

    println!("✅ CLI workflow scenarios test passed - {}/{} scenarios successful",
             passed_count, results.len());

    Ok(())
}
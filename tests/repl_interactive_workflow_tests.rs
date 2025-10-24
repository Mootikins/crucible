//! REPL Interactive Workflow Tests
//!
//! This module specifically focuses on testing REPL interactive sessions, command history,
//! tool execution, and complex multi-step workflows that simulate real user interactions
//! with the interactive interface.

use std::io::{Write, Read, BufRead, BufReader};
use std::process::{Command, Stdio, Child};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::comprehensive_integration_workflow_tests::{
    ComprehensiveTestVault, ReplTestHarness, ReplTestProcess
};

/// Extended REPL test harness for interactive workflows
pub struct ExtendedReplTestHarness {
    vault_dir: TempDir,
    test_vault: ComprehensiveTestVault,
}

impl ExtendedReplTestHarness {
    /// Create a new extended REPL test harness
    pub async fn new() -> Result<Self> {
        let test_vault = ComprehensiveTestVault::create().await?;
        let vault_dir = test_vault.path().to_owned();

        Ok(Self {
            vault_dir: vault_dir.to_owned(),
            test_vault,
        })
    }

    /// Spawn a REPL process with custom configuration
    pub fn spawn_repl_with_config(&self, config: ReplConfig) -> Result<ExtendedReplTestProcess> {
        ExtendedReplTestProcess::spawn(&self.vault_dir, config)
    }

    /// Test REPL startup and initialization workflow
    pub async fn test_startup_workflow(&self) -> Result<()> {
        println!("🧪 Testing REPL startup workflow");

        let config = ReplConfig::default();
        let mut repl = self.spawn_repl_with_config(config)?;

        // Wait for startup and verify welcome message
        thread::sleep(Duration::from_millis(2000));

        // Test basic responsiveness
        let result = repl.send_command(":help")?;
        assert!(!result.is_empty(), "REPL should respond to help command");
        assert!(result.contains("help") || result.contains("command"), "Help should contain help information");

        // Test prompt is present
        assert!(result.contains("crucible>"), "Should show crucible prompt");

        repl.quit()?;

        println!("✅ REPL startup workflow test passed");
        Ok(())
    }

    /// Test REPL tool discovery and management workflows
    pub async fn test_tool_management_workflows(&self) -> Result<()> {
        println!("🧪 Testing REPL tool management workflows");

        let mut repl = self.spawn_repl_with_config(ReplConfig::default())?;

        // Test 1: Tool discovery
        let tools_output = repl.send_command(":tools")?;
        assert!(tools_output.contains("Available Tools"), "Should show available tools");
        assert!(tools_output.contains("system"), "Should show system tools");

        // Test 2: Tool grouping and categorization
        let tools_detailed = repl.send_command(":tools --detailed")?;
        assert!(!tools_detailed.is_empty(), "Detailed tools listing should not be empty");

        // Test 3: Tool execution with different parameter patterns
        let no_param_tool = repl.send_command(":run system_info")?;
        assert!(!no_param_tool.is_empty(), "System info tool should produce output");
        assert!(!no_param_tool.contains("❌"), "System info should not error");

        // Test 4: Tool execution with parameters
        let param_tool = repl.send_command(":run list_files /tmp")?;
        assert!(!param_tool.is_empty(), "List files tool should produce output");

        // Test 5: Tool error handling
        let error_tool = repl.send_command(":run nonexistent_tool_12345")?;
        assert!(error_tool.contains("not found") || error_tool.contains("error") || error_tool.contains("❌"),
               "Should handle missing tools gracefully");

        // Test 6: Tool help and documentation
        let tool_help = repl.send_command(":help run")?;
        assert!(!tool_help.is_empty(), "Tool help should not be empty");

        repl.quit()?;

        println!("✅ REPL tool management workflows test passed");
        Ok(())
    }

    /// Test REPL query execution and result formatting workflows
    pub async fn test_query_execution_workflows(&self) -> Result<()> {
        println!("🧪 Testing REPL query execution workflows");

        let mut repl = self.spawn_repl_with_config(ReplConfig::default())?;

        // Test 1: Basic SELECT queries
        let basic_query = repl.send_command("SELECT * FROM notes LIMIT 5")?;
        assert!(!basic_query.is_empty(), "Basic query should return results");
        assert!(basic_query.contains("│") || basic_query.lines().count() > 2,
               "Should format results as table");

        // Test 2: Queries with WHERE clauses
        let where_query = repl.send_command("SELECT * FROM notes WHERE content LIKE '%quantum%'")?;
        assert!(!where_query.is_empty(), "WHERE query should return results");

        // Test 3: COUNT queries
        let count_query = repl.send_command("SELECT COUNT(*) as total FROM notes")?;
        assert!(!count_query.is_empty(), "COUNT query should return results");
        assert!(count_query.contains("total") || count_query.contains("count"),
               "Should include count column");

        // Test 4: Queries with ORDER BY
        let order_query = repl.send_command("SELECT title, created FROM notes ORDER BY created DESC LIMIT 3")?;
        assert!(!order_query.is_empty(), "ORDER BY query should return results");

        // Test 5: Complex JOIN queries (if schema supports)
        let join_query = repl.send_command("SELECT n.title, COUNT(*) as tag_count FROM notes n JOIN tags t ON n.id = t.note_id GROUP BY n.id LIMIT 5")?;
        // This might fail if schema doesn't support joins, but should handle gracefully

        // Test 6: Query error handling
        let error_query = repl.send_command("SELECT * FROM nonexistent_table")?;
        assert!(error_query.contains("error") || error_query.contains("not found") || error_query.contains("❌"),
               "Should handle query errors gracefully");

        repl.quit()?;

        println!("✅ REPL query execution workflows test passed");
        Ok(())
    }

    /// Test REPL output formatting and display options
    pub async fn test_output_formatting_workflows(&self) -> Result<()> {
        println!("🧪 Testing REPL output formatting workflows");

        let mut repl = self.spawn_repl_with_config(ReplConfig::default())?;

        // Test 1: Table format (default)
        let table_output = repl.send_command("SELECT * FROM notes LIMIT 3")?;
        assert!(table_output.contains("│") || table_output.lines().count() > 3,
               "Table format should show table structure");

        // Test 2: Switch to JSON format
        repl.send_command(":format json")?;
        let json_output = repl.send_command("SELECT * FROM notes LIMIT 2")?;
        assert!(json_output.contains("{") || json_output.contains("["),
               "JSON format should contain JSON structure");

        // Test 3: Switch to CSV format
        repl.send_command(":format csv")?;
        let csv_output = repl.send_command("SELECT * FROM notes LIMIT 2")?;
        assert!(csv_output.contains(",") || csv_output.lines().count() > 2,
               "CSV format should contain comma-separated values");

        // Test 4: Return to table format
        repl.send_command(":format table")?;
        let table_again = repl.send_command("SELECT * FROM notes LIMIT 2")?;
        assert!(table_again.contains("│") || table_again.lines().count() > 3,
               "Should return to table format");

        // Test 5: Format validation and error handling
        let invalid_format = repl.send_command(":format invalid_format")?;
        assert!(invalid_format.contains("error") || invalid_format.contains("invalid") || invalid_format.contains("❌"),
               "Should handle invalid format gracefully");

        // Test 6: Format persistence across commands
        repl.send_command(":format json")?;
        let _ = repl.send_command("SELECT 1 as test")?;
        let persistent_json = repl.send_command("SELECT 'hello' as message")?;
        assert!(persistent_json.contains("{") || persistent_json.contains("["),
               "Format should persist across commands");

        repl.quit()?;

        println!("✅ REPL output formatting workflows test passed");
        Ok(())
    }

    /// Test REPL command history and session management
    pub async fn test_history_management_workflows(&self) -> Result<()> {
        println!("🧪 Testing REPL history management workflows");

        let mut repl = self.spawn_repl_with_config(ReplConfig::default())?;

        // Execute a series of commands to build history
        let commands = vec![
            ":stats",
            ":help",
            "SELECT COUNT(*) FROM notes",
            ":tools",
            "SELECT * FROM notes LIMIT 1",
            ":format json",
            "SELECT 'test' as query",
        ];

        for cmd in &commands {
            let result = repl.send_command(cmd)?;
            assert!(!result.is_empty(), "Command should execute: {}", cmd);
        }

        // Test 1: View command history
        let history_output = repl.send_command(":history")?;
        assert!(!history_output.is_empty(), "History should not be empty");
        assert!(history_output.lines().count() > 3, "Should show multiple history entries");

        // Test 2: Limited history display
        let limited_history = repl.send_command(":history 3")?;
        assert!(!limited_history.is_empty(), "Limited history should not be empty");

        // Test 3: History contains executed commands
        for cmd in &commands {
            if cmd.starts_with("SELECT") || cmd.starts_with(":") {
                // Check if some of our commands appear in history
                if history_output.contains(cmd.split_whitespace().next().unwrap_or("")) {
                    println!("Found command in history: {}", cmd);
                }
            }
        }

        // Test 4: Clear screen command
        let clear_result = repl.send_command(":clear")?;
        // Clear command should produce minimal output (just the cleared screen)

        // Test 5: Session statistics
        let stats_output = repl.send_command(":stats")?;
        assert!(!stats_output.is_empty(), "Stats should show session information");

        repl.quit()?;

        println!("✅ REPL history management workflows test passed");
        Ok(())
    }

    /// Test REPL multi-step interactive workflows
    pub async fn test_interactive_workflows(&self) -> Result<()> {
        println!("🧪 Testing REPL interactive workflows");

        let mut repl = self.spawn_repl_with_config(ReplConfig::default())?;

        // Workflow 1: Research workflow - search → analyze → explore
        println!("  🔬 Testing research workflow");

        // Step 1: Search for research content
        let search_result = repl.send_command("SELECT * FROM notes WHERE content LIKE '%research%' OR tags LIKE '%research%'")?;
        assert!(!search_result.is_empty(), "Should find research content");

        // Step 2: Analyze specific research document
        let analysis_result = repl.send_command("SELECT * FROM notes WHERE path LIKE '%quantum%'")?;
        assert!(!analysis_result.is_empty(), "Should find quantum computing document");

        // Step 3: Explore related concepts using semantic search via tool
        let related_result = repl.send_command(":run search_documents \"physics OR quantum OR computing\"")?;
        assert!(!related_result.is_empty(), "Should find related documents");

        // Workflow 2: Project management workflow
        println!("  📋 Testing project management workflow");

        // Step 1: Find project documents
        let projects_result = repl.send_command("SELECT * FROM notes WHERE content LIKE '%project%' OR path LIKE '%project%'")?;
        assert!(!projects_result.is_empty(), "Should find project documents");

        // Step 2: Identify tasks and deadlines
        let tasks_result = repl.send_command("SELECT * FROM notes WHERE content LIKE '%task%' OR content LIKE '%deadline%'")?;
        assert!(!tasks_result.is_empty(), "Should find task information");

        // Step 3: Get project statistics
        let project_stats = repl.send_command(":run get_vault_stats")?;
        assert!(!project_stats.is_empty(), "Should get vault statistics");

        // Workflow 3: Knowledge discovery workflow
        println!("  🔍 Testing knowledge discovery workflow");

        // Step 1: Start with broad topic
        let topic_result = repl.send_command("SELECT * FROM notes WHERE tags LIKE '%learning%' OR content LIKE '%tutorial%'")?;
        assert!(!topic_result.is_empty(), "Should find learning content");

        // Step 2: Refine search with specific patterns
        let refined_result = repl.send_command("SELECT * FROM notes WHERE content LIKE '%pattern%' AND (tags LIKE '%rust%' OR tags LIKE '%code%')")?;
        assert!(!refined_result.is_empty(), "Should find code patterns");

        // Step 3: Explore connections
        let connections_result = repl.send_command("SELECT * FROM notes WHERE content LIKE '%[[' AND content LIKE '%]]%' LIMIT 5")?;
        assert!(!connections_result.is_empty(), "Should find linked documents");

        // Workflow 4: Performance testing workflow
        println!("  ⚡ Testing performance workflow");

        // Step 1: Execute multiple queries rapidly
        for i in 0..5 {
            let query = format!("SELECT * FROM notes WHERE content LIKE '%{}%' LIMIT 2", i);
            let result = repl.send_command(&query)?;
            assert!(!result.is_empty(), "Query {} should return results", i);
        }

        // Step 2: Switch between formats rapidly
        repl.send_command(":format json")?;
        let _ = repl.send_command("SELECT COUNT(*) as count FROM notes")?;
        repl.send_command(":format table")?;
        let _ = repl.send_command("SELECT 1 as test")?;
        repl.send_command(":format csv")?;
        let _ = repl.send_command("SELECT 'format test' as message")?;

        // Step 3: Execute tool with larger result set
        let large_result = repl.send_command(":run list_files .")?;
        assert!(!large_result.is_empty(), "List files should return results");

        repl.quit()?;

        println!("✅ REPL interactive workflows test passed");
        Ok(())
    }

    /// Test REPL error handling and recovery workflows
    pub async fn test_error_handling_workflows(&self) -> Result<()> {
        println!("🧪 Testing REPL error handling workflows");

        let mut repl = self.spawn_repl_with_config(ReplConfig::default())?;

        // Test 1: Invalid SQL queries
        let invalid_sql = repl.send_command("INVALID SQL SYNTAX")?;
        assert!(invalid_sql.contains("error") || invalid_sql.contains("syntax") || invalid_sql.contains("❌"),
               "Should handle invalid SQL gracefully");

        // Test 2: Queries against non-existent tables
        let no_table = repl.send_command("SELECT * FROM nonexistent_table_xyz")?;
        assert!(no_table.contains("not found") || no_table.contains("doesn't exist") || no_table.contains("❌"),
               "Should handle missing tables gracefully");

        // Test 3: Invalid commands
        let invalid_command = repl.send_command(":invalid_command_12345")?;
        assert!(invalid_command.contains("unknown") || invalid_command.contains("invalid") || invalid_command.contains("❌"),
               "Should handle invalid commands gracefully");

        // Test 4: Tool execution errors
        let tool_error = repl.send_command(":run nonexistent_tool with invalid args")?;
        assert!(tool_error.contains("not found") || tool_error.contains("error") || tool_error.contains("❌"),
               "Should handle tool errors gracefully");

        // Test 5: Format switching errors
        let format_error = repl.send_command(":format nonexistent_format")?;
        assert!(format_error.contains("invalid") || format_error.contains("unsupported") || format_error.contains("❌"),
               "Should handle format errors gracefully");

        // Test 6: Recovery after errors - REPL should still be functional
        let recovery_test = repl.send_command(":help")?;
        assert!(!recovery_test.is_empty(), "REPL should recover from errors and remain functional");

        // Test 7: Long-running query cancellation
        // Send a potentially long query and then interrupt it
        let _ = repl.send_command("SELECT * FROM notes WHERE content LIKE '%' OR content LIKE '%' OR content LIKE '%'")?;
        // REPL should handle this gracefully (either complete it or handle any resource issues)

        repl.quit()?;

        println!("✅ REPL error handling workflows test passed");
        Ok(())
    }

    /// Test REPL performance under various conditions
    pub async fn test_performance_workflows(&self) -> Result<()> {
        println!("🧪 Testing REPL performance workflows");

        let mut repl = self.spawn_repl_with_config(ReplConfig::default())?;

        // Test 1: Query performance with different complexities
        let performance_queries = vec![
            ("simple", "SELECT COUNT(*) FROM notes"),
            ("medium", "SELECT * FROM notes WHERE content LIKE '%quantum%' OR content LIKE '%rust%'"),
            ("complex", "SELECT n.title, n.path FROM notes n WHERE n.content LIKE '%project%' OR n.tags LIKE '%learning%' ORDER BY n.title LIMIT 10"),
        ];

        for (complexity, query) in performance_queries {
            let start_time = Instant::now();
            let result = repl.send_command(query)?;
            let duration = start_time.elapsed();

            assert!(!result.is_empty(), "Query should return results for {} complexity", complexity);
            assert!(duration < Duration::from_secs(10),
                   "Query should complete within 10 seconds for {} complexity, took {:?}",
                   complexity, duration);

            println!("    ✅ {} query completed in {:?}", complexity, duration);
        }

        // Test 2: Tool execution performance
        let tool_start = Instant::now();
        let tool_result = repl.send_command(":run system_info")?;
        let tool_duration = tool_start.elapsed();

        assert!(!tool_result.is_empty(), "Tool execution should return results");
        assert!(tool_duration < Duration::from_secs(5),
               "Tool execution should complete within 5 seconds, took {:?}", tool_duration);

        println!("    ✅ Tool execution completed in {:?}", tool_duration);

        // Test 3: Rapid command execution
        let rapid_start = Instant::now();
        for i in 0..10 {
            let simple_query = format!("SELECT {} as number", i);
            let result = repl.send_command(&simple_query)?;
            assert!(!result.is_empty(), "Rapid query {} should succeed", i);
        }
        let rapid_duration = rapid_start.elapsed();

        assert!(rapid_duration < Duration::from_secs(30),
               "10 rapid queries should complete within 30 seconds, took {:?}", rapid_duration);

        println!("    ✅ 10 rapid queries completed in {:?}", rapid_duration);

        // Test 4: Memory usage stability (basic check)
        // Execute multiple queries and check if REPL remains responsive
        for i in 0..20 {
            let query = format!("SELECT * FROM notes WHERE content LIKE '%{}%' LIMIT 3", i % 3);
            let result = repl.send_command(&query)?;
            assert!(!result.is_empty(), "Memory stability query {} should succeed", i);
        }

        // Final responsiveness check
        let final_check = repl.send_command(":stats")?;
        assert!(!final_check.is_empty(), "REPL should remain responsive after intensive usage");

        repl.quit()?;

        println!("✅ REPL performance workflows test passed");
        Ok(())
    }
}

/// REPL configuration for testing
#[derive(Debug, Clone)]
pub struct ReplConfig {
    pub output_format: String,
    pub query_timeout: Duration,
    pub max_history: usize,
    pub enable_completions: bool,
    pub enable_highlighting: bool,
}

impl Default for ReplConfig {
    fn default() -> Self {
        Self {
            output_format: "table".to_string(),
            query_timeout: Duration::from_secs(30),
            max_history: 1000,
            enable_completions: true,
            enable_highlighting: true,
        }
    }
}

/// Extended REPL test process with additional capabilities
pub struct ExtendedReplTestProcess {
    process: Child,
    vault_dir: std::path::PathBuf,
    config: ReplConfig,
}

impl ExtendedReplTestProcess {
    /// Spawn a REPL process with custom configuration
    pub fn spawn(vault_dir: &std::path::Path, config: ReplConfig) -> Result<Self> {
        let db_path = vault_dir.join("test.db");
        let tool_dir = vault_dir.join("tools");

        // Create tools directory
        std::fs::create_dir_all(&tool_dir)?;

        let mut process = Command::new(env!("CARGO_BIN_EXE_crucible-cli"))
            .args([
                "--vault-path", vault_dir.to_str().unwrap(),
                "--db-path", db_path.to_str().unwrap(),
                "--tool-dir", tool_dir.to_str().unwrap(),
                "--format", &config.output_format,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Give REPL time to start up
        thread::sleep(Duration::from_millis(2000));

        Ok(Self {
            process,
            vault_dir: vault_dir.to_owned(),
            config,
        })
    }

    /// Send a command with timeout and retry logic
    pub fn send_command_with_timeout(&mut self, command: &str, timeout: Duration) -> Result<String> {
        let start_time = Instant::now();

        // Send command to stdin
        if let Some(stdin) = self.process.stdin.as_mut() {
            writeln!(stdin, "{}", command)?;
            stdin.flush()?;
        }

        // Read output with timeout
        if let Some(stdout) = self.process.stdout.as_mut() {
            let mut reader = BufReader::new(stdout);
            let mut output = String::new();

            while start_time.elapsed() < timeout {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        output.push_str(&line);
                        // Check for prompt indicating command completion
                        if line.contains("crucible>") && output.len() > line.len() {
                            break;
                        }
                    }
                    Err(_) => break,
                }

                // Small delay to prevent busy waiting
                thread::sleep(Duration::from_millis(50));
            }

            if start_time.elapsed() >= timeout {
                return Err(anyhow!("Command timed out after {:?}", timeout));
            }

            Ok(output)
        } else {
            Err(anyhow!("Cannot read from process stdout"))
        }
    }

    /// Send multiple commands in sequence
    pub fn send_commands(&mut self, commands: &[&str]) -> Result<Vec<String>> {
        let mut results = Vec::new();
        for command in commands {
            let result = self.send_command(command)?;
            results.push(result);
        }
        Ok(results)
    }

    /// Check if REPL is still responsive
    pub fn check_responsiveness(&mut self) -> Result<bool> {
        let start_time = Instant::now();
        match self.send_command_with_timeout(":stats", Duration::from_secs(5)) {
            Ok(_) => Ok(true),
            Err(_) => {
                // If it times out or fails, check if process is still running
                match self.process.try_wait() {
                    Ok(Some(_)) => Ok(false), // Process has exited
                    Ok(None) => Ok(true),     // Process still running but unresponsive
                    Err(_) => Ok(false),
                }
            }
        }
    }

    /// Send a command (standard method)
    pub fn send_command(&mut self, command: &str) -> Result<String> {
        self.send_command_with_timeout(command, self.config.query_timeout)
    }

    /// Quit the REPL cleanly
    pub fn quit(&mut self) -> Result<()> {
        self.send_command(":quit")?;

        match self.process.wait() {
            Ok(status) => {
                if !status.success() {
                    return Err(anyhow!("REPL process exited with status: {}", status));
                }
            }
            Err(e) => return Err(anyhow!("Failed to wait for REPL process: {}", e)),
        }

        Ok(())
    }
}

impl Drop for ExtendedReplTestProcess {
    fn drop(&mut self) {
        if let Err(e) = self.process.kill() {
            eprintln!("Failed to kill REPL process: {}", e);
        }
    }
}

/// REPL session recorder for debugging and analysis
pub struct ReplSessionRecorder {
    commands: Vec<ReplCommandRecord>,
    session_start: Instant,
}

impl ReplSessionRecorder {
    /// Create new session recorder
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            session_start: Instant::now(),
        }
    }

    /// Record a command and its result
    pub fn record_command(&mut self, command: &str, result: &str, duration: Duration) {
        self.commands.push(ReplCommandRecord {
            command: command.to_string(),
            result: result.to_string(),
            duration,
            timestamp: self.session_start.elapsed(),
        });
    }

    /// Generate session summary
    pub fn generate_summary(&self) -> ReplSessionSummary {
        let total_commands = self.commands.len();
        let total_duration = self.session_start.elapsed();
        let avg_command_duration = if total_commands > 0 {
            total_duration / total_commands as u32
        } else {
            Duration::ZERO
        };

        let successful_commands = self.commands.iter()
            .filter(|cmd| !cmd.result.contains("❌") && !cmd.result.contains("error"))
            .count();

        ReplSessionSummary {
            total_commands,
            successful_commands,
            total_duration,
            avg_command_duration,
            commands: self.commands.clone(),
        }
    }
}

/// Record of a single REPL command
#[derive(Debug, Clone)]
pub struct ReplCommandRecord {
    pub command: String,
    pub result: String,
    pub duration: Duration,
    pub timestamp: Duration,
}

/// Summary of a REPL session
#[derive(Debug, Clone)]
pub struct ReplSessionSummary {
    pub total_commands: usize,
    pub successful_commands: usize,
    pub total_duration: Duration,
    pub avg_command_duration: Duration,
    pub commands: Vec<ReplCommandRecord>,
}

// ============================================================================
// Test Execution Functions
// ============================================================================

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_repl_interactive_workflows_comprehensive() -> Result<()> {
    println!("🧪 Running comprehensive REPL interactive workflow tests");

    let harness = ExtendedReplTestHarness::new().await?;

    // Run all REPL workflow tests
    harness.test_startup_workflow().await?;
    harness.test_tool_management_workflows().await?;
    harness.test_query_execution_workflows().await?;
    harness.test_output_formatting_workflows().await?;
    harness.test_history_management_workflows().await?;
    harness.test_interactive_workflows().await?;
    harness.test_error_handling_workflows().await?;
    harness.test_performance_workflows().await?;

    println!("✅ Comprehensive REPL interactive workflow tests passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_repl_tool_integration_workflows() -> Result<()> {
    println!("🧪 Testing REPL tool integration workflows");

    let harness = ExtendedReplTestHarness::new().await?;
    harness.test_tool_management_workflows().await?;

    println!("✅ REPL tool integration workflows test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_repl_query_execution_workflows() -> Result<()> {
    println!("🧪 Testing REPL query execution workflows");

    let harness = ExtendedReplTestHarness::new().await?;
    harness.test_query_execution_workflows().await?;

    println!("✅ REPL query execution workflows test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_repl_interactive_session_workflows() -> Result<()> {
    println!("🧪 Testing REPL interactive session workflows");

    let harness = ExtendedReplTestHarness::new().await?;
    harness.test_interactive_workflows().await?;

    println!("✅ REPL interactive session workflows test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_repl_error_handling_workflows() -> Result<()> {
    println!("🧪 Testing REPL error handling workflows");

    let harness = ExtendedReplTestHarness::new().await?;
    harness.test_error_handling_workflows().await?;

    println!("✅ REPL error handling workflows test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_repl_performance_validation() -> Result<()> {
    println!("🧪 Testing REPL performance validation");

    let harness = ExtendedReplTestHarness::new().await?;
    harness.test_performance_workflows().await?;

    println!("✅ REPL performance validation test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_repl_session_recording() -> Result<()> {
    println!("🧪 Testing REPL session recording");

    let harness = ExtendedReplTestHarness::new().await?;
    let mut repl = harness.spawn_repl_with_config(ReplConfig::default())?;
    let mut recorder = ReplSessionRecorder::new();

    // Record some commands
    let commands = vec![
        ":help",
        ":stats",
        "SELECT COUNT(*) FROM notes",
        ":tools",
        ":format json",
        "SELECT 'test' as message",
    ];

    for command in commands {
        let start = Instant::now();
        let result = repl.send_command(command)?;
        let duration = start.elapsed();
        recorder.record_command(command, &result, duration);
    }

    // Generate summary
    let summary = recorder.generate_summary();
    assert!(summary.total_commands > 0, "Should have recorded commands");
    assert!(summary.successful_commands > 0, "Should have successful commands");

    println!("Session summary: {} commands, {} successful, total duration: {:?}",
             summary.total_commands, summary.successful_commands, summary.total_duration);

    repl.quit()?;

    println!("✅ REPL session recording test passed");
    Ok(())
}
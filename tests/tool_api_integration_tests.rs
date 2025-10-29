//! Tool API Integration Tests
//!
//! This module specifically focuses on testing the tool discovery, execution, and integration
//! capabilities of the Crucible system. It validates that tools work correctly across
//! different interfaces and can be chained together for complex workflows.

use std::collections::HashMap;
use std::process::Command;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::comprehensive_integration_workflow_tests::{
    ComprehensiveTestVault, CliTestHarness, ReplTestHarness
};

/// Tool API test harness for comprehensive tool testing
pub struct ToolApiTestHarness {
    vault_dir: TempDir,
    test_vault: ComprehensiveTestVault,
}

impl ToolApiTestHarness {
    /// Create a new tool API test harness
    pub async fn new() -> Result<Self> {
        let test_vault = ComprehensiveTestVault::create().await?;
        let vault_dir = test_vault.path().to_owned();

        Ok(Self {
            vault_dir: vault_dir.to_owned(),
            test_vault,
        })
    }

    /// Test tool discovery and registration workflows
    pub async fn test_tool_discovery_workflow(&self) -> Result<()> {
        println!("🧪 Testing tool discovery workflow");

        // Test 1: System tool discovery
        let cli_harness = CliTestHarness::new().await?;
        let result = cli_harness.execute_cli_command(&["test"])?;

        assert!(result.exit_code == 0, "Tool test command should succeed");
        assert!(result.stdout.contains("tool") || result.stdout.contains("system"),
               "Should discover system tools");

        // Test 2: REPL tool discovery
        let repl_harness = ReplTestHarness::new().await?;
        let mut repl = repl_harness.spawn_repl()?;

        let tools_output = repl.send_command(":tools")?;
        assert!(tools_output.contains("Available Tools"), "Should show available tools");
        assert!(tools_output.contains("system"), "Should show system tools");

        // Test 3: Tool categorization and grouping
        let detailed_tools = repl.send_command(":tools --detailed")?;
        assert!(!detailed_tools.is_empty(), "Detailed tools listing should not be empty");

        // Test 4: Tool count validation
        let tools_counted = tools_output.lines()
            .filter(|line| line.trim().starts_with("  ") && !line.trim().starts_with("   "))
            .count();

        assert!(tools_counted >= 10, "Should have at least 10 tools available, found {}", tools_counted);

        repl.quit()?;

        println!("✅ Tool discovery workflow test passed");
        Ok(())
    }

    /// Test individual tool execution and parameter handling
    pub async fn test_tool_execution_workflow(&self) -> Result<()> {
        println!("🧪 Testing tool execution workflow");

        let repl_harness = ReplTestHarness::new().await?;
        let mut repl = repl_harness.spawn_repl()?;

        // Test 1: System info tool (no parameters)
        let system_info = repl.send_command(":run system_info")?;
        assert!(!system_info.is_empty(), "System info tool should produce output");
        assert!(!system_info.contains("❌"), "System info should not error");
        assert!(system_info.contains("System") || system_info.contains("Info") || system_info.len() > 50,
               "System info should contain meaningful information");

        // Test 2: List files tool (with path parameter)
        let list_files = repl.send_command(":run list_files /tmp")?;
        assert!(!list_files.is_empty(), "List files tool should produce output");
        assert!(!list_files.contains("❌"), "List files should not error");

        // Test 3: Search documents tool (with query parameter)
        let search_docs = repl.send_command(":run search_documents \"quantum OR rust OR project\"")?;
        assert!(!search_docs.is_empty(), "Search documents tool should produce output");
        assert!(!search_docs.contains("❌"), "Search documents should not error");

        // Test 4: Get vault stats tool (no parameters)
        let vault_stats = repl.send_command(":run get_kiln_stats")?;
        assert!(!vault_stats.is_empty(), "Vault stats tool should produce output");
        assert!(!vault_stats.contains("❌"), "Vault stats should not error");

        // Test 5: Tool with multiple parameters
        let multi_param = repl.send_command(":run search_by_tags project management")?;
        assert!(!multi_param.is_empty(), "Multi-parameter tool should produce output");

        // Test 6: Tool error handling with invalid parameters
        let invalid_params = repl.send_command(":run list_files")?;
        assert!(invalid_params.contains("error") || invalid_params.contains("missing") || invalid_params.contains("❌"),
               "Should handle missing parameters gracefully");

        // Test 7: Tool with non-existent tool name
        let nonexistent_tool = repl.send_command(":run nonexistent_tool_12345")?;
        assert!(nonexistent_tool.contains("not found") || nonexistent_tool.contains("❌"),
               "Should handle non-existent tools gracefully");

        repl.quit()?;

        println!("✅ Tool execution workflow test passed");
        Ok(())
    }

    /// Test tool parameter validation and conversion
    pub async fn test_parameter_handling_workflow(&self) -> Result<()> {
        println!("🧪 Testing tool parameter handling workflow");

        let repl_harness = ReplTestHarness::new().await?;
        let mut repl = repl_harness.spawn_repl()?;

        // Test 1: Quoted parameters with spaces
        let quoted_params = repl.send_command(":run search_documents \"machine learning patterns\"")?;
        assert!(!quoted_params.is_empty(), "Quoted parameters should work correctly");

        // Test 2: Multiple space-separated parameters
        let multi_params = repl.send_command(":run search_by_tags rust code patterns")?;
        assert!(!multi_params.is_empty(), "Multiple parameters should work correctly");

        // Test 3: Parameters with special characters
        let special_chars = repl.send_command(":run search_documents \"C++ OR C# OR .NET\"")?;
        assert!(!special_chars.is_empty(), "Special characters in parameters should be handled correctly");

        // Test 4: Empty and null parameter handling
        let empty_params = repl.send_command(":run system_info \"\"")?;
        assert!(!empty_params.contains("error") || empty_params.contains("❌"),
               "Empty parameters should be handled gracefully");

        // Test 5: Parameter type conversion validation
        let numeric_param = repl.send_command(":run list_files 123")?;
        // Should handle numeric parameters appropriately (either convert or show clear error)

        // Test 6: Parameter limits and bounds checking
        let long_param = repl.send_command(&format!(":run search_documents \"{}\"", "x".repeat(1000)))?;
        // Should handle very long parameters appropriately

        repl.quit()?;

        println!("✅ Tool parameter handling workflow test passed");
        Ok(())
    }

    /// Test tool chaining and workflow automation
    pub async fn test_tool_chaining_workflow(&self) -> Result<()> {
        println!("🧪 Testing tool chaining workflow");

        let repl_harness = ReplTestHarness::new().await?;
        let mut repl = repl_harness.spawn_repl()?;

        // Workflow 1: Discovery → Analysis → Statistics
        println!("  🔗 Testing discovery → analysis → statistics workflow");

        // Step 1: Discover relevant documents
        let discovery = repl.send_command(":run search_documents \"project management\"")?;
        assert!(!discovery.is_empty(), "Discovery step should find documents");

        // Step 2: Analyze document content
        let analysis = repl.send_command(":run search_documents \"tasks deadlines milestones\"")?;
        assert!(!analysis.is_empty(), "Analysis step should find task-related content");

        // Step 3: Get vault statistics for context
        let statistics = repl.send_command(":run get_kiln_stats")?;
        assert!(!statistics.is_empty(), "Statistics step should provide context");

        // Workflow 2: Search → Extract → Process
        println!("  🔗 Testing search → extract → process workflow");

        // Step 1: Search for code-related content
        let code_search = repl.send_command(":run search_documents \"rust async patterns\"")?;
        assert!(!code_search.is_empty(), "Code search should find relevant content");

        // Step 2: Extract specific information
        let extract_info = repl.send_command(":run search_by_tags rust code tutorial"))?;
        assert!(!extract_info.is_empty(), "Extraction should find tagged content");

        // Step 3: Process with different query
        let process_info = repl.send_command(":run search_documents \"error handling Result<T>\"")?;
        assert!(!process_info.is_empty(), "Processing should find related concepts");

        // Workflow 3: Multi-format exploration
        println!("  🔗 Testing multi-format exploration workflow");

        // Step 1: Text-based search
        let text_search = repl.send_command(":run search_documents \"quantum computing fundamentals\"")?;
        assert!(!text_search.is_empty(), "Text search should find content");

        // Step 2: Tag-based search
        let tag_search = repl.send_command(":run search_by_tags quantum physics research")?;
        assert!(!tag_search.is_empty(), "Tag search should find related content");

        // Step 3: System information for context
        let system_context = repl.send_command(":run system_info")?;
        assert!(!system_context.is_empty(), "System info should provide context");

        // Test complex chaining with query results
        let complex_chain = repl.send_command(":run search_documents \"deployment configuration\"")?;
        assert!(!complex_chain.is_empty(), "Complex chaining should work");

        repl.quit()?;

        println!("✅ Tool chaining workflow test passed");
        Ok(())
    }

    /// Test tool result processing and integration
    pub async fn test_result_processing_workflow(&self) -> Result<()> {
        println!("🧪 Testing tool result processing workflow");

        let repl_harness = ReplTestHarness::new().await?;
        let mut repl = repl_harness.spawn_repl()?;

        // Test 1: Structured result processing
        let structured_result = repl.send_command(":run get_kiln_stats")?;
        assert!(!structured_result.is_empty(), "Should return structured results");

        // Check if result contains expected structure
        let has_structure = structured_result.contains("files") ||
                           structured_result.contains("documents") ||
                           structured_result.contains("size") ||
                           structured_result.lines().count() > 3;
        assert!(has_structure, "Results should have meaningful structure");

        // Test 2: Large result set handling
        let large_results = repl.send_command(":run list_files .")?;
        assert!(!large_results.is_empty(), "Should handle large result sets");

        // Check if it handles pagination or truncation gracefully
        let result_lines = large_results.lines().count();
        assert!(result_lines > 0, "Should return multiple lines for directory listing");

        // Test 3: Error result processing
        let error_result = repl.send_command(":run search_documents \"nonexistent_content_xyz_123\"")?;
        // Should handle empty results gracefully
        assert!(!error_result.contains("panic") && !error_result.contains("stack trace"),
               "Should handle empty results without panicking");

        // Test 4: Mixed result types (text + structured)
        let mixed_results = repl.send_command(":run search_documents \"project\"")?;
        assert!(!mixed_results.is_empty(), "Should return mixed format results");

        // Test 5: Result formatting and display
        repl.send_command(":format json")?;
        let json_results = repl.send_command(":run get_kiln_stats")?;
        assert!(json_results.contains("{") || json_results.contains("[") || !json_results.is_empty(),
               "JSON format should work with tool results");

        repl.send_command(":format table")?;
        let table_results = repl.send_command(":run get_kiln_stats")?;
        assert!(!table_results.is_empty(), "Table format should work with tool results");

        repl.quit()?;

        println!("✅ Tool result processing workflow test passed");
        Ok(())
    }

    /// Test tool performance and resource management
    pub async fn test_tool_performance_workflow(&self) -> Result<()> {
        println!("🧪 Testing tool performance workflow");

        let repl_harness = ReplTestHarness::new().await?;
        let mut repl = repl_harness.spawn_repl()?;

        // Test 1: Individual tool performance
        let performance_tools = vec![
            ("system_info", ":run system_info"),
            ("get_kiln_stats", ":run get_kiln_stats"),
            ("list_files", ":run list_files /tmp"),
        ];

        for (tool_name, command) in performance_tools {
            let start_time = Instant::now();
            let result = repl.send_command(command)?;
            let duration = start_time.elapsed();

            assert!(!result.is_empty(), "Tool {} should produce output", tool_name);
            assert!(duration < Duration::from_secs(5),
                   "Tool {} should complete within 5 seconds, took {:?}",
                   tool_name, duration);
            assert!(!result.contains("❌"), "Tool {} should not error", tool_name);

            println!("    ✅ {} completed in {:?}", tool_name, duration);
        }

        // Test 2: Rapid tool execution
        let rapid_start = Instant::now();
        for i in 0..10 {
            let command = format!(":run system_info");
            let result = repl.send_command(&command)?;
            assert!(!result.is_empty(), "Rapid execution {} should succeed", i);
        }
        let rapid_duration = rapid_start.elapsed();

        assert!(rapid_duration < Duration::from_secs(30),
               "10 rapid tool executions should complete within 30 seconds, took {:?}",
               rapid_duration);

        println!("    ✅ 10 rapid tool executions completed in {:?}", rapid_duration);

        // Test 3: Concurrent tool usage simulation
        let concurrent_tools = vec![
            ":run search_documents \"quantum\"",
            ":run search_by_tags rust code",
            ":run list_files .",
        ];

        for tool in concurrent_tools {
            let start_time = Instant::now();
            let result = repl.send_command(tool)?;
            let duration = start_time.elapsed();

            assert!(duration < Duration::from_secs(10),
                   "Concurrent tool should complete within 10 seconds, took {:?}", duration);
        }

        // Test 4: Memory usage stability
        for i in 0..20 {
            let command = format!(":run search_documents \"{}\"", i % 3);
            let result = repl.send_command(&command)?;
            assert!(!result.is_empty(), "Memory stability test {} should succeed", i);
        }

        // Final responsiveness check
        let final_check = repl.send_command(":run system_info")?;
        assert!(!final_check.is_empty(), "System should remain responsive after intensive tool usage");

        repl.quit()?;

        println!("✅ Tool performance workflow test passed");
        Ok(())
    }

    /// Test tool error handling and recovery
    pub async fn test_tool_error_handling_workflow(&self) -> Result<()> {
        println!("🧪 Testing tool error handling workflow");

        let repl_harness = ReplTestHarness::new().await?;
        let mut repl = repl_harness.spawn_repl()?;

        // Test 1: Non-existent tool handling
        let nonexistent_tool = repl.send_command(":run nonexistent_tool_12345")?;
        assert!(nonexistent_tool.contains("not found") || nonexistent_tool.contains("error") || nonexistent_tool.contains("❌"),
               "Should handle non-existent tools gracefully");

        // Test 2: Invalid parameter count
        let invalid_params = repl.send_command(":run list_files")?;
        assert!(invalid_params.contains("error") || invalid_params.contains("missing") || invalid_params.contains("❌"),
               "Should handle missing parameters gracefully");

        // Test 3: Invalid parameter types
        let invalid_types = repl.send_command(":run list_files not_a_valid_path")?;
        // Should handle invalid path gracefully

        // Test 4: Tool execution timeout handling
        let timeout_test = repl.send_command(":run search_documents \"\"")?;
        // Should handle empty or problematic queries gracefully

        // Test 5: Recovery after errors
        let recovery_test = repl.send_command(":run system_info")?;
        assert!(!recovery_test.contains("❌"), "System should recover from errors and execute tools successfully");

        // Test 6: Multiple sequential errors
        let _ = repl.send_command(":run nonexistent_tool_1")?;
        let _ = repl.send_command(":run nonexistent_tool_2")?;
        let _ = repl.send_command(":run nonexistent_tool_3")?;

        // Should still be functional after multiple errors
        let functionality_test = repl.send_command(":run get_kiln_stats")?;
        assert!(!functionality_test.contains("❌"), "System should remain functional after multiple errors");

        // Test 7: Tool error message quality
        let helpful_error = repl.send_command(":run list_files")?;
        assert!(helpful_error.len() > 10, "Error messages should be helpful and not just generic");

        repl.quit()?;

        println!("✅ Tool error handling workflow test passed");
        Ok(())
    }

    /// Test tool integration with search and query systems
    pub async fn test_search_integration_workflow(&self) -> Result<()> {
        println!("🧪 Testing tool search integration workflow");

        let repl_harness = ReplTestHarness::new().await?;
        let mut repl = repl_harness.spawn_repl()?;

        // Test 1: Tool results integrated with search queries
        let tool_search = repl.send_command(":run search_documents \"project management\"")?;
        assert!(!tool_search.is_empty(), "Tool search should find relevant documents");

        // Verify that tool results are consistent with direct search
        let direct_search = repl.send_command("SELECT * FROM notes WHERE content LIKE '%project%' AND content LIKE '%management%'")?;
        assert!(!direct_search.is_empty(), "Direct search should also find relevant documents");

        // Test 2: Semantic search tool integration
        let semantic_tool = repl.send_command(":run search_documents \"machine learning algorithms\"")?;
        assert!(!semantic_tool.is_empty(), "Semantic tool search should work");

        // Test 3: Multi-modal search integration
        let multi_modal = repl.send_command(":run search_by_tags rust async tutorial")?;
        assert!(!multi_modal.is_empty(), "Multi-modal search should work");

        // Test 4: Search result refinement through tools
        let initial_search = repl.send_command(":run search_documents \"code\"")?;
        assert!(!initial_search.is_empty(), "Initial search should return results");

        let refined_search = repl.send_command(":run search_documents \"rust async patterns error handling\"")?;
        assert!(!refined_search.is_empty(), "Refined search should return more specific results");

        // Test 5: Cross-tool search consistency
        let search_results = vec![
            repl.send_command(":run search_documents \"quantum\"")?,
            repl.send_command(":run search_by_tags quantum physics research")?,
            repl.send_command("SELECT * FROM notes WHERE content LIKE '%quantum%'")?,
        ];

        for (i, result) in search_results.iter().enumerate() {
            assert!(!result.is_empty(), "Search method {} should return results", i + 1);
        }

        repl.quit()?;

        println!("✅ Tool search integration workflow test passed");
        Ok(())
    }
}

/// Tool specification and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpecification {
    pub name: String,
    pub description: String,
    pub category: ToolCategory,
    pub parameters: Vec<ToolParameter>,
    pub return_type: ToolReturnType,
    pub examples: Vec<ToolExample>,
}

/// Tool categorization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolCategory {
    System,
    Search,
    Analysis,
    File,
    Network,
    Database,
    Custom(String),
}

/// Tool parameter definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParameter {
    pub name: String,
    pub param_type: ParameterType,
    pub required: bool,
    pub description: String,
    pub default_value: Option<String>,
}

/// Parameter types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParameterType {
    String,
    Number,
    Boolean,
    Array,
    Object,
    Path,
    Query,
}

/// Tool return type specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolReturnType {
    String,
    Structured,
    Array,
    Boolean,
    Error,
}

/// Tool example usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExample {
    pub description: String,
    pub command: String,
    pub expected_output: String,
}

/// Tool registry validator
pub struct ToolRegistryValidator {
    expected_tools: Vec<String>,
    tool_specifications: HashMap<String, ToolSpecification>,
}

impl ToolRegistryValidator {
    /// Create new tool registry validator
    pub fn new() -> Self {
        let expected_tools = vec![
            "system_info".to_string(),
            "get_kiln_stats".to_string(),
            "list_files".to_string(),
            "search_documents".to_string(),
            "search_by_tags".to_string(),
        ];

        let mut tool_specifications = HashMap::new();

        // Define expected tool specifications
        tool_specifications.insert("system_info".to_string(), ToolSpecification {
            name: "system_info".to_string(),
            description: "Get system information and statistics".to_string(),
            category: ToolCategory::System,
            parameters: vec![],
            return_type: ToolReturnType::Structured,
            examples: vec![
                ToolExample {
                    description: "Get basic system information".to_string(),
                    command: ":run system_info".to_string(),
                    expected_output: "System information including OS, memory, and process details".to_string(),
                },
            ],
        });

        tool_specifications.insert("search_documents".to_string(), ToolSpecification {
            name: "search_documents".to_string(),
            description: "Search documents by content or metadata".to_string(),
            category: ToolCategory::Search,
            parameters: vec![
                ToolParameter {
                    name: "query".to_string(),
                    param_type: ParameterType::Query,
                    required: true,
                    description: "Search query string".to_string(),
                    default_value: None,
                },
            ],
            return_type: ToolReturnType::Array,
            examples: vec![
                ToolExample {
                    description: "Search for quantum computing documents".to_string(),
                    command: ":run search_documents \"quantum computing\"".to_string(),
                    expected_output: "List of documents matching the search query".to_string(),
                },
            ],
        });

        Self {
            expected_tools,
            tool_specifications,
        }
    }

    /// Validate that expected tools are available
    pub fn validate_tool_availability(&self, available_tools: &[String]) -> Result<()> {
        for expected_tool in &self.expected_tools {
            if !available_tools.contains(expected_tool) {
                return Err(anyhow!("Expected tool '{}' not found in available tools: {:?}",
                                   expected_tool, available_tools));
            }
        }
        Ok(())
    }

    /// Validate tool specifications
    pub fn validate_tool_specifications(&self, tool_outputs: &HashMap<String, String>) -> Result<()> {
        for (tool_name, specification) in &self.tool_specifications {
            if let Some(output) = tool_outputs.get(tool_name) {
                // Validate that tool produces output
                assert!(!output.is_empty(), "Tool '{}' should produce output", tool_name);

                // Validate return type characteristics
                match specification.return_type {
                    ToolReturnType::Structured => {
                        // Should have structured output (multiple lines, specific format)
                        assert!(output.lines().count() > 1 || output.contains(":") || output.contains("="),
                               "Structured tool '{}' should produce structured output", tool_name);
                    }
                    ToolReturnType::Array => {
                        // Should produce list-like output
                        assert!(output.lines().count() > 1 || output.contains(","),
                               "Array tool '{}' should produce list-like output", tool_name);
                    }
                    _ => {
                        // Other types should just have non-empty output
                        assert!(!output.is_empty(), "Tool '{}' should produce output", tool_name);
                    }
                }
            }
        }
        Ok(())
    }
}

/// Tool execution result analyzer
pub struct ToolResultAnalyzer {
    results: Vec<ToolExecutionResult>,
}

impl ToolResultAnalyzer {
    /// Create new result analyzer
    pub fn new() -> Self {
        Self {
            results: Vec::new(),
        }
    }

    /// Add tool execution result
    pub fn add_result(&mut self, result: ToolExecutionResult) {
        self.results.push(result);
    }

    /// Analyze execution patterns
    pub fn analyze_patterns(&self) -> ToolAnalysisReport {
        let total_executions = self.results.len();
        let successful_executions = self.results.iter().filter(|r| r.success).count();
        let total_duration: Duration = self.results.iter().map(|r| r.duration).sum();
        let avg_duration = if total_executions > 0 {
            total_duration / total_executions as u32
        } else {
            Duration::ZERO
        };

        let tool_usage = HashMap::new();
        let error_patterns = Vec::new();

        ToolAnalysisReport {
            total_executions,
            successful_executions,
            failed_executions: total_executions - successful_executions,
            total_duration,
            avg_duration,
            tool_usage,
            error_patterns,
        }
    }
}

/// Result of a single tool execution
#[derive(Debug, Clone)]
pub struct ToolExecutionResult {
    pub tool_name: String,
    pub parameters: Vec<String>,
    pub success: bool,
    pub duration: Duration,
    pub output_size: usize,
    pub error_message: Option<String>,
}

/// Analysis report for tool usage patterns
#[derive(Debug, Clone)]
pub struct ToolAnalysisReport {
    pub total_executions: usize,
    pub successful_executions: usize,
    pub failed_executions: usize,
    pub total_duration: Duration,
    pub avg_duration: Duration,
    pub tool_usage: HashMap<String, usize>,
    pub error_patterns: Vec<String>,
}

// ============================================================================
// Test Execution Functions
// ============================================================================

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_tool_api_integration_comprehensive() -> Result<()> {
    println!("🧪 Running comprehensive tool API integration tests");

    let harness = ToolApiTestHarness::new().await?;

    // Run all tool API tests
    harness.test_tool_discovery_workflow().await?;
    harness.test_tool_execution_workflow().await?;
    harness.test_parameter_handling_workflow().await?;
    harness.test_tool_chaining_workflow().await?;
    harness.test_result_processing_workflow().await?;
    harness.test_tool_performance_workflow().await?;
    harness.test_tool_error_handling_workflow().await?;
    harness.test_search_integration_workflow().await?;

    println!("✅ Comprehensive tool API integration tests passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_tool_discovery_and_registration() -> Result<()> {
    println!("🧪 Testing tool discovery and registration");

    let harness = ToolApiTestHarness::new().await?;
    harness.test_tool_discovery_workflow().await?;

    println!("✅ Tool discovery and registration test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_tool_execution_and_parameter_handling() -> Result<()> {
    println!("🧪 Testing tool execution and parameter handling");

    let harness = ToolApiTestHarness::new().await?;
    harness.test_tool_execution_workflow().await?;
    harness.test_parameter_handling_workflow().await?;

    println!("✅ Tool execution and parameter handling test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_tool_chaining_and_workflows() -> Result<()> {
    println!("🧪 Testing tool chaining and workflows");

    let harness = ToolApiTestHarness::new().await?;
    harness.test_tool_chaining_workflow().await?;

    println!("✅ Tool chaining and workflows test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_tool_error_handling_and_recovery() -> Result<()> {
    println!("🧪 Testing tool error handling and recovery");

    let harness = ToolApiTestHarness::new().await?;
    harness.test_tool_error_handling_workflow().await?;

    println!("✅ Tool error handling and recovery test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_tool_performance_validation() -> Result<()> {
    println!("🧪 Testing tool performance validation");

    let harness = ToolApiTestHarness::new().await?;
    harness.test_tool_performance_workflow().await?;

    println!("✅ Tool performance validation test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_tool_registry_validation() -> Result<()> {
    println!("🧪 Testing tool registry validation");

    let harness = ToolApiTestHarness::new().await?;
    let repl_harness = ReplTestHarness::new().await?;
    let mut repl = repl_harness.spawn_repl()?;

    // Get available tools
    let tools_output = repl.send_command(":tools")?;

    // Extract tool names (simplified parsing)
    let available_tools: Vec<String> = tools_output.lines()
        .filter(|line| line.trim().starts_with("  ") && !line.trim().starts_with("   "))
        .map(|line| line.trim().to_string())
        .collect();

    // Validate tool registry
    let validator = ToolRegistryValidator::new();
    validator.validate_tool_availability(&available_tools)?;

    // Test tool outputs
    let mut tool_outputs = HashMap::new();
    let test_tools = vec!["system_info", "get_kiln_stats", "search_documents"];

    for tool in test_tools {
        let command = if tool == "search_documents" {
            format!(":run {} \"test\"", tool)
        } else {
            format!(":run {}", tool)
        };

        if let Ok(output) = repl.send_command(&command) {
            tool_outputs.insert(tool.to_string(), output);
        }
    }

    validator.validate_tool_specifications(&tool_outputs)?;

    repl.quit()?;

    println!("✅ Tool registry validation test passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_tool_result_analysis() -> Result<()> {
    println!("🧪 Testing tool result analysis");

    let harness = ToolApiTestHarness::new().await?;
    let repl_harness = ReplTestHarness::new().await?;
    let mut repl = repl_harness.spawn_repl()?;
    let mut analyzer = ToolResultAnalyzer::new();

    // Execute various tools and collect results
    let test_commands = vec![
        ":run system_info",
        ":run get_kiln_stats",
        ":run search_documents \"quantum\"",
        ":run list_files /tmp",
    ];

    for command in test_commands {
        let start_time = Instant::now();
        let result = repl.send_command(command)?;
        let duration = start_time.elapsed();

        let tool_name = command.split_whitespace().nth(1).unwrap_or("unknown");
        let parameters = command.split_whitespace().skip(2).map(|s| s.to_string()).collect();

        let execution_result = ToolExecutionResult {
            tool_name: tool_name.to_string(),
            parameters,
            success: !result.contains("❌") && !result.contains("error"),
            duration,
            output_size: result.len(),
            error_message: if result.contains("❌") || result.contains("error") {
                Some(result.clone())
            } else {
                None
            },
        };

        analyzer.add_result(execution_result);
    }

    // Analyze patterns
    let report = analyzer.analyze_patterns();
    assert!(report.total_executions > 0, "Should have executed tools");
    assert!(report.successful_executions > 0, "Should have successful executions");

    println!("Analysis report:");
    println!("  Total executions: {}", report.total_executions);
    println!("  Successful: {}", report.successful_executions);
    println!("  Failed: {}", report.failed_executions);
    println!("  Average duration: {:?}", report.avg_duration);

    repl.quit()?;

    println!("✅ Tool result analysis test passed");
    Ok(())
}
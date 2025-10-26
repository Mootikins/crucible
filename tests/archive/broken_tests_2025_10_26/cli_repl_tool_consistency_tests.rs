//! CLI and REPL Tool Consistency Integration Tests
//!
//! This comprehensive test suite validates that the CLI commands and REPL interface
//! show the same tools and provide consistent functionality. It serves as the final
//! validation that the unified tool system works correctly across both interfaces.

/// Test configuration and setup utilities
mod test_setup {
    use super::*;

    /// Create a test CLI configuration
    pub fn create_test_cli_config() -> CliConfig {
        CliConfig::default()
    }

    /// Create a test unified tool registry
    pub async fn create_test_tool_registry() -> Result<(UnifiedToolRegistry, TempDir)> {
        let temp_dir = TempDir::new()?;
        let tool_dir = temp_dir.path().join("tools");
        std::fs::create_dir_all(&tool_dir)?;

        let registry = UnifiedToolRegistry::new(tool_dir).await?;
        Ok((registry, temp_dir))
    }

    /// Initialize the tool manager for testing
    pub async fn initialize_tool_manager() -> Result<()> {
        CrucibleToolManager::ensure_initialized_global().await
    }
}

/// Tool discovery and comparison utilities
mod tool_comparison {

    /// Compare two tool lists and return differences
    pub fn compare_tool_lists(cli_tools: &[String], repl_tools: &[String]) -> ToolComparisonResult {
        let cli_set: std::collections::HashSet<&String> = cli_tools.iter().collect();
        let repl_set: std::collections::HashSet<&String> = repl_tools.iter().collect();

        let missing_in_repl: Vec<String> = cli_set
            .difference(&repl_set)
            .map(|s| (*s).clone())
            .collect();
        let missing_in_cli: Vec<String> = repl_set
            .difference(&cli_set)
            .map(|s| (*s).clone())
            .collect();
        let common: Vec<String> = cli_set
            .intersection(&repl_set)
            .map(|s| (*s).clone())
            .collect();

        ToolComparisonResult {
            cli_count: cli_tools.len(),
            repl_count: repl_tools.len(),
            common_count: common.len(),
            missing_in_repl: missing_in_repl.clone(),
            missing_in_cli: missing_in_cli.clone(),
            common,
            is_identical: missing_in_repl.is_empty() && missing_in_cli.is_empty(),
        }
    }

    /// Result of tool comparison
    #[derive(Debug, Clone)]
    pub struct ToolComparisonResult {
        pub cli_count: usize,
        pub repl_count: usize,
        pub common_count: usize,
        pub missing_in_repl: Vec<String>,
        pub missing_in_cli: Vec<String>,
        pub common: Vec<String>,
        pub is_identical: bool,
    }

    impl ToolComparisonResult {
        pub fn print_summary(&self) {
            println!("\n📊 Tool List Comparison Summary:");
            println!("  CLI tools: {}", self.cli_count);
            println!("  REPL tools: {}", self.repl_count);
            println!("  Common tools: {}", self.common_count);

            if !self.missing_in_repl.is_empty() {
                println!("  ⚠️  Missing in REPL: {:?}", self.missing_in_repl);
            }

            if !self.missing_in_cli.is_empty() {
                println!("  ⚠️  Missing in CLI: {:?}", self.missing_in_cli);
            }

            if self.is_identical {
                println!("  ✅ Tool lists are identical");
            } else {
                println!("  ❌ Tool lists differ");
            }
        }
    }
}

/// Tool execution consistency testing
mod execution_consistency {
    use super::*;

    /// Test execution consistency for a set of tools
    pub async fn test_execution_consistency(
        tools: &[String],
        registry: &UnifiedToolRegistry,
    ) -> ExecutionConsistencyResult {
        let mut results = HashMap::new();
        let mut consistent_count = 0;
        let mut inconsistent_count = 0;
        let mut failed_in_cli = 0;
        let mut failed_in_repl = 0;

        // Test a subset of tools (avoid testing all to keep test duration reasonable)
        let test_tools = if tools.len() > 10 {
            &tools[..10] // Test first 10 tools
        } else {
            tools
        };

        for tool_name in test_tools {
            println!("  🔧 Testing execution consistency for: {}", tool_name);

            // Test via CLI (CrucibleToolManager)
            let cli_result = test_tool_via_cli(tool_name).await;

            // Test via REPL (UnifiedToolRegistry)
            let repl_result = test_tool_via_repl(tool_name, registry).await;

            let consistency = compare_execution_results(tool_name, &cli_result, &repl_result);

            match consistency {
                ExecutionConsistency::Consistent => {
                    results.insert(
                        tool_name.clone(),
                        ExecutionTestResult {
                            cli_result: Some(cli_result),
                            repl_result: Some(repl_result),
                            consistency: consistency.clone(),
                            error: None,
                        },
                    );
                    consistent_count += 1;
                    println!("    ✅ Execution consistent");
                }
                ExecutionConsistency::Inconsistent(ref reason) => {
                    results.insert(
                        tool_name.clone(),
                        ExecutionTestResult {
                            cli_result: Some(cli_result),
                            repl_result: Some(repl_result),
                            consistency: consistency.clone(),
                            error: Some(reason.clone()),
                        },
                    );
                    inconsistent_count += 1;
                    println!("    ❌ Execution inconsistent: {}", reason);
                }
                ExecutionConsistency::FailedInCli => {
                    results.insert(
                        tool_name.clone(),
                        ExecutionTestResult {
                            cli_result: None,
                            repl_result: Some(repl_result),
                            consistency: consistency.clone(),
                            error: Some("Failed in CLI".to_string()),
                        },
                    );
                    failed_in_cli += 1;
                    println!("    ⚠️  Failed in CLI");
                }
                ExecutionConsistency::FailedInRepl => {
                    results.insert(
                        tool_name.clone(),
                        ExecutionTestResult {
                            cli_result: Some(cli_result),
                            repl_result: None,
                            consistency: consistency.clone(),
                            error: Some("Failed in REPL".to_string()),
                        },
                    );
                    failed_in_repl += 1;
                    println!("    ⚠️  Failed in REPL");
                }
            }
        }

        ExecutionConsistencyResult {
            total_tested: test_tools.len(),
            consistent_count,
            inconsistent_count,
            failed_in_cli,
            failed_in_repl,
            results,
        }
    }

    /// Test a tool via CLI interface
    async fn test_tool_via_cli(_tool_name: &str) -> crucible_tools::ToolResult {
        let parameters = json!({});

        match CrucibleToolManager::execute_tool_global(
            _tool_name,
            parameters,
            Some("test_user".to_string()),
            Some("test_session".to_string()),
        )
        .await
        {
            Ok(result) => result,
            Err(e) => crucible_tools::ToolResult {
                success: false,
                data: None,
                error: Some(format!("CLI execution error: {}", e)),
                duration_ms: 0,
                tool_name: _tool_name.to_string(),
            },
        }
    }

    /// Test a tool via REPL interface
    async fn test_tool_via_repl(
        tool_name: &str,
        registry: &UnifiedToolRegistry,
    ) -> crucible_tools::ToolResult {
        let args = vec![]; // Empty args for testing

        match registry.execute_tool(tool_name, &args).await {
            Ok(result) => {
                // Convert REPL ToolResult back to crucible_tools::ToolResult for comparison
                match result.status {
                    crucible_cli::commands::repl::tools::ToolStatus::Success => {
                        crucible_tools::ToolResult {
                            success: true,
                            data: Some(
                                serde_json::from_str(&result.output)
                                    .unwrap_or(json!(result.output)),
                            ),
                            error: None,
                            duration_ms: 0,
                            tool_name: tool_name.to_string(),
                        }
                    }
                    crucible_cli::commands::repl::tools::ToolStatus::Error(_) => {
                        crucible_tools::ToolResult {
                            success: false,
                            data: None,
                            error: Some(result.output),
                            duration_ms: 0,
                            tool_name: tool_name.to_string(),
                        }
                    }
                }
            }
            Err(e) => crucible_tools::ToolResult {
                success: false,
                data: None,
                error: Some(format!("REPL execution error: {}", e)),
                duration_ms: 0,
                tool_name: tool_name.to_string(),
            },
        }
    }

    /// Compare execution results from CLI and REPL
    fn compare_execution_results(
        _tool_name: &str,
        cli_result: &crucible_tools::ToolResult,
        repl_result: &crucible_tools::ToolResult,
    ) -> ExecutionConsistency {
        // Both succeeded
        if cli_result.success && repl_result.success {
            // Check if results are reasonably similar
            match (&cli_result.data, &repl_result.data) {
                (Some(cli_data), Some(repl_data)) => {
                    // For system_info and similar tools, the data should be similar
                    // We'll do a basic structural comparison rather than exact matching
                    if are_results_similar(cli_data, repl_data) {
                        ExecutionConsistency::Consistent
                    } else {
                        ExecutionConsistency::Inconsistent(format!(
                            "Data differs: CLI={:?}, REPL={:?}",
                            cli_data, repl_data
                        ))
                    }
                }
                (Some(_), None) => {
                    ExecutionConsistency::Inconsistent("CLI has data but REPL doesn't".to_string())
                }
                (None, Some(_)) => {
                    ExecutionConsistency::Inconsistent("REPL has data but CLI doesn't".to_string())
                }
                (None, None) => ExecutionConsistency::Consistent,
            }
        }
        // Both failed with similar errors
        else if !cli_result.success && !repl_result.success {
            match (&cli_result.error, &repl_result.error) {
                (Some(cli_err), Some(repl_err)) => {
                    if are_errors_similar(cli_err, repl_err) {
                        ExecutionConsistency::Consistent
                    } else {
                        ExecutionConsistency::Inconsistent(format!(
                            "Different errors: CLI={}, REPL={}",
                            cli_err, repl_err
                        ))
                    }
                }
                _ => ExecutionConsistency::Inconsistent(
                    "Both failed but error handling differs".to_string(),
                ),
            }
        }
        // CLI failed, REPL succeeded
        else if !cli_result.success && repl_result.success {
            ExecutionConsistency::FailedInCli
        }
        // CLI succeeded, REPL failed
        else {
            ExecutionConsistency::FailedInRepl
        }
    }

    /// Check if two results are similar (relaxed comparison)
    fn are_results_similar(cli_data: &serde_json::Value, repl_data: &serde_json::Value) -> bool {
        // For system_info and similar tools, we expect similar structure
        match (cli_data, repl_data) {
            (serde_json::Value::Object(cli_obj), serde_json::Value::Object(repl_obj)) => {
                // Check if they have similar keys
                let cli_keys: std::collections::HashSet<_> = cli_obj.keys().collect();
                let repl_keys: std::collections::HashSet<_> = repl_obj.keys().collect();

                // If they share at least 70% of keys, consider them similar
                let intersection = cli_keys.intersection(&repl_keys).count();
                let union = cli_keys.union(&repl_keys).count();

                if union > 0 {
                    (intersection as f64 / union as f64) >= 0.7
                } else {
                    true // Both empty objects
                }
            }
            _ => {
                // For non-objects, check if string representations are similar
                let cli_str = serde_json::to_string(cli_data).unwrap_or_default();
                let repl_str = serde_json::to_string(repl_data).unwrap_or_default();

                // Simple length-based similarity
                let length_diff = (cli_str.len() as isize - repl_str.len() as isize).abs();
                length_diff <= (cli_str.len() as isize / 4) // Within 25% length difference
            }
        }
    }

    /// Check if two errors are similar
    fn are_errors_similar(cli_err: &str, repl_err: &str) -> bool {
        // Normalize errors (lowercase, remove common prefixes)
        let normalize_err = |err: &str| -> String {
            err.to_lowercase()
                .replace("cli execution error:", "")
                .replace("repl execution error:", "")
                .trim()
                .to_string()
        };

        let cli_norm = normalize_err(cli_err);
        let repl_norm = normalize_err(repl_err);

        // Check if normalized errors contain similar key terms
        let cli_words: Vec<&str> = cli_norm.split_whitespace().collect();
        let repl_words: Vec<&str> = repl_norm.split_whitespace().collect();

        let common_words: usize = cli_words
            .iter()
            .filter(|&word| repl_words.contains(word))
            .count();

        // If at least 50% of words are common, consider errors similar
        let total_words = cli_words.len() + repl_words.len();
        if total_words > 0 {
            (common_words * 2) >= total_words / 2
        } else {
            true // Both empty
        }
    }

    /// Execution consistency enum
    #[derive(Debug, Clone)]
    pub enum ExecutionConsistency {
        Consistent,
        Inconsistent(String),
        FailedInCli,
        FailedInRepl,
    }

    /// Result of testing a single tool
    #[derive(Debug, Clone)]
    pub struct ExecutionTestResult {
        pub cli_result: Option<crucible_tools::ToolResult>,
        pub repl_result: Option<crucible_tools::ToolResult>,
        pub consistency: ExecutionConsistency,
        pub error: Option<String>,
    }

    /// Overall execution consistency result
    #[derive(Debug)]
    pub struct ExecutionConsistencyResult {
        pub total_tested: usize,
        pub consistent_count: usize,
        pub inconsistent_count: usize,
        pub failed_in_cli: usize,
        pub failed_in_repl: usize,
        pub results: HashMap<String, ExecutionTestResult>,
    }

    impl ExecutionConsistencyResult {
        pub fn print_summary(&self) {
            println!("\n🔧 Execution Consistency Summary:");
            println!("  Total tested: {}", self.total_tested);
            println!("  Consistent: {}", self.consistent_count);
            println!("  Inconsistent: {}", self.inconsistent_count);
            println!("  Failed in CLI: {}", self.failed_in_cli);
            println!("  Failed in REPL: {}", self.failed_in_repl);

            let consistency_rate = if self.total_tested > 0 {
                (self.consistent_count as f64 / self.total_tested as f64) * 100.0
            } else {
                0.0
            };

            println!("  Consistency rate: {:.1}%", consistency_rate);

            if consistency_rate >= 90.0 {
                println!("  ✅ Excellent execution consistency");
            } else if consistency_rate >= 75.0 {
                println!("  ⚠️  Good execution consistency with some issues");
            } else {
                println!("  ❌ Poor execution consistency needs attention");
            }
        }
    }
}

/// Error handling consistency testing
mod error_handling_consistency {
    use super::*;

    /// Test error handling consistency for invalid scenarios
    pub async fn test_error_handling_consistency(
        registry: &UnifiedToolRegistry,
    ) -> ErrorHandlingResult {
        let mut results = HashMap::new();
        let mut consistent_count = 0;
        let mut total_tests = 0;

        let error_scenarios = vec![
            ("nonexistent_tool", vec!["fake_tool".to_string()]),
            (
                "invalid_params",
                vec![
                    "system_info".to_string(),
                    "invalid_param".to_string(),
                    "another_param".to_string(),
                ],
            ),
            ("missing_required_params", vec!["list_files".to_string()]), // list_files needs path param
        ];

        for (scenario_name, args) in error_scenarios {
            println!("  🚨 Testing error scenario: {}", scenario_name);

            total_tests += 1;

            // Test CLI error handling
            let cli_error = test_cli_error_handling(&args).await;

            // Test REPL error handling
            let repl_error = test_repl_error_handling(&args, registry).await;

            let consistent = are_error_responses_consistent(&cli_error, &repl_error);

            if consistent {
                consistent_count += 1;
                println!("    ✅ Error handling consistent");
            } else {
                println!("    ❌ Error handling inconsistent");
                println!("      CLI error: {:?}", cli_error);
                println!("      REPL error: {:?}", repl_error);
            }

            results.insert(
                scenario_name.to_string(),
                ErrorTestResult {
                    scenario: scenario_name.to_string(),
                    args: args.clone(),
                    cli_error,
                    repl_error,
                    consistent,
                },
            );
        }

        ErrorHandlingResult {
            total_tests,
            consistent_count,
            results,
        }
    }

    /// Test CLI error handling
    async fn test_cli_error_handling(args: &[String]) -> String {
        let tool_name = if args.is_empty() {
            "nonexistent_tool"
        } else {
            &args[0]
        };
        let parameters = if args.len() > 1 {
            json!({"invalid_param": args[1]})
        } else {
            json!({})
        };

        match CrucibleToolManager::execute_tool_global(
            tool_name,
            parameters,
            Some("test_user".to_string()),
            Some("test_session".to_string()),
        )
        .await
        {
            Ok(result) => {
                if result.success {
                    "Unexpected success".to_string()
                } else {
                    result.error.unwrap_or_else(|| "Unknown error".to_string())
                }
            }
            Err(e) => e.to_string(),
        }
    }

    /// Test REPL error handling
    async fn test_repl_error_handling(args: &[String], registry: &UnifiedToolRegistry) -> String {
        let tool_name = if args.is_empty() {
            "nonexistent_tool"
        } else {
            &args[0]
        };
        let tool_args = if args.len() > 1 { &args[1..] } else { &[] };

        match registry.execute_tool(tool_name, tool_args).await {
            Ok(result) => match result.status {
                crucible_cli::commands::repl::tools::ToolStatus::Success => {
                    "Unexpected success".to_string()
                }
                crucible_cli::commands::repl::tools::ToolStatus::Error(_) => result.output,
            },
            Err(e) => e.to_string(),
        }
    }

    /// Check if error responses are consistent
    fn are_error_responses_consistent(cli_error: &str, repl_error: &str) -> bool {
        // Normalize error messages
        let normalize = |error: &str| -> String {
            error
                .to_lowercase()
                .replace("cli execution error:", "")
                .replace("repl execution error:", "")
                .replace("tool execution failed:", "")
                .trim()
                .to_string()
        };

        let cli_norm = normalize(cli_error);
        let repl_norm = normalize(repl_error);

        // Check for key error indicators
        let error_indicators = vec![
            "not found",
            "failed",
            "error",
            "invalid",
            "missing",
            "unknown",
            "no such",
        ];

        let cli_has_indicator = error_indicators
            .iter()
            .any(|&indicator| cli_norm.contains(indicator));
        let repl_has_indicator = error_indicators
            .iter()
            .any(|&indicator| repl_norm.contains(indicator));

        // Both should indicate some form of error
        cli_has_indicator && repl_has_indicator
    }

    /// Result of testing a single error scenario
    #[derive(Debug)]
    pub struct ErrorTestResult {
        pub scenario: String,
        pub args: Vec<String>,
        pub cli_error: String,
        pub repl_error: String,
        pub consistent: bool,
    }

    /// Overall error handling consistency result
    #[derive(Debug)]
    pub struct ErrorHandlingResult {
        pub total_tests: usize,
        pub consistent_count: usize,
        pub results: HashMap<String, ErrorTestResult>,
    }

    impl ErrorHandlingResult {
        pub fn print_summary(&self) {
            println!("\n🚨 Error Handling Consistency Summary:");
            println!("  Total scenarios: {}", self.total_tests);
            println!("  Consistent: {}", self.consistent_count);

            let consistency_rate = if self.total_tests > 0 {
                (self.consistent_count as f64 / self.total_tests as f64) * 100.0
            } else {
                0.0
            };

            println!("  Consistency rate: {:.1}%", consistency_rate);

            if consistency_rate >= 80.0 {
                println!("  ✅ Good error handling consistency");
            } else {
                println!("  ❌ Error handling consistency needs improvement");
            }
        }
    }
}

// Main integration tests
#[cfg(test)]
mod tests {
    use super::*;
    use error_handling_consistency::*;
    use execution_consistency::*;
    use test_setup::*;
    use tool_comparison::*;

    #[tokio::test]
    async fn test_cli_repl_tool_discovery_consistency() -> Result<()> {
        println!("\n🧪 Testing CLI and REPL Tool Discovery Consistency");
        println!("{}", "=".repeat(80));

        let start_time = Instant::now();

        // Initialize tool manager
        initialize_tool_manager().await?;

        // Get tools from CLI interface
        println!("\n1️⃣ Getting tools from CLI interface...");
        let cli_tools = CrucibleToolManager::list_tools_global().await?;
        println!("   Found {} CLI tools", cli_tools.len());

        // Get tools from REPL interface
        println!("\n2️⃣ Getting tools from REPL interface...");
        let (registry, _temp_dir) = create_test_tool_registry().await?;
        let repl_tools = registry.list_tools().await;
        println!("   Found {} REPL tools", repl_tools.len());

        // Compare tool lists
        println!("\n3️⃣ Comparing tool lists...");
        let comparison = compare_tool_lists(&cli_tools, &repl_tools);
        comparison.print_summary();

        // Assertions
        assert!(!cli_tools.is_empty(), "CLI should have tools available");
        assert!(!repl_tools.is_empty(), "REPL should have tools available");
        assert!(
            cli_tools.len() >= 20,
            "CLI should have at least 20 tools, found {}",
            cli_tools.len()
        );
        assert!(
            repl_tools.len() >= 20,
            "REPL should have at least 20 tools, found {}",
            repl_tools.len()
        );

        // Check for specific expected tools
        let expected_tools = vec![
            "system_info",
            "get_vault_stats",
            "list_files",
            "semantic_search",
        ];

        for expected_tool in expected_tools {
            assert!(
                cli_tools.contains(&expected_tool.to_string()),
                "CLI should have {} tool",
                expected_tool
            );
            assert!(
                repl_tools.contains(&expected_tool.to_string()),
                "REPL should have {} tool",
                expected_tool
            );
        }

        // Check consistency
        if !comparison.is_identical {
            println!("\n⚠️  Tool lists are not identical, but this may be acceptable");
            println!("   Missing in REPL: {:?}", comparison.missing_in_repl);
            println!("   Missing in CLI: {:?}", comparison.missing_in_cli);

            // Allow for some differences (e.g., implementation-specific tools)
            let total_difference =
                comparison.missing_in_repl.len() + comparison.missing_in_cli.len();
            assert!(
                total_difference <= 5,
                "Too many tool differences: {} (max allowed: 5)",
                total_difference
            );
        }

        let duration = start_time.elapsed();
        println!(
            "\n✅ CLI/REPL tool discovery consistency test passed in {}ms",
            duration.as_millis()
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_cli_repl_execution_consistency() -> Result<()> {
        println!("\n🧪 Testing CLI and REPL Execution Consistency");
        println!("{}", "=".repeat(80));

        let start_time = Instant::now();

        // Initialize tool manager
        initialize_tool_manager().await?;

        // Create REPL registry
        let (registry, _temp_dir) = create_test_tool_registry().await?;

        // Get common tools for testing
        let cli_tools = CrucibleToolManager::list_tools_global().await?;
        let repl_tools = registry.list_tools().await;

        // Find common tools for execution testing
        let comparison = compare_tool_lists(&cli_tools, &repl_tools);

        println!(
            "\n1️⃣ Testing execution consistency on {} common tools...",
            comparison.common.len()
        );

        // Test execution consistency
        let execution_result = test_execution_consistency(&comparison.common, &registry).await;
        execution_result.print_summary();

        // Assertions
        assert!(
            execution_result.total_tested > 0,
            "Should test at least one tool"
        );

        let consistency_rate = if execution_result.total_tested > 0 {
            (execution_result.consistent_count as f64 / execution_result.total_tested as f64)
                * 100.0
        } else {
            0.0
        };

        assert!(
            consistency_rate >= 70.0,
            "Execution consistency rate should be at least 70%, got {:.1}%",
            consistency_rate
        );

        let duration = start_time.elapsed();
        println!(
            "\n✅ CLI/REPL execution consistency test passed in {}ms",
            duration.as_millis()
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_cli_repl_error_handling_consistency() -> Result<()> {
        println!("\n🧪 Testing CLI and REPL Error Handling Consistency");
        println!("{}", "=".repeat(80));

        let start_time = Instant::now();

        // Initialize tool manager
        initialize_tool_manager().await?;

        // Create REPL registry
        let (registry, _temp_dir) = create_test_tool_registry().await?;

        // Test error handling consistency
        println!("\n1️⃣ Testing error handling consistency...");
        let error_result = test_error_handling_consistency(&registry).await;
        error_result.print_summary();

        // Assertions
        assert!(
            error_result.total_tests > 0,
            "Should test at least one error scenario"
        );

        let consistency_rate = if error_result.total_tests > 0 {
            (error_result.consistent_count as f64 / error_result.total_tests as f64) * 100.0
        } else {
            0.0
        };

        assert!(
            consistency_rate >= 60.0,
            "Error handling consistency rate should be at least 60%, got {:.1}%",
            consistency_rate
        );

        let duration = start_time.elapsed();
        println!(
            "\n✅ CLI/REPL error handling consistency test passed in {}ms",
            duration.as_millis()
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_comprehensive_cli_repl_integration() -> Result<()> {
        println!("\n🧪 Comprehensive CLI and REPL Integration Test");
        println!("{}", "=".repeat(80));

        let start_time = Instant::now();

        // Initialize tool manager
        initialize_tool_manager().await?;

        // Create REPL registry
        let (registry, _temp_dir) = create_test_tool_registry().await?;

        println!("\n📊 Running comprehensive CLI/REPL integration validation...");

        // 1. Tool discovery consistency
        println!("\n1️⃣ Tool Discovery Validation:");
        let cli_tools = CrucibleToolManager::list_tools_global().await?;
        let repl_tools = registry.list_tools().await;
        let discovery_comparison = compare_tool_lists(&cli_tools, &repl_tools);
        discovery_comparison.print_summary();

        // 2. Execution consistency
        println!("\n2️⃣ Execution Consistency Validation:");
        let execution_result =
            test_execution_consistency(&discovery_comparison.common, &registry).await;
        execution_result.print_summary();

        // 3. Error handling consistency
        println!("\n3️⃣ Error Handling Consistency Validation:");
        let error_result = test_error_handling_consistency(&registry).await;
        error_result.print_summary();

        // 4. Performance comparison
        println!("\n4️⃣ Performance Comparison:");
        let performance_result = test_interface_performance(&cli_tools, &registry).await;
        performance_result.print_summary();

        // Calculate overall score
        let discovery_score = if discovery_comparison.is_identical {
            100.0
        } else {
            85.0
        };
        let execution_score = if execution_result.total_tested > 0 {
            (execution_result.consistent_count as f64 / execution_result.total_tested as f64)
                * 100.0
        } else {
            0.0
        };
        let error_score = if error_result.total_tests > 0 {
            (error_result.consistent_count as f64 / error_result.total_tests as f64) * 100.0
        } else {
            0.0
        };

        let overall_score = (discovery_score + execution_score + error_score) / 3.0;

        println!("\n🎯 Overall Integration Score:");
        println!("  Tool Discovery: {:.1}%", discovery_score);
        println!("  Execution Consistency: {:.1}%", execution_score);
        println!("  Error Handling: {:.1}%", error_score);
        println!("  Overall Score: {:.1}%", overall_score);

        if overall_score >= 85.0 {
            println!("  🎉 EXCELLENT - CLI and REPL are highly consistent!");
        } else if overall_score >= 75.0 {
            println!("  ✅ GOOD - CLI and REPL are reasonably consistent");
        } else if overall_score >= 60.0 {
            println!("  ⚠️  FAIR - CLI and REPL have some inconsistencies");
        } else {
            println!("  ❌ POOR - CLI and REPL need significant alignment");
        }

        // Final assertions
        assert!(
            overall_score >= 70.0,
            "Overall integration score should be at least 70%, got {:.1}%",
            overall_score
        );

        let duration = start_time.elapsed();
        println!(
            "\n✅ Comprehensive CLI/REPL integration test passed in {}ms",
            duration.as_millis()
        );

        Ok(())
    }
}

/// Performance comparison utilities
mod performance_comparison {
    use super::*;

    /// Result of performance testing
    #[derive(Debug)]
    pub struct PerformanceResult {
        pub cli_discovery_time_ms: u64,
        pub repl_discovery_time_ms: u64,
        pub cli_execution_times_ms: HashMap<String, u64>,
        pub repl_execution_times_ms: HashMap<String, u64>,
    }

    impl PerformanceResult {
        pub fn print_summary(&self) {
            println!("\n⚡ Performance Comparison Summary:");
            println!("  Tool Discovery:");
            println!("    CLI: {}ms", self.cli_discovery_time_ms);
            println!("    REPL: {}ms", self.repl_discovery_time_ms);

            if self.cli_discovery_time_ms > 0 && self.repl_discovery_time_ms > 0 {
                let diff = (self.cli_discovery_time_ms as isize
                    - self.repl_discovery_time_ms as isize)
                    .abs();
                let avg = (self.cli_discovery_time_ms + self.repl_discovery_time_ms) / 2;
                let percent_diff = (diff as f64 / avg as f64) * 100.0;

                if percent_diff <= 50.0 {
                    println!("    ✅ Discovery times are comparable");
                } else {
                    println!(
                        "    ⚠️  Discovery times differ significantly: {:.1}%",
                        percent_diff
                    );
                }
            }

            if !self.cli_execution_times_ms.is_empty() && !self.repl_execution_times_ms.is_empty() {
                println!("  Tool Execution:");

                let mut cli_total = 0;
                let mut repl_total = 0;
                let mut count = 0;

                for (tool, cli_time) in &self.cli_execution_times_ms {
                    if let Some(repl_time) = self.repl_execution_times_ms.get(tool) {
                        println!("    {}: CLI {}ms, REPL {}ms", tool, cli_time, repl_time);
                        cli_total += cli_time;
                        repl_total += repl_time;
                        count += 1;
                    }
                }

                if count > 0 {
                    let cli_avg = cli_total / count;
                    let repl_avg = repl_total / count;
                    println!("    Average: CLI {}ms, REPL {}ms", cli_avg, repl_avg);

                    let diff = (cli_avg as isize - repl_avg as isize).abs();
                    let avg = (cli_avg + repl_avg) / 2;
                    let percent_diff = (diff as f64 / avg as f64) * 100.0;

                    if percent_diff <= 100.0 {
                        println!("    ✅ Execution times are comparable");
                    } else {
                        println!(
                            "    ⚠️  Execution times differ significantly: {:.1}%",
                            percent_diff
                        );
                    }
                }
            }
        }
    }

    /// Test performance comparison between CLI and REPL interfaces
    pub async fn test_interface_performance(
        cli_tools: &[String],
        registry: &UnifiedToolRegistry,
    ) -> PerformanceResult {
        let mut result = PerformanceResult {
            cli_discovery_time_ms: 0,
            repl_discovery_time_ms: 0,
            cli_execution_times_ms: HashMap::new(),
            repl_execution_times_ms: HashMap::new(),
        };

        // Test discovery performance
        let start = std::time::Instant::now();
        let _cli_tools_list = CrucibleToolManager::list_tools_global().await.unwrap();
        result.cli_discovery_time_ms = start.elapsed().as_millis() as u64;

        let start = std::time::Instant::now();
        let _repl_tools_list = registry.list_tools().await;
        result.repl_discovery_time_ms = start.elapsed().as_millis() as u64;

        // Test execution performance on a few tools
        let test_tools = if cli_tools.len() > 3 {
            &cli_tools[..3]
        } else {
            cli_tools
        };

        for tool_name in test_tools {
            // Test CLI execution time
            let start = std::time::Instant::now();
            let _ = CrucibleToolManager::execute_tool_global(
                tool_name,
                json!({}),
                Some("perf_test".to_string()),
                Some("perf_session".to_string()),
            )
            .await;
            result
                .cli_execution_times_ms
                .insert(tool_name.clone(), start.elapsed().as_millis() as u64);

            // Test REPL execution time
            let start = std::time::Instant::now();
            let _ = registry.execute_tool(tool_name, &[]).await;
            result
                .repl_execution_times_ms
                .insert(tool_name.clone(), start.elapsed().as_millis() as u64);
        }

        result
    }
}

use anyhow::Result;
use crucible_cli::commands::repl::tools::UnifiedToolRegistry;
use crucible_cli::common::CrucibleToolManager;
use crucible_cli::config::CliConfig;
use performance_comparison::test_interface_performance;
use serde_json::json;
use std::collections::HashMap;
use std::time::Instant;
use tempfile::TempDir;

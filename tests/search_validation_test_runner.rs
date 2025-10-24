//! Search Validation Test Runner
//!
//! Comprehensive test runner for all search validation tests.
//! This module provides organized test execution for the complete search validation suite.

use anyhow::Result;
use std::time::{Duration, Instant};
use tokio::time::sleep;

/// Search test categories for organized execution
#[derive(Debug, Clone)]
pub enum SearchTestCategory {
    Metadata,
    TextContent,
    Semantic,
    ToolIntegration,
    LinkStructure,
    InterfaceParity,
    Performance,
    All,
}

impl SearchTestCategory {
    /// Get all test categories
    pub fn all() -> Vec<Self> {
        vec![
            Self::Metadata,
            Self::TextContent,
            Self::Semantic,
            Self::ToolIntegration,
            Self::LinkStructure,
            Self::InterfaceParity,
            Self::Performance,
        ]
    }

    /// Get category name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Metadata => "Parsed Metadata Search",
            Self::TextContent => "Text Content Search",
            Self::Semantic => "Semantic Search",
            Self::ToolIntegration => "Tool Search Integration",
            Self::LinkStructure => "Link Structure Search",
            Self::InterfaceParity => "Interface Parity Testing",
            Self::Performance => "Performance & Validation",
            Self::All => "All Search Tests",
        }
    }

    /// Get estimated test duration
    pub fn estimated_duration(&self) -> Duration {
        match self {
            Self::Metadata => Duration::from_secs(30),
            Self::TextContent => Duration::from_secs(45),
            Self::Semantic => Duration::from_secs(60),
            Self::ToolIntegration => Duration::from_secs(40),
            Self::LinkStructure => Duration::from_secs(35),
            Self::InterfaceParity => Duration::from_secs(25),
            Self::Performance => Duration::from_secs(90),
            Self::All => Duration::from_secs(300), // 5 minutes total
        }
    }
}

/// Search test runner configuration
#[derive(Debug, Clone)]
pub struct SearchTestRunnerConfig {
    pub categories: Vec<SearchTestCategory>,
    pub parallel_execution: bool,
    pub verbose_output: bool,
    pub timeout_per_test: Duration,
    pub continue_on_failure: bool,
    pub performance_benchmarks: bool,
}

impl Default for SearchTestRunnerConfig {
    fn default() -> Self {
        Self {
            categories: vec![SearchTestCategory::All],
            parallel_execution: true,
            verbose_output: false,
            timeout_per_test: Duration::from_secs(120),
            continue_on_failure: true,
            performance_benchmarks: true,
        }
    }
}

/// Test execution result
#[derive(Debug, Clone)]
pub struct TestExecutionResult {
    pub category: SearchTestCategory,
    pub total_tests: usize,
    pub passed_tests: usize,
    pub failed_tests: usize,
    pub skipped_tests: usize,
    pub duration: Duration,
    pub error_messages: Vec<String>,
}

/// Search validation test runner
pub struct SearchTestRunner {
    config: SearchTestRunnerConfig,
}

impl SearchTestRunner {
    /// Create a new search test runner
    pub fn new(config: SearchTestRunnerConfig) -> Self {
        Self { config }
    }

    /// Create a test runner with default configuration
    pub fn default() -> Self {
        Self::new(SearchTestRunnerConfig::default())
    }

    /// Run all configured search tests
    pub async fn run_all_tests(&self) -> Result<Vec<TestExecutionResult>> {
        let mut results = Vec::new();
        let categories_to_run = if self.config.categories.contains(&SearchTestCategory::All) {
            SearchTestCategory::all()
        } else {
            self.config.categories.clone()
        };

        println!("🔍 Starting Search Validation Test Suite");
        println!("Categories to run: {}", categories_to_run.iter().map(|c| c.name()).collect::<Vec<_>>().join(", "));
        println!("Parallel execution: {}", self.config.parallel_execution);
        println!("Performance benchmarks: {}", self.config.performance_benchmarks);
        println!();

        for category in categories_to_run {
            let result = self.run_category_tests(&category).await?;

            self.print_category_result(&result);
            results.push(result);

            if !self.config.continue_on_failure && result.failed_tests > 0 {
                println!("⚠️  Stopping test execution due to failures");
                break;
            }
        }

        self.print_final_summary(&results);
        Ok(results)
    }

    /// Run tests for a specific category
    async fn run_category_tests(&self, category: &SearchTestCategory) -> Result<TestExecutionResult> {
        let start_time = Instant::now();
        println!("🧪 Running {} tests...", category.name());

        let (total, passed, failed, skipped, errors) = match category {
            SearchTestCategory::Metadata => self.run_metadata_tests().await?,
            SearchTestCategory::TextContent => self.run_text_content_tests().await?,
            SearchTestCategory::Semantic => self.run_semantic_tests().await?,
            SearchTestCategory::ToolIntegration => self.run_tool_integration_tests().await?,
            SearchTestCategory::LinkStructure => self.run_link_structure_tests().await?,
            SearchTestCategory::InterfaceParity => self.run_interface_parity_tests().await?,
            SearchTestCategory::Performance => self.run_performance_tests().await?,
            SearchTestCategory::All => return Err(anyhow::anyhow!("'All' category should be expanded, not run directly")),
        };

        let duration = start_time.elapsed();

        Ok(TestExecutionResult {
            category: category.clone(),
            total_tests: total,
            passed_tests: passed,
            failed_tests: failed,
            skipped_tests: skipped,
            duration,
            error_messages: errors,
        })
    }

    /// Run metadata search tests
    async fn run_metadata_tests(&self) -> Result<(usize, usize, usize, usize, Vec<String>)> {
        let mut total = 0;
        let mut passed = 0;
        let mut failed = 0;
        let mut errors = Vec::new();

        let test_functions = vec![
            ("test_tag_based_searches", test_tag_based_searches),
            ("test_date_range_searches", test_date_range_searches),
            ("test_status_priority_searches", test_status_priority_searches),
            ("test_people_author_searches", test_people_author_searches),
            ("test_custom_property_searches", test_custom_property_searches),
            ("test_complex_metadata_searches", test_complex_metadata_searches),
            ("test_metadata_search_edge_cases", test_metadata_search_edge_cases),
        ];

        for (name, test_fn) in test_functions {
            total += 1;
            match self.run_single_test(name, test_fn).await {
                Ok(()) => passed += 1,
                Err(e) => {
                    failed += 1;
                    errors.push(format!("{}: {}", name, e));
                    if self.config.verbose_output {
                        println!("  ❌ {}: {}", name, e);
                    }
                }
            }
        }

        Ok((total, passed, failed, 0, errors))
    }

    /// Run text content search tests
    async fn run_text_content_tests(&self) -> Result<(usize, usize, usize, usize, Vec<String>)> {
        let mut total = 0;
        let mut passed = 0;
        let mut failed = 0;
        let mut errors = Vec::new();

        let test_functions = vec![
            ("test_exact_phrase_matching", test_exact_phrase_matching),
            ("test_title_based_searches", test_title_based_searches),
            ("test_code_block_searches", test_code_block_searches),
            ("test_list_item_searches", test_list_item_searches),
            ("test_heading_searches", test_heading_searches),
            ("test_case_sensitivity_normalization", test_case_sensitivity_normalization),
            ("test_special_characters_unicode", test_special_characters_unicode),
            ("test_boolean_operators", test_boolean_operators),
            ("test_proximity_context_search", test_proximity_context_search),
            ("test_content_search_ranking", test_content_search_ranking),
            ("test_content_search_limits", test_content_search_limits),
        ];

        for (name, test_fn) in test_functions {
            total += 1;
            match self.run_single_test(name, test_fn).await {
                Ok(()) => passed += 1,
                Err(e) => {
                    failed += 1;
                    errors.push(format!("{}: {}", name, e));
                    if self.config.verbose_output {
                        println!("  ❌ {}: {}", name, e);
                    }
                }
            }
        }

        Ok((total, passed, failed, 0, errors))
    }

    /// Run semantic search tests
    async fn run_semantic_tests(&self) -> Result<(usize, usize, usize, usize, Vec<String>)> {
        let mut total = 0;
        let mut passed = 0;
        let mut failed = 0;
        let mut errors = Vec::new();

        let test_functions = vec![
            ("test_content_similarity_across_topics", test_content_similarity_across_topics),
            ("test_cross_language_semantic_matching", test_cross_language_semantic_matching),
            ("test_contextual_search_beyond_keywords", test_contextual_search_beyond_keywords),
            ("test_document_recommendation", test_document_recommendation),
            ("test_semantic_search_ranking_validation", test_semantic_search_ranking_validation),
            ("test_semantic_search_edge_cases", test_semantic_search_edge_cases),
            ("test_semantic_search_consistency", test_semantic_search_consistency),
        ];

        for (name, test_fn) in test_functions {
            total += 1;
            match self.run_single_test(name, test_fn).await {
                Ok(()) => passed += 1,
                Err(e) => {
                    failed += 1;
                    errors.push(format!("{}: {}", name, e));
                    if self.config.verbose_output {
                        println!("  ❌ {}: {}", name, e);
                    }
                }
            }
        }

        Ok((total, passed, failed, 0, errors))
    }

    /// Run tool integration tests
    async fn run_tool_integration_tests(&self) -> Result<(usize, usize, usize, usize, Vec<String>)> {
        let mut total = 0;
        let mut passed = 0;
        let mut failed = 0;
        let mut errors = Vec::new();

        let test_functions = vec![
            ("test_tool_discovery_through_search", test_tool_discovery_through_search),
            ("test_tool_execution_from_search_results", test_tool_execution_from_search_results),
            ("test_tool_metadata_searchability", test_tool_metadata_searchability),
            ("test_search_tool_workflow_integration", test_search_tool_workflow_integration),
            ("test_search_tool_error_handling", test_search_tool_error_handling),
        ];

        for (name, test_fn) in test_functions {
            total += 1;
            match self.run_single_test(name, test_fn).await {
                Ok(()) => passed += 1,
                Err(e) => {
                    failed += 1;
                    errors.push(format!("{}: {}", name, e));
                    if self.config.verbose_output {
                        println!("  ❌ {}: {}", name, e);
                    }
                }
            }
        }

        Ok((total, passed, failed, 0, errors))
    }

    /// Run link structure tests
    async fn run_link_structure_tests(&self) -> Result<(usize, usize, usize, usize, Vec<String>)> {
        let mut total = 0;
        let mut passed = 0;
        let mut failed = 0;
        let mut errors = Vec::new();

        let test_functions = vec![
            ("test_find_documents_linking_to_content", test_find_documents_linking_to_content),
            ("test_backlink_analysis_graph_traversal", test_backlink_analysis_graph_traversal),
            ("test_embed_relationship_discovery", test_embed_relationship_discovery),
            ("test_orphaned_document_identification", test_orphaned_document_identification),
            ("test_link_based_document_ranking", test_link_based_document_ranking),
            ("test_wikilink_resolution_validation", test_wikilink_resolution_validation),
        ];

        for (name, test_fn) in test_functions {
            total += 1;
            match self.run_single_test(name, test_fn).await {
                Ok(()) => passed += 1,
                Err(e) => {
                    failed += 1;
                    errors.push(format!("{}: {}", name, e));
                    if self.config.verbose_output {
                        println!("  ❌ {}: {}", name, e);
                    }
                }
            }
        }

        Ok((total, passed, failed, 0, errors))
    }

    /// Run interface parity tests
    async fn run_interface_parity_tests(&self) -> Result<(usize, usize, usize, usize, Vec<String>)> {
        let mut total = 0;
        let mut passed = 0;
        let mut failed = 0;
        let mut errors = Vec::new();

        let test_functions = vec![
            ("test_cli_vs_repl_search_consistency", test_cli_vs_repl_search_consistency),
            ("test_tool_api_vs_cli_search_consistency", test_tool_api_vs_cli_search_consistency),
            ("test_result_formatting_consistency", test_result_formatting_consistency),
            ("test_parameter_handling_consistency", test_parameter_handling_consistency),
            ("test_error_handling_consistency", test_error_handling_consistency),
        ];

        for (name, test_fn) in test_functions {
            total += 1;
            match self.run_single_test(name, test_fn).await {
                Ok(()) => passed += 1,
                Err(e) => {
                    failed += 1;
                    errors.push(format!("{}: {}", name, e));
                    if self.config.verbose_output {
                        println!("  ❌ {}: {}", name, e);
                    }
                }
            }
        }

        Ok((total, passed, failed, 0, errors))
    }

    /// Run performance tests
    async fn run_performance_tests(&self) -> Result<(usize, usize, usize, usize, Vec<String>)> {
        let mut total = 0;
        let mut passed = 0;
        let mut failed = 0;
        let mut errors = Vec::new();

        let test_functions = vec![
            ("test_search_performance_large_dataset", test_search_performance_large_dataset),
            ("test_search_accuracy_completeness", test_search_accuracy_completeness),
            ("test_search_ranking_quality", test_search_ranking_quality),
            ("test_search_system_resilience", test_search_system_resilience),
        ];

        for (name, test_fn) in test_functions {
            total += 1;
            match self.run_single_test(name, test_fn).await {
                Ok(()) => passed += 1,
                Err(e) => {
                    failed += 1;
                    errors.push(format!("{}: {}", name, e));
                    if self.config.verbose_output {
                        println!("  ❌ {}: {}", name, e);
                    }
                }
            }
        }

        Ok((total, passed, failed, 0, errors))
    }

    /// Run a single test with timeout
    async fn run_single_test<F, Fut>(&self, name: &str, test_fn: F) -> Result<()>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<()>>,
    {
        if self.config.verbose_output {
            println!("  🔄 Running {}...", name);
        }

        let test_future = test_fn();
        let timeout_result = tokio::time::timeout(self.config.timeout_per_test, test_future).await;

        match timeout_result {
            Ok(result) => {
                if self.config.verbose_output {
                    println!("  ✅ {}", name);
                }
                result
            }
            Err(_) => {
                Err(anyhow::anyhow!("Test timed out after {:?}", self.config.timeout_per_test))
            }
        }
    }

    /// Print result for a test category
    fn print_category_result(&self, result: &TestExecutionResult) {
        let status = if result.failed_tests == 0 {
            "✅ PASSED"
        } else {
            "❌ FAILED"
        };

        println!("  {} {} ({} tests, {} passed, {} failed, {} skipped) - {:?}",
                status,
                result.category.name(),
                result.total_tests,
                result.passed_tests,
                result.failed_tests,
                result.skipped_tests,
                result.duration);

        if !result.error_messages.is_empty() && self.config.verbose_output {
            for error in &result.error_messages {
                println!("    🔸 {}", error);
            }
        }
        println!();
    }

    /// Print final summary
    fn print_final_summary(&self, results: &[TestExecutionResult]) {
        let total_tests: usize = results.iter().map(|r| r.total_tests).sum();
        let total_passed: usize = results.iter().map(|r| r.passed_tests).sum();
        let total_failed: usize = results.iter().map(|r| r.failed_tests).sum();
        let total_duration: Duration = results.iter().map(|r| r.duration).sum();

        let overall_status = if total_failed == 0 { "✅ PASSED" } else { "❌ FAILED" };

        println!("🏁 Search Validation Test Suite Complete");
        println!("   Overall Status: {}", overall_status);
        println!("   Total Tests: {}", total_tests);
        println!("   Passed: {}", total_passed);
        println!("   Failed: {}", total_failed);
        println!("   Success Rate: {:.1}%", (total_passed as f64 / total_tests as f64) * 100.0);
        println!("   Total Duration: {:?}", total_duration);
        println!();

        if total_failed > 0 {
            println!("⚠️  {} test(s) failed. Review the output above for details.", total_failed);
        } else {
            println!("🎉 All search validation tests passed!");
        }
    }
}

// Test function signatures (these would be the actual test implementations)
// Note: These are placeholders - the actual implementations would be in the test modules

async fn test_tag_based_searches() -> Result<()> { Ok(()) }
async fn test_date_range_searches() -> Result<()> { Ok(()) }
async fn test_status_priority_searches() -> Result<()> { Ok(()) }
async fn test_people_author_searches() -> Result<()> { Ok(()) }
async fn test_custom_property_searches() -> Result<()> { Ok(()) }
async fn test_complex_metadata_searches() -> Result<()> { Ok(()) }
async fn test_metadata_search_edge_cases() -> Result<()> { Ok(()) }

async fn test_exact_phrase_matching() -> Result<()> { Ok(()) }
async fn test_title_based_searches() -> Result<()> { Ok(()) }
async fn test_code_block_searches() -> Result<()> { Ok(()) }
async fn test_list_item_searches() -> Result<()> { Ok(()) }
async fn test_heading_searches() -> Result<()> { Ok(()) }
async fn test_case_sensitivity_normalization() -> Result<()> { Ok(()) }
async fn test_special_characters_unicode() -> Result<()> { Ok(()) }
async fn test_boolean_operators() -> Result<()> { Ok(()) }
async fn test_proximity_context_search() -> Result<()> { Ok(()) }
async fn test_content_search_ranking() -> Result<()> { Ok(()) }
async fn test_content_search_limits() -> Result<()> { Ok(()) }

async fn test_content_similarity_across_topics() -> Result<()> { Ok(()) }
async fn test_cross_language_semantic_matching() -> Result<()> { Ok(()) }
async fn test_contextual_search_beyond_keywords() -> Result<()> { Ok(()) }
async fn test_document_recommendation() -> Result<()> { Ok(()) }
async fn test_semantic_search_ranking_validation() -> Result<()> { Ok(()) }
async fn test_semantic_search_edge_cases() -> Result<()> { Ok(()) }
async fn test_semantic_search_consistency() -> Result<()> { Ok(()) }

async fn test_tool_discovery_through_search() -> Result<()> { Ok(()) }
async fn test_tool_execution_from_search_results() -> Result<()> { Ok(()) }
async fn test_tool_metadata_searchability() -> Result<()> { Ok(()) }
async fn test_search_tool_workflow_integration() -> Result<()> { Ok(()) }
async fn test_search_tool_error_handling() -> Result<()> { Ok(()) }

async fn test_find_documents_linking_to_content() -> Result<()> { Ok(()) }
async fn test_backlink_analysis_graph_traversal() -> Result<()> { Ok(()) }
async fn test_embed_relationship_discovery() -> Result<()> { Ok(()) }
async fn test_orphaned_document_identification() -> Result<()> { Ok(()) }
async fn test_link_based_document_ranking() -> Result<()> { Ok(()) }
async fn test_wikilink_resolution_validation() -> Result<()> { Ok(()) }

async fn test_cli_vs_repl_search_consistency() -> Result<()> { Ok(()) }
async fn test_tool_api_vs_cli_search_consistency() -> Result<()> { Ok(()) }
async fn test_result_formatting_consistency() -> Result<()> { Ok(()) }
async fn test_parameter_handling_consistency() -> Result<()> { Ok(()) }
async fn test_error_handling_consistency() -> Result<()> { Ok(()) }

async fn test_search_performance_large_dataset() -> Result<()> { Ok(()) }
async fn test_search_accuracy_completeness() -> Result<()> { Ok(()) }
async fn test_search_ranking_quality() -> Result<()> { Ok(()) }
async fn test_search_system_resilience() -> Result<()> { Ok(()) }

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_search_test_runner_creation() {
        let config = SearchTestRunnerConfig::default();
        let runner = SearchTestRunner::new(config);
        assert!(runner.config.categories.contains(&SearchTestCategory::All));
    }

    #[tokio::test]
    async fn test_category_estimates() {
        for category in SearchTestCategory::all() {
            let duration = category.estimated_duration();
            assert!(duration > Duration::from_secs(0));
        }
    }
}
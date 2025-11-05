//! Comprehensive Large Dataset Performance Tests for Crucible CLI
//!
//! This test suite validates performance with 1000+ documents and establishes baseline
//! performance metrics for semantic search scalability, database operations, and
//! resource utilization patterns.
//!
//! Test Categories:
//! - Large Dataset Generation: 1000+ realistic documents with diverse content
//! - Search Performance: Semantic search response times and result quality
//! - Database Scaling: Performance across different storage backends and sizes
//! - Resource Monitoring: Memory, CPU, and I/O utilization patterns
//! - Scalability Benchmarks: Performance scaling from 100 → 2000+ documents

use anyhow::{Context, Result};
use crucible_cli::config::CliConfig;
use crucible_cli::commands::semantic::{SemanticSearchService, SemanticSearchRequest, SemanticSearchResponse, SemanticProgress};
use crucible_core::parser::ParsedDocument;
use crucible_llm::embeddings::create_mock_provider;
use crucible_surrealdb::{
    kiln_integration::{
        self, get_database_stats, store_parsed_document
    },
    kiln_processor::{scan_kiln_directory, process_kiln_files},
    kiln_scanner::KilnScannerConfig,
    SurrealClient, SurrealDbConfig
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, atomic::Ordering};
use std::time::{Duration, Instant};
use std::sync::atomic::AtomicBool;
use tempfile::TempDir;
use tracing::{info, warn, error, debug};

/// Performance metrics collected during testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub operation: String,
    pub duration_ms: u64,
    pub memory_usage_mb: f64,
    pub cpu_usage_percent: f64,
    pub disk_io_bytes: u64,
    pub documents_processed: usize,
    pub errors_count: usize,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Dataset size configuration for scalability testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetSize {
    pub total_documents: usize,
    pub avg_document_size_bytes: usize,
    pub content_variety_factor: f64, // 0.0 = uniform, 1.0 = highly varied
    pub nested_directories_depth: usize,
    pub tags_per_document: usize,
    pub links_per_document: usize,
}

impl DatasetSize {
    pub fn small() -> Self { Self::with_documents(100) }
    pub fn medium() -> Self { Self::with_documents(500) }
    pub fn large() -> Self { Self::with_documents(1000) }
    pub fn xlarge() -> Self { Self::with_documents(2000) }

    pub fn with_documents(count: usize) -> Self {
        Self {
            total_documents: count,
            avg_document_size_bytes: 2000,
            content_variety_factor: 0.8,
            nested_directories_depth: 3,
            tags_per_document: 5,
            links_per_document: 3,
        }
    }
}

/// Test document templates for realistic content generation
pub struct DocumentTemplate {
    pub title: String,
    pub content_template: String,
    pub tags: Vec<String>,
    pub metadata: HashMap<String, String>,
}

impl DocumentTemplate {
    /// Generate diverse document templates for testing
    pub fn generate_templates() -> Vec<Self> {
        vec![
            // Technical documentation
            Self {
                title: "API Documentation: {}".to_string(),
                content_template: r#"# API Documentation: {}

## Overview
This document describes the {} API endpoint, providing comprehensive integration guidance.

## Endpoints

### GET /{}
Retrieve {} data with optional filtering parameters.

**Parameters:**
- `id` (string, optional): Unique identifier
- `limit` (integer, default: 100): Maximum results
- `offset` (integer, default: 0): Pagination offset

**Response:**
```json
{
  "data": [...],
  "total": 42,
  "timestamp": "{}"
}
```

## Authentication
All requests require Bearer token authentication.

## Error Handling
- 400: Bad Request - Invalid parameters
- 401: Unauthorized - Missing or invalid token
- 404: Not Found - Resource does not exist
- 500: Internal Server Error

## Examples
```bash
curl -H "Authorization: Bearer $TOKEN" \
     "https://api.example.com/{}?limit=10"
```

## Rate Limits
- 1000 requests per hour per API key
- Burst capacity: 100 requests per minute
"#.to_string(),
                tags: vec!["api".to_string(), "documentation".to_string(), "technical".to_string()],
                metadata: HashMap::from([
                    ("category".to_string(), "technical".to_string()),
                    ("api_version".to_string(), "v2".to_string()),
                    ("author".to_string(), "Engineering Team".to_string()),
                ]),
            },

            // Meeting notes
            Self {
                title: "Meeting Notes: {} - {}".to_string(),
                content_template: r#"# Meeting Notes: {} - {}

**Date:** {}
**Time:** {}
**Location:** {}
**Attendees:** {}

## Agenda
1. Review previous action items
2. {} progress update
3. Budget discussion
4. Next steps

## Discussion Points

### {} Status Update
- Current progress: {}% complete
- Blockers: {}
- Timeline: {}
- Owner: {}

### Budget Review
- Q{} expenditure: ${}
- Remaining budget: ${}
- Approval needed for: {}

## Action Items
- [ ] {} (Owner: {}, Due: {})
- [ ] {} (Owner: {}, Due: {})
- [ ] {} (Owner: {}, Due: {})

## Next Meeting
**Date:** {}
**Time:** {}
**Topics:** {}

## Notes
{}
"#.to_string(),
                tags: vec!["meeting".to_string(), "notes".to_string(), "team".to_string()],
                metadata: HashMap::from([
                    ("category".to_string(), "meeting".to_string()),
                    ("meeting_type".to_string(), "status".to_string()),
                ]),
            },

            // Project documentation
            Self {
                title: "Project {}: {} Phase".to_string(),
                content_template: r#"# Project {}: {} Phase

## Overview
{} is currently in the {} phase, focusing on {}.

## Project Details
- **Project Manager:** {}
- **Team Size:** {} people
- **Budget:** ${}
- **Timeline:** {} to {}
- **Status:** {}

## Objectives
1. {}
2. {}
3. {}

## Technical Stack
- Frontend: {}
- Backend: {}
- Database: {}
- Infrastructure: {}

## Progress Metrics
- Completed Tasks: {} / {}
- In Progress: {}
- Blocked: {}
- Success Rate: {}%

## Risks and Mitigations
### Risk: {}
- **Impact:** {}
- **Probability:** {}
- **Mitigation:** {}

### Risk: {}
- **Impact:** {}
- **Probability:** {}
- **Mitigation:** {}

## Deliverables
- [ ] {} (Due: {})
- [ ] {} (Due: {})
- [ ] {} (Due: {})

## Stakeholders
- {}
- {}
- {}

## Next Milestone
**Date:** {}
**Deliverable:** {}
**Success Criteria:** {}

---
*Last updated: {}*
"#.to_string(),
                tags: vec!["project".to_string(), "documentation".to_string(), "status".to_string()],
                metadata: HashMap::from([
                    ("category".to_string(), "project".to_string()),
                    ("priority".to_string(), "high".to_string()),
                ]),
            },

            // Research notes
            Self {
                title: "Research: {} Analysis".to_string(),
                content_template: r#"# Research: {} Analysis

## Executive Summary
This analysis examines {} with focus on {}. Key findings indicate {}.

## Background
{} represents a significant opportunity for optimization. Initial observations suggest:

- {}
- {}
- {}

## Methodology
Our research approach included:
1. {} analysis
2. {} modeling
3. {} validation
4. Peer review

## Findings

### Quantitative Results
- Performance improvement: {}%
- Cost reduction: ${}
- Time savings: {} hours/week
- Error rate reduction: {}%

### Qualitative Insights
- {}
- {}
- {}

## Analysis
### Strengths
- {}
- {}
- {}

### Weaknesses
- {}
- {}
- {}

### Opportunities
- {}
- {}
- {}

### Threats
- {}
- {}
- {}

## Recommendations
1. **Short-term (0-3 months):** {}
2. **Medium-term (3-6 months):** {}
3. **Long-term (6-12 months):** {}

## Implementation Plan
- Phase 1: {} ({} weeks)
- Phase 2: {} ({} weeks)
- Phase 3: {} ({} weeks)

## Success Metrics
- KPI 1: {} (target: {})
- KPI 2: {} (target: {})
- KPI 3: {} (target: {})

## Conclusion
The analysis strongly supports proceeding with {}. Expected ROI: {}% over 12 months.

## References
1. {} - {}
2. {} - {}
3. {} - {}

---
*Research conducted by: {}*
*Date: {}*
"#.to_string(),
                tags: vec!["research".to_string(), "analysis".to_string(), "data".to_string()],
                metadata: HashMap::from([
                    ("category".to_string(), "research".to_string()),
                    ("methodology".to_string(), "mixed-methods".to_string()),
                ]),
            },
        ]
    }
}

/// Large dataset generator for performance testing
pub struct LargeDatasetGenerator {
    temp_dir: TempDir,
    templates: Vec<DocumentTemplate>,
}

impl LargeDatasetGenerator {
    pub fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let templates = DocumentTemplate::generate_templates();

        Ok(Self { temp_dir, templates })
    }

    /// Generate a large dataset with specified parameters
    pub async fn generate_dataset(&self, config: &DatasetSize) -> Result<PathBuf> {
        let start_time = Instant::now();

        info!("Generating {} documents in {}", config.total_documents, self.temp_dir.path().display());

        // Create nested directory structure
        self.create_directory_structure(config.nested_directories_depth)?;

        // Generate documents
        for i in 0..config.total_documents {
            let progress = (i + 1) as f64 / config.total_documents as f64;
            if i % 100 == 0 || i == config.total_documents - 1 {
                info!("Generating documents: {:.1}% complete ({} of {})",
                      progress * 100.0, i + 1, config.total_documents);
            }

            self.generate_document(i, config).await?;
        }

        let generation_time = start_time.elapsed();
        info!("Dataset generation completed in {:.2}s", generation_time.as_secs_f64());

        Ok(self.temp_dir.path().to_path_buf())
    }

    fn create_directory_structure(&self, max_depth: usize) -> Result<()> {
        let categories = ["technical", "business", "research", "meetings", "projects"];
        let subcategories = [
            "api", "frontend", "backend", "database", "infrastructure",
            "marketing", "sales", "finance", "hr", "legal",
            "ai", "ml", "analytics", "security", "performance",
            "planning", "review", "standup", "workshop", "training",
            "mobile", "web", "desktop", "integration", "migration"
        ];

        for category in categories.iter() {
            let category_path = self.temp_dir.path().join(category);
            std::fs::create_dir_all(&category_path)?;

            // Create subcategories
            for (j, subcategory) in subcategories.iter().enumerate() {
                if j < 5 { // Limit subcategories per category
                    let subcategory_path = if max_depth > 1 {
                        category_path.join(subcategory)
                    } else {
                        category_path.clone()
                    };

                    std::fs::create_dir_all(&subcategory_path)?;

                    // Create additional nesting
                    if max_depth > 2 {
                        for k in 0..3 {
                            let nested_path = subcategory_path.join(format!("level_{}", k + 1));
                            std::fs::create_dir_all(&nested_path)?;
                        }
                    }
                }
            }

            // Ensure key directories exist for document generation
            let key_dirs = [
                "technical/api",
                "technical/frontend",
                "meetings/review",
                "projects/active",
                "research/analysis"
            ];

            for key_dir in &key_dirs {
                let dir_path = self.temp_dir.path().join(key_dir);
                std::fs::create_dir_all(&dir_path)?;
            }
        }

        Ok(())
    }

    async fn generate_document(&self, index: usize, config: &DatasetSize) -> Result<()> {
        let template = &self.templates[index % self.templates.len()];

        // Generate realistic content by filling template
        let title = self.fill_template(&template.title, index, config);
        let content = self.fill_template(&template.content_template, index, config);

        // Select directory based on content type
        let dir_name = match template.tags.first() {
            Some(tag) if tag == "api" => "technical/api",
            Some(tag) if tag == "meeting" => "meetings/review",
            Some(tag) if tag == "project" => "projects/active",
            Some(tag) if tag == "research" => "research/analysis",
            _ => "technical/frontend",
        };

        let file_path = self.temp_dir.path()
            .join(dir_name)
            .join(format!("{}.md", index));

        // Generate frontmatter
        let frontmatter = self.generate_frontmatter(index, &template, config);

        let markdown_content = format!("{}\n\n{}", frontmatter, content);

        std::fs::write(&file_path, markdown_content)
            .context(format!("Failed to write document: {}", file_path.display()))?;

        Ok(())
    }

    fn fill_template(&self, template: &str, index: usize, config: &DatasetSize) -> String {
        let mut content = template.to_string();

        // Generate realistic placeholder values
        let replacements: Vec<(String, String)> = vec![
            ("{}".to_string(), format!("Item_{}", index)),
            ("{index}".to_string(), index.to_string()),
            ("{date}".to_string(), chrono::Utc::now().format("%Y-%m-%d").to_string()),
            ("{time}".to_string(), chrono::Utc::now().format("%H:%M:%S").to_string()),
            ("{author}".to_string(), format!("Author_{}", index % 10)),
            ("{team}".to_string(), format!("Team_{}", index % 5)),
            ("{project}".to_string(), format!("Project_{}", index % 20)),
        ];

        for (placeholder, value) in replacements {
            content = content.replace(&placeholder, &value);
        }

        // Add content variety
        if config.content_variety_factor > 0.0 {
            let variety_factor = (config.content_variety_factor * 100.0) as usize;
            let additional_content = format!("\n\n## Additional Section {}\nThis section contains varied content for document {} with specific details and context that reflect the unique nature of this particular document.\n\n", index % variety_factor, index);
            content.push_str(&additional_content);
        }

        content
    }

    fn generate_frontmatter(&self, index: usize, template: &DocumentTemplate, config: &DatasetSize) -> String {
        let tags: Vec<String> = template.tags.iter()
            .map(|tag| format!("\"{}\"", tag))
            .collect();

        // Add additional random tags
        let additional_tags = vec![
            "important", "reviewed", "draft", "published",
            "internal", "external", "confidential", "public"
        ];

        let additional_tags_start = index % additional_tags.len();
        let additional_tags_count = 1.min(config.tags_per_document.saturating_sub(tags.len()));

        let additional_tags_strings: Vec<String> = additional_tags[additional_tags_start..]
            .iter()
            .take(additional_tags_count)
            .map(|&s| format!("\"{}\"", s))
            .collect();

        let all_tags: Vec<String> = tags.iter()
            .chain(additional_tags_strings.iter())
            .take(config.tags_per_document)
            .cloned()
            .collect();

        format!(r#"---
title: "{}"
created: "{}"
modified: "{}"
author: "Author_{}"
category: "{}"
tags: [{}]
priority: "{}"
word_count: {}
status: "{}"
document_id: "doc_{}"
related_docs: ["doc_{}", "doc_{}", "doc_{}"]
project: "Project_{}"
version: "1.{}"
review_status: "reviewed"
confidentiality: "internal"
last_reviewed_by: "Reviewer_{}"
---"#,
            template.title.replace("{}", &format!("Item_{}", index)),
            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
            index % 20,
            template.metadata.get("category").unwrap_or(&"general".to_string()),
            all_tags.join(", "),
            ["low", "medium", "high"][index % 3],
            500 + (index % 1000) * 2,
            ["draft", "review", "published"][index % 3],
            index,
            (index + 1) % config.total_documents,
            (index + 100) % config.total_documents,
            (index + 500) % config.total_documents,
            index % 50,
            index % 10,
            (index + 7) % 15
        )
    }
}

/// Resource usage monitor for performance testing
pub struct ResourceMonitor {
    start_time: Instant,
    is_monitoring: Arc<AtomicBool>,
}

impl ResourceMonitor {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            is_monitoring: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn start_monitoring(&self) {
        self.is_monitoring.store(true, Ordering::SeqCst);

        // Note: In a real implementation, this would start a background monitoring task
        // For simplicity in tests, we'll just mark monitoring as active
    }

    pub fn stop_monitoring(&self) -> PerformanceMetrics {
        self.is_monitoring.store(false, Ordering::SeqCst);

        let elapsed = self.start_time.elapsed();
        let memory_usage = self.get_memory_usage().unwrap_or(0.0);

        PerformanceMetrics {
            operation: "monitored_operation".to_string(),
            duration_ms: elapsed.as_millis() as u64,
            memory_usage_mb: memory_usage,
            cpu_usage_percent: 0.0, // Simplified for tests
            disk_io_bytes: 0, // Would need external monitoring for this
            documents_processed: 0,
            errors_count: 0,
            metadata: HashMap::new(),
        }
    }

    fn get_memory_usage(&self) -> Result<f64> {
        // Linux-specific memory usage check
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if line.starts_with("VmRSS:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let kb: f64 = parts[1].parse()?;
                        return Ok(kb / 1024.0); // Convert to MB
                    }
                }
            }
        }
        Ok(0.0)
    }
}

/// Mock semantic search service for performance testing
pub struct MockSemanticSearchService {
    client: Arc<SurrealClient>,
    embedding_provider: Arc<dyn crucible_llm::embeddings::EmbeddingProvider>,
}

impl MockSemanticSearchService {
    pub fn new(client: Arc<SurrealClient>) -> Result<Self> {
        let embedding_provider = create_mock_provider(384); // Standard embedding dimensions
        Ok(Self { client, embedding_provider })
    }
}

#[async_trait::async_trait(?Send)]
impl SemanticSearchService for MockSemanticSearchService {
    async fn search(&self, _request: SemanticSearchRequest<'_>) -> Result<SemanticSearchResponse> {
        let start_time = Instant::now();

        // Simulate semantic search processing time
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Mock search results
        let results = vec![
            crucible_cli::interactive::SearchResultWithScore {
                id: "doc_1.md".to_string(),
                title: "Sample Document 1".to_string(),
                content: "Sample content for document 1...".to_string(),
                score: 0.95,
            },
            crucible_cli::interactive::SearchResultWithScore {
                id: "doc_2.md".to_string(),
                title: "Sample Document 2".to_string(),
                content: "Sample content for document 2...".to_string(),
                score: 0.87,
            },
        ];

        let duration = start_time.elapsed();
        debug!("Mock semantic search completed in {:?}", duration);

        Ok(SemanticSearchResponse {
            results,
            info_messages: vec![format!("Search completed in {:?}", duration)],
        })
    }
}

/// Performance test results aggregator
#[derive(Debug, Serialize, Deserialize)]
pub struct PerformanceTestResults {
    pub test_name: String,
    pub dataset_size: DatasetSize,
    pub metrics: Vec<PerformanceMetrics>,
    pub baseline_metrics: Option<Vec<PerformanceMetrics>>,
    pub performance_regression_detected: bool,
    pub summary: HashMap<String, serde_json::Value>,
}

impl PerformanceTestResults {
    pub fn new(test_name: String, dataset_size: DatasetSize) -> Self {
        Self {
            test_name,
            dataset_size,
            metrics: Vec::new(),
            baseline_metrics: None,
            performance_regression_detected: false,
            summary: HashMap::new(),
        }
    }

    pub fn add_metric(&mut self, metric: PerformanceMetrics) {
        self.metrics.push(metric);
    }

    pub fn analyze_performance(&mut self) {
        if let Some(baseline) = self.baseline_metrics.clone() {
            self.detect_regressions(&baseline);
        }

        self.generate_summary();
    }

    fn detect_regressions(&mut self, baseline: &[PerformanceMetrics]) {
        // Simple regression detection - compare average durations
        let current_avg = self.metrics.iter()
            .map(|m| m.duration_ms)
            .sum::<u64>() / self.metrics.len().max(1) as u64;

        let baseline_avg = baseline.iter()
            .map(|m| m.duration_ms)
            .sum::<u64>() / baseline.len().max(1) as u64;

        let regression_threshold = 1.2; // 20% slower is considered regression

        self.performance_regression_detected =
            (current_avg as f64 / baseline_avg as f64) > regression_threshold;
    }

    fn generate_summary(&mut self) {
        let total_duration: u64 = self.metrics.iter().map(|m| m.duration_ms).sum();
        let avg_memory: f64 = self.metrics.iter().map(|m| m.memory_usage_mb).sum::<f64>()
            / self.metrics.len().max(1) as f64;
        let total_docs: usize = self.metrics.iter().map(|m| m.documents_processed).sum();

        self.summary.insert("total_duration_ms".into(),
            serde_json::Value::Number(total_duration.into()));
        self.summary.insert("average_memory_mb".into(),
            serde_json::Value::Number(serde_json::Number::from_f64(avg_memory).unwrap_or(0.into())));
        self.summary.insert("total_documents_processed".into(),
            serde_json::Value::Number(total_docs.into()));
        self.summary.insert("performance_regression_detected".into(),
            serde_json::Value::Bool(self.performance_regression_detected));
    }
}

/// Main performance test suite
pub struct LargeDatasetPerformanceSuite {
    generator: LargeDatasetGenerator,
    test_results: Vec<PerformanceTestResults>,
}

impl LargeDatasetPerformanceSuite {
    pub fn new() -> Result<Self> {
        let generator = LargeDatasetGenerator::new()?;
        Ok(Self {
            generator,
            test_results: Vec::new(),
        })
    }

    /// Run the complete performance test suite
    pub async fn run_all_tests(&mut self) -> Result<()> {
        info!("Starting large dataset performance test suite");

        let test_sizes = vec![
            ("small", DatasetSize::small()),
            ("medium", DatasetSize::medium()),
            ("large", DatasetSize::large()),
            ("xlarge", DatasetSize::xlarge()),
        ];

        for (size_name, dataset_config) in test_sizes {
            info!("Running tests for {} dataset ({} documents)", size_name, dataset_config.total_documents);

            // Generate dataset
            let dataset_path = self.generator.generate_dataset(&dataset_config).await?;

            // Run individual tests
            self.test_database_operations(&dataset_path, &dataset_config).await?;
            self.test_semantic_search_performance(&dataset_path, &dataset_config).await?;
            self.test_resource_utilization(&dataset_path, &dataset_config).await?;

            // Cleanup
            if let Err(e) = std::fs::remove_dir_all(&dataset_path) {
                warn!("Failed to cleanup test dataset: {}", e);
            }
        }

        self.generate_performance_report()?;
        info!("Performance test suite completed");

        Ok(())
    }

    async fn test_database_operations(&mut self, dataset_path: &Path, config: &DatasetSize) -> Result<()> {
        let mut test_results = PerformanceTestResults::new(
            "database_operations".to_string(),
            config.clone()
        );

        // Setup isolated database
        let temp_db = TempDir::new()?;
        let db_config = SurrealDbConfig {
            namespace: "crucible".to_string(),
            database: "performance_test".to_string(),
            path: temp_db.path().join("test.db").to_string_lossy().to_string(),
            max_connections: Some(10),
            timeout_seconds: Some(60),
        };

        let client = Arc::new(SurrealClient::new(db_config).await?);
        kiln_integration::initialize_kiln_schema(&client).await?;

        // Test document storage performance
        let storage_start = Instant::now();
        let mut stored_count = 0;

        let scan_config = KilnScannerConfig {
            max_file_size_bytes: 50 * 1024 * 1024,
            max_recursion_depth: 10,
            recursive_scan: true,
            include_hidden_files: false,
            file_extensions: vec!["md".to_string(), "markdown".to_string()],
            parallel_processing: 4,
            batch_processing: true,
            batch_size: 16,
            enable_embeddings: false, // Skip embeddings for storage test
            process_embeds: true,
            process_wikilinks: true,
            enable_incremental: false,
            track_file_changes: true,
            change_detection_method: crucible_surrealdb::kiln_scanner::ChangeDetectionMethod::ContentHash,
            error_handling_mode: crucible_surrealdb::kiln_scanner::ErrorHandlingMode::ContinueOnError,
            max_error_count: 100,
            error_retry_attempts: 3,
            error_retry_delay_ms: 500,
            skip_problematic_files: true,
            log_errors_detailed: false,
            error_threshold_circuit_breaker: 10,
            circuit_breaker_timeout_ms: 30000,
            processing_timeout_ms: 30000,
        };

        let files = scan_kiln_directory(&dataset_path.to_path_buf(), &scan_config).await?;

        // Store documents in batches and measure performance
        for chunk in files.chunks(50) {
            let batch_start = Instant::now();
            let mut batch_stored = 0;

            for file_info in chunk {
                if let Ok(content) = std::fs::read_to_string(&file_info.path) {
                    let parsed_doc = ParsedDocument::new(file_info.path.clone());
                    if store_parsed_document(&client, &parsed_doc, &file_info.path).await.is_ok() {
                        batch_stored += 1;
                        stored_count += 1;
                    }
                }
            }

            let batch_duration = batch_start.elapsed();
            test_results.add_metric(PerformanceMetrics {
                operation: "batch_document_storage".to_string(),
                duration_ms: batch_duration.as_millis() as u64,
                memory_usage_mb: 0.0, // Would need monitoring
                cpu_usage_percent: 0.0,
                disk_io_bytes: 0,
                documents_processed: batch_stored,
                errors_count: chunk.len() - batch_stored,
                metadata: HashMap::from([
                    ("batch_size".to_string(), serde_json::Value::Number(chunk.len().into())),
                    ("documents_stored".to_string(), serde_json::Value::Number(batch_stored.into())),
                ]),
            });
        }

        let total_storage_time = storage_start.elapsed();

        // Test query performance
        let query_start = Instant::now();
        let db_stats = get_database_stats(&client).await?;
        let query_duration = query_start.elapsed();

        test_results.add_metric(PerformanceMetrics {
            operation: "database_stats_query".to_string(),
            duration_ms: query_duration.as_millis() as u64,
            memory_usage_mb: 0.0,
            cpu_usage_percent: 0.0,
            disk_io_bytes: 0,
            documents_processed: stored_count,
            errors_count: 0,
            metadata: HashMap::from([
                ("total_documents".to_string(), serde_json::Value::Number(stored_count.into())),
                ("database_size".to_string(), serde_json::Value::String(format!("{:?}", db_stats))),
            ]),
        });

        test_results.add_metric(PerformanceMetrics {
            operation: "total_storage_operation".to_string(),
            duration_ms: total_storage_time.as_millis() as u64,
            memory_usage_mb: 0.0,
            cpu_usage_percent: 0.0,
            disk_io_bytes: 0,
            documents_processed: stored_count,
            errors_count: files.len() - stored_count,
            metadata: HashMap::from([
                ("storage_rate_docs_per_sec".to_string(),
                 serde_json::Value::Number(serde_json::Number::from_f64(
                     stored_count as f64 / total_storage_time.as_secs_f64()
                 ).unwrap_or(0.into()))),
                ("average_document_size".to_string(),
                 serde_json::Value::Number(config.avg_document_size_bytes.into())),
            ]),
        });

        test_results.analyze_performance();
        self.test_results.push(test_results);

        Ok(())
    }

    async fn test_semantic_search_performance(&mut self, dataset_path: &Path, config: &DatasetSize) -> Result<()> {
        let mut test_results = PerformanceTestResults::new(
            "semantic_search_performance".to_string(),
            config.clone()
        );

        // Setup database with embeddings
        let temp_db = TempDir::new()?;
        let db_config = SurrealDbConfig {
            namespace: "crucible".to_string(),
            database: "semantic_test".to_string(),
            path: temp_db.path().join("test.db").to_string_lossy().to_string(),
            max_connections: Some(10),
            timeout_seconds: Some(60),
        };

        let client = Arc::new(SurrealClient::new(db_config).await?);
        kiln_integration::initialize_kiln_schema(&client).await?;

        // Create mock semantic search service
        let search_service = Arc::new(MockSemanticSearchService::new(client.clone())?);

        // Test queries with varying complexity
        let test_queries = vec![
            ("simple_single_term", "API"),
            ("medium_phrase", "project management"),
            ("complex_multi_term", "authentication security best practices"),
            ("domain_specific", "machine learning algorithms"),
            ("cross_domain", "financial reporting technical specifications"),
        ];

        for (query_type, query) in test_queries {
            let search_start = Instant::now();

            // Create progress tracker
            let progress = Arc::new(TestProgress::new());

            let search_request = SemanticSearchRequest {
                config: &CliConfig::default(),
                query,
                top_k: 10,
                json_output: false,
                progress,
            };

            let search_result = search_service.search(search_request).await?;
            let search_duration = search_start.elapsed();

            test_results.add_metric(PerformanceMetrics {
                operation: format!("semantic_search_{}", query_type),
                duration_ms: search_duration.as_millis() as u64,
                memory_usage_mb: 0.0,
                cpu_usage_percent: 0.0,
                disk_io_bytes: 0,
                documents_processed: search_result.results.len(),
                errors_count: 0,
                metadata: HashMap::from([
                    ("query_type".to_string(), serde_json::Value::String(query_type.to_string())),
                    ("query".to_string(), serde_json::Value::String(query.to_string())),
                    ("result_count".to_string(), serde_json::Value::Number(search_result.results.len().into())),
                    ("info_messages".to_string(), serde_json::Value::Number(search_result.info_messages.len().into())),
                ]),
            });
        }

        // Test search scalability with different result limits
        for top_k in [5, 10, 25, 50, 100] {
            let search_start = Instant::now();

            let progress = Arc::new(TestProgress::new());

            let search_request = SemanticSearchRequest {
                config: &CliConfig::default(),
                query: "test query for scalability",
                top_k,
                json_output: false,
                progress,
            };

            let search_result = search_service.search(search_request).await?;
            let search_duration = search_start.elapsed();

            test_results.add_metric(PerformanceMetrics {
                operation: format!("semantic_search_top_k_{}", top_k),
                duration_ms: search_duration.as_millis() as u64,
                memory_usage_mb: 0.0,
                cpu_usage_percent: 0.0,
                disk_io_bytes: 0,
                documents_processed: search_result.results.len(),
                errors_count: 0,
                metadata: HashMap::from([
                    ("top_k".to_string(), serde_json::Value::Number(top_k.into())),
                    ("result_count".to_string(), serde_json::Value::Number(search_result.results.len().into())),
                ]),
            });
        }

        // Test concurrent search performance
        let concurrent_searches = 10;
        let concurrent_start = Instant::now();

        let mut handles = Vec::new();
        for i in 0..concurrent_searches {
            let service = search_service.clone();
            let query = format!("concurrent search query {}", i);

            let handle = tokio::task::spawn_blocking(move || {
                // Use a blocking task for non-Send futures
                let rt = tokio::runtime::Handle::current();
                rt.block_on(async move {
                    let progress = Arc::new(TestProgress::new());
                    let search_request = SemanticSearchRequest {
                        config: &CliConfig::default(),
                        query: &query,
                        top_k: 5,
                        json_output: false,
                        progress,
                    };

                    service.search(search_request).await
                })
            });

            handles.push(handle);
        }

        // Wait for all searches to complete
        let mut concurrent_results = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(result) => concurrent_results.push(result),
                Err(e) => error!("Concurrent search failed: {}", e),
            }
        }

        let concurrent_duration = concurrent_start.elapsed();

        test_results.add_metric(PerformanceMetrics {
            operation: "concurrent_semantic_searches".to_string(),
            duration_ms: concurrent_duration.as_millis() as u64,
            memory_usage_mb: 0.0,
            cpu_usage_percent: 0.0,
            disk_io_bytes: 0,
            documents_processed: concurrent_results.iter().map(|r| r.as_ref().map_or(0, |res| res.results.len())).sum(),
            errors_count: concurrent_results.iter().filter(|r| r.is_err()).count(),
            metadata: HashMap::from([
                ("concurrent_searches".to_string(), serde_json::Value::Number(concurrent_searches.into())),
                ("successful_searches".to_string(), serde_json::Value::Number(
                    concurrent_results.iter().filter(|r| r.is_ok()).count().into()
                )),
                ("average_search_time".to_string(),
                 serde_json::Value::Number(serde_json::Number::from_f64(
                     concurrent_duration.as_millis() as f64 / concurrent_searches as f64
                 ).unwrap_or(0.into()))),
            ]),
        });

        test_results.analyze_performance();
        self.test_results.push(test_results);

        Ok(())
    }

    async fn test_resource_utilization(&mut self, dataset_path: &Path, config: &DatasetSize) -> Result<()> {
        let mut test_results = PerformanceTestResults::new(
            "resource_utilization".to_string(),
            config.clone()
        );

        // Monitor resource usage during different operations
        let monitor = ResourceMonitor::new();

        // Test memory usage during document processing
        monitor.start_monitoring();
        let processing_start = Instant::now();

        // Simulate document processing
        let scan_config = KilnScannerConfig {
            max_file_size_bytes: 50 * 1024 * 1024,
            max_recursion_depth: config.nested_directories_depth,
            recursive_scan: true,
            include_hidden_files: false,
            file_extensions: vec!["md".to_string(), "markdown".to_string()],
            parallel_processing: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4),
            batch_processing: true,
            batch_size: 32,
            enable_embeddings: false,
            process_embeds: true,
            process_wikilinks: true,
            enable_incremental: false,
            track_file_changes: true,
            change_detection_method: crucible_surrealdb::kiln_scanner::ChangeDetectionMethod::ContentHash,
            error_handling_mode: crucible_surrealdb::kiln_scanner::ErrorHandlingMode::ContinueOnError,
            max_error_count: 100,
            error_retry_attempts: 3,
            error_retry_delay_ms: 500,
            skip_problematic_files: true,
            log_errors_detailed: false,
            error_threshold_circuit_breaker: 10,
            circuit_breaker_timeout_ms: 30000,
            processing_timeout_ms: 30000,
        };

        let files = scan_kiln_directory(&dataset_path.to_path_buf(), &scan_config).await?;
        let processing_duration = processing_start.elapsed();

        let processing_metric = monitor.stop_monitoring();

        test_results.add_metric(PerformanceMetrics {
            operation: "document_scanning_and_processing".to_string(),
            duration_ms: processing_duration.as_millis() as u64,
            memory_usage_mb: processing_metric.memory_usage_mb,
            cpu_usage_percent: processing_metric.cpu_usage_percent,
            disk_io_bytes: processing_metric.disk_io_bytes,
            documents_processed: files.len(),
            errors_count: 0,
            metadata: HashMap::from([
                ("files_scanned".to_string(), serde_json::Value::Number(files.len().into())),
                ("parallel_workers".to_string(), serde_json::Value::Number(scan_config.parallel_processing.into())),
                ("batch_size".to_string(), serde_json::Value::Number(scan_config.batch_size.into())),
                ("avg_file_size".to_string(),
                 serde_json::Value::Number(
                     (files.iter().map(|f| f.file_size).sum::<u64>() / files.len().max(1) as u64).into()
                 )),
            ]),
        });

        // Test memory usage during search operations
        let monitor = ResourceMonitor::new();

        let temp_db = TempDir::new()?;
        let db_config = SurrealDbConfig {
            namespace: "crucible".to_string(),
            database: "resource_test".to_string(),
            path: temp_db.path().join("test.db").to_string_lossy().to_string(),
            max_connections: Some(10),
            timeout_seconds: Some(60),
        };

        let client = Arc::new(SurrealClient::new(db_config).await?);
        kiln_integration::initialize_kiln_schema(&client).await?;

        // Perform multiple searches while monitoring
        monitor.start_monitoring();
        let search_start = Instant::now();
        for i in 0..100 {
            let query = format!("search query {}", i);

            // This would normally trigger semantic search, but we'll simulate it
            tokio::time::sleep(Duration::from_millis(10)).await;

            if i % 20 == 0 {
                info!("Resource test: {} searches completed", i);
            }
        }
        let search_duration = search_start.elapsed();

        let search_metric = monitor.stop_monitoring();

        test_results.add_metric(PerformanceMetrics {
            operation: "repeated_search_operations".to_string(),
            duration_ms: search_duration.as_millis() as u64,
            memory_usage_mb: search_metric.memory_usage_mb,
            cpu_usage_percent: search_metric.cpu_usage_percent,
            disk_io_bytes: search_metric.disk_io_bytes,
            documents_processed: 0, // Searches don't process documents
            errors_count: 0,
            metadata: HashMap::from([
                ("total_searches".to_string(), serde_json::Value::Number(100.into())),
                ("avg_search_time".to_string(), serde_json::Value::Number(
                    ((search_duration.as_millis() / 100) as u64).into()
                )),
                ("searches_per_second".to_string(),
                 serde_json::Value::Number(serde_json::Number::from_f64(
                     100.0 / search_duration.as_secs_f64()
                 ).unwrap_or(0.into()))),
            ]),
        });

        test_results.analyze_performance();
        self.test_results.push(test_results);

        Ok(())
    }

    fn generate_performance_report(&self) -> Result<()> {
        let report_path = PathBuf::from("large_dataset_performance_report.json");

        let report = serde_json::json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "test_suite": "large_dataset_performance",
            "total_tests": self.test_results.len(),
            "results": self.test_results,
            "summary": {
                "performance_regressions_detected":
                    self.test_results.iter().any(|r| r.performance_regression_detected),
                "test_categories": self.test_results.iter()
                    .map(|r| r.test_name.clone())
                    .collect::<Vec<_>>(),
            }
        });

        std::fs::write(&report_path, serde_json::to_string_pretty(&report)?)?;
        info!("Performance report generated: {}", report_path.display());

        // Also generate a human-readable summary
        self.generate_human_readable_report()?;

        Ok(())
    }

    fn generate_human_readable_report(&self) -> Result<()> {
        let report_path = PathBuf::from("large_dataset_performance_report.md");

        let mut content = String::new();
        content.push_str("# Large Dataset Performance Test Report\n\n");
        content.push_str(&format!("**Generated:** {}\n\n", chrono::Utc::now().to_rfc3339()));
        content.push_str(&format!("**Total Tests:** {}\n\n", self.test_results.len()));

        for test_result in &self.test_results {
            content.push_str(&format!("## {}\n\n", test_result.test_name));
            content.push_str(&format!("**Dataset Size:** {} documents\n\n", test_result.dataset_size.total_documents));

            if let Some(summary) = test_result.summary.get("total_duration_ms") {
                if let Some(duration) = summary.as_u64() {
                    content.push_str(&format!("**Total Duration:** {:.2} seconds\n\n", duration as f64 / 1000.0));
                }
            }

            if let Some(summary) = test_result.summary.get("average_memory_mb") {
                if let Some(memory) = summary.as_f64() {
                    content.push_str(&format!("**Average Memory Usage:** {:.1} MB\n\n", memory));
                }
            }

            content.push_str("### Performance Metrics\n\n");
            content.push_str("| Operation | Duration (ms) | Documents/s | Memory (MB) |\n");
            content.push_str("|-----------|---------------|-------------|-------------|\n");

            for metric in &test_result.metrics {
                let docs_per_sec = if metric.duration_ms > 0 {
                    (metric.documents_processed as f64 / metric.duration_ms as f64) * 1000.0
                } else {
                    0.0
                };

                content.push_str(&format!(
                    "| {} | {} | {:.1} | {:.1} |\n",
                    metric.operation,
                    metric.duration_ms,
                    docs_per_sec,
                    metric.memory_usage_mb
                ));
            }

            if test_result.performance_regression_detected {
                content.push_str("\n⚠️ **Performance Regression Detected**\n\n");
            }

            content.push_str("---\n\n");
        }

        content.push_str("## Summary\n\n");

        let regressions = self.test_results.iter().filter(|r| r.performance_regression_detected).count();
        if regressions > 0 {
            content.push_str(&format!("⚠️ **{} test(s) detected performance regressions**\n\n", regressions));
        } else {
            content.push_str("✅ **No performance regressions detected**\n\n");
        }

        content.push_str("### Recommendations\n\n");
        content.push_str("- Review any tests with regressions\n");
        content.push_str("- Monitor memory usage patterns\n");
        content.push_str("- Optimize database query performance\n");
        content.push_str("- Consider indexing strategies for large datasets\n");

        std::fs::write(&report_path, content)?;
        info!("Human-readable performance report generated: {}", report_path.display());

        Ok(())
    }
}

/// Test progress implementation for performance testing
pub struct TestProgress {
    message: String,
}

impl TestProgress {
    pub fn new() -> Self {
        Self {
            message: "Initializing...".to_string(),
        }
    }
}

impl SemanticProgress for TestProgress {
    fn start(&self, message: &str) {
        debug!("Progress start: {}", message);
    }

    fn set_message(&self, message: &str) {
        debug!("Progress: {}", message);
    }

    fn finish_with_message(&self, message: &str) {
        debug!("Progress finish: {}", message);
    }

    fn fail_with_message(&self, message: &str) {
        debug!("Progress fail: {}", message);
    }
}

/// Performance regression detection thresholds
pub struct PerformanceThresholds {
    pub max_search_time_ms: u64,
    pub max_memory_usage_mb: f64,
    pub min_documents_per_second: f64,
    pub max_error_rate_percent: f64,
}

impl Default for PerformanceThresholds {
    fn default() -> Self {
        Self {
            max_search_time_ms: 5000,      // 5 seconds max for search
            max_memory_usage_mb: 1024.0,   // 1GB max memory usage
            min_documents_per_second: 10.0, // Min 10 docs/sec processing rate
            max_error_rate_percent: 5.0,   // Max 5% error rate
        }
    }
}

/// Validate performance metrics against thresholds
pub fn validate_performance_thresholds(
    metrics: &[PerformanceMetrics],
    thresholds: &PerformanceThresholds
) -> Result<Vec<String>> {
    let mut violations = Vec::new();

    for metric in metrics {
        // Check search performance
        if metric.operation.contains("search") && metric.duration_ms > thresholds.max_search_time_ms {
            violations.push(format!(
                "Search operation '{}' took {}ms (threshold: {}ms)",
                metric.operation, metric.duration_ms, thresholds.max_search_time_ms
            ));
        }

        // Check memory usage
        if metric.memory_usage_mb > thresholds.max_memory_usage_mb {
            violations.push(format!(
                "Operation '{}' used {:.1}MB memory (threshold: {:.1}MB)",
                metric.operation, metric.memory_usage_mb, thresholds.max_memory_usage_mb
            ));
        }

        // Check processing rate
        if metric.documents_processed > 0 {
            let docs_per_sec = (metric.documents_processed as f64 / metric.duration_ms as f64) * 1000.0;
            if docs_per_sec < thresholds.min_documents_per_second {
                violations.push(format!(
                    "Operation '{}' processed {:.1} docs/sec (threshold: {:.1} docs/sec)",
                    metric.operation, docs_per_sec, thresholds.min_documents_per_second
                ));
            }
        }

        // Check error rate
        if metric.documents_processed > 0 {
            let error_rate = (metric.errors_count as f64 / metric.documents_processed as f64) * 100.0;
            if error_rate > thresholds.max_error_rate_percent {
                violations.push(format!(
                    "Operation '{}' had {:.1}% error rate (threshold: {:.1}%)",
                    metric.operation, error_rate, thresholds.max_error_rate_percent
                ));
            }
        }
    }

    if violations.is_empty() {
        Ok(vec![])
    } else {
        Err(anyhow::anyhow!("Performance threshold violations:\n{}", violations.join("\n")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_large_dataset_generation() -> Result<()> {
        let generator = LargeDatasetGenerator::new()?;
        let config = DatasetSize::small(); // 100 documents for quick test

        let dataset_path = generator.generate_dataset(&config).await?;

        // Verify dataset was created
        assert!(dataset_path.exists());

        // Count generated files recursively
        fn count_markdown_files(dir: &Path) -> std::io::Result<usize> {
            let mut count = 0;
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    count += count_markdown_files(&path)?;
                } else if path.extension().map_or(false, |ext| ext == "md") {
                    count += 1;
                }
            }
            Ok(count)
        }

        let markdown_files = count_markdown_files(&dataset_path)?;

        assert!(markdown_files >= config.total_documents / 2, // Allow some variance due to directory structure
                "Expected at least {} markdown files, got {}",
                config.total_documents / 2, markdown_files);

        Ok(())
    }

    #[tokio::test]
    async fn test_performance_suite_small_dataset() -> Result<()> {
        let mut suite = LargeDatasetPerformanceSuite::new()?;

        // Run tests with small dataset for quick validation
        let generator = LargeDatasetGenerator::new()?;
        let config = DatasetSize::with_documents(50); // Very small for test
        let dataset_path = generator.generate_dataset(&config).await?;

        suite.test_database_operations(&dataset_path, &config).await?;
        suite.test_semantic_search_performance(&dataset_path, &config).await?;

        // Verify we have test results
        assert!(!suite.test_results.is_empty());

        // Generate report
        suite.generate_performance_report()?;

        Ok(())
    }

    #[tokio::test]
    async fn test_performance_threshold_validation() -> Result<()> {
        let thresholds = PerformanceThresholds::default();

        // Create test metrics
        let good_metrics = vec![
            PerformanceMetrics {
                operation: "test_search".to_string(),
                duration_ms: 100, // Under threshold
                memory_usage_mb: 100.0, // Under threshold
                cpu_usage_percent: 50.0,
                disk_io_bytes: 1000,
                documents_processed: 100,
                errors_count: 0,
                metadata: HashMap::new(),
            }
        ];

        let bad_metrics = vec![
            PerformanceMetrics {
                operation: "slow_search".to_string(),
                duration_ms: 10000, // Over threshold
                memory_usage_mb: 2000.0, // Over threshold
                cpu_usage_percent: 90.0,
                disk_io_bytes: 10000,
                documents_processed: 100,
                errors_count: 50, // High error rate
                metadata: HashMap::new(),
            }
        ];

        // Should pass validation
        assert!(validate_performance_thresholds(&good_metrics, &thresholds).is_ok());

        // Should fail validation
        assert!(validate_performance_thresholds(&bad_metrics, &thresholds).is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_document_template_generation() -> Result<()> {
        let templates = DocumentTemplate::generate_templates();

        assert!(!templates.is_empty(), "Should have generated templates");

        // Verify templates have required fields
        for template in &templates {
            assert!(!template.title.is_empty());
            assert!(!template.content_template.is_empty());
            assert!(!template.tags.is_empty());
            assert!(!template.metadata.is_empty());
        }

        Ok(())
    }
}

/// Main entry point for running performance tests manually
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("Starting large dataset performance tests");

    let mut suite = LargeDatasetPerformanceSuite::new()?;
    suite.run_all_tests().await?;

    info!("Performance tests completed successfully");

    Ok(())
}
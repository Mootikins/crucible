# Crucible Examples and Tutorials

> **Status**: Active Examples
> **Version**: 1.0.0
> **Date**: 2025-10-20
> **Purpose**: Practical examples and tutorials for the new service architecture

## Table of Contents

- [Getting Started](#getting-started)
- [Basic Examples](#basic-examples)
- [Advanced Examples](#advanced-examples)
- [Tutorials](#tutorials)
- [Tool Creation Examples](#tool-creation-examples)
- [Service Integration Examples](#service-integration-examples)

## Getting Started

### Installation and Setup

#### Prerequisites
```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Install Node.js (for frontend development)
curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
sudo apt install nodejs

# Install pnpm
npm install -g pnpm
```

#### Project Setup
```bash
# Clone repository
git clone https://github.com/matthewkrohn/crucible.git
cd crucible

# Install dependencies
pnpm install

# Run development setup
./scripts/setup.sh

# Build all crates
cargo build
```

#### First Application
```rust
// main.rs
use crucible_services::ServiceRegistry;
use crucible_tools::ToolRegistry;
use crucible_config::ConfigManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration
    let config_manager = ConfigManager::new();
    let config = config_manager.load().await?;

    // Initialize services
    let mut services = ServiceRegistry::new(config.services.clone());
    services.start_all().await?;

    // Initialize tools
    let mut tools = ToolRegistry::new();
    tools.register_tools();

    // Create a search tool example
    let query = "example search query";
    let search_result = services.execute_tool("search", json!({
        "query": query,
        "limit": 10
    })).await?;

    println!("Search results: {:?}", search_result);
    Ok(())
}
```

## Basic Examples

### Example 1: Basic Service Usage

#### Creating and Using Services
```rust
// services_example.rs
use crucible_services::{ServiceRegistry, SearchService, SearchOptions};
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create service registry
    let mut registry = ServiceRegistry::new();

    // Register search service
    registry.register("search", SearchService::new().await?);

    // Start all services
    registry.start_all().await?;

    // Use search service
    if let Some(search) = registry.get::<SearchService>("search") {
        let results = search.search(
            "Rust programming",
            SearchOptions {
                limit: Some(5),
                ..Default::default()
            }
        ).await?;

        println!("Found {} results", results.total);
        for result in results.results {
            println!("- {}", result.title);
        }
    }

    Ok(())
}
```

#### Configuration Example
```yaml
# config/services.yaml
services:
  search:
    enabled: true
    type: "crucible_services::SearchService"
    config:
      index_path: "./indexes"
      max_results: 100

  agent:
    enabled: true
    type: "crucible_services::AgentService"
    config:
      model: "gpt-3.5-turbo"
      max_tokens: 2000

tools:
  static:
    - name: "search"
      module: "crucible_tools::search"
      function: "search_notes"
    - name: "metadata"
      module: "crucible_tools::metadata"
      function: "extract_metadata"

  dynamic:
    - name: "custom_tool"
      path: "./tools/custom_tool.rn"
      hot_reload: true
```

### Example 2: Static Tool Creation

#### Simple Static Tool
```rust
// tools/simple_tool.rs
use crucible_rune_macros::rune_tool;
use crate::ToolResult;

#[rune_tool(
    desc = "Add two numbers",
    category = "math",
    tags = ["arithmetic", "calculation"]
)]
pub fn add_numbers(a: i32, b: i32) -> ToolResult<i32> {
    ToolResult::Success(a + b)
}

#[rune_tool(
    desc = "Calculate factorial",
    category = "math",
    tags = ["math", "factorial"]
)]
pub fn factorial(n: u64) -> ToolResult<u64> {
    if n > 20 {
        return ToolResult::Error("Input too large".to_string());
    }

    let mut result = 1;
    for i in 1..=n {
        result *= i;
    }

    ToolResult::Success(result)
}

// Tool result type
pub enum ToolResult<T> {
    Success(T),
    Error(String),
}

impl<T> ToolResult<T> {
    pub fn unwrap(self) -> T {
        match self {
            ToolResult::Success(val) => val,
            ToolResult::Error(msg) => panic!("Tool error: {}", msg),
        }
    }
}
```

#### Advanced Static Tool
```rust
// tools/document_tool.rs
use crucible_rune_macros::rune_tool;
use serde_json::Value;
use std::path::Path;

#[rune_tool(
    desc = "Extract document metadata",
    category: "document",
    tags: ["metadata", "parsing"]
)]
pub fn extract_document_metadata(
    path: String,
    extract_frontmatter: Option<bool>,
) -> ToolResult<Value> {
    let extract_frontmatter = extract_frontmatter.unwrap_or(true);

    if !Path::new(&path).exists() {
        return ToolResult::Error(format!("File not found: {}", path));
    }

    // Read file content
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    let mut metadata = Value::Object(serde_json::Map::new());
    metadata["path"] = Value::String(path);
    metadata["size"] = Value::Number(content.len().into());

    // Extract word count
    let words: Vec<&str> = content.split_whitespace().collect();
    metadata["word_count"] = Value::Number(words.len().into());

    // Extract frontmatter if requested
    if extract_frontmatter {
        if let Ok(fm) = extract_yaml_frontmatter(&content) {
            metadata["frontmatter"] = Value::Object(fm);
        }
    }

    ToolResult::Success(metadata)
}

fn extract_yaml_frontmatter(content: &str) -> Result<serde_json::Map<String, Value>, String> {
    // Simple frontmatter extraction
    let lines: Vec<&str> = content.lines().collect();
    let mut in_frontmatter = false;
    let mut frontmatter_lines = Vec::new();

    for line in lines {
        if line.trim() == "---" && !in_frontmatter {
            in_frontmatter = true;
            continue;
        } else if line.trim() == "---" && in_frontmatter {
            break;
        } else if in_frontmatter {
            frontmatter_lines.push(line);
        }
    }

    if frontmatter_lines.is_empty() {
        return Ok(serde_json::Map::new());
    }

    // Parse YAML frontmatter
    let frontmatter_text = frontmatter_lines.join("\n");
    let value: Value = serde_yaml::from_str(&frontmatter_text)
        .map_err(|e| format!("Failed to parse frontmatter: {}", e))?;

    if let Value::Object(obj) = value {
        Ok(obj)
    } else {
        Ok(serde_json::Map::new())
    }
}
```

### Example 3: Dynamic Tool with Rune

#### Simple Dynamic Tool
```rune
// tools/count_words.rn
/// Count words in text
pub fn count_words(text: string) -> map {
    let words = text.split_whitespace();
    {
        "word_count": len(words),
        "character_count": len(text),
        "line_count": text.split("\n").len(),
        "status": "success"
    }
}

/// Calculate reading time
pub fn reading_time(text: string, words_per_minute: int?) -> map {
    let wpm = words_per_minute.unwrap_or(200);
    let word_count = count_words(text).word_count;
    let minutes = (word_count as float / wpm as float).ceil();

    {
        "minutes": minutes,
        "word_count": word_count,
        "words_per_minute": wpm,
        "status": "success"
    }
}
```

#### Advanced Dynamic Tool
```rune
// tools/analysis_tool.rn
/// Analyze document content
pub fn analyze_document(content: string, options: map?) -> map {
    let default_options = {
        "min_word_length": 3,
        "stop_words": ["the", "a", "an", "and", "or", "but"],
        "max_results": 20
    };

    let opts = options.unwrap_or(default_options);
    let words = content.to_lowercase().split_whitespace();

    // Filter out stop words and short words
    let filtered = words.filter(fn(word) {
        let word = word.trim();
        word.len() >= opts.min_word_length and
        !opts.stop_words.contains(word) and
        word != "" and
        !word.is_numeric()
    });

    // Count word frequency
    let word_count = {};
    for word in filtered {
        word_count[word] = word_count.get(word, 0) + 1;
    }

    // Sort by frequency
    let sorted = word_count.entries().sort(fn(a, b) {
        b.1 > a.1
    }).slice(0, opts.max_results);

    // Calculate reading statistics
    let total_words = words.len();
    let unique_words = word_count.len();
    let vocabulary_richness = unique_words as float / total_words as float;

    {
        "total_words": total_words,
        "unique_words": unique_words,
        "vocabulary_richness": vocabulary_richness,
        "common_words": sorted,
        "status": "success",
        "analysis": {
            "complexity": if vocabulary_richness > 0.5 {
                "complex"
            } else if vocabulary_richness > 0.3 {
                "moderate"
            } else {
                "simple"
            }
        }
    }
}
```

## Advanced Examples

### Example 1: Custom Service Creation

#### Custom Search Service
```rust
// services/custom_search.rs
use crucible_services::{Service, ServiceRegistry};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomSearchConfig {
    pub engine: String,
    pub api_key: Option<String>,
    pub timeout: Duration,
    pub max_retries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub score: f64,
}

pub struct CustomSearchService {
    config: CustomSearchConfig,
    client: reqwest::Client,
}

impl CustomSearchService {
    pub fn new(config: CustomSearchConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .expect("Failed to create HTTP client");

        Self { config, client }
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>, String> {
        match self.config.engine.as_str() {
            "google" => self.google_search(query, limit).await,
            "bing" => self.bing_search(query, limit).await,
            "duckduckgo" => self.duckduckgo_search(query, limit).await,
            _ => Err(format!("Unknown search engine: {}", self.config.engine)),
        }
    }

    async fn google_search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>, String> {
        let api_key = self.config.api_key.as_ref()
            .ok_or("Google API key not provided")?;

        let url = format!(
            "https://www.googleapis.com/customsearch/v1?key={}&cx=017576662512468239146:omuauf_lfve&q={}",
            api_key, query
        );

        let response = self.client.get(&url).send().await
            .map_err(|e| format!("Failed to make request: {}", e))?;

        let data: serde_json::Value = response.json().await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        let mut results = Vec::new();
        if let Some(items) = data["items"].as_array() {
            for (i, item) in items.iter().take(limit).enumerate() {
                if let (Some(title), Some(link), Some(snippet)) = (
                    item["title"].as_str(),
                    item["link"].as_str(),
                    item["snippet"].as_str(),
                ) {
                    results.push(SearchResult {
                        title: title.to_string(),
                        url: link.to_string(),
                        snippet: snippet.to_string(),
                        score: (limit - i) as f64 / limit as f64,
                    });
                }
            }
        }

        Ok(results)
    }
}

#[async_trait]
impl Service for CustomSearchService {
    async fn start(&mut self) -> Result<(), String> {
        // Initialize service if needed
        tracing::info!("Starting custom search service with engine: {}", self.config.engine);
        Ok(())
    }

    async fn stop(&self) -> Result<(), String> {
        // Cleanup service if needed
        tracing::info!("Stopping custom search service");
        Ok(())
    }

    fn is_running(&self) -> bool {
        true
    }

    fn status(&self) -> crucible_services::ServiceStatus {
        crucible_services::ServiceStatus::Running
    }

    fn info(&self) -> crucible_services::ServiceInfo {
        crucible_services::ServiceInfo {
            name: "custom_search".to_string(),
            version: "1.0.0".to_string(),
            description: "Custom search service with multiple engine support".to_string(),
            dependencies: vec!["http".to_string()],
            metrics: crucible_services::ServiceMetrics::default(),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
```

#### Service Registry Integration
```rust
// main.rs
use crucible_services::ServiceRegistry;
use services::custom_search::{CustomSearchService, CustomSearchConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create custom search service
    let search_config = CustomSearchConfig {
        engine: "google".to_string(),
        api_key: std::env::var("GOOGLE_API_KEY").ok(),
        timeout: Duration::from_secs(10),
        max_retries: 3,
    };

    let mut search_service = CustomSearchService::new(search_config);

    // Create service registry
    let mut registry = ServiceRegistry::new();
    registry.register("custom_search", search_service);

    // Start all services
    registry.start_all().await?;

    // Use the service
    if let Some(search) = registry.get::<CustomSearchService>("custom_search") {
        let results = search.search("Rust programming tutorials", 5).await?;

        println!("Search results:");
        for result in results {
            println!("- {}: {}", result.title, result.url);
        }
    }

    Ok(())
}
```

### Example 2: Custom Tool with Macros

#### Advanced Tool with Schema Validation
```rust
// tools/advanced_tool.rs
use crucible_rune_macros::rune_tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AdvancedSearchOptions {
    /// Search query
    pub query: String,

    /// Maximum number of results
    #[schemars(default = "default_limit")]
    pub limit: Option<usize>,

    /// Fields to search in
    pub fields: Option<Vec<String>>,

    /// Search filters
    pub filters: Option<SearchFilters>,

    /// Sort options
    pub sort: Option<SortOptions>,

    #[schemars(default = "default_timeout")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SearchFilters {
    /// Date range filter
    pub date_range: Option<DateRange>,

    /// Tag filter
    pub tags: Option<Vec<String>>,

    /// Author filter
    pub authors: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DateRange {
    pub start: Option<String>,
    pub end: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SortOptions {
    pub field: String,
    pub direction: SortDirection,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum SortDirection {
    Asc,
    Desc,
}

fn default_limit() -> usize { 10 }
fn default_timeout() -> u64 { 5000 }

#[rune_tool(
    desc = "Advanced search with multiple options",
    category = "search",
    tags = ["advanced", "search", "filters"]
)]
pub fn advanced_search(
    options: AdvancedSearchOptions,
) -> ToolResult<serde_json::Value> {
    // Validate query
    if options.query.trim().is_empty() {
        return ToolResult::Error("Query cannot be empty".to_string());
    }

    // Validate timeout
    let timeout = std::time::Duration::from_millis(options.timeout_ms.unwrap_or(5000));

    // Execute search (simulated)
    let results = simulate_search(options).await.map_err(|e| {
        format!("Search failed: {}", e)
    })?;

    ToolResult::Success(serde_json::json!({
        "query": options.query,
        "results": results,
        "total": results.len(),
        "options": options
    }))
}

async fn simulate_search(options: AdvancedSearchOptions) -> Result<Vec<serde_json::Value>, String> {
    // Simulate search delay
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Generate mock results
    let mut results = Vec::new();
    for i in 0..options.limit.unwrap_or(10) {
        results.push(serde_json::json!({
            "id": format!("doc_{}", i),
            "title": format!("Result {} for '{}'", i + 1, options.query),
            "snippet": format!("This is a snippet for result {} containing the search terms.", i + 1),
            "score": 1.0 - (i as f64 * 0.1),
            "date": "2025-01-01",
            "tags": vec!["example", "test", "search"],
        }));
    }

    Ok(results)
}
```

#### Tool Registry Integration
```rust
// tools/mod.rs
pub mod advanced_tool;
pub mod simple_tool;

use crucible_tools::ToolRegistry;
use advanced_tool::advanced_search;
use simple_tool::{add_numbers, factorial};

pub fn register_tools(registry: &mut ToolRegistry) {
    registry.register("advanced_search", advanced_search);
    registry.register("add_numbers", add_numbers);
    registry.register("factorial", factorial);
}
```

## Tutorials

### Tutorial 1: Building a Search Tool

#### Step 1: Define the Tool Interface
```rust
// tutorials/search_tool/step1.rs
use crucible_rune_macros::rune_tool;
use crate::ToolResult;

#[rune_tool(
    desc = "Search documents by content",
    category = "search",
    tags = ["content", "text", "find"]
)]
pub fn search_documents(query: String) -> ToolResult<Vec<String>> {
    // Basic search implementation
    let mut results = Vec::new();

    // Search in current directory (simplified)
    if std::path::Path::new(&query).exists() {
        results.push(query.clone());
    }

    ToolResult::Success(results)
}
```

#### Step 2: Add Configuration
```rust
// tutorials/search_tool/step2.rs
use crucible_rune_macros::rune_tool;
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SearchConfig {
    /// Search directory
    pub directory: String,

    /// Include hidden files
    #[schemars(default)]
    pub include_hidden: bool,

    /// Maximum results
    #[schemars(default = "default_max_results")]
    pub max_results: usize,
}

fn default_max_results() -> usize { 100 }

#[rune_tool(
    desc = "Search documents in a directory",
    category = "search",
    tags = ["directory", "file", "content"]
)]
pub fn search_in_directory(
    query: String,
    config: Option<SearchConfig>,
) -> ToolResult<Vec<String>> {
    let config = config.unwrap_or(SearchConfig {
        directory: ".".to_string(),
        include_hidden: false,
        max_results: 100,
    });

    // Validate directory
    if !std::path::Path::new(&config.directory).exists() {
        return ToolResult::Error(format!("Directory not found: {}", config.directory));
    }

    // Search implementation
    let results = perform_search(&query, &config);

    ToolResult::Success(results)
}

fn perform_search(query: &str, config: &SearchConfig) -> Vec<String> {
    let mut results = Vec::new();

    // Walk through directory
    if let Ok(entries) = std::fs::read_dir(&config.directory) {
        for entry in entries.flatten() {
            let path = entry.path();

            // Skip hidden files if not included
            if !config.include_hidden && path.file_name().map_or(false, |name| name.to_string_lossy().starts_with('.')) {
                continue;
            }

            // Check if file contains query
            if let Ok(content) = std::fs::read_to_string(&path) {
                if content.contains(query) {
                    if let Some(path_str) = path.to_str() {
                        results.push(path_str.to_string());
                    }
                }
            }

            // Limit results
            if results.len() >= config.max_results {
                break;
            }
        }
    }

    results
}
```

#### Step 3: Add Advanced Features
```rust
// tutorials/search_tool/step3.rs
use crucible_rune_macros::rune_tool;
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AdvancedSearchOptions {
    /// Search query
    pub query: String,

    /// Search directory
    pub directory: String,

    /// File pattern to match
    pub file_pattern: Option<String>,

    /// Include hidden files
    #[schemars(default)]
    pub include_hidden: bool,

    /// Case sensitive search
    #[schemars(default)]
    pub case_sensitive: bool,

    /// Maximum results
    #[schemars(default = "default_max_results")]
    pub max_results: usize,

    /// File content only
    #[schemars(default)]
    pub content_only: bool,

    /// Metadata search
    pub search_metadata: Option<bool>,
}

fn default_max_results() -> usize { 100 }

#[rune_tool(
    desc = "Advanced document search with filters",
    category = "search",
    tags = ["advanced", "document", "find", "filter"]
)]
pub fn advanced_search(
    options: AdvancedSearchOptions,
) -> ToolResult<serde_json::Value> {
    // Validate options
    if !std::path::Path::new(&options.directory).exists() {
        return ToolResult::Error(format!("Directory not found: {}", options.directory));
    }

    if options.query.trim().is_empty() {
        return ToolResult::Error("Query cannot be empty".to_string());
    }

    // Perform search
    let results = perform_advanced_search(&options);

    ToolResult::Success(serde_json::json!({
        "query": options.query,
        "directory": options.directory,
        "results": results,
        "total": results.len(),
        "options": options
    }))
}

fn perform_advanced_search(options: &AdvancedSearchOptions) -> Vec<serde_json::Value> {
    let mut results = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&options.directory) {
        for entry in entries.flatten() {
            let path = entry.path();

            // Filter by file pattern
            if let Some(pattern) = &options.file_pattern {
                if !path.file_name()
                    .and_then(|name| name.to_str())
                    .map_or(false, |name| name.contains(pattern))
                {
                    continue;
                }
            }

            // Skip hidden files
            if !options.include_hidden && path.file_name().map_or(false, |name| name.to_string_lossy().starts_with('.')) {
                continue;
            }

            // Check file type
            if let Some(file_type) = path.extension() {
                let file_type_str = file_type.to_string_lossy().to_lowercase();
                if matches!(file_type_str.as_str(), "txt" | "md" | "rs" | "py" | "js" | "ts" | "json" | "yaml" | "yml") {
                    // Search in file content
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        let search_query = if options.case_sensitive {
                            options.query.clone()
                        } else {
                            options.query.to_lowercase()
                        };

                        let search_content = if options.case_sensitive {
                            content.clone()
                        } else {
                            content.to_lowercase()
                        };

                        if search_content.contains(&search_query) {
                            if let Some(path_str) = path.to_str() {
                                let mut result = serde_json::json!({
                                    "path": path_str,
                                    "size": std::fs::metadata(&path).unwrap_or_default().len(),
                                    "modified": std::fs::metadata(&path).and_then(|m| m.modified()).ok().map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()),
                                });

                                // Add content snippet
                                if !options.content_only {
                                    let lines: Vec<&str> = content.lines().collect();
                                    let snippet = find_snippet(&lines, &options.query);
                                    result["snippet"] = serde_json::Value::String(snippet);
                                }

                                results.push(result);
                            }
                        }
                    }
                }
            }

            if results.len() >= options.max_results {
                break;
            }
        }
    }

    results
}

fn find_snippet(lines: &[&str], query: &str) -> String {
    for line in lines {
        if line.to_lowercase().contains(&query.to_lowercase()) {
            let snippet = line.trim();
            let max_len = 100;
            if snippet.len() > max_len {
                format!("...{}...", &snippet[..max_len/2], &snippet[snippet.len()-max_len/2..])
            } else {
                snippet.to_string()
            }
        }
    }
    "No snippet found".to_string()
}
```

### Tutorial 2: Building an Analysis Tool

#### Step 1: Basic Analysis
```rust
// tutorials/analysis_tool/step1.rs
use crucible_rune_macros::rune_tool;
use serde_json::Value;

#[rune_tool(
    desc = "Analyze text content",
    category = "analysis",
    tags = ["text", "analysis", "stats"]
)]
pub fn analyze_text(text: String) -> ToolResult<Value> {
    if text.trim().is_empty() {
        return ToolResult::Error("Text cannot be empty".to_string());
    }

    let words: Vec<&str> = text.split_whitespace().collect();
    let characters = text.len();
    let lines: Vec<&str> = text.lines().collect();

    let analysis = Value::Object(serde_json::Map::from([
        ("character_count".to_string(), Value::Number(characters.into())),
        ("word_count".to_string(), Value::Number(words.len().into())),
        ("line_count".to_string(), Value::Number(lines.len().into())),
        ("average_words_per_line".to_string(), Value::Number(
            if lines.is_empty() { 0.0 } else { words.len() as f64 / lines.len() as f64 }
        )),
    ]));

    ToolResult::Success(analysis)
}
```

#### Step 2: Advanced Analysis
```rust
// tutorials/analysis_tool/step2.rs
use crucible_rune_macros::rune_tool;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisOptions {
    pub text: String,

    /// Calculate word frequency
    #[serde(default)]
    pub word_frequency: bool,

    /// Calculate reading time
    #[serde(default)]
    pub reading_time: bool,

    /// Language detection
    #[serde(default)]
    pub detect_language: bool,

    /// Sentiment analysis
    #[serde(default)]
    pub sentiment_analysis: bool,

    /// Minimum word length for frequency
    #[serde(default = "default_min_word_length")]
    pub min_word_length: usize,
}

fn default_min_word_length() -> usize { 3 }

#[rune_tool(
    desc = "Advanced text analysis with multiple metrics",
    category = "analysis",
    tags = ["text", "analysis", "statistics", "frequency"]
)]
pub fn advanced_text_analysis(options: AnalysisOptions) -> ToolResult<Value> {
    if options.text.trim().is_empty() {
        return ToolResult::Error("Text cannot be empty".to_string());
    }

    let mut result = serde_json::Map::new();

    // Basic statistics
    let words: Vec<&str> = options.text.split_whitespace().collect();
    let characters = options.text.len();
    let lines: Vec<&str> = options.text.lines().collect();

    result.insert("basic_stats".to_string(), Value::Object(serde_json::Map::from([
        ("character_count".to_string(), Value::Number(characters.into())),
        ("word_count".to_string(), Value::Number(words.len().into())),
        ("line_count".to_string(), Value::Number(lines.len().into())),
        ("average_words_per_line".to_string(), Value::Number(
            if lines.is_empty() { 0.0 } else { words.len() as f64 / lines.len() as f64 }
        )),
    ])));

    // Word frequency
    if options.word_frequency {
        let frequency = calculate_word_frequency(&options.text, options.min_word_length);
        result.insert("word_frequency".to_string(), Value::Array(
            frequency.into_iter().map(|(word, count)| {
                Value::Object(serde_json::Map::from([
                    ("word".to_string(), Value::String(word)),
                    ("count".to_string(), Value::Number(count.into())),
                ]))
            }).collect()
        ));
    }

    // Reading time
    if options.reading_time {
        let reading_time = calculate_reading_time(&words);
        result.insert("reading_time".to_string(), reading_time);
    }

    // Language detection (simplified)
    if options.detect_language {
        let language = detect_language(&options.text);
        result.insert("detected_language".to_string(), Value::String(language));
    }

    // Sentiment analysis (simplified)
    if options.sentiment_analysis {
        let sentiment = analyze_sentiment(&options.text);
        result.insert("sentiment".to_string(), sentiment);
    }

    ToolResult::Success(Value::Object(result))
}

fn calculate_word_frequency(text: &str, min_length: usize) -> Vec<(String, usize)> {
    let mut frequency = std::collections::HashMap::new();

    for word in text.split_whitespace() {
        let clean_word = word.trim().chars().filter(|c| c.is_alphanumeric()).collect::<String>();
        if clean_word.len() >= min_length {
            *frequency.entry(clean_word.to_lowercase()).or_insert(0) += 1;
        }
    }

    let mut frequency: Vec<_> = frequency.into_iter().collect();
    frequency.sort_by(|a, b| b.1.cmp(&a.1));

    frequency
}

fn calculate_reading_time(words: &[&str]) -> Value {
    let avg_words_per_minute = 200.0;
    let minutes = words.len() as f64 / avg_words_per_minute;
    let seconds = (minutes * 60.0) % 60.0;

    Value::Object(serde_json::Map::from([
        ("minutes".to_string(), Value::Number(minutes.floor().into())),
        ("seconds".to_string(), Value::Number(seconds.round().into())),
        ("total_seconds".to_string(), Value::Number((minutes * 60.0).round().into())),
    ]))
}

fn detect_language(text: &str) -> String {
    // Simplified language detection
    let text_lower = text.to_lowercase();

    if text_lower.contains("the ") && text_lower.contains("and ") && text_lower.contains("is ") {
        "English".to_string()
    } else if text_lower.contains("le ") && text_lower.contains("et ") && text_lower.contains("est ") {
        "French".to_string()
    } else if text_lower.contains("el ") && text_lower.contains("y ") && text_lower.contains("es ") {
        "Spanish".to_string()
    } else if text_lower.contains("der ") && text_lower.contains("und ") && text_lower.contains("ist ") {
        "German".to_string()
    } else {
        "Unknown".to_string()
    }
}

fn analyze_sentiment(text: &str) -> Value {
    // Simplified sentiment analysis
    let positive_words = ["good", "great", "excellent", "amazing", "wonderful", "fantastic", "awesome"];
    let negative_words = ["bad", "terrible", "awful", "horrible", "poor", "worst", "sad"];

    let words: Vec<&str> = text.split_whitespace().collect();
    let positive_count = words.iter().filter(|word| positive_words.contains(&word.to_lowercase().as_str())).count();
    let negative_count = words.iter().filter(|word| negative_words.contains(&word.to_lowercase().as_str())).count();

    let sentiment = match (positive_count, negative_count) {
        (_, n) if n > positive_count => "negative",
        (p, _) if p > negative_count => "positive",
        _ => "neutral",
    };

    Value::Object(serde_json::Map::from([
        ("sentiment".to_string(), Value::String(sentiment.to_string())),
        ("positive_words".to_string(), Value::Number(positive_count.into())),
        ("negative_words".to_string(), Value::Number(negative_count.into())),
    ]))
}
```

## Tool Creation Examples

### Example 1: Data Processing Tool

#### CSV Processing Tool
```rust
// tools/csv_processor.rs
use crucible_rune_macros::rune_tool;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, Read};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CSVProcessingOptions {
    pub data: String,

    /// Delimiter character
    #[serde(default = "default_delimiter")]
    pub delimiter: char,

    /// Has header row
    #[serde(default)]
    pub has_header: bool,

    /// Columns to select
    pub columns: Option<Vec<String>>,

    /// Filter rows
    pub filter: Option<Vec<FilterCondition>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterCondition {
    pub column: String,
    pub operator: FilterOperator,
    pub value: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterOperator {
    Equals,
    NotEquals,
    GreaterThan,
    LessThan,
    Contains,
    StartsWith,
    EndsWith,
}

fn default_delimiter() -> char { ',' }

#[rune_tool(
    desc = "Process CSV data with filtering and transformation",
    category = "data",
    tags = ["csv", "processing", "filter", "transform"]
)]
pub fn process_csv(options: CSVProcessingOptions) -> ToolResult<Value> {
    if options.data.trim().is_empty() {
        return ToolResult::Error("CSV data cannot be empty".to_string());
    }

    // Parse CSV
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(options.delimiter as u8)
        .from_reader(options.data.as_bytes());

    // Read header if present
    let headers: Vec<String> = if options.has_header {
        reader.headers().map(|h| h.iter().map(|s| s.to_string()).collect())
            .unwrap_or_else(|_| vec![])
    } else {
        (0..reader.headers().map(|h| h.len()).unwrap_or(0))
            .map(|i| format!("column_{}", i + 1))
            .collect()
    };

    // Process rows
    let mut processed_rows = Vec::new();
    let mut record = csv::StringRecord::new();

    while reader.read_record(&mut record).unwrap_or(false) {
        let mut row = serde_json::Map::new();

        // Add all columns
        for (i, header) in headers.iter().enumerate() {
            if let Some(value) = record.get(i) {
                row.insert(header.clone(), Value::String(value.to_string()));
            }
        }

        // Filter rows
        if let Some(filters) = &options.filter {
            if !row_matches_filters(&row, filters) {
                continue;
            }
        }

        // Select columns
        if let Some(columns) = &options.columns {
            let mut selected_row = serde_json::Map::new();
            for col in columns {
                if let Some(value) = row.get(col) {
                    selected_row.insert(col.clone(), value.clone());
                }
            }
            processed_rows.push(Value::Object(selected_row));
        } else {
            processed_rows.push(Value::Object(row));
        }
    }

    ToolResult::Success(Value::Object(serde_json::Map::from([
        ("headers".to_string(), Value::Array(
            headers.into_iter().map(Value::String).collect()
        )),
        ("rows".to_string(), Value::Array(processed_rows)),
        ("total_rows".to_string(), Value::Number(processed_rows.len().into())),
    ])))
}

fn row_matches_filters(row: &serde_json::Map<String, Value>, filters: &[FilterCondition]) -> bool {
    for condition in filters {
        if let Some(value) = row.get(&condition.column) {
            if !condition_matches(value, &condition.operator, &condition.value) {
                return false;
            }
        } else {
            return false;
        }
    }
    true
}

fn condition_matches(actual: &Value, operator: &FilterOperator, expected: &Value) -> bool {
    match operator {
        FilterOperator::Equals => actual == expected,
        FilterOperator::NotEquals => actual != expected,
        FilterOperator::GreaterThan => {
            if let (Some(a), Some(b)) = (actual.as_f64(), expected.as_f64()) {
                a > b
            } else if let (Some(a), Some(b)) = (actual.as_u64(), expected.as_u64()) {
                a > b
            } else {
                false
            }
        }
        FilterOperator::LessThan => {
            if let (Some(a), Some(b)) = (actual.as_f64(), expected.as_f64()) {
                a < b
            } else if let (Some(a), Some(b)) = (actual.as_u64(), expected.as_u64()) {
                a < b
            } else {
                false
            }
        }
        FilterOperator::Contains => {
            if let (Some(a), Some(b)) = (actual.as_str(), expected.as_str()) {
                a.contains(b)
            } else {
                false
            }
        }
        FilterOperator::StartsWith => {
            if let (Some(a), Some(b)) = (actual.as_str(), expected.as_str()) {
                a.starts_with(b)
            } else {
                false
            }
        }
        FilterOperator::EndsWith => {
            if let (Some(a), Some(b)) = (actual.as_str(), expected.as_str()) {
                a.ends_with(b)
            } else {
                false
            }
        }
    }
}
```

### Example 2: Web API Tool

#### REST API Tool
```rust
// tools/api_tool.rs
use crucible_rune_macros::rune_tool;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct APIRequest {
    pub url: String,

    /// HTTP method
    #[serde(default = "default_method")]
    pub method: String,

    /// Headers
    pub headers: Option<Vec<Header>>,

    /// Request body
    pub body: Option<Value>,

    /// Timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Header {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct APIResponse {
    pub status_code: u16,
    pub headers: Vec<Header>,
    pub body: Value,
    pub duration_ms: u64,
}

fn default_method() -> String { "GET".to_string() }
fn default_timeout() -> u64 { 30 }

#[rune_tool(
    desc = "Make HTTP requests to REST APIs",
    category = "web",
    tags = ["http", "api", "rest", "request"]
)]
pub async fn make_api_request(request: APIRequest) -> ToolResult<APIResponse> {
    let client = reqwest::Client::new();
    let mut req = match request.method.to_uppercase().as_str() {
        "GET" => client.get(&request.url),
        "POST" => client.post(&request.url),
        "PUT" => client.put(&request.url),
        "DELETE" => client.delete(&request.url),
        "PATCH" => client.patch(&request.url),
        _ => return ToolResult::Error(format!("Unsupported method: {}", request.method)),
    };

    // Add headers
    if let Some(headers) = request.headers {
        for header in headers {
            req = req.header(&header.name, &header.value);
        }
    }

    // Add body
    if let Some(body) = request.body {
        req = req.json(&body);
    }

    // Set timeout
    req = req.timeout(std::time::Duration::from_secs(request.timeout));

    // Make request
    let start = std::time::Instant::now();
    let response = req.send().await;
    let duration = start.elapsed().as_millis() as u64;

    match response {
        Ok(resp) => {
            let status_code = resp.status().as_u16();

            // Read response body
            let body_text = resp.text().await;
            let body = match body_text {
                Ok(text) => {
                    if let Ok(json) = serde_json::from_str(&text) {
                        json
                    } else {
                        Value::String(text)
                    }
                }
                Err(_) => Value::String("Failed to read response body".to_string()),
            };

            let mut response_headers = Vec::new();
            for (name, value) in resp.headers().iter() {
                if let Ok(value_str) = value.to_str() {
                    response_headers.push(Header {
                        name: name.to_string(),
                        value: value_str.to_string(),
                    });
                }
            }

            ToolResult::Success(APIResponse {
                status_code,
                headers: response_headers,
                body,
                duration_ms: duration,
            })
        }
        Err(e) => ToolResult::Error(format!("Request failed: {}", e)),
    }
}
```

## Service Integration Examples

### Example 1: Service Composition

#### Combined Search Service
```rust
// services/combined_search.rs
use crucible_services::{Service, ServiceRegistry};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombinedSearchConfig {
    pub local_search: LocalSearchConfig,
    pub web_search: WebSearchConfig,
    pub fusion_strategy: FusionStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalSearchConfig {
    pub enabled: bool,
    pub index_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchConfig {
    pub enabled: bool,
    pub engine: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FusionStrategy {
    RankBased,
    ScoreBased,
    Hybrid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub source: String, // "local" or "web"
    pub score: f64,
    pub timestamp: u64,
}

pub struct CombinedSearchService {
    config: CombinedSearchConfig,
    local_search: Option<LocalSearchService>,
    web_search: Option<WebSearchService>,
    registry: ServiceRegistry,
}

impl CombinedSearchService {
    pub fn new(config: CombinedSearchConfig, registry: ServiceRegistry) -> Self {
        let local_search = if config.local_search.enabled {
            Some(LocalSearchService::new(config.local_search.clone()))
        } else {
            None
        };

        let web_search = if config.web_search.enabled {
            Some(WebSearchService::new(config.web_search.clone()))
        } else {
            None
        };

        Self {
            config,
            local_search,
            web_search,
            registry,
        }
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>, String> {
        let start = Instant::now();
        let mut all_results = Vec::new();

        // Search locally
        if let Some(local) = &self.local_search {
            match local.search(query).await {
                Ok(results) => {
                    for mut result in results {
                        result.source = "local".to_string();
                        all_results.push(result);
                    }
                }
                Err(e) => {
                    tracing::warn!("Local search failed: {}", e);
                }
            }
        }

        // Search web
        if let Some(web) = &self.web_search {
            match web.search(query).await {
                Ok(results) => {
                    for mut result in results {
                        result.source = "web".to_string();
                        all_results.push(result);
                    }
                }
                Err(e) => {
                    tracing::warn!("Web search failed: {}", e);
                }
            }
        }

        // Fuse results based on strategy
        let fused_results = self.fuse_results(all_results, limit);

        tracing::info!("Combined search completed in {:?}", start.elapsed());
        Ok(fused_results)
    }

    fn fuse_results(&self, mut results: Vec<SearchResult>, limit: usize) -> Vec<SearchResult> {
        match self.config.fusion_strategy {
            FusionStrategy::RankBased => {
                // Simple rank-based fusion
                results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
                results.truncate(limit);
                results
            }
            FusionStrategy::ScoreBased => {
                // Normalize scores and combine
                let local_results: Vec<_> = results.iter()
                    .filter(|r| r.source == "local")
                    .collect();
                let web_results: Vec<_> = results.iter()
                    .filter(|r| r.source == "web")
                    .collect();

                let max_local = local_results.iter().map(|r| r.score).max().unwrap_or(1.0);
                let max_web = web_results.iter().map(|r| r.score).max().unwrap_or(1.0);

                let mut fused = Vec::new();
                for result in &results {
                    let normalized_score = match result.source.as_str() {
                        "local" => result.score / max_local,
                        "web" => result.score / max_web,
                        _ => result.score,
                    };

                    fused.push(SearchResult {
                        title: result.title.clone(),
                        url: result.url.clone(),
                        snippet: result.snippet.clone(),
                        source: result.source.clone(),
                        score: normalized_score,
                        timestamp: result.timestamp,
                    });
                }

                fused.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
                fused.truncate(limit);
                fused
            }
            FusionStrategy::Hybrid => {
                // Combine rank and source diversity
                let mut local_results: Vec<_> = results.iter()
                    .filter(|r| r.source == "local")
                    .cloned()
                    .collect();
                let mut web_results: Vec<_> = results.iter()
                    .filter(|r| r.source == "web")
                    .cloned()
                    .collect();

                local_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
                web_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

                let mut fused = Vec::new();
                let mut local_idx = 0;
                let mut web_idx = 0;

                while fused.len() < limit && (local_idx < local_results.len() || web_idx < web_results.len()) {
                    if local_idx < local_results.len() && (fused.len() % 2 == 0 || web_idx >= web_results.len()) {
                        fused.push(local_results[local_idx].clone());
                        local_idx += 1;
                    } else if web_idx < web_results.len() {
                        fused.push(web_results[web_idx].clone());
                        web_idx += 1;
                    }
                }

                fused
            }
        }
    }
}

#[async_trait]
impl Service for CombinedSearchService {
    async fn start(&mut self) -> Result<(), String> {
        tracing::info!("Starting combined search service");
        Ok(())
    }

    async fn stop(&self) -> Result<(), String> {
        tracing::info!("Stopping combined search service");
        Ok(())
    }

    fn is_running(&self) -> bool {
        true
    }

    fn status(&self) -> crucible_services::ServiceStatus {
        crucible_services::ServiceStatus::Running
    }

    fn info(&self) -> crucible_services::ServiceInfo {
        crucible_services::ServiceInfo {
            name: "combined_search".to_string(),
            version: "1.0.0".to_string(),
            description: "Combined local and web search service".to_string(),
            dependencies: vec!["local_search".to_string(), "web_search".to_string()],
            metrics: crucible_services::ServiceMetrics::default(),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
```

#### Service Registration and Usage
```rust
// main.rs
use crucible_services::ServiceRegistry;
use services::combined_search::{CombinedSearchService, CombinedSearchConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create service registry
    let mut registry = ServiceRegistry::new();

    // Register combined search service
    let search_config = CombinedSearchConfig {
        local_search: LocalSearchConfig {
            enabled: true,
            index_path: "./indexes".to_string(),
        },
        web_search: WebSearchConfig {
            enabled: true,
            engine: "google".to_string(),
            api_key: std::env::var("GOOGLE_API_KEY").ok(),
        },
        fusion_strategy: FusionStrategy::Hybrid,
    };

    let combined_search = CombinedSearchService::new(search_config, registry.clone());
    registry.register("combined_search", combined_search);

    // Start services
    registry.start_all().await?;

    // Use the combined search service
    if let Some(search) = registry.get::<CombinedSearchService>("combined_search") {
        let results = search.search("Rust programming tutorials", 10).await?;

        println!("Combined search results:");
        for result in results {
            println!("[{}] {}: {}", result.source, result.title, result.url);
        }
    }

    Ok(())
}
```

---

*This examples and tutorials document will be updated as new features are added to the Crucible system. Check for the latest version in the documentation repository.*
//! Kiln Repository - Phase 1B Implementation
//!
//! This module provides the kiln repository pattern using the Phase 1A parsing system.
//! It orchestrates `KilnScanner` and `KilnParser` to provide search and query operations
//! over kiln data.

use crate::kiln_parser::KilnParser;
use crate::kiln_scanner::KilnScanner;
use crate::kiln_types::{KilnError, KilnFile, KilnResult};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Kiln repository using Phase 1A parsing system
#[derive(Debug, Clone)]
pub struct KilnRepository {
    /// Path to the kiln directory
    kiln_path: Arc<String>,
    /// Scanner for discovering markdown files
    scanner: Arc<KilnScanner>,
    /// Parser for extracting metadata
    parser: Arc<KilnParser>,
    /// Cache for parsed files to avoid re-parsing
    file_cache: Arc<RwLock<HashMap<String, KilnFile>>>,
    /// Cache for tag statistics
    tag_cache: Arc<RwLock<HashMap<String, usize>>>,
}

impl KilnRepository {
    /// Create new kiln operations with explicit path
    ///
    /// Note: This function requires an explicit path to avoid hardcoded defaults.
    /// Callers should obtain the path from configuration or pass it explicitly.
    #[must_use] 
    pub fn new(kiln_path: &str) -> Self {
        let kiln_path = Arc::new(kiln_path.to_string());
        let scanner = Arc::new(KilnScanner::new(&kiln_path));
        let parser = Arc::new(KilnParser::new());

        Self {
            kiln_path,
            scanner,
            parser,
            file_cache: Arc::new(RwLock::new(HashMap::new())),
            tag_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create new kiln repository from global execution context
    ///
    /// This reads the `kiln_path` from the global `TOOL_EXECUTION_CONTEXT` set by
    /// `CrucibleToolManager`. Returns an error if no context is set or no path is configured.
    pub fn from_context() -> KilnResult<Self> {
        use crate::types::get_kiln_path_from_context;

        let kiln_path =
            get_kiln_path_from_context().map_err(|e| KilnError::InvalidPath(e.to_string()))?;

        Ok(Self::new(kiln_path.to_str().ok_or_else(|| {
            KilnError::InvalidPath("Invalid UTF-8 in kiln path".to_string())
        })?))
    }

    /// Get the kiln path
    #[must_use] 
    pub fn kiln_path(&self) -> &str {
        &self.kiln_path
    }

    /// Scan and parse all markdown files in the kiln
    pub async fn scan_and_parse_kiln(&self) -> KilnResult<Vec<KilnFile>> {
        let start_time = std::time::Instant::now();
        info!("Starting kiln scan and parse for: {}", self.kiln_path);

        // Check if kiln exists
        if !self.scanner.kiln_exists().await {
            return Err(KilnError::FileNotFound(format!(
                "Kiln path does not exist: {}",
                self.kiln_path
            )));
        }

        // Scan for markdown files
        let markdown_files = self.scanner.scan_markdown_files().await?;
        debug!("Found {} markdown files", markdown_files.len());

        // Parse each file
        let mut kiln_files = Vec::new();
        let mut cache = self.file_cache.write().await;

        for relative_path in markdown_files {
            let absolute_path = self.scanner.get_absolute_path(&relative_path);
            let path_str = absolute_path.to_string_lossy().to_string();

            match self.parser.parse_file(&path_str).await {
                Ok(kiln_file) => {
                    // Cache the parsed file
                    cache.insert(path_str.clone(), kiln_file.clone());
                    kiln_files.push(kiln_file);
                }
                Err(e) => {
                    warn!("Failed to parse file {}: {}", path_str, e);
                    // Continue with other files
                }
            }
        }

        let duration = start_time.elapsed();
        info!(
            "Successfully parsed {} files in {:?}",
            kiln_files.len(),
            duration
        );

        // Update tag cache
        self.update_tag_cache(&kiln_files).await;

        Ok(kiln_files)
    }

    /// Search files by frontmatter properties
    pub async fn search_by_properties(&self, properties: Value) -> KilnResult<Vec<Value>> {
        info!("Searching by properties: {:?}", properties);

        let kiln_files = self.scan_and_parse_kiln().await?;
        let mut matching_files = Vec::new();

        // Convert search properties to HashMap for easier comparison
        let search_props: HashMap<String, Value> = if let Value::Object(map) = properties {
            map.into_iter().collect()
        } else {
            HashMap::new()
        };

        for kiln_file in kiln_files {
            let mut matches_all = true;

            // Check each search property against file's frontmatter
            for (key, search_value) in &search_props {
                if let Some(file_value) = kiln_file.metadata.frontmatter.get(key) {
                    if !self.values_match(search_value, file_value) {
                        matches_all = false;
                        break;
                    }
                } else {
                    matches_all = false;
                    break;
                }
            }

            if matches_all {
                matching_files.push(self.kiln_file_to_json(&kiln_file));
            }
        }

        info!("Found {} files matching properties", matching_files.len());
        Ok(matching_files)
    }

    /// Search files by tags
    pub async fn search_by_tags(&self, search_tags: Vec<String>) -> KilnResult<Vec<Value>> {
        info!("Searching by tags: {:?}", search_tags);

        let kiln_files = self.scan_and_parse_kiln().await?;
        let mut matching_files = Vec::new();

        for kiln_file in kiln_files {
            let file_tags = kiln_file.get_tags();

            // Check if file has ALL the search tags (AND logic)
            let has_all_tags = search_tags.iter().all(|search_tag| {
                file_tags
                    .iter()
                    .any(|file_tag| self.tags_match(search_tag, file_tag))
            });

            if has_all_tags {
                matching_files.push(self.kiln_file_to_json(&kiln_file));
            }
        }

        info!("Found {} files matching tags", matching_files.len());
        Ok(matching_files)
    }

    /// Search files in a specific folder
    pub async fn search_by_folder(
        &self,
        folder_path: &str,
        recursive: bool,
    ) -> KilnResult<Vec<Value>> {
        info!(
            "Searching in folder: {} (recursive: {})",
            folder_path, recursive
        );

        let files = if recursive {
            self.scanner.scan_markdown_files().await?
        } else {
            self.scanner.scan_markdown_files_non_recursive().await?
        };

        let mut matching_files = Vec::new();

        for relative_path in files {
            let path_str = relative_path.to_string_lossy();

            // Check if file is in the requested folder
            if path_str.starts_with(folder_path) {
                let absolute_path = self.scanner.get_absolute_path(&relative_path);
                let path_str = absolute_path.to_string_lossy().to_string();

                // Get file metadata without parsing full content
                if let Ok(metadata) = std::fs::metadata(&path_str) {
                    let modified_time = match metadata.modified() {
                        Ok(system_time) => {
                            let dt = chrono::DateTime::<chrono::Utc>::from(system_time);
                            dt.to_rfc3339()
                        }
                        Err(_) => "unknown".to_string(),
                    };

                    let file_info = json!({
                        "path": path_str,
                        "name": relative_path.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown"),
                        "size": metadata.len(),
                        "modified": modified_time
                    });
                    matching_files.push(file_info);
                }
            }
        }

        info!("Found {} files in folder", matching_files.len());
        Ok(matching_files)
    }

    /// Calculate real kiln statistics
    pub async fn get_kiln_stats(&self) -> KilnResult<Value> {
        info!("Calculating kiln statistics");

        let kiln_files = self.scan_and_parse_kiln().await?;

        // Calculate statistics
        let total_notes = kiln_files.len();
        let total_size_bytes: u64 = kiln_files.iter().map(|f| f.metadata.size).sum();
        let total_size_mb = total_size_bytes as f64 / (1024.0 * 1024.0);

        // Count unique folders
        let mut folders = HashSet::new();
        for file in &kiln_files {
            if let Some(parent) = file.path.parent() {
                folders.insert(parent.to_string_lossy().to_string());
            }
        }

        // Count unique tags
        let tag_counts = self.get_tag_counts().await;
        let total_tags = tag_counts.len();

        // Get last indexed time
        let last_indexed = chrono::Utc::now().to_rfc3339();

        let stats = json!({
            "total_notes": total_notes,
            "total_size_mb": total_size_mb,
            "folders": folders.len(),
            "tags": total_tags,
            "last_indexed": last_indexed,
            "kiln_type": "obsidian",
            "kiln_path": self.kiln_path.as_str(),
            "file_count_by_extension": {
                "md": total_notes
            }
        });

        info!(
            "Kiln stats: {} notes, {:.2} MB, {} folders, {} tags",
            total_notes,
            total_size_mb,
            folders.len(),
            total_tags
        );

        Ok(stats)
    }

    /// List all tags in the kiln with counts
    pub async fn list_tags(&self) -> KilnResult<Value> {
        info!("Listing all kiln tags");

        // Ensure kiln is scanned and tag cache is populated
        self.scan_and_parse_kiln().await?;

        let tag_counts = self.get_tag_counts().await;
        let mut tags = Vec::new();

        for (tag, count) in tag_counts {
            // Determine category based on tag content
            let category = if tag.contains("project") || tag.contains("work") {
                "work"
            } else if tag.contains("ai") || tag.contains("tech") || tag.contains("code") {
                "technology"
            } else if tag.contains("config") || tag.contains("meta") {
                "meta"
            } else {
                "general"
            };

            tags.push(json!({
                "name": tag,
                "count": count,
                "category": category
            }));
        }

        // Sort by count (descending)
        tags.sort_by(|a, b| {
            b.get("count")
                .unwrap()
                .as_u64()
                .unwrap()
                .cmp(&a.get("count").unwrap().as_u64().unwrap())
        });

        let result = json!({
            "tags": tags,
            "total_tags": tags.len()
        });

        info!("Found {} unique tags", tags.len());
        Ok(result)
    }

    /// Get cached tag counts
    async fn get_tag_counts(&self) -> HashMap<String, usize> {
        let cache = self.tag_cache.read().await;
        cache.clone()
    }

    /// Update tag cache from kiln files
    async fn update_tag_cache(&self, kiln_files: &[KilnFile]) {
        let mut tag_counts = HashMap::new();

        for file in kiln_files {
            let tags = file.get_tags();
            for tag in tags {
                *tag_counts.entry(tag).or_insert(0) += 1;
            }
        }

        let mut cache = self.tag_cache.write().await;
        *cache = tag_counts;
    }

    /// Check if two tag values match (case-insensitive, fuzzy matching)
    fn tags_match(&self, search_tag: &str, file_tag: &str) -> bool {
        let search_lower = search_tag.to_lowercase();
        let file_lower = file_tag.to_lowercase();

        // Exact match
        if search_lower == file_lower {
            return true;
        }

        // Partial match (search tag is contained in file tag)
        if file_lower.contains(&search_lower) || search_lower.contains(&file_lower) {
            return true;
        }

        // Hyphen/underscore normalization
        let search_normalized = search_lower.replace(['-', '_'], " ");
        let file_normalized = file_lower.replace(['-', '_'], " ");

        search_normalized == file_normalized
    }

    /// Check if two JSON values match
    fn values_match(&self, search_value: &Value, file_value: &Value) -> bool {
        match (search_value, file_value) {
            (Value::String(search), Value::String(file)) => {
                search.to_lowercase() == file.to_lowercase()
            }
            (Value::Array(search_arr), Value::Array(file_arr)) => {
                // Check if search array elements are all present in file array
                search_arr.iter().all(|search_elem| {
                    file_arr
                        .iter()
                        .any(|file_elem| self.values_match(search_elem, file_elem))
                })
            }
            (Value::Number(search), Value::Number(file)) => search == file,
            (Value::Bool(search), Value::Bool(file)) => search == file,
            _ => false,
        }
    }

    /// Convert `KilnFile` to JSON format
    fn kiln_file_to_json(&self, kiln_file: &KilnFile) -> Value {
        json!({
            "path": kiln_file.path.to_string_lossy(),
            "name": kiln_file.get_title(),
            "folder": kiln_file.path.parent()
                .and_then(|p| p.to_str())
                .unwrap_or(""),
            "properties": kiln_file.metadata.frontmatter,
            "tags": kiln_file.get_tags(),
            "type": kiln_file.get_type(),
            "status": kiln_file.get_status(),
            "created": kiln_file.metadata.created.map_or_else(|| "unknown".to_string(), |dt| dt.to_rfc3339()),
            "modified": kiln_file.metadata.modified.to_rfc3339(),
            "size": kiln_file.metadata.size,
            "hash": kiln_file.hash
        })
    }
}

// Default implementation removed - callers must provide explicit path.
// Use KilnRepository::new(path) instead.

// Tests removed - these were testing against hardcoded user vault paths.
// Proper tests should use TempDir with isolated test environments.

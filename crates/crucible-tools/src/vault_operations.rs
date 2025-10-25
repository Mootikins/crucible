//! Real Vault Operations - Phase 1B Implementation
//!
//! This module provides real vault operations using the Phase 1A parsing system.
//! It replaces the mock implementations with actual functionality that scans
//! and processes real vault data from the test vault.

use crate::vault_types::{VaultFile, VaultError, VaultResult};
use crate::vault_scanner::VaultScanner;
use crate::vault_parser::VaultParser;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, debug, warn};

/// Default vault path for testing
const DEFAULT_VAULT_PATH: &str = "/home/moot/Documents/crucible-testing";

/// Real vault operations using Phase 1A parsing system
#[derive(Debug, Clone)]
pub struct RealVaultOperations {
    /// Path to the vault directory
    vault_path: Arc<String>,
    /// Scanner for discovering markdown files
    scanner: Arc<VaultScanner>,
    /// Parser for extracting metadata
    parser: Arc<VaultParser>,
    /// Cache for parsed files to avoid re-parsing
    file_cache: Arc<RwLock<HashMap<String, VaultFile>>>,
    /// Cache for tag statistics
    tag_cache: Arc<RwLock<HashMap<String, usize>>>,
}

impl RealVaultOperations {
    /// Create new vault operations with default test vault path
    pub fn new() -> Self {
        Self::with_path(DEFAULT_VAULT_PATH)
    }

    /// Create new vault operations with custom path
    pub fn with_path(vault_path: &str) -> Self {
        let vault_path = Arc::new(vault_path.to_string());
        let scanner = Arc::new(VaultScanner::new(&vault_path));
        let parser = Arc::new(VaultParser::new());

        Self {
            vault_path,
            scanner,
            parser,
            file_cache: Arc::new(RwLock::new(HashMap::new())),
            tag_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get the vault path
    pub fn vault_path(&self) -> &str {
        &self.vault_path
    }

    /// Scan and parse all markdown files in the vault
    pub async fn scan_and_parse_vault(&self) -> VaultResult<Vec<VaultFile>> {
        let start_time = std::time::Instant::now();
        info!("Starting vault scan and parse for: {}", self.vault_path);

        // Check if vault exists
        if !self.scanner.vault_exists().await {
            return Err(VaultError::FileNotFound(
                format!("Vault path does not exist: {}", self.vault_path)
            ));
        }

        // Scan for markdown files
        let markdown_files = self.scanner.scan_markdown_files().await?;
        debug!("Found {} markdown files", markdown_files.len());

        // Parse each file
        let mut vault_files = Vec::new();
        let mut cache = self.file_cache.write().await;

        for relative_path in markdown_files {
            let absolute_path = self.scanner.get_absolute_path(&relative_path);
            let path_str = absolute_path.to_string_lossy().to_string();

            match self.parser.parse_file(&path_str).await {
                Ok(vault_file) => {
                    // Cache the parsed file
                    cache.insert(path_str.clone(), vault_file.clone());
                    vault_files.push(vault_file);
                }
                Err(e) => {
                    warn!("Failed to parse file {}: {}", path_str, e);
                    // Continue with other files
                }
            }
        }

        let duration = start_time.elapsed();
        info!("Successfully parsed {} files in {:?}", vault_files.len(), duration);

        // Update tag cache
        self.update_tag_cache(&vault_files).await;

        Ok(vault_files)
    }

    /// Search files by frontmatter properties
    pub async fn search_by_properties(&self, properties: Value) -> VaultResult<Vec<Value>> {
        info!("Searching by properties: {:?}", properties);

        let vault_files = self.scan_and_parse_vault().await?;
        let mut matching_files = Vec::new();

        // Convert search properties to HashMap for easier comparison
        let search_props: HashMap<String, Value> = if let Value::Object(map) = properties {
            map.into_iter().collect()
        } else {
            HashMap::new()
        };

        for vault_file in vault_files {
            let mut matches_all = true;

            // Check each search property against file's frontmatter
            for (key, search_value) in &search_props {
                match vault_file.metadata.frontmatter.get(key) {
                    Some(file_value) => {
                        if !self.values_match(search_value, file_value) {
                            matches_all = false;
                            break;
                        }
                    }
                    None => {
                        matches_all = false;
                        break;
                    }
                }
            }

            if matches_all {
                matching_files.push(self.vault_file_to_json(&vault_file));
            }
        }

        info!("Found {} files matching properties", matching_files.len());
        Ok(matching_files)
    }

    /// Search files by tags
    pub async fn search_by_tags(&self, search_tags: Vec<String>) -> VaultResult<Vec<Value>> {
        info!("Searching by tags: {:?}", search_tags);

        let vault_files = self.scan_and_parse_vault().await?;
        let mut matching_files = Vec::new();

        for vault_file in vault_files {
            let file_tags = vault_file.get_tags();

            // Check if file has ALL the search tags (AND logic)
            let has_all_tags = search_tags.iter().all(|search_tag| {
                file_tags.iter().any(|file_tag| {
                    self.tags_match(search_tag, file_tag)
                })
            });

            if has_all_tags {
                matching_files.push(self.vault_file_to_json(&vault_file));
            }
        }

        info!("Found {} files matching tags", matching_files.len());
        Ok(matching_files)
    }

    /// Search files in a specific folder
    pub async fn search_by_folder(&self, folder_path: &str, recursive: bool) -> VaultResult<Vec<Value>> {
        info!("Searching in folder: {} (recursive: {})", folder_path, recursive);

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

    /// Calculate real vault statistics
    pub async fn get_vault_stats(&self) -> VaultResult<Value> {
        info!("Calculating vault statistics");

        let vault_files = self.scan_and_parse_vault().await?;

        // Calculate statistics
        let total_notes = vault_files.len();
        let total_size_bytes: u64 = vault_files.iter().map(|f| f.metadata.size).sum();
        let total_size_mb = total_size_bytes as f64 / (1024.0 * 1024.0);

        // Count unique folders
        let mut folders = HashSet::new();
        for file in &vault_files {
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
            "vault_type": "obsidian",
            "vault_path": self.vault_path.as_str(),
            "file_count_by_extension": {
                "md": total_notes
            }
        });

        info!("Vault stats: {} notes, {:.2} MB, {} folders, {} tags",
              total_notes, total_size_mb, folders.len(), total_tags);

        Ok(stats)
    }

    /// List all tags in the vault with counts
    pub async fn list_tags(&self) -> VaultResult<Value> {
        info!("Listing all vault tags");

        // Ensure vault is scanned and tag cache is populated
        self.scan_and_parse_vault().await?;

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
            b.get("count").unwrap().as_u64().unwrap()
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

    /// Update tag cache from vault files
    async fn update_tag_cache(&self, vault_files: &[VaultFile]) {
        let mut tag_counts = HashMap::new();

        for file in vault_files {
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
        let search_normalized = search_lower.replace('-', " ").replace('_', " ");
        let file_normalized = file_lower.replace('-', " ").replace('_', " ");

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
                    file_arr.iter().any(|file_elem| self.values_match(search_elem, file_elem))
                })
            }
            (Value::Number(search), Value::Number(file)) => {
                search == file
            }
            (Value::Bool(search), Value::Bool(file)) => {
                search == file
            }
            _ => false,
        }
    }

    /// Convert VaultFile to JSON format
    fn vault_file_to_json(&self, vault_file: &VaultFile) -> Value {
        json!({
            "path": vault_file.path.to_string_lossy(),
            "name": vault_file.get_title(),
            "folder": vault_file.path.parent()
                .and_then(|p| p.to_str())
                .unwrap_or(""),
            "properties": vault_file.metadata.frontmatter,
            "tags": vault_file.get_tags(),
            "type": vault_file.get_type(),
            "status": vault_file.get_status(),
            "created": vault_file.metadata.created
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| "unknown".to_string()),
            "modified": vault_file.metadata.modified.to_rfc3339(),
            "size": vault_file.metadata.size,
            "hash": vault_file.hash
        })
    }
}

impl Default for RealVaultOperations {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_real_vault_operations_creation() {
        let ops = RealVaultOperations::new();
        assert_eq!(ops.vault_path(), DEFAULT_VAULT_PATH);
    }

    #[tokio::test]
    async fn test_vault_exists() {
        let ops = RealVaultOperations::new();
        assert!(ops.scanner.vault_exists().await);
    }

    #[tokio::test]
    async fn test_scan_and_parse_vault() {
        let ops = RealVaultOperations::new();
        let result = ops.scan_and_parse_vault().await;
        assert!(result.is_ok(), "Vault scan should succeed");

        let files = result.unwrap();
        assert!(!files.is_empty(), "Should find files in test vault");

        // Should find PRIME.md specifically
        let has_prime = files.iter().any(|f| f.path.to_string_lossy().contains("PRIME.md"));
        assert!(has_prime, "Should find PRIME.md in test vault");
    }
}
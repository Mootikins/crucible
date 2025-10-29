//! Kiln File Types - Phase 1A TDD Implementation
//!
//! This module contains the core data structures for kiln file parsing.
//! Implemented to make the failing tests pass with minimal functionality.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

/// Represents a kiln file with metadata and content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KilnFile {
    /// File path relative to kiln root
    pub path: PathBuf,
    /// File metadata
    pub metadata: FileMetadata,
    /// File content (markdown without frontmatter)
    pub content: String,
    /// SHA256 hash for change detection
    pub hash: String,
}

/// Metadata extracted from kiln files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    /// Title extracted from filename or first heading
    pub title: Option<String>,
    /// Frontmatter key-value pairs
    pub frontmatter: HashMap<String, Value>,
    /// File creation date (from frontmatter or filesystem)
    pub created: Option<DateTime<Utc>>,
    /// File modification date (from filesystem)
    pub modified: DateTime<Utc>,
    /// File size in bytes
    pub size: u64,
}

impl FileMetadata {
    /// Create new file metadata
    pub fn new() -> Self {
        Self {
            title: None,
            frontmatter: HashMap::new(),
            created: None,
            modified: Utc::now(),
            size: 0,
        }
    }

    /// Get a frontmatter value as string
    pub fn get_string(&self, key: &str) -> Option<String> {
        self.frontmatter
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// Get a frontmatter value as array of strings
    pub fn get_string_array(&self, key: &str) -> Vec<String> {
        self.frontmatter
            .get(key)
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Check if frontmatter contains a key
    pub fn has_key(&self, key: &str) -> bool {
        self.frontmatter.contains_key(key)
    }
}

impl Default for FileMetadata {
    fn default() -> Self {
        Self::new()
    }
}

/// Kiln-specific error types
/// Errors that can occur during kiln operations
#[derive(Debug, thiserror::Error)]
pub enum KilnError {
    /// File not found at the specified path
    #[error("File not found: {0}")]
    FileNotFound(String),

    /// IO error during file operations
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// Error parsing YAML frontmatter
    #[error("Frontmatter parsing error: {0}")]
    FrontmatterParseError(String),

    /// YAML parsing error
    #[error("YAML parsing error: {0}")]
    YamlError(#[from] serde_yaml::Error),

    /// Hash calculation error
    #[error("Hash calculation error: {0}")]
    HashError(String),

    /// Invalid file path provided
    #[error("Invalid file path: {0}")]
    InvalidPath(String),
}

/// Result type for kiln operations
pub type KilnResult<T> = Result<T, KilnError>;

impl KilnFile {
    /// Create a new kiln file
    pub fn new(path: PathBuf, content: String, hash: String) -> Self {
        let size = content.len() as u64;
        let mut metadata = FileMetadata::new();
        metadata.size = size;
        metadata.modified = Utc::now();

        Self {
            path,
            metadata,
            content,
            hash,
        }
    }

    /// Get the file title (from metadata or filename)
    pub fn get_title(&self) -> String {
        self.metadata
            .title
            .clone()
            .or_else(|| {
                // Extract from filename
                self.path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| "Untitled".to_string())
    }

    /// Get file tags from frontmatter
    pub fn get_tags(&self) -> Vec<String> {
        self.metadata.get_string_array("tags")
    }

    /// Get file type from frontmatter
    pub fn get_type(&self) -> Option<String> {
        self.metadata.get_string("type")
    }

    /// Get file status from frontmatter
    pub fn get_status(&self) -> Option<String> {
        self.metadata.get_string("status")
    }

    /// Check if file has changed since last scan
    pub fn has_changed(&self, new_hash: &str) -> bool {
        self.hash != new_hash
    }
}

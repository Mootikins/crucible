//! Vault Change Detection - Phase 1A TDD Implementation
//!
//! This module provides change detection functionality using SHA256 hashing.
//! Implemented to make the failing tests pass with minimal functionality.

use crate::vault_types::{VaultError, VaultResult};
use std::fs;
use sha2::{Sha256, Digest};

/// Change detector for vault files using SHA256 hashing
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ChangeDetector {
    /// Whether to cache hashes for performance
    cache_enabled: bool,
}

impl ChangeDetector {
    /// Create a new change detector
    pub fn new() -> Self {
        Self {
            cache_enabled: false,
        }
    }

    /// Create a change detector with caching enabled
    pub fn with_cache() -> Self {
        Self {
            cache_enabled: true,
        }
    }

    /// Calculate SHA256 hash for a file
    pub async fn calculate_file_hash(&self, file_path: &str) -> VaultResult<String> {
        let file_path = file_path.to_string();

        // Read file content in blocking task
        let content = tokio::task::spawn_blocking(move || {
            fs::read_to_string(&file_path)
        }).await.map_err(|e| VaultError::HashError(format!("Task join error: {}", e)))??;

        // Calculate hash
        Ok(self.calculate_content_hash(&content))
    }

    /// Calculate SHA256 hash for string content
    pub fn calculate_content_hash(&self, content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Calculate hash for file bytes
    pub fn calculate_bytes_hash(&self, bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        format!("{:x}", hasher.finalize())
    }

    /// Check if file has changed by comparing hashes
    pub async fn file_has_changed(&self, file_path: &str, previous_hash: &str) -> VaultResult<bool> {
        let current_hash = self.calculate_file_hash(file_path).await?;
        Ok(current_hash != previous_hash)
    }

    /// Check if content has changed
    pub fn content_has_changed(&self, content: &str, previous_hash: &str) -> bool {
        let current_hash = self.calculate_content_hash(content);
        current_hash != previous_hash
    }

    /// Get hash length (should always be 64 for SHA256)
    pub fn hash_length(&self) -> usize {
        64
    }

    /// Validate if a string looks like a valid SHA256 hash
    pub fn is_valid_hash(&self, hash: &str) -> bool {
        hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit())
    }

    /// Check if the file exists
    pub async fn file_exists(&self, file_path: &str) -> bool {
        let file_path = file_path.to_string();

        tokio::task::spawn_blocking(move || {
            std::path::Path::new(&file_path).exists()
        }).await.unwrap_or(false)
    }

    /// Get file size in bytes
    pub async fn get_file_size(&self, file_path: &str) -> VaultResult<u64> {
        let file_path = file_path.to_string();

        tokio::task::spawn_blocking(move || {
            let metadata = fs::metadata(&file_path)?;
            Ok(metadata.len())
        }).await.map_err(|e| VaultError::HashError(format!("Task join error: {}", e)))?
    }

    /// Get file modification time
    pub async fn get_file_modified_time(&self, file_path: &str) -> VaultResult<chrono::DateTime<chrono::Utc>> {
        let file_path = file_path.to_string();

        tokio::task::spawn_blocking(move || {
            let metadata = fs::metadata(&file_path)?;
            let modified = metadata.modified()?;
            Ok(chrono::DateTime::from(modified))
        }).await.map_err(|e| VaultError::HashError(format!("Task join error: {}", e)))?
    }
}

impl Default for ChangeDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[tokio::test]
    async fn test_change_detector_creates_successfully() {
        let detector = ChangeDetector::new();
        assert!(!detector.cache_enabled);

        let cached_detector = ChangeDetector::with_cache();
        assert!(cached_detector.cache_enabled);
    }

    #[tokio::test]
    async fn test_content_hash_calculation() {
        let detector = ChangeDetector::new();
        let content = "Hello, World!";

        let hash = detector.calculate_content_hash(content);

        assert_eq!(hash.len(), 64);
        assert!(detector.is_valid_hash(&hash));

        // Same content should produce same hash
        let hash2 = detector.calculate_content_hash(content);
        assert_eq!(hash, hash2);

        // Different content should produce different hash
        let different_hash = detector.calculate_content_hash("Hello, Different World!");
        assert_ne!(hash, different_hash);
    }

    #[tokio::test]
    async fn test_file_hash_calculation() {
        let detector = ChangeDetector::new();
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let content = "Test file content for hashing";
        fs::write(&file_path, content).unwrap();

        let hash = detector.calculate_file_hash(file_path.to_str().unwrap()).await.unwrap();

        assert_eq!(hash.len(), 64);
        assert!(detector.is_valid_hash(&hash));

        // Should match content hash
        let content_hash = detector.calculate_content_hash(content);
        assert_eq!(hash, content_hash);
    }

    #[tokio::test]
    async fn test_file_has_changed() {
        let detector = ChangeDetector::new();
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let initial_content = "Initial content";
        fs::write(&file_path, initial_content).unwrap();

        // Get initial hash
        let initial_hash = detector.calculate_file_hash(file_path.to_str().unwrap()).await.unwrap();

        // File should not have changed
        assert!(!detector.file_has_changed(file_path.to_str().unwrap(), &initial_hash).await.unwrap());

        // Modify file
        fs::write(&file_path, "Modified content").unwrap();

        // File should have changed
        assert!(detector.file_has_changed(file_path.to_str().unwrap(), &initial_hash).await.unwrap());
    }

    #[tokio::test]
    async fn test_file_not_found() {
        let detector = ChangeDetector::new();
        let result = detector.calculate_file_hash("/nonexistent/file.txt").await;

        assert!(result.is_err());
        match result.unwrap_err() {
            VaultError::IoError(_) => {}, // Expected
            other => panic!("Expected IoError, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_file_metadata() {
        let detector = ChangeDetector::new();
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let content = "Test content";
        fs::write(&file_path, content).unwrap();

        let path_str = file_path.to_str().unwrap();

        // Test file existence
        assert!(detector.file_exists(path_str).await);
        assert!(!detector.file_exists("/nonexistent/file.txt").await);

        // Test file size
        let size = detector.get_file_size(path_str).await.unwrap();
        assert_eq!(size, content.len() as u64);

        // Test modification time
        let modified = detector.get_file_modified_time(path_str).await.unwrap();
        let now = chrono::Utc::now();
        assert!(now.signed_duration_since(modified).num_seconds() < 10); // Within last 10 seconds
    }

    #[tokio::test]
    async fn test_hash_validation() {
        let detector = ChangeDetector::new();

        // Valid SHA256 hash
        let valid_hash = "a591a6d40bf420404a011733cfb7b190d62c65bf0bcda32b57b277d9ad9f146e";
        assert!(detector.is_valid_hash(valid_hash));

        // Invalid hash (wrong length)
        let short_hash = "a591a6d40bf420404a011733cfb7b190d62c65bf0bcda32b57b277d9ad9f14";
        assert!(!detector.is_valid_hash(short_hash));

        // Invalid hash (non-hex characters)
        let invalid_chars = "z591a6d40bf420404a011733cfb7b190d62c65bf0bcda32b57b277d9ad9f146e";
        assert!(!detector.is_valid_hash(invalid_chars));

        // Test hash length
        assert_eq!(detector.hash_length(), 64);
    }
}
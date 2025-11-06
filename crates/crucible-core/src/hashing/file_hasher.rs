//! File hashing implementation using the ContentHasher trait
//!
//! This module provides a concrete implementation of the `ContentHasher` trait
//! for file operations. It supports both BLAKE3 and SHA256 algorithms with
//! efficient streaming I/O for large files.
//!
//! # Architecture
//!
//! The `FileHasher` is designed to be:
//! - **Algorithm Agnostic**: Can work with BLAKE3 or SHA256
//! - **Memory Efficient**: Uses streaming for large files
//! - **Async Native**: Non-blocking I/O throughout
//! - **Error Resilient**: Comprehensive error handling
//!
//! # Examples
//!
//! ```rust
//! use crucible_core::hashing::file_hasher::FileHasher;
//! use crucible_core::traits::change_detection::ContentHasher;
//! use crucible_core::types::hashing::HashAlgorithm;
//! use std::path::Path;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let hasher = FileHasher::new(HashAlgorithm::Blake3);
//!     let path = Path::new("example.txt");
//!
//!     let hash = hasher.hash_file(path).await?;
//!     println!("File hash: {}", hash);
//!
//!     Ok(())
//! }
//! ```

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use blake3::Hasher as Blake3Hasher;
use sha2::{Digest, Sha256};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, BufReader};

use crate::traits::change_detection::ContentHasher;
use crate::types::hashing::{
    BlockHash, BlockHashInfo, FileHash, FileHashInfo, HashAlgorithm, HashError,
};

/// Implementation of the ContentHasher trait for file operations
///
/// This struct provides efficient file and block hashing using either BLAKE3
/// or SHA256 algorithms. It's designed to handle large files through streaming
/// I/O and supports batch operations for better performance.
///
/// # Performance Characteristics
///
/// - **BLAKE3**: ~5-10 GB/s on modern CPUs, SIMD-optimized
/// - **SHA256**: ~2-3 GB/s on modern CPUs, hardware acceleration available
/// - **Memory Usage**: O(1) for streaming operations (constant buffer size)
/// - **Parallel Processing**: Batch operations can be parallelized
///
/// # Thread Safety
///
/// The `FileHasher` is `Send + Sync` and can be safely shared across threads.
/// All operations are async and non-blocking.
#[derive(Debug, Clone)]
pub struct FileHasher {
    algorithm: HashAlgorithm,
    buffer_size: usize,
}

impl FileHasher {
    /// Create a new FileHasher with the specified algorithm
    ///
    /// # Arguments
    ///
    /// * `algorithm` - The hash algorithm to use (BLAKE3 or SHA256)
    ///
    /// # Examples
    ///
    /// ```rust
    /// use crucible_core::hashing::file_hasher::FileHasher;
    /// use crucible_core::types::hashing::HashAlgorithm;
    ///
    /// let hasher = FileHasher::new(HashAlgorithm::Blake3);
    /// ```
    pub fn new(algorithm: HashAlgorithm) -> Self {
        Self {
            algorithm,
            buffer_size: 64 * 1024, // 64KB buffer for streaming
        }
    }

    /// Create a new FileHasher with custom buffer size
    ///
    /// # Arguments
    ///
    /// * `algorithm` - The hash algorithm to use
    /// * `buffer_size` - Buffer size for streaming operations in bytes
    ///
    /// # Examples
    ///
    /// ```rust
    /// use crucible_core::hashing::file_hasher::FileHasher;
    /// use crucible_core::types::hashing::HashAlgorithm;
    ///
    /// let hasher = FileHasher::with_buffer_size(HashAlgorithm::Blake3, 128 * 1024);
    /// ```
    pub fn with_buffer_size(algorithm: HashAlgorithm, buffer_size: usize) -> Self {
        Self {
            algorithm,
            buffer_size,
        }
    }

    /// Hash a file using streaming I/O
    ///
    /// This internal method handles the actual file reading and hashing
    /// using a buffered reader for efficiency.
    async fn hash_file_streaming(&self, path: &Path) -> Result<Vec<u8>, HashError> {
        let file = File::open(path).await?;
        let mut reader = BufReader::new(file);
        let mut buffer = vec![0u8; self.buffer_size];

        match self.algorithm {
            HashAlgorithm::Blake3 => {
                let mut hasher = Blake3Hasher::new();
                loop {
                    let bytes_read = reader.read(&mut buffer).await?;
                    if bytes_read == 0 {
                        break;
                    }
                    hasher.update(&buffer[..bytes_read]);
                }
                Ok(hasher.finalize().as_bytes().to_vec())
            }
            HashAlgorithm::Sha256 => {
                let mut hasher = Sha256::new();
                loop {
                    let bytes_read = reader.read(&mut buffer).await?;
                    if bytes_read == 0 {
                        break;
                    }
                    hasher.update(&buffer[..bytes_read]);
                }
                Ok(hasher.finalize().to_vec())
            }
        }
    }

    /// Get file metadata for change detection
    async fn get_file_metadata(&self, path: &Path) -> Result<(u64, std::time::SystemTime), HashError> {
        let metadata = tokio::fs::metadata(path).await?;
        let size = metadata.len();
        let modified = metadata
            .modified()
            .unwrap_or_else(|_| std::time::SystemTime::now());
        Ok((size, modified))
    }
}

#[async_trait]
impl ContentHasher for FileHasher {
    fn algorithm(&self) -> HashAlgorithm {
        self.algorithm
    }

    async fn hash_file(&self, path: &Path) -> Result<FileHash, HashError> {
        let hash_bytes = self.hash_file_streaming(path).await?;
        if hash_bytes.len() != 32 {
            return Err(HashError::InvalidLength { len: hash_bytes.len() });
        }
        let mut array = [0u8; 32];
        array.copy_from_slice(&hash_bytes);
        Ok(FileHash::new(array))
    }

    async fn hash_files_batch(&self, paths: &[PathBuf]) -> Result<Vec<FileHash>, HashError> {
        let mut results = Vec::with_capacity(paths.len());

        // Process files concurrently for better performance
        let futures: Vec<_> = paths.iter().map(|path| self.hash_file(path)).collect();
        let hash_results = futures::future::join_all(futures).await;

        for result in hash_results {
            results.push(result?);
        }

        Ok(results)
    }

    async fn hash_block(&self, content: &str) -> Result<BlockHash, HashError> {
        let hash_bytes = match self.algorithm {
            HashAlgorithm::Blake3 => {
                let mut hasher = Blake3Hasher::new();
                hasher.update(content.as_bytes());
                hasher.finalize().as_bytes().to_vec()
            }
            HashAlgorithm::Sha256 => {
                let mut hasher = Sha256::new();
                hasher.update(content.as_bytes());
                hasher.finalize().to_vec()
            }
        };

        if hash_bytes.len() != 32 {
            return Err(HashError::InvalidLength { len: hash_bytes.len() });
        }
        let mut array = [0u8; 32];
        array.copy_from_slice(&hash_bytes);
        Ok(BlockHash::new(array))
    }

    async fn hash_blocks_batch(&self, contents: &[String]) -> Result<Vec<BlockHash>, HashError> {
        let mut results = Vec::with_capacity(contents.len());

        // Process blocks concurrently
        let futures: Vec<_> = contents.iter().map(|content| self.hash_block(content)).collect();
        let hash_results = futures::future::join_all(futures).await;

        for result in hash_results {
            results.push(result?);
        }

        Ok(results)
    }

    async fn hash_file_info(&self, path: &Path, relative_path: String) -> Result<FileHashInfo, HashError> {
        let content_hash = self.hash_file(path).await?;
        let (size, modified) = self.get_file_metadata(path).await?;

        Ok(FileHashInfo::new(
            content_hash,
            size,
            modified,
            self.algorithm,
            relative_path,
        ))
    }

    async fn hash_block_info(
        &self,
        content: &str,
        block_type: String,
        start_offset: usize,
        end_offset: usize,
    ) -> Result<BlockHashInfo, HashError> {
        let content_hash = self.hash_block(content).await?;

        Ok(BlockHashInfo::new(
            content_hash,
            block_type,
            start_offset,
            end_offset,
            self.algorithm,
        ))
    }

    async fn verify_file_hash(&self, path: &Path, expected_hash: &FileHash) -> Result<bool, HashError> {
        match self.hash_file(path).await {
            Ok(actual_hash) => Ok(actual_hash == *expected_hash),
            Err(_) => Ok(false), // If we can't hash the file, consider it failed
        }
    }

    async fn verify_block_hash(&self, content: &str, expected_hash: &BlockHash) -> Result<bool, HashError> {
        match self.hash_block(content).await {
            Ok(actual_hash) => Ok(actual_hash == *expected_hash),
            Err(_) => Ok(false),
        }
    }
}

/// Constants for commonly used hashers
pub const BLAKE3_HASHER: FileHasher = FileHasher {
    algorithm: HashAlgorithm::Blake3,
    buffer_size: 64 * 1024,
};

pub const SHA256_HASHER: FileHasher = FileHasher {
    algorithm: HashAlgorithm::Sha256,
    buffer_size: 64 * 1024,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;
    use tokio::fs;

    #[tokio::test]
    async fn test_hash_file_blake3() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"Hello, World!").unwrap();
        temp_file.flush().unwrap();

        let hasher = FileHasher::new(HashAlgorithm::Blake3);
        let hash = hasher.hash_file(temp_file.path()).await.unwrap();

        // Verify the hash matches expected BLAKE3 hash
        let expected_hex = "288a86a79f20a3d6dccdca7713beaed178798296bdfa7913fa2a62d9727bf8f8";
        assert_eq!(hash.to_hex(), expected_hex);
    }

    #[tokio::test]
    async fn test_hash_file_sha256() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"Hello, World!").unwrap();
        temp_file.flush().unwrap();

        let hasher = FileHasher::new(HashAlgorithm::Sha256);
        let hash = hasher.hash_file(temp_file.path()).await.unwrap();

        // Verify the hash matches expected SHA256 hash
        let expected_hex = "ae97eca8f8ae1672bcc5c79e3fbafd8ee86f65f775e2250a291d3788b7a8af95";
        assert_eq!(hash.to_hex(), expected_hex);
    }

    #[tokio::test]
    async fn test_hash_block() {
        let hasher = FileHasher::new(HashAlgorithm::Blake3);
        let content = "# Hello World\nThis is a test heading.";

        let hash = hasher.hash_block(content).await.unwrap();

        // Verify hash is deterministic
        let hash2 = hasher.hash_block(content).await.unwrap();
        assert_eq!(hash, hash2);
    }

    #[tokio::test]
    async fn test_hash_files_batch() {
        let mut temp_file1 = NamedTempFile::new().unwrap();
        temp_file1.write_all(b"File 1 content").unwrap();
        temp_file1.flush().unwrap();

        let mut temp_file2 = NamedTempFile::new().unwrap();
        temp_file2.write_all(b"File 2 content").unwrap();
        temp_file2.flush().unwrap();

        let hasher = FileHasher::new(HashAlgorithm::Blake3);
        let paths = vec![
            temp_file1.path().to_path_buf(),
            temp_file2.path().to_path_buf(),
        ];

        let hashes = hasher.hash_files_batch(&paths).await.unwrap();
        assert_eq!(hashes.len(), 2);

        // Verify individual hashes
        let hash1 = hasher.hash_file(temp_file1.path()).await.unwrap();
        let hash2 = hasher.hash_file(temp_file2.path()).await.unwrap();
        assert_eq!(hashes[0], hash1);
        assert_eq!(hashes[1], hash2);
    }

    #[tokio::test]
    async fn test_hash_file_info() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"Test content").unwrap();
        temp_file.flush().unwrap();

        let hasher = FileHasher::new(HashAlgorithm::Blake3);
        let relative_path = "test.txt".to_string();

        let info = hasher.hash_file_info(temp_file.path(), relative_path.clone()).await.unwrap();

        assert_eq!(info.relative_path, relative_path);
        assert_eq!(info.size, 12); // "Test content" length
        assert_eq!(info.algorithm, HashAlgorithm::Blake3);
        assert!(!info.content_hash.is_zero());
    }

    #[tokio::test]
    async fn test_verify_file_hash() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"Verification test").unwrap();
        temp_file.flush().unwrap();

        let hasher = FileHasher::new(HashAlgorithm::Blake3);
        let correct_hash = hasher.hash_file(temp_file.path()).await.unwrap();

        // Test correct hash verification
        assert!(hasher.verify_file_hash(temp_file.path(), &correct_hash).await.unwrap());

        // Test incorrect hash verification
        let wrong_hash = FileHash::new([0u8; 32]);
        assert!(!hasher.verify_file_hash(temp_file.path(), &wrong_hash).await.unwrap());
    }

    #[tokio::test]
    async fn test_verify_block_hash() {
        let hasher = FileHasher::new(HashAlgorithm::Blake3);
        let content = "Test block content";

        let correct_hash = hasher.hash_block(content).await.unwrap();

        // Test correct hash verification
        assert!(hasher.verify_block_hash(content, &correct_hash).await.unwrap());

        // Test incorrect hash verification
        let wrong_hash = BlockHash::new([0u8; 32]);
        assert!(!hasher.verify_block_hash(content, &wrong_hash).await.unwrap());
    }

    #[test]
    fn test_constants() {
        assert_eq!(BLAKE3_HASHER.algorithm(), HashAlgorithm::Blake3);
        assert_eq!(SHA256_HASHER.algorithm(), HashAlgorithm::Sha256);
    }

    #[tokio::test]
    async fn test_error_handling() {
        let hasher = FileHasher::new(HashAlgorithm::Blake3);
        let non_existent_path = Path::new("/non/existent/file.txt");

        // Should handle missing file gracefully
        let result = hasher.hash_file(non_existent_path).await;
        assert!(result.is_err());
    }
}
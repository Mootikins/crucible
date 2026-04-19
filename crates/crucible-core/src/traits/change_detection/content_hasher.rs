use std::path::Path;

use async_trait::async_trait;

use crate::types::hashing::{
    BlockHash, BlockHashInfo, FileHash, FileHashInfo, HashAlgorithm, HashError,
};

/// Trait for content hashing operations
///
/// This trait provides the interface for hashing files and content blocks.
/// Implementations should be thread-safe and support async operations.
/// The trait is object-safe and can be used as a trait object.
///
/// # Design Principles
///
/// - **Async Support**: All methods are async to support non-blocking I/O
/// - **Error Handling**: Comprehensive error handling with specific error types
/// - **Algorithm Agnostic**: Support for multiple hash algorithms
/// - **Object Safety**: Can be used as `dyn ContentHasher`
/// - **Send + Sync**: Safe to use across threads
///
/// # Examples
///
#[async_trait]
pub trait ContentHasher: Send + Sync {
    fn algorithm(&self) -> HashAlgorithm;

    /// Hash a single file using streaming I/O
    ///
    /// This method should read the file efficiently using streaming operations
    /// to handle large files without loading everything into memory.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file to hash
    ///
    /// # Returns
    ///
    /// The content hash of the file
    ///
    /// # Errors
    ///
    /// Returns `HashError` if the file cannot be read or hashing fails
    async fn hash_file(&self, path: &Path) -> Result<FileHash, HashError>;

    /// Hash multiple files in parallel
    ///
    /// This method should efficiently hash multiple files, potentially using
    /// parallel processing for better performance on multi-core systems.
    ///
    /// # Arguments
    ///
    /// * `paths` - Slice of file paths to hash
    ///
    /// # Returns
    ///
    /// Vector of hashes in the same order as the input paths
    ///
    /// # Errors
    ///
    /// Returns `HashError` if any file cannot be read or hashing fails
    async fn hash_files_batch(
        &self,
        paths: &[std::path::PathBuf],
    ) -> Result<Vec<FileHash>, HashError>;

    /// Hash a content block (e.g., heading, paragraph, code block)
    ///
    /// This method hashes individual content blocks extracted from documents.
    /// It should be fast and efficient since blocks are typically small.
    ///
    /// # Arguments
    ///
    /// * `content` - The content to hash
    ///
    /// # Returns
    ///
    /// The block hash
    ///
    /// # Errors
    ///
    /// Returns `HashError` if hashing fails
    async fn hash_block(&self, content: &str) -> Result<BlockHash, HashError>;

    /// Hash multiple blocks in batch
    ///
    /// This method efficiently hashes multiple content blocks, which is useful
    /// when processing a complete note.
    ///
    /// # Arguments
    ///
    /// * `contents` - Vector of content strings to hash
    ///
    /// # Returns
    ///
    /// Vector of block hashes in the same order as input
    ///
    /// # Errors
    ///
    /// Returns `HashError` if hashing fails
    async fn hash_blocks_batch(&self, contents: &[String]) -> Result<Vec<BlockHash>, HashError>;

    /// Create comprehensive file hash info including metadata
    ///
    /// This method hashes a file and includes important metadata for
    /// change detection and file system operations.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file to hash
    /// * `relative_path` - Relative path from vault root
    ///
    /// # Returns
    ///
    /// Complete file hash information
    ///
    /// # Errors
    ///
    /// Returns `HashError` if file operations fail
    async fn hash_file_info(
        &self,
        path: &Path,
        relative_path: String,
    ) -> Result<FileHashInfo, HashError>;

    /// Create comprehensive block hash info
    ///
    /// This method hashes a content block and includes metadata for
    /// content addressing and change detection.
    ///
    /// # Arguments
    ///
    /// * `content` - The content to hash
    /// * `block_type` - Type of block (heading, paragraph, code, etc.)
    /// * `start_offset` - Start position in source note
    /// * `end_offset` - End position in source note
    ///
    /// # Returns
    ///
    /// Complete block hash information
    ///
    /// # Errors
    ///
    /// Returns `HashError` if hashing fails
    async fn hash_block_info(
        &self,
        content: &str,
        block_type: String,
        start_offset: usize,
        end_offset: usize,
    ) -> Result<BlockHashInfo, HashError>;

    /// Verify a file's hash matches the expected value
    ///
    /// This method is useful for integrity checking and validation.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file to verify
    /// * `expected_hash` - Expected hash value
    ///
    /// # Returns
    ///
    /// `true` if the hash matches, `false` otherwise
    ///
    /// # Errors
    ///
    /// Returns `HashError` if file operations fail
    async fn verify_file_hash(
        &self,
        path: &Path,
        expected_hash: &FileHash,
    ) -> Result<bool, HashError>;

    /// Verify a block's hash matches the expected value
    ///
    /// This method is useful for content integrity checking.
    ///
    /// # Arguments
    ///
    /// * `content` - Content to verify
    /// * `expected_hash` - Expected hash value
    ///
    /// # Returns
    ///
    /// `true` if the hash matches, `false` otherwise
    ///
    /// # Errors
    ///
    /// Returns `HashError` if hashing fails
    async fn verify_block_hash(
        &self,
        content: &str,
        expected_hash: &BlockHash,
    ) -> Result<bool, HashError>;
}

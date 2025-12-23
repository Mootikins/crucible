//! Block Processing and HashedBlock Types
//!
//! This module provides functionality for processing content into blocks,
//! managing block sizes, and creating hashed block representations.

use crate::storage::{StorageError, StorageResult};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// Configuration for block processing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlockSize {
    /// Fixed 1KB blocks
    Small,
    /// Fixed 4KB blocks (default)
    #[default]
    Medium,
    /// Fixed 8KB blocks
    Large,
    /// Adaptive block sizing based on content size
    Adaptive { min: usize, max: usize },
    /// Custom block size
    Custom(usize),
}

impl BlockSize {
    /// Get the actual block size in bytes
    pub fn size_bytes(&self) -> usize {
        match self {
            Self::Small => 1024,  // 1KB
            Self::Medium => 4096, // 4KB
            Self::Large => 8192,  // 8KB
            Self::Adaptive { max, .. } => *max,
            Self::Custom(size) => *size,
        }
    }

    /// Get the minimum block size for adaptive sizing
    pub fn min_size(&self) -> usize {
        match self {
            Self::Small => 1024,
            Self::Medium => 4096,
            Self::Large => 8192,
            Self::Custom(size) => *size,
            Self::Adaptive { min, .. } => *min,
        }
    }

    /// Calculate optimal block size for content length
    pub fn calculate_optimal(&self, content_len: usize) -> usize {
        match self {
            Self::Small => 1024,
            Self::Medium => 4096,
            Self::Large => 8192,
            Self::Custom(size) => *size,
            Self::Adaptive { min, max } => {
                // For very small content, use the content length + overhead
                if content_len <= *min {
                    content_len.max(512) // Minimum 512 bytes for efficiency
                } else if content_len <= *max {
                    // For medium content, use content length
                    content_len
                } else {
                    // For large content, use maximum block size
                    *max
                }
            }
        }
    }
}

/// A hashed block representing a chunk of content
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HashedBlock {
    /// SHA256 hash of the block content
    pub hash: String,
    /// Raw block data
    pub data: Vec<u8>,
    /// Length of the block in bytes
    pub length: usize,
    /// Block index within the original content
    pub index: usize,
    /// Offset of the block in the original content
    pub offset: usize,
    /// Whether this is the last block of the content
    pub is_last: bool,
}

impl HashedBlock {
    /// Create a new hashed block
    ///
    /// # Arguments
    /// * `hash` - SHA256 hash of the data
    /// * `data` - Raw block data
    /// * `index` - Block index within content
    /// * `offset` - Offset in original content
    /// * `is_last` - Whether this is the final block
    pub fn new(hash: String, data: Vec<u8>, index: usize, offset: usize, is_last: bool) -> Self {
        let length = data.len();
        Self {
            hash,
            data,
            length,
            index,
            offset,
            is_last,
        }
    }

    /// Create a hashed block from raw data and compute its hash
    ///
    /// # Arguments
    /// * `data` - Raw block data
    /// * `index` - Block index within content
    /// * `offset` - Offset in original content
    /// * `is_last` - Whether this is the final block
    /// * `hasher` - Hash function implementation
    pub fn from_data<H>(
        data: Vec<u8>,
        index: usize,
        offset: usize,
        is_last: bool,
        hasher: &H,
    ) -> StorageResult<Self>
    where
        H: crate::storage::ContentHasher + ?Sized,
    {
        if data.is_empty() {
            return Err(StorageError::BlockSize(
                "Empty block data not allowed".to_string(),
            ));
        }

        let hash = hasher.hash_block(&data);
        Ok(Self::new(hash, data, index, offset, is_last))
    }

    /// Get a reference to the block data as a string slice
    ///
    /// # Returns
    /// `Some(&str)` if the data is valid UTF-8, `None` otherwise
    pub fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.data).ok()
    }

    /// Check if this block contains valid UTF-8 text
    pub fn is_text(&self) -> bool {
        std::str::from_utf8(&self.data).is_ok()
    }

    /// Get the approximate number of lines in this block
    pub fn line_count(&self) -> usize {
        self.data.iter().filter(|&&b| b == b'\n').count() + 1
    }
}

impl PartialOrd for HashedBlock {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HashedBlock {
    fn cmp(&self, other: &Self) -> Ordering {
        // Order by index first, then by offset
        match self.index.cmp(&other.index) {
            Ordering::Equal => self.offset.cmp(&other.offset),
            other => other,
        }
    }
}

/// Block processor for splitting content into hashed blocks
pub struct BlockProcessor {
    block_size: BlockSize,
}

impl Default for BlockProcessor {
    fn default() -> Self {
        Self::new(BlockSize::default())
    }
}

impl BlockProcessor {
    /// Create a new block processor with the specified block size
    pub fn new(block_size: BlockSize) -> Self {
        Self { block_size }
    }

    /// Process content into hashed blocks
    ///
    /// # Arguments
    /// * `content` - Raw content to process
    /// * `hasher` - Hash function implementation
    ///
    /// # Returns
    /// Vector of hashed blocks or error if processing fails
    pub fn process_content<H>(&self, content: &[u8], hasher: &H) -> StorageResult<Vec<HashedBlock>>
    where
        H: crate::storage::ContentHasher + ?Sized,
    {
        if content.is_empty() {
            return Err(StorageError::BlockSize(
                "Empty content cannot be processed".to_string(),
            ));
        }

        let content_len = content.len();
        let block_size = self.block_size.calculate_optimal(content_len);

        let mut blocks = Vec::new();
        let mut offset = 0;
        let mut index = 0;

        while offset < content_len {
            let remaining = content_len - offset;
            let current_block_size = block_size.min(remaining);

            let block_data = content[offset..offset + current_block_size].to_vec();
            let is_last = offset + current_block_size >= content_len;

            let block = HashedBlock::from_data(block_data, index, offset, is_last, hasher)?;
            blocks.push(block);

            offset += current_block_size;
            index += 1;
        }

        Ok(blocks)
    }

    /// Process text content with line-aware splitting
    ///
    /// This method attempts to split blocks at line boundaries when possible
    /// to maintain text coherence while respecting block size limits.
    ///
    /// # Arguments
    /// * `content` - Text content to process
    /// * `hasher` - Hash function implementation
    ///
    /// # Returns
    /// Vector of hashed blocks or error if processing fails
    pub fn process_text_content<H>(
        &self,
        content: &str,
        hasher: &H,
    ) -> StorageResult<Vec<HashedBlock>>
    where
        H: crate::storage::ContentHasher + ?Sized,
    {
        if content.is_empty() {
            return Err(StorageError::BlockSize(
                "Empty content cannot be processed".to_string(),
            ));
        }

        let content_bytes = content.as_bytes();
        let content_len = content_bytes.len();
        let block_size = self.block_size.calculate_optimal(content_len);

        // For small content, just process as a single block
        if content_len <= block_size {
            let block = HashedBlock::from_data(content_bytes.to_vec(), 0, 0, true, hasher)?;
            return Ok(vec![block]);
        }

        let mut blocks = Vec::new();
        let mut offset = 0;
        let mut index = 0;

        while offset < content_len {
            let remaining = content_len - offset;
            let target_size = block_size.min(remaining);

            // Try to find a good line break position
            let block_end = if offset + target_size >= content_len {
                content_len
            } else {
                // Look for a line break within 10% of target size
                let search_start =
                    offset + (target_size * 9 / 10).max(target_size.saturating_sub(100));
                let search_end = offset + target_size;

                content_bytes[search_start..search_end]
                    .iter()
                    .position(|&b| b == b'\n')
                    .map(|pos| offset + search_start + pos + 1)
                    .unwrap_or(offset + target_size)
            };

            let block_data = content_bytes[offset..block_end].to_vec();
            let is_last = block_end >= content_len;

            let block = HashedBlock::from_data(block_data, index, offset, is_last, hasher)?;
            blocks.push(block);

            offset = block_end;
            index += 1;
        }

        Ok(blocks)
    }

    /// Get the configured block size
    pub fn block_size(&self) -> BlockSize {
        self.block_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::traits::ContentHasher;

    // Test hasher implementation
    struct TestHasher;

    impl ContentHasher for TestHasher {
        fn hash_block(&self, data: &[u8]) -> String {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let mut hasher = DefaultHasher::new();
            data.hash(&mut hasher);
            format!("{:x}", hasher.finish())
        }

        fn hash_nodes(&self, left: &str, right: &str) -> String {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let combined = format!("{}{}", left, right);
            let mut hasher = DefaultHasher::new();
            combined.hash(&mut hasher);
            format!("{:x}", hasher.finish())
        }

        fn algorithm_name(&self) -> &'static str {
            "test"
        }

        fn hash_length(&self) -> usize {
            16
        }
    }

    #[test]
    fn test_block_sizes() {
        assert_eq!(BlockSize::Small.size_bytes(), 1024);
        assert_eq!(BlockSize::Medium.size_bytes(), 4096);
        assert_eq!(BlockSize::Large.size_bytes(), 8192);
        assert_eq!(BlockSize::Custom(2048).size_bytes(), 2048);
        assert_eq!(
            BlockSize::Adaptive {
                min: 512,
                max: 2048
            }
            .size_bytes(),
            2048
        );
    }

    #[test]
    fn test_adaptive_block_size() {
        let adaptive = BlockSize::Adaptive {
            min: 1024,
            max: 4096,
        };

        // Very small content
        assert_eq!(adaptive.calculate_optimal(256), 512);
        assert_eq!(adaptive.calculate_optimal(800), 800);

        // Medium content
        assert_eq!(adaptive.calculate_optimal(2000), 2000);
        assert_eq!(adaptive.calculate_optimal(3000), 3000);

        // Large content
        assert_eq!(adaptive.calculate_optimal(5000), 4096);
        assert_eq!(adaptive.calculate_optimal(10000), 4096);
    }

    #[test]
    fn test_hashed_block_creation() {
        let data = b"Hello, World!";
        let hasher = TestHasher;
        let block = HashedBlock::from_data(data.to_vec(), 0, 0, true, &hasher).unwrap();

        assert_eq!(block.length, data.len());
        assert_eq!(block.index, 0);
        assert_eq!(block.offset, 0);
        assert!(block.is_last);
        assert_eq!(block.as_str(), Some("Hello, World!"));
        assert!(block.is_text());
    }

    #[test]
    fn test_block_processor() {
        let processor = BlockProcessor::new(BlockSize::Small); // 1KB blocks
        let content = "Hello, World! This is a test of the block processor.";
        let hasher = TestHasher;

        let blocks = processor.process_text_content(content, &hasher).unwrap();

        assert!(!blocks.is_empty());
        assert_eq!(blocks[0].as_str(), Some(content));
        assert!(blocks[0].is_last);
    }

    #[test]
    fn test_empty_content_error() {
        let processor = BlockProcessor::default();
        let hasher = TestHasher;

        assert!(processor.process_text_content("", &hasher).is_err());
        assert!(processor.process_content(&[], &hasher).is_err());
    }

    #[test]
    fn test_text_aware_splitting() {
        let processor = BlockProcessor::new(BlockSize::Small); // 1KB blocks
        let content = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5";
        let hasher = TestHasher;

        let blocks = processor.process_text_content(content, &hasher).unwrap();

        assert!(!blocks.is_empty());

        // Verify blocks contain complete lines
        for block in &blocks {
            if let Some(text) = block.as_str() {
                // Blocks should start with line number or be properly split
                assert!(text.starts_with("Line") || text.contains('\n'));
            }
        }
    }
}

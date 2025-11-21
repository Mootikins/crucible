//! Enhanced Parser Integration Bridge
//!
//! This module provides the enhanced integration between the Phase 1B parser and the new
//! Phase 2 content-addressed storage system with proper dependency inversion. It enables
//! seamless parsing with automatic storage integration, Merkle tree creation, and change detection.
//!
//! ## Architecture
//!
//! The enhanced bridge follows SOLID principles:
//! - **Single Responsibility**: Each component has a focused purpose
//! - **Open/Closed**: Extensible through trait implementations
//! - **Liskov Substitution**: All parsers can be used interchangeably
//! - **Interface Segregation**: Small, focused trait interfaces
//! - **Dependency Inversion**: Depends on abstractions, not concretions
//!
//! ## Key Components
//!
//! - **StorageAwareParser**: Enhanced parser with storage integration
//! - **ParserStorageCoordinator**: Coordinates parsing and storage operations
//! - **ContentAwareParsing**: Creates HashedBlocks from parsed content
//! - **Change Detection Integration**: Tracks parsing changes over time
//! - **Dependency Inversion**: Flexible trait-based architecture

use crate::hashing::blake3::Blake3Hasher;
use crate::parser::error::ParserResult;
use crate::parser::traits::{MarkdownParser, ParserCapabilities};
use crate::storage::builder::ContentAddressedStorageBuilder;
use crate::storage::diff::EnhancedChangeDetector;
use crate::storage::{
    BlockSize, ChangeSource, ContentAddressedStorage, ContentHasher, EnhancedTreeChange,
    HashedBlock, MerkleTree,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use crate::parser::types::ParsedNote;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

/// Configuration for storage-aware parsing
#[derive(Debug, Clone)]
pub struct StorageAwareParserConfig {
    /// Block size for content processing
    pub block_size: BlockSize,
    /// Enable automatic storage of parsed content
    pub enable_storage: bool,
    /// Enable automatic Merkle tree creation
    pub enable_merkle_trees: bool,
    /// Enable change detection
    pub enable_change_detection: bool,
    /// Enable content deduplication
    pub enable_deduplication: bool,
    /// Store parsing metadata alongside content
    pub store_metadata: bool,
    /// Enable parallel processing for large documents
    pub enable_parallel_processing: bool,
    /// Threshold for parallel processing (minimum note size)
    pub parallel_threshold: usize,
}

impl Default for StorageAwareParserConfig {
    fn default() -> Self {
        Self {
            block_size: BlockSize::Medium,
            enable_storage: true,
            enable_merkle_trees: true,
            enable_change_detection: true,
            enable_deduplication: true,
            store_metadata: true,
            enable_parallel_processing: true,
            parallel_threshold: 64 * 1024, // 64KB
        }
    }
}

/// Result type for storage-aware parsing operations
#[derive(Debug, Clone, Default)]
pub struct StorageAwareParseResult {
    /// The parsed note
    pub note: ParsedNote,
    /// Optional Merkle tree for content integrity
    pub merkle_tree: Option<MerkleTree>,
    /// Hashed blocks for efficient storage
    pub blocks: Vec<HashedBlock>,
    /// Content hash (root hash of Merkle tree or content hash)
    pub content_hash: String,
    /// Storage operation results
    pub storage_result: Option<StorageOperationResult>,
    /// Parsed content statistics
    pub statistics: ParseStatistics,
    /// Changes detected since last parse (if applicable)
    pub changes: Option<Vec<EnhancedTreeChange>>,
}

/// Statistics about the parsing operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseStatistics {
    /// Total parsing time in milliseconds
    pub parse_time_ms: u64,
    /// Total storage time in milliseconds
    pub storage_time_ms: u64,
    /// Number of blocks created
    pub block_count: usize,
    /// Total size of processed content in bytes
    pub content_size_bytes: usize,
    /// Number of unique blocks (after deduplication)
    pub unique_blocks: usize,
    /// Deduplication ratio (0.0 to 1.0)
    pub deduplication_ratio: f32,
    /// Timestamp of parsing operation
    pub parsed_at: DateTime<Utc>,
    /// Whether parallel processing was used
    pub parallel_processing_used: bool,
}

impl Default for ParseStatistics {
    fn default() -> Self {
        Self {
            parse_time_ms: 0,
            storage_time_ms: 0,
            block_count: 0,
            content_size_bytes: 0,
            unique_blocks: 0,
            deduplication_ratio: 0.0,
            parsed_at: Utc::now(),
            parallel_processing_used: false,
        }
    }
}

/// Results of storage operations
#[derive(Debug, Clone)]
pub struct StorageOperationResult {
    /// Root hash of stored content
    pub root_hash: String,
    /// Number of blocks stored
    pub blocks_stored: usize,
    /// Number of existing blocks found (deduplication)
    pub existing_blocks: usize,
    /// Storage operation duration in milliseconds
    pub storage_duration_ms: u64,
    /// Whether the Merkle tree was stored
    pub tree_stored: bool,
}

/// Trait for parsers with storage integration
///
/// This trait extends the base MarkdownParser trait with storage-aware capabilities
/// while maintaining backward compatibility through trait inheritance.
#[async_trait]
pub trait StorageAwareMarkdownParser: MarkdownParser + Send + Sync {
    /// Parse a file with storage integration
    ///
    /// # Arguments
    /// * `path` - Path to the file to parse
    /// * `storage` - Optional storage backend for content storage
    ///
    /// # Returns
    /// Enhanced parse result with storage information
    async fn parse_file_with_storage(
        &self,
        path: &Path,
        storage: Option<Arc<dyn ContentAddressedStorage>>,
    ) -> ParserResult<StorageAwareParseResult>;

    /// Parse content with storage integration
    ///
    /// # Arguments
    /// * `content` - Content to parse
    /// * `source_path` - Original path of the content
    /// * `storage` - Optional storage backend for content storage
    ///
    /// # Returns
    /// Enhanced parse result with storage information
    async fn parse_content_with_storage(
        &self,
        content: &str,
        source_path: &Path,
        storage: Option<Arc<dyn ContentAddressedStorage>>,
    ) -> ParserResult<StorageAwareParseResult>;

    /// Compare with previous parse result and detect changes
    ///
    /// # Arguments
    /// * `new_content` - New content to parse and compare
    /// * `source_path` - Path of the content
    /// * `previous_result` - Previous parse result to compare against
    /// * `storage` - Optional storage backend
    ///
    /// # Returns
    /// Enhanced parse result with change information
    async fn parse_and_compare(
        &self,
        new_content: &str,
        source_path: &Path,
        previous_result: &StorageAwareParseResult,
        storage: Option<Arc<dyn ContentAddressedStorage>>,
    ) -> ParserResult<StorageAwareParseResult>;

    /// Get the parser configuration
    fn config(&self) -> &StorageAwareParserConfig;

    /// Update the parser configuration
    fn set_config(&mut self, config: StorageAwareParserConfig);
}

/// Enhanced parser implementation with storage integration
///
/// This implementation wraps a base MarkdownParser and adds storage-aware capabilities
/// including automatic Merkle tree creation, content-addressed storage, and change detection.
pub struct StorageAwareParser {
    /// Base parser implementation
    base_parser: Box<dyn MarkdownParser>,
    /// Storage-aware parser configuration
    config: StorageAwareParserConfig,
    /// Content hasher for block processing
    hasher: Arc<dyn ContentHasher>,
    /// Enhanced change detector
    #[allow(dead_code)] // Reserved for future change detection improvements
    change_detector: EnhancedChangeDetector,
}

impl StorageAwareParser {
    /// Create a new storage-aware parser with default configuration
    ///
    /// # Arguments
    /// * `base_parser` - Base parser implementation to wrap
    ///
    /// # Returns
    /// New storage-aware parser instance
    pub fn new(base_parser: Box<dyn MarkdownParser>) -> Self {
        Self {
            base_parser,
            config: StorageAwareParserConfig::default(),
            hasher: Arc::new(Blake3Hasher::new()),
            change_detector: EnhancedChangeDetector::new(),
        }
    }

    /// Create a storage-aware parser with custom configuration
    ///
    /// # Arguments
    /// * `base_parser` - Base parser implementation to wrap
    /// * `config` - Parser configuration
    /// * `hasher` - Custom hasher implementation
    ///
    /// # Returns
    /// New storage-aware parser instance
    pub fn with_config(
        base_parser: Box<dyn MarkdownParser>,
        config: StorageAwareParserConfig,
        hasher: Arc<dyn ContentHasher>,
    ) -> Self {
        Self {
            base_parser,
            config,
            hasher,
            change_detector: EnhancedChangeDetector::new(),
        }
    }

    /// Create a storage-aware parser with storage backend builder
    ///
    /// # Arguments
    /// * `base_parser` - Base parser implementation to wrap
    /// * `storage_builder` - Storage builder for creating storage backends
    /// * `config` - Parser configuration
    ///
    /// # Returns
    /// New storage-aware parser instance
    pub fn with_storage_builder(
        base_parser: Box<dyn MarkdownParser>,
        _storage_builder: &ContentAddressedStorageBuilder,
        config: StorageAwareParserConfig,
    ) -> Self {
        // Extract hasher from builder or use default
        let hasher = Arc::new(Blake3Hasher::new());

        Self {
            base_parser,
            config,
            hasher,
            change_detector: EnhancedChangeDetector::new(),
        }
    }

    /// Parse content into hashed blocks
    ///
    /// # Arguments
    /// * `content` - Content to process into blocks
    ///
    /// # Returns
    /// Vector of hashed blocks
    fn create_hashed_blocks(&self, content: &str) -> ParserResult<Vec<HashedBlock>> {
        let content_bytes = content.as_bytes();
        let block_size = self
            .config
            .block_size
            .calculate_optimal(content_bytes.len());

        if content_bytes.is_empty() {
            return Err(crate::parser::ParserError::ParseFailed(
                "Cannot create blocks from empty content".to_string(),
            ));
        }

        let mut blocks = Vec::new();
        let mut offset = 0;

        while offset < content_bytes.len() {
            let remaining = content_bytes.len() - offset;
            let chunk_size = block_size.min(remaining);
            let chunk_end = offset + chunk_size;

            let chunk_data = content_bytes[offset..chunk_end].to_vec();
            let is_last = chunk_end == content_bytes.len();

            let block =
                HashedBlock::from_data(chunk_data, blocks.len(), offset, is_last, &*self.hasher)
                    .map_err(|e| {
                        crate::parser::ParserError::ParseFailed(format!(
                            "Failed to create hashed block: {}",
                            e
                        ))
                    })?;

            blocks.push(block);
            offset = chunk_end;
        }

        Ok(blocks)
    }

    /// Create Merkle tree from hashed blocks
    ///
    /// # Arguments
    /// * `blocks` - Hashed blocks to create tree from
    ///
    /// # Returns
    /// Merkle tree or error if creation fails
    fn create_merkle_tree(&self, blocks: &[HashedBlock]) -> ParserResult<MerkleTree> {
        if blocks.is_empty() {
            return Err(crate::parser::ParserError::ParseFailed(
                "Cannot create Merkle tree from empty blocks".to_string(),
            ));
        }

        MerkleTree::from_blocks(blocks, &*self.hasher).map_err(|e| {
            crate::parser::ParserError::ParseFailed(format!(
                "Failed to create Merkle tree: {}",
                e
            ))
        })
    }

    /// Store content and metadata in storage backend
    ///
    /// # Arguments
    /// * `blocks` - Blocks to store
    /// * `tree` - Merkle tree to store
    /// * `note` - Parsed note metadata
    /// * `storage` - Storage backend
    ///
    /// # Returns
    /// Storage operation result
    async fn store_content(
        &self,
        blocks: &[HashedBlock],
        tree: &MerkleTree,
        _document: &ParsedNote,
        storage: Arc<dyn ContentAddressedStorage>,
    ) -> ParserResult<StorageOperationResult> {
        let start_time = SystemTime::now();

        // Store blocks
        let mut blocks_stored = 0;
        let mut existing_blocks = 0;

        for block in blocks {
            match storage.block_exists(&block.hash).await {
                Ok(exists) => {
                    if !exists {
                        if let Err(e) = storage.store_block(&block.hash, &block.data).await {
                            return Err(crate::parser::ParserError::ParseFailed(format!(
                                "Failed to store block: {}",
                                e
                            )));
                        }
                        blocks_stored += 1;
                    } else {
                        existing_blocks += 1;
                    }
                }
                Err(e) => {
                    return Err(crate::parser::ParserError::ParseFailed(format!(
                        "Failed to check block existence: {}",
                        e
                    )));
                }
            }
        }

        // Store Merkle tree
        let tree_stored = if let Err(e) = storage.store_tree(&tree.root_hash, tree).await {
            return Err(crate::parser::ParserError::ParseFailed(format!(
                "Failed to store Merkle tree: {}",
                e
            )));
        } else {
            true
        };

        let storage_duration = SystemTime::now()
            .duration_since(start_time)
            .unwrap_or_default()
            .as_millis() as u64;

        Ok(StorageOperationResult {
            root_hash: tree.root_hash.clone(),
            blocks_stored,
            existing_blocks,
            storage_duration_ms: storage_duration,
            tree_stored,
        })
    }

    /// Detect changes compared to previous result
    ///
    /// # Arguments
    /// * `new_blocks` - New blocks
    /// * `new_tree` - New Merkle tree
    /// * `previous_result` - Previous parse result
    ///
    /// # Returns
    /// Detected changes
    async fn detect_changes(
        &self,
        _new_blocks: &[HashedBlock],
        new_tree: &MerkleTree,
        previous_result: &StorageAwareParseResult,
    ) -> ParserResult<Option<Vec<EnhancedTreeChange>>> {
        if !self.config.enable_change_detection {
            return Ok(None);
        }

        if let Some(previous_tree) = &previous_result.merkle_tree {
            let changes = new_tree
                .compare_enhanced(previous_tree, &*self.hasher, ChangeSource::UserEdit)
                .map_err(|e| {
                    crate::parser::ParserError::ParseFailed(format!(
                        "Failed to detect changes: {}",
                        e
                    ))
                })?;

            Ok(Some(changes))
        } else {
            Ok(None)
        }
    }

    /// Calculate parse statistics
    ///
    /// # Arguments
    /// * `blocks` - Processed blocks
    /// * `parse_duration_ms` - Parsing time
    /// * `storage_duration_ms` - Storage time
    /// * `parallel_used` - Whether parallel processing was used
    ///
    /// # Returns
    /// Parse statistics
    fn calculate_statistics(
        &self,
        blocks: &[HashedBlock],
        parse_duration_ms: u64,
        storage_duration_ms: u64,
        parallel_used: bool,
    ) -> ParseStatistics {
        let content_size: usize = blocks.iter().map(|b| b.length).sum();
        let unique_hashes: std::collections::HashSet<_> = blocks.iter().map(|b| &b.hash).collect();
        let unique_blocks = unique_hashes.len();
        let deduplication_ratio = if blocks.is_empty() {
            1.0
        } else {
            1.0 - (unique_blocks as f32 / blocks.len() as f32)
        };

        ParseStatistics {
            parse_time_ms: parse_duration_ms,
            storage_time_ms: storage_duration_ms,
            block_count: blocks.len(),
            content_size_bytes: content_size,
            unique_blocks,
            deduplication_ratio,
            parsed_at: Utc::now(),
            parallel_processing_used: parallel_used,
        }
    }
}

#[async_trait]
impl MarkdownParser for StorageAwareParser {
    async fn parse_file(&self, path: &Path) -> ParserResult<ParsedNote> {
        self.base_parser.parse_file(path).await
    }

    async fn parse_content(&self, content: &str, source_path: &Path) -> ParserResult<ParsedNote> {
        self.base_parser.parse_content(content, source_path).await
    }

    fn capabilities(&self) -> ParserCapabilities {
        self.base_parser.capabilities()
    }

    fn can_parse(&self, path: &Path) -> bool {
        self.base_parser.can_parse(path)
    }
}

#[async_trait]
impl StorageAwareMarkdownParser for StorageAwareParser {
    async fn parse_file_with_storage(
        &self,
        path: &Path,
        storage: Option<Arc<dyn ContentAddressedStorage>>,
    ) -> ParserResult<StorageAwareParseResult> {
        let start_time = SystemTime::now();

        // Parse the file using base parser
        let note = self.base_parser.parse_file(path).await?;

        // Extract plain text content for block processing
        let content = &note.content.plain_text;

        // Create hashed blocks
        let blocks = if self.config.enable_storage || self.config.enable_merkle_trees {
            self.create_hashed_blocks(content)?
        } else {
            Vec::new()
        };

        // Create Merkle tree
        let merkle_tree = if self.config.enable_merkle_trees && !blocks.is_empty() {
            Some(self.create_merkle_tree(&blocks)?)
        } else {
            None
        };

        // Store content if storage is provided
        let storage_result = if let Some(storage_backend) = storage {
            if self.config.enable_storage && !blocks.is_empty() {
                if let Some(ref tree) = merkle_tree {
                    Some(
                        self.store_content(&blocks, tree, &note, storage_backend)
                            .await?,
                    )
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let parse_duration = SystemTime::now()
            .duration_since(start_time)
            .unwrap_or_default()
            .as_millis() as u64;

        let storage_duration = storage_result
            .as_ref()
            .map(|r| r.storage_duration_ms)
            .unwrap_or(0);

        // Calculate content hash
        let content_hash = if let Some(ref tree) = merkle_tree {
            tree.root_hash.clone()
        } else if let Some(ref result) = storage_result {
            result.root_hash.clone()
        } else {
            ContentHasher::hash_block(self.hasher.as_ref(), content.as_bytes())
        };

        // Calculate statistics
        let statistics = self.calculate_statistics(
            &blocks,
            parse_duration,
            storage_duration,
            false, // No parallel processing for single file
        );

        Ok(StorageAwareParseResult {
            note,
            merkle_tree,
            blocks,
            content_hash,
            storage_result,
            statistics,
            changes: None, // No previous result to compare with
        })
    }

    async fn parse_content_with_storage(
        &self,
        content: &str,
        source_path: &Path,
        storage: Option<Arc<dyn ContentAddressedStorage>>,
    ) -> ParserResult<StorageAwareParseResult> {
        let start_time = SystemTime::now();

        // Parse the content using base parser
        let note = self.base_parser.parse_content(content, source_path).await?;

        // Create hashed blocks
        let blocks = if self.config.enable_storage || self.config.enable_merkle_trees {
            self.create_hashed_blocks(content)?
        } else {
            Vec::new()
        };

        // Create Merkle tree
        let merkle_tree = if self.config.enable_merkle_trees && !blocks.is_empty() {
            Some(self.create_merkle_tree(&blocks)?)
        } else {
            None
        };

        // Store content if storage is provided
        let storage_result = if let Some(storage_backend) = storage {
            if self.config.enable_storage && !blocks.is_empty() {
                if let Some(ref tree) = merkle_tree {
                    Some(
                        self.store_content(&blocks, tree, &note, storage_backend)
                            .await?,
                    )
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let parse_duration = SystemTime::now()
            .duration_since(start_time)
            .unwrap_or_default()
            .as_millis() as u64;

        let storage_duration = storage_result
            .as_ref()
            .map(|r| r.storage_duration_ms)
            .unwrap_or(0);

        // Calculate content hash
        let content_hash = if let Some(ref tree) = merkle_tree {
            tree.root_hash.clone()
        } else if let Some(ref result) = storage_result {
            result.root_hash.clone()
        } else {
            ContentHasher::hash_block(self.hasher.as_ref(), content.as_bytes())
        };

        // Calculate statistics
        let statistics = self.calculate_statistics(
            &blocks,
            parse_duration,
            storage_duration,
            false, // No parallel processing in current implementation
        );

        Ok(StorageAwareParseResult {
            note,
            merkle_tree,
            blocks,
            content_hash,
            storage_result,
            statistics,
            changes: None, // No previous result to compare with
        })
    }

    async fn parse_and_compare(
        &self,
        new_content: &str,
        source_path: &Path,
        previous_result: &StorageAwareParseResult,
        storage: Option<Arc<dyn ContentAddressedStorage>>,
    ) -> ParserResult<StorageAwareParseResult> {
        let start_time = SystemTime::now();

        // Parse the new content
        let note = self.base_parser.parse_content(new_content, source_path).await?;

        // Create hashed blocks
        let blocks = if self.config.enable_storage || self.config.enable_merkle_trees {
            self.create_hashed_blocks(new_content)?
        } else {
            Vec::new()
        };

        // Create Merkle tree
        let merkle_tree = if self.config.enable_merkle_trees && !blocks.is_empty() {
            Some(self.create_merkle_tree(&blocks)?)
        } else {
            None
        };

        // Detect changes
        let changes =
            if let (Some(ref tree), true) = (&merkle_tree, self.config.enable_change_detection) {
                self.detect_changes(&blocks, tree, previous_result).await?
            } else {
                None
            };

        // Store content if storage is provided
        let storage_result = if let Some(storage_backend) = storage {
            if self.config.enable_storage && !blocks.is_empty() {
                if let Some(ref tree) = merkle_tree {
                    Some(
                        self.store_content(&blocks, tree, &note, storage_backend)
                            .await?,
                    )
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let parse_duration = SystemTime::now()
            .duration_since(start_time)
            .unwrap_or_default()
            .as_millis() as u64;

        let storage_duration = storage_result
            .as_ref()
            .map(|r| r.storage_duration_ms)
            .unwrap_or(0);

        // Calculate content hash
        let content_hash = if let Some(ref tree) = merkle_tree {
            tree.root_hash.clone()
        } else if let Some(ref result) = storage_result {
            result.root_hash.clone()
        } else {
            self.hasher.hash_block(new_content.as_bytes())
        };

        // Calculate statistics
        let statistics = self.calculate_statistics(
            &blocks,
            parse_duration,
            storage_duration,
            false, // No parallel processing in current implementation
        );

        Ok(StorageAwareParseResult {
            note,
            merkle_tree,
            blocks,
            content_hash,
            storage_result,
            statistics,
            changes,
        })
    }

    fn config(&self) -> &StorageAwareParserConfig {
        &self.config
    }

    fn set_config(&mut self, config: StorageAwareParserConfig) {
        self.config = config;
    }
}

/// Factory functions for creating storage-aware parsers
pub mod factory {
    use super::*;
    // use crate::parser::bridge::ParserAdapter; // disabled

    /// Create a storage-aware parser with default configuration
    ///
    /// # Returns
    /// New storage-aware parser instance
    pub fn create_storage_aware_parser() -> impl StorageAwareMarkdownParser {
        let base_parser = Box::new(crate::parser::pulldown::PulldownParser::new());
        StorageAwareParser::new(base_parser)
    }

    /// Create a storage-aware parser with custom configuration
    ///
    /// # Arguments
    /// * `config` - Parser configuration
    ///
    /// # Returns
    /// New storage-aware parser instance
    pub fn create_storage_aware_parser_with_config(
        config: StorageAwareParserConfig,
    ) -> impl StorageAwareMarkdownParser {
        let base_parser = Box::new(crate::parser::pulldown::PulldownParser::new());
        let hasher = Arc::new(Blake3Hasher::new());
        StorageAwareParser::with_config(base_parser, config, hasher)
    }

    /// Create a storage-aware parser with custom hasher
    ///
    /// # Arguments
    /// * `hasher` - Custom hasher implementation
    ///
    /// # Returns
    /// New storage-aware parser instance
    pub fn create_storage_aware_parser_with_hasher(
        hasher: Arc<dyn ContentHasher>,
    ) -> impl StorageAwareMarkdownParser {
        let base_parser = Box::new(crate::parser::pulldown::PulldownParser::new());
        StorageAwareParser::with_config(base_parser, StorageAwareParserConfig::default(), hasher)
    }

    /// Create a storage-aware parser from a base parser
    ///
    /// # Arguments
    /// * `base_parser` - Base parser implementation
    ///
    /// # Returns
    /// New storage-aware parser instance
    pub fn create_storage_aware_parser_from_base(
        base_parser: Box<dyn MarkdownParser>,
    ) -> impl StorageAwareMarkdownParser {
        StorageAwareParser::new(base_parser)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // use crate::parser::bridge::ParserAdapter; // disabled
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    /// Mock hasher for testing
    #[derive(Debug, Clone)]
    struct MockContentHasher {
        name: String,
    }

    impl MockContentHasher {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
            }
        }
    }

    impl ContentHasher for MockContentHasher {
        fn hash_block(&self, data: &[u8]) -> String {
            let mut hasher = DefaultHasher::new();
            data.hash(&mut hasher);
            format!("mock_{}_{:x}", self.name, hasher.finish())
        }

        fn hash_nodes(&self, left: &str, right: &str) -> String {
            let combined = format!("{}:{}", left, right);
            let mut hasher = DefaultHasher::new();
            combined.hash(&mut hasher);
            format!("mock_node_{}_{:x}", self.name, hasher.finish())
        }

        fn algorithm_name(&self) -> &'static str {
            "mock"
        }

        fn hash_length(&self) -> usize {
            16
        }
    }

    #[test]
    fn test_storage_aware_parser_config_default() {
        let config = StorageAwareParserConfig::default();

        assert_eq!(config.block_size, BlockSize::Medium);
        assert!(config.enable_storage);
        assert!(config.enable_merkle_trees);
        assert!(config.enable_change_detection);
        assert!(config.enable_deduplication);
        assert!(config.store_metadata);
        assert!(config.enable_parallel_processing);
        assert_eq!(config.parallel_threshold, 64 * 1024);
    }

    #[test]
    fn test_storage_aware_parser_creation() {
        let base_parser = Box::new(crate::parser::pulldown::PulldownParser::new());
        let parser = StorageAwareParser::new(base_parser);

        assert!(parser.config.enable_storage);
        assert!(parser.config.enable_merkle_trees);
        assert_eq!(parser.config.block_size, BlockSize::Medium);
    }

    #[test]
    fn test_storage_aware_parser_with_config() {
        let base_parser = Box::new(crate::parser::pulldown::PulldownParser::new());
        let config = StorageAwareParserConfig {
            enable_storage: false,
            enable_merkle_trees: false,
            block_size: BlockSize::Small,
            ..Default::default()
        };

        let parser = StorageAwareParser::with_config(
            base_parser,
            config.clone(),
            Arc::new(MockContentHasher::new("test")),
        );

        assert!(!parser.config.enable_storage);
        assert!(!parser.config.enable_merkle_trees);
        assert_eq!(parser.config.block_size, BlockSize::Small);
    }

    #[test]
    fn test_create_hashed_blocks() {
        // use crate::parser::bridge::ParserAdapter; // disabled
        let base_parser = Box::new(crate::parser::pulldown::PulldownParser::new());
        let parser = StorageAwareParser::new(base_parser);
        let content = "Hello, World! This is a test.";

        let result = parser.create_hashed_blocks(content);
        assert!(result.is_ok());

        let blocks = result.unwrap();
        assert!(!blocks.is_empty());

        // Check that blocks have correct properties
        for (i, block) in blocks.iter().enumerate() {
            assert_eq!(block.index, i);
            assert!(!block.hash.is_empty());
            assert!(!block.data.is_empty());
        }

        // Check that the last block is marked correctly
        assert!(blocks.last().unwrap().is_last);
    }

    #[test]
    fn test_create_hashed_blocks_empty_content() {
        // use crate::parser::bridge::ParserAdapter; // disabled
        let base_parser = Box::new(crate::parser::pulldown::PulldownParser::new());
        let parser = StorageAwareParser::new(base_parser);
        let content = "";

        let result = parser.create_hashed_blocks(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_create_merkle_tree() {
        // use crate::parser::bridge::ParserAdapter; // disabled
        let base_parser = Box::new(crate::parser::pulldown::PulldownParser::new());
        let parser = StorageAwareParser::new(base_parser);
        let content = "Hello, World! This is a test for Merkle tree creation.";

        let blocks = parser.create_hashed_blocks(content).unwrap();
        let result = parser.create_merkle_tree(&blocks);

        assert!(result.is_ok());

        let tree = result.unwrap();
        assert!(!tree.root_hash.is_empty());
        assert_eq!(tree.block_count, blocks.len());
        assert_eq!(tree.leaf_hashes.len(), blocks.len());
    }

    #[test]
    fn test_create_merkle_tree_empty_blocks() {
        // use crate::parser::bridge::ParserAdapter; // disabled
        let base_parser = Box::new(crate::parser::pulldown::PulldownParser::new());
        let parser = StorageAwareParser::new(base_parser);
        let blocks: Vec<HashedBlock> = vec![];

        let result = parser.create_merkle_tree(&blocks);
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_parse_content_with_storage_no_backend() {
        let parser = factory::create_storage_aware_parser();
        let content = "# Test Note\n\nThis is a test note with some content.";
        let source_path = Path::new("test.md");

        let result = parser
            .parse_content_with_storage(content, source_path, None)
            .await;

        assert!(result.is_ok());
        let parse_result = result.unwrap();

        // Check note structure
        assert!(!parse_result.note.content.plain_text.is_empty());
        assert_eq!(parse_result.note.path, source_path);

        // Check blocks
        assert!(!parse_result.blocks.is_empty());

        // Check Merkle tree
        assert!(parse_result.merkle_tree.is_some());
        assert!(!parse_result.content_hash.is_empty());

        // Check statistics
        assert!(parse_result.statistics.parse_time_ms > 0);
        assert_eq!(
            parse_result.statistics.block_count,
            parse_result.blocks.len()
        );
        assert!(parse_result.statistics.content_size_bytes > 0);

        // No storage backend provided, so no storage result
        assert!(parse_result.storage_result.is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_parse_and_compare() {
        let parser = factory::create_storage_aware_parser();
        let content1 = "# Version 1\n\nThis is the first version.";
        let content2 = "# Version 2\n\nThis is the second version with changes.";
        let source_path = Path::new("test.md");

        // Parse initial version
        let result1 = parser
            .parse_content_with_storage(content1, source_path, None)
            .await
            .unwrap();

        // Parse and compare with new version
        let result2 = parser
            .parse_and_compare(content2, source_path, &result1, None)
            .await
            .unwrap();

        // Check that both results have structure
        assert!(!result2.note.content.plain_text.is_empty());
        assert!(!result2.blocks.is_empty());
        assert!(result2.merkle_tree.is_some());

        // Changes should be detected
        assert!(result2.changes.is_some());
        let changes = result2.changes.unwrap();
        assert!(!changes.is_empty());
    }

    #[test]
    fn test_factory_functions() {
        // Test default factory
        let parser1 = factory::create_storage_aware_parser();
        assert!(parser1.config().enable_storage);

        // Test config factory
        let config = StorageAwareParserConfig {
            enable_storage: false,
            ..Default::default()
        };
        let parser2 = factory::create_storage_aware_parser_with_config(config);
        assert!(!parser2.config().enable_storage);

        // Test hasher factory
        let mock_hasher = Arc::new(MockContentHasher::new("factory_test"));
        let parser3 = factory::create_storage_aware_parser_with_hasher(mock_hasher);
        assert!(parser3.config().enable_storage);
    }

    #[test]
    fn test_calculate_statistics() {
        // use crate::parser::bridge::ParserAdapter; // disabled
        let base_parser = Box::new(crate::parser::pulldown::PulldownParser::new());
        let parser = StorageAwareParser::new(base_parser);
        let content = "Test content for statistics calculation.";
        let blocks = parser.create_hashed_blocks(content).unwrap();

        let stats = parser.calculate_statistics(&blocks, 100, 50, false);

        assert_eq!(stats.parse_time_ms, 100);
        assert_eq!(stats.storage_time_ms, 50);
        assert_eq!(stats.block_count, blocks.len());
        assert_eq!(stats.unique_blocks, blocks.len());
        assert_eq!(stats.deduplication_ratio, 0.0);
        assert!(!stats.parallel_processing_used);
    }

    #[test]
    fn test_storage_aware_parser_trait_compatibility() {
        // Test that the implementation satisfies the trait bounds
        fn assert_storage_aware_parser<T: StorageAwareMarkdownParser>(_: T) {}

        let parser = factory::create_storage_aware_parser();
        assert_storage_aware_parser(parser);
    }

    #[test]
    fn test_parse_statistics_serialization() {
        let stats = ParseStatistics {
            parse_time_ms: 100,
            storage_time_ms: 50,
            block_count: 5,
            content_size_bytes: 1024,
            unique_blocks: 5,
            deduplication_ratio: 0.0,
            parsed_at: Utc::now(),
            parallel_processing_used: false,
        };

        // Test that we can serialize and deserialize
        let json = serde_json::to_string(&stats).unwrap();
        let deserialized: ParseStatistics = serde_json::from_str(&json).unwrap();

        assert_eq!(stats.parse_time_ms, deserialized.parse_time_ms);
        assert_eq!(stats.block_count, deserialized.block_count);
        assert_eq!(stats.content_size_bytes, deserialized.content_size_bytes);
    }
}

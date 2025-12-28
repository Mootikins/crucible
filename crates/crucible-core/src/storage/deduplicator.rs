//! Block Deduplication System
//!
//! This module provides functionality for detecting and managing duplicate blocks
//! across documents in the knowledge management system. It enables storage optimization
//! by identifying identical content blocks and tracking their reuse patterns.

use crate::storage::StorageResult;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Trait for deduplication operations
#[async_trait]
pub trait Deduplicator: Send + Sync {
    /// Analyze blocks for duplicates and deduplication opportunities
    async fn analyze_blocks(&self, block_hashes: &[String])
        -> StorageResult<DeduplicationAnalysis>;

    /// Get comprehensive deduplication statistics
    async fn get_deduplication_stats(&self) -> StorageResult<DeduplicationStats>;

    /// Find duplicate blocks above a threshold
    async fn find_duplicates(&self, min_occurrences: usize) -> StorageResult<Vec<DuplicateGroup>>;

    /// Calculate storage savings from deduplication
    async fn calculate_storage_savings(&self) -> StorageResult<StorageSavings>;

    /// Get block usage patterns
    async fn get_block_usage_patterns(
        &self,
        block_hashes: &[String],
    ) -> StorageResult<Vec<BlockUsagePattern>>;
}

/// Analysis result for a set of blocks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeduplicationAnalysis {
    /// Total blocks analyzed
    pub total_blocks: usize,
    /// Unique blocks (no duplicates)
    pub unique_blocks: usize,
    /// Duplicate blocks
    pub duplicate_blocks: usize,
    /// Total occurrences including duplicates
    pub total_occurrences: usize,
    /// Deduplication ratio (0.0 to 1.0)
    pub deduplication_ratio: f64,
    /// Duplicate groups found
    pub duplicate_groups: Vec<DuplicateGroup>,
    /// Blocks analyzed
    pub analyzed_blocks: Vec<String>,
    /// Analysis timestamp
    pub analyzed_at: DateTime<Utc>,
}

/// Group of identical blocks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    /// Block hash
    pub block_hash: String,
    /// Number of occurrences
    pub occurrence_count: usize,
    /// Documents containing this block
    pub documents: Vec<String>,
    /// Estimated storage size of this block
    pub estimated_block_size: usize,
    /// Storage saved by deduplication
    pub storage_saved: usize,
    /// First seen timestamp
    pub first_seen: DateTime<Utc>,
    /// Last seen timestamp
    pub last_seen: DateTime<Utc>,
}

/// Comprehensive deduplication statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeduplicationStats {
    /// Total unique blocks in system
    pub total_unique_blocks: usize,
    /// Total block instances (including duplicates)
    pub total_block_instances: usize,
    /// Number of duplicate blocks
    pub duplicate_blocks: usize,
    /// Overall deduplication ratio
    pub deduplication_ratio: f64,
    /// Total storage saved by deduplication
    pub total_storage_saved: usize,
    /// Most duplicated blocks
    pub most_duplicated_blocks: Vec<DuplicateGroup>,
    /// Block type distribution
    pub block_type_distribution: HashMap<String, usize>,
    /// Note duplication patterns
    pub document_patterns: Vec<DocumentDuplicationPattern>,
    /// Statistics calculated at
    pub calculated_at: DateTime<Utc>,
}

/// Storage savings information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSavings {
    /// Total bytes saved by deduplication
    pub total_bytes_saved: usize,
    /// Percentage of storage saved
    pub percentage_saved: f64,
    /// Potential future savings
    pub potential_savings: usize,
    /// By block type
    pub savings_by_type: HashMap<String, StorageSavingsByType>,
    /// Calculated at
    pub calculated_at: DateTime<Utc>,
}

/// Storage savings for a specific block type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSavingsByType {
    /// Block type name
    pub block_type: String,
    /// Bytes saved
    pub bytes_saved: usize,
    /// Percentage saved for this type
    pub percentage_saved: f64,
    /// Number of blocks of this type
    pub block_count: usize,
}

/// Usage pattern for a specific block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockUsagePattern {
    /// Block hash
    pub block_hash: String,
    /// Usage frequency (times used)
    pub usage_frequency: usize,
    /// Documents using this block
    pub documents: Vec<String>,
    /// Usage timeline
    pub usage_timeline: Vec<UsageEvent>,
    /// Block type
    pub block_type: String,
    /// Content preview (first 100 chars)
    pub content_preview: String,
}

/// Single usage event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageEvent {
    /// Note ID
    pub document_id: String,
    /// Timestamp when block was added to note
    pub timestamp: DateTime<Utc>,
    /// Event type (added, modified, referenced)
    pub event_type: UsageEventType,
}

/// Types of usage events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UsageEventType {
    /// Block was added to note
    Added,
    /// Block was modified in note
    Modified,
    /// Block was referenced by another note
    Referenced,
}

/// Note duplication pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentDuplicationPattern {
    /// Note ID
    pub document_id: String,
    /// Total blocks in note
    pub total_blocks: usize,
    /// Unique blocks (not shared with other documents)
    pub unique_blocks: usize,
    /// Shared blocks (also in other documents)
    pub shared_blocks: usize,
    /// Duplication ratio (shared / total)
    pub duplication_ratio: f64,
    /// Most similar documents
    pub similar_documents: Vec<DocumentSimilarity>,
}

/// Similarity between two documents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSimilarity {
    /// Similar note ID
    pub document_id: String,
    /// Number of shared blocks
    pub shared_blocks: usize,
    /// Similarity score (0.0 to 1.0)
    pub similarity_score: f64,
}


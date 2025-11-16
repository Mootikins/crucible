//! Enrichment Pipeline
//!
//! This module implements the enrichment pipeline as defined in
//! ARCHITECTURE.md, following clean architecture principles with proper
//! separation of concerns.
//!
//! ## Five-Phase Architecture
//!
//! 1. **Quick Filter**: File date + BLAKE3 hash check (skip if unchanged)
//! 2. **Parse**: Full file parse to AST using crucible-parser
//! 3. **Merkle Diff**: Build Merkle tree, diff with stored tree, identify changed blocks
//! 4. **Enrich**: Call EnrichmentService with changed block list
//! 5. **Store**: Store EnrichedNote in single transaction
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_enrichment::{EnrichmentPipeline, DefaultEnrichmentService};
//! use std::path::Path;
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create enrichment service with provider
//!     let enrichment_service = Arc::new(DefaultEnrichmentService::without_embeddings());
//!
//!     // Create processor
//!     let processor = EnrichmentPipeline::new(enrichment_service);
//!
//!     // Process a note
//!     let note_path = Path::new("notes/example.md");
//!     let result = processor.process(note_path).await?;
//!
//!     println!("Processed note with {} embeddings", result.enriched.embeddings.len());
//!     Ok(())
//! }
//! ```

use crate::EnrichedNote;
use crucible_core::hashing::BLAKE3_HASHER;
use crucible_core::merkle::HybridMerkleTree;
use crucible_core::traits::ContentHasher;
use anyhow::{Context, Result};
use crucible_parser::{CrucibleParser, ParsedNote, traits::MarkdownParserImplementation};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info};

/// Result of note processing through all five phases
#[derive(Debug)]
pub struct EnrichmentResult {
    /// The enriched note ready for storage
    pub enriched: EnrichedNote,

    /// Metrics from each processing phase
    pub metrics: ProcessingMetrics,

    /// List of block IDs that changed (drove enrichment)
    pub changed_blocks: Vec<String>,

    /// Whether this was a full re-process or incremental update
    pub was_incremental: bool,
}

/// Metrics collected during note processing
#[derive(Debug, Clone, Default)]
pub struct ProcessingMetrics {
    /// Phase 1: Quick filter time
    pub phase1_filter_duration: Duration,

    /// Phase 2: Parse duration
    pub phase2_parse_duration: Duration,

    /// Phase 3: Merkle diff duration
    pub phase3_merkle_duration: Duration,

    /// Phase 4: Enrichment duration
    pub phase4_enrich_duration: Duration,

    /// Total processing time
    pub total_duration: Duration,

    /// File hash from Phase 1
    pub file_hash: Option<String>,

    /// Whether Phase 1 quick filter indicated changes
    pub file_changed: bool,

    /// Number of blocks processed in enrichment
    pub blocks_enriched: usize,

    /// Number of changed blocks identified by Merkle diff
    pub changed_blocks_count: usize,
}

/// Configuration for the enrichment pipeline
#[derive(Debug, Clone)]
pub struct EnrichmentPipelineConfig {
    /// Whether to enable Phase 1 quick filter optimization
    /// (file date + BLAKE3 hash check to skip unchanged files)
    pub enable_quick_filter: bool,
}

impl Default for EnrichmentPipelineConfig {
    fn default() -> Self {
        Self {
            enable_quick_filter: true,
        }
    }
}

/// Five-phase note processor
///
/// Orchestrates the complete enrichment pipeline from file input
/// to enriched note output ready for storage.
pub struct EnrichmentPipeline {
    /// Enrichment service for Phase 4
    enrichment_service: Arc<dyn crucible_core::enrichment::EnrichmentService>,

    /// Parser for Phase 2
    parser: CrucibleParser,

    /// Processor configuration
    config: EnrichmentPipelineConfig,
}

impl EnrichmentPipeline {
    /// Create a new note processor with the given enrichment service
    ///
    /// # Arguments
    ///
    /// * `enrichment_service` - Service for embedding generation and metadata extraction
    ///
    /// # Example
    ///
    /// ```rust
    /// use crucible_enrichment::{EnrichmentPipeline, DefaultEnrichmentService};
    /// use std::sync::Arc;
    ///
    /// let service = Arc::new(DefaultEnrichmentService::without_embeddings());
    /// let processor = EnrichmentPipeline::new(service);
    /// ```
    pub fn new(enrichment_service: Arc<dyn crucible_core::enrichment::EnrichmentService>) -> Self {
        Self {
            enrichment_service,
            parser: CrucibleParser::new(),
            config: EnrichmentPipelineConfig::default(),
        }
    }

    /// Create a processor with custom configuration
    pub fn with_config(enrichment_service: Arc<dyn crucible_core::enrichment::EnrichmentService>, config: EnrichmentPipelineConfig) -> Self {
        Self {
            enrichment_service,
            parser: CrucibleParser::new(),
            config,
        }
    }

    /// Process a note through all five phases
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the markdown file to process
    ///
    /// # Returns
    ///
    /// EnrichmentResult with enriched note and metrics
    ///
    /// # Errors
    ///
    /// Returns an error if any phase fails (file not found, parse error, etc.)
    pub async fn process(&self, path: &Path) -> Result<EnrichmentResult> {
        let start_time = Instant::now();
        let mut metrics = ProcessingMetrics::default();

        info!("Starting note processing for: {}", path.display());

        // Phase 1: Quick Filter (file date + BLAKE3 hash check)
        let (file_changed, file_hash) = if self.config.enable_quick_filter {
            self.phase1_quick_filter(path, &mut metrics).await?
        } else {
            (true, None)
        };

        metrics.file_changed = file_changed;
        metrics.file_hash = file_hash.clone();

        // TODO: If !file_changed, we could skip to Phase 5 (just update metadata)
        // For now, we always proceed to ensure completeness

        // Phase 2: Parse to AST
        let parsed = self.phase2_parse(path, &mut metrics).await?;

        // Phase 3: Build Merkle tree and identify changed blocks
        let (merkle_tree, changed_blocks) = self.phase3_merkle_diff(&parsed, &mut metrics).await?;

        metrics.changed_blocks_count = changed_blocks.len();

        // Phase 4: Enrichment (only for changed blocks)
        let enriched = self.phase4_enrich(parsed, merkle_tree, &changed_blocks, &mut metrics).await?;

        metrics.blocks_enriched = enriched.embeddings.len();
        metrics.total_duration = start_time.elapsed();

        let was_incremental = self.config.enable_quick_filter && !changed_blocks.is_empty();

        info!(
            "Note processing complete in {:?} ({} changed blocks, {} embeddings)",
            metrics.total_duration,
            metrics.changed_blocks_count,
            metrics.blocks_enriched
        );

        Ok(EnrichmentResult {
            enriched,
            metrics,
            changed_blocks,
            was_incremental,
        })
    }

    /// Phase 1: Quick file filter using date modified and BLAKE3 hash
    ///
    /// Returns (file_changed: bool, file_hash: Option<String>)
    async fn phase1_quick_filter(
        &self,
        path: &Path,
        metrics: &mut ProcessingMetrics,
    ) -> Result<(bool, Option<String>)> {
        let start = Instant::now();

        debug!("Phase 1: Quick filter for {}", path.display());

        // Get file metadata (modified time)
        let metadata = fs::metadata(path)
            .with_context(|| format!("Failed to read file metadata: {}", path.display()))?;

        let modified_time = metadata.modified()
            .context("Failed to get file modified time")?;

        debug!("File modified: {:?}", modified_time);

        // Compute BLAKE3 hash of file
        let file_hash = BLAKE3_HASHER.hash_file(path).await
            .with_context(|| format!("Failed to compute file hash: {}", path.display()))?;

        let hash_hex = file_hash.to_string();
        debug!("File BLAKE3 hash: {}", hash_hex);

        // TODO: Storage integration for Phase 3
        // - Query storage for last processed modified time and hash
        // - If modified time and hash match, return (false, Some(hash)) to skip processing
        // - If different, return (true, Some(hash)) to trigger processing
        // - Store new modified time and hash after successful processing in Phase 5
        //
        // For now, always return true (file changed) until storage integration is complete.
        // This ensures correctness while maintaining Phase 1 infrastructure.
        let file_changed = true;

        metrics.phase1_filter_duration = start.elapsed();
        debug!("Phase 1 complete in {:?} (hash: {})", metrics.phase1_filter_duration, hash_hex);

        Ok((file_changed, Some(hash_hex)))
    }

    /// Phase 2: Parse markdown file to AST
    async fn phase2_parse(
        &self,
        path: &Path,
        metrics: &mut ProcessingMetrics,
    ) -> Result<ParsedNote> {
        let start = Instant::now();

        debug!("Phase 2: Parsing {}", path.display());

        // Parse markdown file
        let parsed = self.parser.parse_file(path)
            .await
            .context("Failed to parse markdown file")?;

        metrics.phase2_parse_duration = start.elapsed();
        debug!("Phase 2 complete in {:?} (parsed {} blocks)",
            metrics.phase2_parse_duration,
            parsed.content.headings.len() + parsed.content.paragraphs.len()
        );

        Ok(parsed)
    }

    /// Phase 3: Build Merkle tree and identify changed blocks
    ///
    /// Returns (merkle_tree, changed_blocks)
    async fn phase3_merkle_diff(
        &self,
        parsed: &ParsedNote,
        metrics: &mut ProcessingMetrics,
    ) -> Result<(HybridMerkleTree, Vec<String>)> {
        let start = Instant::now();

        debug!("Phase 3: Building Merkle tree and computing diff (placeholder)");

        // TODO: Build Merkle tree from parsed content
        // - Use existing Merkle tree implementation
        // - Build tree from block hashes in ParsedNote
        // - Load previous tree from storage
        // - Compute diff to identify changed blocks
        //
        // For now, create empty tree and treat all blocks as changed
        let merkle_tree = HybridMerkleTree::default();
        let all_block_ids = self.extract_all_block_ids(parsed);
        let changed_blocks = all_block_ids;

        metrics.phase3_merkle_duration = start.elapsed();
        debug!("Phase 3 complete in {:?} ({} changed blocks)",
            metrics.phase3_merkle_duration,
            changed_blocks.len()
        );

        Ok((merkle_tree, changed_blocks))
    }

    /// Phase 4: Enrich the parsed note with embeddings, metadata, and relations
    async fn phase4_enrich(
        &self,
        parsed: ParsedNote,
        merkle_tree: HybridMerkleTree,
        changed_blocks: &[String],
        metrics: &mut ProcessingMetrics,
    ) -> Result<EnrichedNote> {
        let start = Instant::now();

        debug!("Phase 4: Enriching note ({} changed blocks)", changed_blocks.len());

        // Call enrichment service
        let enriched = self.enrichment_service
            .enrich_with_tree(parsed, merkle_tree, changed_blocks.to_vec())
            .await
            .context("Failed to enrich note")?;

        metrics.phase4_enrich_duration = start.elapsed();
        debug!("Phase 4 complete in {:?} (generated {} embeddings, {} relations)",
            metrics.phase4_enrich_duration,
            enriched.embeddings.len(),
            enriched.inferred_relations.len()
        );

        Ok(enriched)
    }

    /// Extract all block IDs from a parsed note
    ///
    /// This is used when we don't have a previous Merkle tree to diff against
    fn extract_all_block_ids(&self, parsed: &ParsedNote) -> Vec<String> {
        let mut block_ids = Vec::new();

        // Add heading block IDs
        for (idx, _) in parsed.content.headings.iter().enumerate() {
            block_ids.push(format!("heading_{}", idx));
        }

        // Add paragraph block IDs
        for (idx, _) in parsed.content.paragraphs.iter().enumerate() {
            block_ids.push(format!("paragraph_{}", idx));
        }

        // Add code block IDs
        for (idx, _) in parsed.content.code_blocks.iter().enumerate() {
            block_ids.push(format!("code_{}", idx));
        }

        // Add list block IDs
        for (idx, _) in parsed.content.lists.iter().enumerate() {
            block_ids.push(format!("list_{}", idx));
        }

        // Add blockquote IDs
        for (idx, _) in parsed.content.blockquotes.iter().enumerate() {
            block_ids.push(format!("blockquote_{}", idx));
        }

        block_ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_markdown_file(content: &str) -> (TempDir, std::path::PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");
        fs::write(&file_path, content).unwrap();
        (temp_dir, file_path)
    }

    #[tokio::test]
    async fn test_document_processor_creation() {
        let service = Arc::new(crate::DefaultEnrichmentService::without_embeddings());
        let processor = EnrichmentPipeline::new(service);

        assert!(processor.config.enable_quick_filter);
    }

    #[tokio::test]
    async fn test_process_simple_document() {
        let service = Arc::new(crate::DefaultEnrichmentService::without_embeddings());
        let processor = EnrichmentPipeline::new(service);

        let content = "# Test Heading\n\nThis is a test paragraph with more than three words.";
        let (_temp, path) = create_test_markdown_file(content);

        let result = processor.process(&path).await;
        assert!(result.is_ok(), "Processing failed: {:?}", result.err());

        let result = result.unwrap();
        assert!(result.metrics.total_duration.as_millis() > 0);
        assert!(result.enriched.parsed.metadata.word_count > 0);
    }

    #[tokio::test]
    async fn test_process_without_embedding_provider() {
        // When service has no embedding provider, should still enrich with metadata
        let service = Arc::new(crate::DefaultEnrichmentService::without_embeddings());
        let processor = EnrichmentPipeline::new(service);

        let content = "# Heading\n\nParagraph text here with enough words to meet minimum.";
        let (_temp, path) = create_test_markdown_file(content);

        let result = processor.process(&path).await.unwrap();

        // Should have no embeddings when no provider configured
        assert_eq!(result.enriched.embeddings.len(), 0);
        assert_eq!(result.metrics.blocks_enriched, 0);

        // But should still have metadata
        assert!(result.enriched.metadata.reading_time_minutes >= 0.0);
    }

    #[tokio::test]
    async fn test_extract_all_block_ids() {
        use crucible_parser::types::*;

        let service = Arc::new(crate::DefaultEnrichmentService::without_embeddings());
        let processor = EnrichmentPipeline::new(service);

        let mut parsed = ParsedNote {
            path: std::path::PathBuf::from("test.md"),
            content: NoteContent::default(),
            frontmatter: None,
            wikilinks: Vec::new(),
            tags: Vec::new(),
            inline_links: Vec::new(),
            callouts: Vec::new(),
            latex_expressions: Vec::new(),
            footnotes: FootnoteMap::new(),
            parsed_at: chrono::Utc::now(),
            content_hash: "test".to_string(),
            file_size: 0,
            parse_errors: Vec::new(),
            block_hashes: vec![],
            merkle_root: None,
            metadata: Default::default(),
        };

        // Add some content
        parsed.content.headings.push(Heading {
            level: 1,
            text: "Test".to_string(),
            offset: 0,
            id: None,
        });
        parsed.content.paragraphs.push(Paragraph {
            content: "Test paragraph".to_string(),
            offset: 10,
            word_count: 2,
        });

        let block_ids = processor.extract_all_block_ids(&parsed);

        assert_eq!(block_ids.len(), 2);
        assert!(block_ids.contains(&"heading_0".to_string()));
        assert!(block_ids.contains(&"paragraph_0".to_string()));
    }
}

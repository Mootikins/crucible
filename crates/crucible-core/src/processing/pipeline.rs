//! Pipeline Orchestrator Trait
//!
//! Defines the abstraction for note processing pipelines following the Dependency
//! Inversion Principle. This allows frontends to depend on the trait rather than
//! concrete implementations, enabling testability and flexibility.
/// Result of processing a note through the pipeline
#[derive(Debug, Clone)]
pub enum ProcessingResult {
    /// Note was processed successfully with changes
    Success {
        /// Number of blocks that were changed
        changed_blocks: usize,
        /// Whether embeddings were generated
        embeddings_generated: bool,
        /// Non-fatal warnings encountered during processing
        warnings: Vec<String>,
    },
    /// Note was skipped (unchanged file hash)
    Skipped,
    /// Note had no content changes (same Merkle tree)
    NoChanges,
}

impl ProcessingResult {
    /// Create a success result
    pub fn success(changed_blocks: usize, embeddings_generated: bool) -> Self {
        Self::success_with_warnings(changed_blocks, embeddings_generated, Vec::new())
    }

    /// Create a success result with non-fatal warnings
    pub fn success_with_warnings(
        changed_blocks: usize,
        embeddings_generated: bool,
        warnings: Vec<String>,
    ) -> Self {
        Self::Success {
            changed_blocks,
            embeddings_generated,
            warnings,
        }
    }

    /// Create a skipped result
    pub fn skipped() -> Self {
        Self::Skipped
    }

    /// Create a no changes result
    pub fn no_changes() -> Self {
        Self::NoChanges
    }

    /// Check if processing was successful
    pub fn is_success(&self) -> bool {
        matches!(self, ProcessingResult::Success { .. })
    }

    /// Check if processing was skipped
    pub fn is_skipped(&self) -> bool {
        matches!(self, ProcessingResult::Skipped)
    }

    /// Get the number of changed blocks, if applicable
    pub fn changed_blocks(&self) -> Option<usize> {
        match self {
            ProcessingResult::Success { changed_blocks, .. } => Some(*changed_blocks),
            _ => None,
        }
    }

    /// Check if embeddings were generated
    pub fn embeddings_generated(&self) -> bool {
        match self {
            ProcessingResult::Success {
                embeddings_generated,
                ..
            } => *embeddings_generated,
            _ => false,
        }
    }

    /// Get warnings, if applicable
    pub fn warnings(&self) -> Option<&[String]> {
        match self {
            ProcessingResult::Success { warnings, .. } => Some(warnings),
            _ => None,
        }
    }
}

/// Metrics collected during pipeline execution
#[derive(Debug, Clone, Default)]
pub struct PipelineMetrics {
    /// Time spent in Phase 1 (quick filter)
    pub phase1_duration_ms: u64,
    /// Time spent in Phase 2 (parse)
    pub phase2_duration_ms: u64,
    /// Time spent in Phase 3 (Merkle diff)
    pub phase3_duration_ms: u64,
    /// Time spent in Phase 4 (enrichment)
    pub phase4_duration_ms: u64,
    /// Time spent in Phase 5 (storage)
    pub phase5_duration_ms: u64,
    /// Total pipeline execution time
    pub total_duration_ms: u64,
}

impl PipelineMetrics {
    /// Create a new metrics instance
    pub fn new() -> Self {
        Self::default()
    }

    /// Get total processing time
    pub fn total_duration(&self) -> u64 {
        self.total_duration_ms
    }

    /// Get breakdown of time per phase
    pub fn phase_breakdown(&self) -> Vec<(&'static str, u64)> {
        vec![
            ("Phase 1 (Quick Filter)", self.phase1_duration_ms),
            ("Phase 2 (Parse)", self.phase2_duration_ms),
            ("Phase 3 (Merkle Diff)", self.phase3_duration_ms),
            ("Phase 4 (Enrichment)", self.phase4_duration_ms),
            ("Phase 5 (Storage)", self.phase5_duration_ms),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_processing_result_success() {
        let result = ProcessingResult::success(5, true);
        assert!(result.is_success());
        assert_eq!(result.changed_blocks(), Some(5));
        assert!(result.embeddings_generated());
        assert_eq!(result.warnings(), Some(&[] as &[String]));
    }

    #[test]
    fn test_processing_result_success_with_warnings() {
        let result = ProcessingResult::success_with_warnings(
            2,
            false,
            vec!["frontmatter parse warning".to_string()],
        );

        assert!(result.is_success());
        assert_eq!(result.changed_blocks(), Some(2));
        assert!(!result.embeddings_generated());
        assert_eq!(result.warnings().map(|w| w.len()), Some(1));
    }

    #[test]
    fn test_processing_result_skipped() {
        let result = ProcessingResult::skipped();
        assert!(result.is_skipped());
        assert_eq!(result.changed_blocks(), None);
        assert!(!result.embeddings_generated());
    }

    #[test]
    fn test_processing_result_no_changes() {
        let result = ProcessingResult::no_changes();
        assert!(!result.is_success());
        assert!(!result.is_skipped());
        assert_eq!(result.changed_blocks(), None);
    }

    #[test]
    fn test_pipeline_metrics() {
        let mut metrics = PipelineMetrics::new();
        metrics.phase1_duration_ms = 10;
        metrics.phase2_duration_ms = 50;
        metrics.phase3_duration_ms = 30;
        metrics.phase4_duration_ms = 200;
        metrics.phase5_duration_ms = 100;
        metrics.total_duration_ms = 390;

        assert_eq!(metrics.total_duration(), 390);

        let breakdown = metrics.phase_breakdown();
        assert_eq!(breakdown.len(), 5);
        assert_eq!(breakdown[0], ("Phase 1 (Quick Filter)", 10));
        assert_eq!(breakdown[4], ("Phase 5 (Storage)", 100));
    }
}

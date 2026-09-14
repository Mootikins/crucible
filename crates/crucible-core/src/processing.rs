//! Note processing result
//!
//! `ProcessingResult` is the outcome the daemon pipeline reports for one note.

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
}

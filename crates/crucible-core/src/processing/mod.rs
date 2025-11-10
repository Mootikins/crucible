//! Document Processing Handoff Types
//!
//! This module provides clean "handoff" types that enable proper layer separation
//! between file parsing, coordination, and database operations in the queue-based
//! architecture.
//!
//! ## Architecture Philosophy
//!
//! The key insight is that **"the parser doesn't know how to structure the db transaction"**.
//! This module provides the bridge that allows:
//! - Parser layer: Extract information from files, no database knowledge
//! - Core layer: Provide handoff types for clean communication
//! - Processing layer: Coordinate processing without transaction structure knowledge
//! - Database layer: Build and execute transaction sequences with dependency resolution

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

// Re-export ParsedDocument from parser for convenience
pub use crucible_parser::types::ParsedDocument;

/// A processed document ready for database transaction building
///
/// This type represents the complete output of the file parsing process,
/// containing everything needed to build database transactions but without
/// any knowledge of how those transactions should be structured.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedDocument {
    /// The parsed document content and structure
    pub document: ParsedDocument,

    /// The root path of the kiln this document belongs to
    pub kiln_root: PathBuf,

    /// When this document was processed
    pub processed_at: SystemTime,

    /// The processing context and metadata
    pub context: ProcessingContext,
}

/// Context information about the processing operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingContext {
    /// The processing job ID this document belongs to
    pub job_id: String,

    /// The source of this processing operation
    pub source: ProcessingSource,

    /// Priority level for this document
    pub priority: ProcessingPriority,

    /// Additional metadata about the processing
    pub metadata: ProcessingMetadata,
}

/// Where this processing operation originated
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProcessingSource {
    /// Initial startup scan
    StartupScan,

    /// Real-time file system watcher
    FileSystemWatcher,

    /// Manual user-triggered refresh
    ManualRefresh,

    /// Incremental change detection
    IncrementalUpdate,

    /// Batch processing operation
    BatchProcessing,
}

/// Processing priority levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProcessingPriority {
    /// Low priority - background processing
    Low = 1,

    /// Normal priority - standard processing
    Normal = 2,

    /// High priority - user-requested operations
    High = 3,

    /// Critical priority - system operations
    Critical = 4,
}

impl Default for ProcessingPriority {
    fn default() -> Self {
        Self::Normal
    }
}

/// Additional processing metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingMetadata {
    /// Whether this is a new document or an update
    pub is_new_document: bool,

    /// Whether embeddings should be generated
    pub generate_embeddings: bool,

    /// Whether links and relationships should be processed
    pub process_relationships: bool,

    /// Custom processing flags
    pub flags: std::collections::HashMap<String, String>,
}

impl Default for ProcessingMetadata {
    fn default() -> Self {
        Self {
            is_new_document: true,
            generate_embeddings: true,
            process_relationships: true,
            flags: std::collections::HashMap::new(),
        }
    }
}

/// A document processing job that coordinates multiple document processing operations
///
/// This represents a complete processing job that may involve multiple documents
/// and provides the coordination needed for efficient batch processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentProcessingJob {
    /// Unique identifier for this processing job
    pub job_id: String,

    /// When this job was created
    pub created_at: SystemTime,

    /// The source that triggered this job
    pub source: ProcessingSource,

    /// Default priority for documents in this job
    pub default_priority: ProcessingPriority,

    /// Job-wide configuration
    pub config: JobConfiguration,

    /// Statistics about this job
    pub stats: JobStats,
}

/// Configuration for a processing job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobConfiguration {
    /// The kiln root being processed
    pub kiln_root: PathBuf,

    /// Whether to process embeddings
    pub process_embeddings: bool,

    /// Whether to process relationships
    pub process_relationships: bool,

    /// Batch size for processing operations
    pub batch_size: Option<usize>,

    /// Maximum concurrent processing threads
    pub max_concurrent: Option<usize>,

    /// Processing timeout per document
    pub document_timeout: Option<std::time::Duration>,
}

impl Default for JobConfiguration {
    fn default() -> Self {
        Self {
            kiln_root: PathBuf::from("."),
            process_embeddings: true,
            process_relationships: true,
            batch_size: None,
            max_concurrent: None,
            document_timeout: None,
        }
    }
}

/// Statistics for a processing job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobStats {
    /// Number of documents successfully processed
    pub successful: usize,

    /// Number of documents that failed processing
    pub failed: usize,

    /// Number of documents skipped
    pub skipped: usize,

    /// Total documents in this job
    pub total: usize,

    /// When processing started
    pub started_at: Option<SystemTime>,

    /// When processing completed
    pub completed_at: Option<SystemTime>,

    /// Processing duration
    pub duration: Option<std::time::Duration>,
}

impl Default for JobStats {
    fn default() -> Self {
        Self {
            successful: 0,
            failed: 0,
            skipped: 0,
            total: 0,
            started_at: None,
            completed_at: None,
            duration: None,
        }
    }
}

/// Result of processing a single document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DocumentProcessingResult {
    /// Document was successfully processed and queued for database operations
    Success {
        /// The processed document
        document: ProcessedDocument,
        /// Number of database transactions generated
        transaction_count: usize,
        /// Processing time
        processing_time: std::time::Duration,
    },

    /// Document processing failed
    Failure {
        /// Document path that failed
        path: PathBuf,
        /// Error description
        error: String,
        /// Processing time before failure
        processing_time: std::time::Duration,
    },

    /// Document was skipped (e.g., unchanged)
    Skipped {
        /// Document path that was skipped
        path: PathBuf,
        /// Reason for skipping
        reason: String,
    },
}

impl DocumentProcessingResult {
    /// Get the document path if available
    pub fn path(&self) -> Option<&PathBuf> {
        match self {
            DocumentProcessingResult::Success { document, .. } => Some(&document.document.path),
            DocumentProcessingResult::Failure { path, .. } => Some(path),
            DocumentProcessingResult::Skipped { path, .. } => Some(path),
        }
    }

    /// Check if the result represents success
    pub fn is_success(&self) -> bool {
        matches!(self, DocumentProcessingResult::Success { .. })
    }

    /// Get the processing time
    pub fn processing_time(&self) -> std::time::Duration {
        match self {
            DocumentProcessingResult::Success {
                processing_time, ..
            }
            | DocumentProcessingResult::Failure {
                processing_time, ..
            } => *processing_time,
            DocumentProcessingResult::Skipped { .. } => std::time::Duration::from_millis(0),
        }
    }
}

impl ProcessedDocument {
    /// Create a new processed document
    pub fn new(
        document: ParsedDocument,
        kiln_root: PathBuf,
        job_id: String,
        source: ProcessingSource,
    ) -> Self {
        Self {
            document,
            kiln_root,
            processed_at: SystemTime::now(),
            context: ProcessingContext {
                job_id,
                source,
                priority: ProcessingPriority::default(),
                metadata: ProcessingMetadata::default(),
            },
        }
    }

    /// Create a processed document with custom context
    pub fn with_context(
        document: ParsedDocument,
        kiln_root: PathBuf,
        context: ProcessingContext,
    ) -> Self {
        Self {
            document,
            kiln_root,
            processed_at: SystemTime::now(),
            context,
        }
    }

    /// Get the document path
    pub fn path(&self) -> &PathBuf {
        &self.document.path
    }

    /// Get the job ID
    pub fn job_id(&self) -> &str {
        &self.context.job_id
    }

    /// Set the processing priority
    pub fn set_priority(&mut self, priority: ProcessingPriority) {
        self.context.priority = priority;
    }

    /// Get the processing priority
    pub fn priority(&self) -> &ProcessingPriority {
        &self.context.priority
    }
}

impl DocumentProcessingJob {
    /// Create a new processing job
    pub fn new(job_id: String, source: ProcessingSource, config: JobConfiguration) -> Self {
        Self {
            job_id,
            created_at: SystemTime::now(),
            source,
            default_priority: ProcessingPriority::default(),
            config,
            stats: JobStats::default(),
        }
    }

    /// Start processing timing
    pub fn start_processing(&mut self) {
        self.stats.started_at = Some(SystemTime::now());
    }

    /// Complete processing timing
    pub fn complete_processing(&mut self) {
        self.stats.completed_at = Some(SystemTime::now());
        if let Some(started_at) = self.stats.started_at {
            self.stats.duration = started_at.elapsed().ok();
        }
    }

    /// Record a successful document processing
    pub fn record_success(&mut self) {
        self.stats.successful += 1;
    }

    /// Record a failed document processing
    pub fn record_failure(&mut self) {
        self.stats.failed += 1;
    }

    /// Record a skipped document
    pub fn record_skip(&mut self) {
        self.stats.skipped += 1;
    }

    /// Set the total number of documents
    pub fn set_total(&mut self, total: usize) {
        self.stats.total = total;
    }

    /// Get completion percentage
    pub fn completion_percentage(&self) -> f64 {
        if self.stats.total == 0 {
            0.0
        } else {
            let processed = self.stats.successful + self.stats.failed + self.stats.skipped;
            (processed as f64 / self.stats.total as f64) * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_parser::types::{DocumentContent, FootnoteMap, ParsedDocument};

    fn create_test_document(path: &str) -> ParsedDocument {
        ParsedDocument {
            path: PathBuf::from(path),
            content: DocumentContent::default(),
            frontmatter: None,
            wikilinks: Vec::new(),
            tags: Vec::new(),
            callouts: Vec::new(),
            latex_expressions: Vec::new(),
            footnotes: FootnoteMap::new(),
            parsed_at: chrono::Utc::now(),
            content_hash: "test_hash".to_string(),
            file_size: 0,
            parse_errors: Vec::new(),
            block_hashes: vec![],
            merkle_root: None,
        }
    }

    #[test]
    fn test_processed_document_creation() {
        let doc = create_test_document("test.md");
        let processed = ProcessedDocument::new(
            doc,
            PathBuf::from("/test"),
            "job-123".to_string(),
            ProcessingSource::StartupScan,
        );

        assert_eq!(processed.path(), &PathBuf::from("test.md"));
        assert_eq!(processed.job_id(), "job-123");
        assert_eq!(processed.priority(), &ProcessingPriority::Normal);
    }

    #[test]
    fn test_processing_job() {
        let mut job = DocumentProcessingJob::new(
            "job-123".to_string(),
            ProcessingSource::StartupScan,
            JobConfiguration::default(),
        );

        job.set_total(10);
        job.start_processing();
        job.record_success();
        job.record_success();
        job.record_failure();
        job.complete_processing();

        assert_eq!(job.stats.total, 10);
        assert_eq!(job.stats.successful, 2);
        assert_eq!(job.stats.failed, 1);
        assert_eq!(job.completion_percentage(), 30.0);
        assert!(job.stats.duration.is_some());
    }

    #[test]
    fn test_document_processing_result() {
        let doc = create_test_document("test.md");
        let processed = ProcessedDocument::new(
            doc,
            PathBuf::from("/test"),
            "job-123".to_string(),
            ProcessingSource::StartupScan,
        );

        let success = DocumentProcessingResult::Success {
            document: processed,
            transaction_count: 3,
            processing_time: std::time::Duration::from_millis(100),
        };

        assert!(success.is_success());
        assert_eq!(success.path(), Some(&PathBuf::from("test.md")));
        assert_eq!(
            success.processing_time(),
            std::time::Duration::from_millis(100)
        );
    }

    #[test]
    fn test_processing_priority_ordering() {
        assert!(ProcessingPriority::Critical > ProcessingPriority::High);
        assert!(ProcessingPriority::High > ProcessingPriority::Normal);
        assert!(ProcessingPriority::Normal > ProcessingPriority::Low);
    }
}

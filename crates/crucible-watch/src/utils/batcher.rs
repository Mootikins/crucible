//! Event batching for grouping related events together.

#![allow(dead_code)]

use crate::FileEvent;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::{debug, trace};

/// Batcher for grouping related events together for efficient processing.
pub struct EventBatcher {
    /// Maximum batch size
    max_batch_size: usize,
    /// Maximum time to wait before emitting a batch
    max_batch_delay: Duration,
    /// Current batch being built
    current_batch: HashMap<BatchKey, BatchGroup>,
    /// Last batch emission time
    last_emission: Instant,
    /// Batch strategy
    strategy: BatchStrategy,
}

/// Strategy for batching events.
pub enum BatchStrategy {
    /// Batch by directory
    ByDirectory,
    /// Batch by file type
    ByFileType,
    /// Batch by time window
    ByTimeWindow,
    /// Batch by event kind
    ByEventKind,
    /// Custom batching logic
    Custom(Box<dyn Fn(&FileEvent) -> String + Send + Sync>),
}

impl std::fmt::Debug for BatchStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ByDirectory => write!(f, "ByDirectory"),
            Self::ByFileType => write!(f, "ByFileType"),
            Self::ByTimeWindow => write!(f, "ByTimeWindow"),
            Self::ByEventKind => write!(f, "ByEventKind"),
            Self::Custom(_) => write!(f, "Custom(<function>)"),
        }
    }
}

impl Clone for BatchStrategy {
    fn clone(&self) -> Self {
        match self {
            Self::ByDirectory => Self::ByDirectory,
            Self::ByFileType => Self::ByFileType,
            Self::ByTimeWindow => Self::ByTimeWindow,
            Self::ByEventKind => Self::ByEventKind,
            Self::Custom(_) => Self::ByDirectory, // Fallback to ByDirectory for unclonable closures
        }
    }
}

/// Key for grouping events into batches.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct BatchKey {
    /// Group identifier
    group_id: String,
    /// Batch type
    batch_type: BatchType,
}

/// Type of batch.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
enum BatchType {
    Directory,
    FileType,
    TimeWindow,
    EventKind,
    Custom,
}

/// Group of related events waiting to be batched.
#[derive(Debug, Clone)]
struct BatchGroup {
    /// Events in this group
    events: Vec<FileEvent>,
    /// Time when this group was created
    created_at: Instant,
    /// Last time an event was added to this group
    last_updated: Instant,
}

impl EventBatcher {
    /// Create a new event batcher.
    pub fn new(max_batch_size: usize, max_batch_delay: Duration) -> Self {
        Self {
            max_batch_size,
            max_batch_delay,
            current_batch: HashMap::new(),
            last_emission: Instant::now(),
            strategy: BatchStrategy::ByDirectory,
        }
    }

    /// Set the batching strategy.
    pub fn with_strategy(mut self, strategy: BatchStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Add an event to the batcher.
    pub fn add_event(&mut self, event: FileEvent) -> Vec<Vec<FileEvent>> {
        trace!("Adding event to batcher: {:?}", event.kind);

        let batch_key = self.create_batch_key(&event);
        let now = Instant::now();

        // Add to existing batch or create new one
        let batch_group = self
            .current_batch
            .entry(batch_key.clone())
            .or_insert_with(|| BatchGroup {
                events: Vec::new(),
                created_at: now,
                last_updated: now,
            });

        batch_group.events.push(event.clone());
        batch_group.last_updated = now;

        // Check if any batches are ready to be emitted
        let mut ready_batches = Vec::new();

        // Check if the current batch group is at capacity
        if batch_group.events.len() >= self.max_batch_size {
            if let Some(mut group) = self.current_batch.remove(&batch_key) {
                let events = std::mem::take(&mut group.events);
                debug!("Emitting batch due to size limit: {} events", events.len());
                ready_batches.push(events);
            }
        }

        // Check for delayed batches
        self.emit_delayed_batches(&mut ready_batches);

        ready_batches
    }

    /// Force emit all pending batches.
    pub fn flush(&mut self) -> Vec<Vec<FileEvent>> {
        debug!("Flushing {} pending batches", self.current_batch.len());

        let mut batches = Vec::new();

        for (_, group) in self.current_batch.drain() {
            batches.push(group.events);
        }

        self.last_emission = Instant::now();
        batches
    }

    /// Create a batch key for an event based on the current strategy.
    fn create_batch_key(&self, event: &FileEvent) -> BatchKey {
        match &self.strategy {
            BatchStrategy::ByDirectory => {
                let directory = event
                    .parent()
                    .unwrap_or_else(|| std::path::PathBuf::from("/"))
                    .to_string_lossy()
                    .to_string();

                BatchKey {
                    group_id: directory,
                    batch_type: BatchType::Directory,
                }
            }
            BatchStrategy::ByFileType => {
                let file_type = event.extension().unwrap_or_else(|| "unknown".to_string());

                BatchKey {
                    group_id: file_type,
                    batch_type: BatchType::FileType,
                }
            }
            BatchStrategy::ByTimeWindow => {
                let window = self.get_time_window(event.timestamp);
                BatchKey {
                    group_id: window,
                    batch_type: BatchType::TimeWindow,
                }
            }
            BatchStrategy::ByEventKind => {
                let kind = match event.kind {
                    crate::events::FileEventKind::Created => "created".to_string(),
                    crate::events::FileEventKind::Modified => "modified".to_string(),
                    crate::events::FileEventKind::Deleted => "deleted".to_string(),
                    crate::events::FileEventKind::Moved { .. } => "moved".to_string(),
                    crate::events::FileEventKind::Batch(_) => "batch".to_string(),
                    crate::events::FileEventKind::Unknown(_) => "unknown".to_string(),
                };

                BatchKey {
                    group_id: kind,
                    batch_type: BatchType::EventKind,
                }
            }
            BatchStrategy::Custom(key_fn) => {
                let key = key_fn(event);
                BatchKey {
                    group_id: key,
                    batch_type: BatchType::Custom,
                }
            }
        }
    }

    /// Get time window for timestamp-based batching.
    fn get_time_window(&self, timestamp: chrono::DateTime<chrono::Utc>) -> String {
        // Create 5-second windows
        let window_seconds = (timestamp.timestamp() / 5) * 5;
        format!("window_{}", window_seconds)
    }

    /// Emit batches that have been waiting too long.
    fn emit_delayed_batches(&mut self, ready_batches: &mut Vec<Vec<FileEvent>>) {
        let now = Instant::now();
        let mut to_remove = Vec::new();

        for (key, group) in &self.current_batch {
            if now.duration_since(group.last_updated) >= self.max_batch_delay {
                to_remove.push(key.clone());
            }
        }

        for key in to_remove {
            if let Some(mut group) = self.current_batch.remove(&key) {
                let events = std::mem::take(&mut group.events);
                debug!("Emitting batch due to delay: {} events", events.len());
                ready_batches.push(events);
            }
        }
    }

    /// Get batcher statistics.
    pub fn get_stats(&self) -> BatcherStats {
        BatcherStats {
            current_batches: self.current_batch.len(),
            total_queued_events: self.current_batch.values().map(|g| g.events.len()).sum(),
            max_batch_size: self.max_batch_size,
            max_batch_delay_ms: self.max_batch_delay.as_millis(),
            strategy: format!("{:?}", self.strategy),
        }
    }
}

/// Statistics for the event batcher.
#[derive(Debug, Clone)]
pub struct BatcherStats {
    /// Number of current batches
    pub current_batches: usize,
    /// Total number of events currently queued
    pub total_queued_events: usize,
    /// Maximum batch size
    pub max_batch_size: usize,
    /// Maximum batch delay in milliseconds
    pub max_batch_delay_ms: u128,
    /// Current batching strategy
    pub strategy: String,
}

/// Batch analyzer for understanding batch composition.
pub struct BatchAnalyzer;

impl BatchAnalyzer {
    /// Analyze a batch of events and return statistics.
    pub fn analyze_batch(events: &[FileEvent]) -> BatchAnalysis {
        let mut analysis = BatchAnalysis::default();

        for event in events {
            analysis.total_events += 1;

            // Count by event kind
            match event.kind {
                crate::events::FileEventKind::Created => analysis.created += 1,
                crate::events::FileEventKind::Modified => analysis.modified += 1,
                crate::events::FileEventKind::Deleted => analysis.deleted += 1,
                crate::events::FileEventKind::Moved { .. } => analysis.moved += 1,
                crate::events::FileEventKind::Batch(_) => analysis.batches += 1,
                crate::events::FileEventKind::Unknown(_) => analysis.unknown += 1,
            }

            // Count by file type
            if let Some(ext) = event.extension() {
                *analysis.file_types.entry(ext).or_insert(0) += 1;
            }

            // Count by directory
            if let Some(parent) = event.parent() {
                *analysis.directories.entry(parent).or_insert(0) += 1;
            }

            // Count files vs directories
            if event.is_dir {
                analysis.directories_count += 1;
            } else {
                analysis.files_count += 1;
            }

            // Estimate total size
            analysis.total_size += crate::utils::EventUtils::estimate_event_size(event);
        }

        analysis
    }

    /// Get recommendations for batch processing.
    pub fn get_recommendations(analysis: &BatchAnalysis) -> Vec<String> {
        let mut recommendations = Vec::new();

        if analysis.total_events > 100 {
            recommendations.push(
                "Large batch detected (>100 events). Consider breaking into smaller batches."
                    .to_string(),
            );
        }

        if analysis.created + analysis.modified > analysis.deleted * 3 {
            recommendations.push(
                "Many create/modify events relative to deletes. Consider optimization for write-heavy workloads.".to_string()
            );
        }

        if analysis.files_count > analysis.directories_count * 10 {
            recommendations.push(
                "File-heavy batch detected. Consider file-specific optimizations.".to_string(),
            );
        }

        let max_file_type_count = analysis.file_types.values().max().unwrap_or(&0);
        if *max_file_type_count > analysis.total_events / 2 {
            recommendations.push(
                "Dominant file type detected. Consider file type-specific handlers.".to_string(),
            );
        }

        if recommendations.is_empty() {
            recommendations.push("Batch composition looks balanced.".to_string());
        }

        recommendations
    }
}

/// Analysis of a batch of events.
#[derive(Debug, Clone, Default)]
pub struct BatchAnalysis {
    /// Total events in batch
    pub total_events: usize,
    /// Number of created events
    pub created: usize,
    /// Number of modified events
    pub modified: usize,
    /// Number of deleted events
    pub deleted: usize,
    /// Number of moved events
    pub moved: usize,
    /// Number of batch events
    pub batches: usize,
    /// Number of unknown events
    pub unknown: usize,
    /// Number of files
    pub files_count: usize,
    /// Number of directories
    pub directories_count: usize,
    /// Estimated total size in bytes
    pub total_size: usize,
    /// Count by file type
    pub file_types: HashMap<String, usize>,
    /// Count by directory
    pub directories: HashMap<std::path::PathBuf, usize>,
}

//! Processing component configuration
//!
//! Configuration for pipeline, concurrency, and change detection.

use serde::{Deserialize, Serialize};

/// Processing component configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingComponentConfig {
    pub enabled: bool,
    pub pipeline: ProcessingPipelineConfig,
    pub change_detection: ChangeDetectionConfig,
    pub concurrency: ConcurrencyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingPipelineConfig {
    pub worker_count: usize,
    pub batch_size: usize,
    pub max_queue_size: usize,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeDetectionConfig {
    pub enabled: bool,
    pub watch_interval_seconds: u64,
    pub debounce_ms: u64,
    pub ignore_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcurrencyConfig {
    pub max_workers: usize,
    pub io_threads: usize,
    pub task_queue_size: usize,
    pub enable_parallel_processing: bool,
}

impl Default for ProcessingComponentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            pipeline: ProcessingPipelineConfig::default(),
            change_detection: ChangeDetectionConfig::default(),
            concurrency: ConcurrencyConfig::default(),
        }
    }
}

impl Default for ProcessingPipelineConfig {
    fn default() -> Self {
        Self {
            worker_count: 4, // Conservative default
            batch_size: 8,
            max_queue_size: 100,
            timeout_seconds: 60,
        }
    }
}

impl Default for ChangeDetectionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            watch_interval_seconds: 5,
            debounce_ms: 1000,
            ignore_patterns: vec![
                "*.tmp".to_string(),
                "*.swp".to_string(),
                ".git/*".to_string(),
                "node_modules/*".to_string(),
            ],
        }
    }
}

impl Default for ConcurrencyConfig {
    fn default() -> Self {
        Self {
            max_workers: 4,
            io_threads: 2,
            task_queue_size: 100,
            enable_parallel_processing: true,
        }
    }
}
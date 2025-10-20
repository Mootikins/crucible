//! Configuration schema for the file watching system.

use crate::WatchBackend;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// File watching configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchConfig {
    /// Main file watching configuration
    pub file_watching: Option<FileWatchingConfig>,
    /// Individual watch profiles
    pub watch_profiles: HashMap<String, WatchProfile>,
    /// Default profile name
    pub default_profile: Option<String>,
    /// Global settings
    pub global: Option<GlobalWatchConfig>,
}

/// Main file watching configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileWatchingConfig {
    /// Whether file watching is enabled
    pub enabled: bool,
    /// Default backend to use
    pub default_backend: WatchBackend,
    /// Maximum number of concurrent watchers
    pub max_concurrent_watchers: Option<usize>,
    /// Event processing configuration
    pub event_processing: Option<EventProcessingConfig>,
    /// Performance settings
    pub performance: Option<PerformanceConfig>,
    /// Logging configuration
    pub logging: Option<WatchLoggingConfig>,
}

/// Individual watch profile configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchProfile {
    /// Profile name
    pub name: String,
    /// Profile description
    pub description: Option<String>,
    /// Paths to watch
    pub paths: Vec<WatchPath>,
    /// Backend configuration for this profile
    pub backend: WatchBackend,
    /// Watch mode
    pub mode: WatchModeConfig,
    /// Event filters
    pub filters: Option<FilterConfig>,
    /// Handlers to use for this profile
    pub handlers: Option<Vec<String>>,
    /// Profile-specific settings
    pub settings: Option<HashMap<String, serde_json::Value>>,
}

/// Configuration for a path to watch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchPath {
    /// Path to watch
    pub path: String,
    /// Whether to watch recursively
    pub recursive: bool,
    /// Path-specific filters
    pub filters: Option<FilterConfig>,
    /// Path-specific settings
    pub settings: Option<HashMap<String, serde_json::Value>>,
}

/// Watch mode configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum WatchModeConfig {
    /// Standard watching
    Standard {
        /// Debounce configuration
        debounce: Option<DebounceConfig>,
    },
    /// Batched watching
    Batched {
        /// Maximum batch size
        max_batch_size: Option<usize>,
        /// Maximum batch delay
        max_batch_delay_ms: Option<u64>,
    },
    /// Low-frequency watching
    LowFrequency {
        /// Polling interval in milliseconds
        interval_ms: u64,
    },
    /// Custom mode
    Custom {
        /// Custom configuration
        config: HashMap<String, serde_json::Value>,
    },
}

/// Debounce configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebounceConfig {
    /// Debounce delay in milliseconds
    pub delay_ms: u64,
    /// Maximum batch size
    pub max_batch_size: Option<usize>,
    /// Whether to deduplicate identical events
    pub deduplicate: Option<bool>,
}

/// Event processing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventProcessingConfig {
    /// Event queue capacity
    pub queue_capacity: Option<usize>,
    /// Backpressure strategy
    pub backpressure_strategy: Option<BackpressureStrategy>,
    /// Maximum concurrent handlers
    pub max_concurrent_handlers: Option<usize>,
    /// Handler timeout in milliseconds
    pub handler_timeout_ms: Option<u64>,
    /// Whether to preserve event order
    pub preserve_order: Option<bool>,
}

/// Backpressure strategy for event processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackpressureStrategy {
    /// Drop new events
    DropNew,
    /// Drop oldest events
    DropOldest,
    /// Block until space is available
    Block,
    /// Drop events with lowest priority
    DropLowPriority,
}

/// Performance configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Memory usage limits
    pub memory: Option<MemoryConfig>,
    /// CPU usage settings
    pub cpu: Option<CpuConfig>,
    /// Monitoring settings
    pub monitoring: Option<MonitoringConfig>,
}

/// Memory configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Maximum memory usage in bytes
    pub max_memory_bytes: Option<u64>,
    /// Event cache size
    pub event_cache_size: Option<usize>,
    /// Statistics history size
    pub stats_history_size: Option<usize>,
}

/// CPU configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuConfig {
    /// Maximum CPU usage percentage (0-100)
    pub max_cpu_percent: Option<f64>,
    /// Worker thread pool size
    pub worker_threads: Option<usize>,
    /// Enable CPU throttling
    pub enable_throttling: Option<bool>,
}

/// Monitoring configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Enable performance monitoring
    pub enabled: Option<bool>,
    /// Metrics collection interval in milliseconds
    pub metrics_interval_ms: Option<u64>,
    /// Export metrics
    pub export_metrics: Option<ExportConfig>,
}

/// Metrics export configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportConfig {
    /// Export format
    pub format: ExportFormat,
    /// Export destination
    pub destination: String,
    /// Export interval in milliseconds
    pub interval_ms: u64,
}

/// Metrics export format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    /// JSON format
    Json,
    /// Prometheus format
    Prometheus,
    /// CSV format
    Csv,
}

/// Event filter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterConfig {
    /// Include patterns
    pub include: Option<Vec<String>>,
    /// Exclude patterns
    pub exclude: Option<Vec<String>>,
    /// File extensions to include
    pub include_extensions: Option<Vec<String>>,
    /// File extensions to exclude
    pub exclude_extensions: Option<Vec<String>>,
    /// Maximum file size in bytes
    pub max_file_size: Option<u64>,
    /// Minimum file size in bytes
    pub min_file_size: Option<u64>,
    /// Advanced filters
    pub advanced: Option<AdvancedFilterConfig>,
}

/// Advanced filter configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedFilterConfig {
    /// Exclude temporary files
    pub exclude_temp_files: Option<bool>,
    /// Exclude system files
    pub exclude_system_files: Option<bool>,
    /// Time window filtering
    pub time_window: Option<TimeWindowConfig>,
    /// Frequency limiting
    pub frequency_limit: Option<FrequencyLimitConfig>,
}

/// Time window configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeWindowConfig {
    /// Start hour (24-hour format)
    pub start_hour: u8,
    /// End hour (24-hour format)
    pub end_hour: u8,
    /// Timezone offset in hours
    pub timezone_offset: Option<i8>,
}

/// Frequency limiting configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrequencyLimitConfig {
    /// Maximum events per time window
    pub max_events: usize,
    /// Time window duration in milliseconds
    pub window_ms: u64,
}

/// Watch logging configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchLoggingConfig {
    /// Enable debug logging
    pub debug: Option<bool>,
    /// Log file path
    pub log_file: Option<String>,
    /// Maximum log file size in bytes
    pub max_file_size: Option<u64>,
    /// Number of log files to retain
    pub max_files: Option<u32>,
    /// Log event details
    pub log_events: Option<bool>,
    /// Log performance metrics
    pub log_performance: Option<bool>,
}

/// Global watch configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalWatchConfig {
    /// Default debounce delay in milliseconds
    pub default_debounce_ms: Option<u64>,
    /// Default queue capacity
    pub default_queue_capacity: Option<usize>,
    /// Default backend options
    pub default_backend_options: Option<HashMap<String, serde_json::Value>>,
    /// Global event filters
    pub global_filters: Option<FilterConfig>,
    /// Auto-restart on error
    pub auto_restart: Option<bool>,
    /// Graceful shutdown timeout in milliseconds
    pub shutdown_timeout_ms: Option<u64>,
}

/// Configuration for the watch manager.
#[derive(Debug, Clone)]
pub struct WatchManagerConfig {
    /// Queue capacity
    pub queue_capacity: usize,
    /// Debounce delay
    pub debounce_delay: Duration,
    /// Enable default handlers
    pub enable_default_handlers: bool,
    /// Maximum concurrent handlers
    pub max_concurrent_handlers: usize,
    /// Performance monitoring enabled
    pub enable_monitoring: bool,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            file_watching: Some(FileWatchingConfig::default()),
            watch_profiles: HashMap::new(),
            default_profile: Some("default".to_string()),
            global: Some(GlobalWatchConfig::default()),
        }
    }
}

impl Default for FileWatchingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_backend: WatchBackend::Notify,
            max_concurrent_watchers: Some(100),
            event_processing: Some(EventProcessingConfig::default()),
            performance: Some(PerformanceConfig::default()),
            logging: Some(WatchLoggingConfig::default()),
        }
    }
}

impl Default for EventProcessingConfig {
    fn default() -> Self {
        Self {
            queue_capacity: Some(10000),
            backpressure_strategy: Some(BackpressureStrategy::DropOldest),
            max_concurrent_handlers: Some(50),
            handler_timeout_ms: Some(5000),
            preserve_order: Some(false),
        }
    }
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            memory: Some(MemoryConfig::default()),
            cpu: Some(CpuConfig::default()),
            monitoring: Some(MonitoringConfig::default()),
        }
    }
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            max_memory_bytes: Some(512 * 1024 * 1024), // 512MB
            event_cache_size: Some(10000),
            stats_history_size: Some(1000),
        }
    }
}

impl Default for CpuConfig {
    fn default() -> Self {
        Self {
            max_cpu_percent: Some(80.0),
            worker_threads: Some(num_cpus::get()),
            enable_throttling: Some(false),
        }
    }
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            enabled: Some(true),
            metrics_interval_ms: Some(5000), // 5 seconds
            export_metrics: None,
        }
    }
}

impl Default for WatchLoggingConfig {
    fn default() -> Self {
        Self {
            debug: Some(false),
            log_file: None,
            max_file_size: Some(10 * 1024 * 1024), // 10MB
            max_files: Some(5),
            log_events: Some(false),
            log_performance: Some(true),
        }
    }
}

impl Default for GlobalWatchConfig {
    fn default() -> Self {
        Self {
            default_debounce_ms: Some(100),
            default_queue_capacity: Some(10000),
            default_backend_options: Some(HashMap::new()),
            global_filters: None,
            auto_restart: Some(true),
            shutdown_timeout_ms: Some(30000), // 30 seconds
        }
    }
}

impl Default for WatchManagerConfig {
    fn default() -> Self {
        Self {
            queue_capacity: 10000,
            debounce_delay: Duration::from_millis(100),
            enable_default_handlers: true,
            max_concurrent_handlers: 50,
            enable_monitoring: true,
        }
    }
}

impl WatchManagerConfig {
    /// Create a new config with custom queue capacity.
    pub fn with_queue_capacity(mut self, capacity: usize) -> Self {
        self.queue_capacity = capacity;
        self
    }

    /// Create a new config with custom debounce delay.
    pub fn with_debounce_delay(mut self, delay: Duration) -> Self {
        self.debounce_delay = delay;
        self
    }

    /// Create a new config with monitoring enabled/disabled.
    pub fn with_monitoring(mut self, enabled: bool) -> Self {
        self.enable_monitoring = enabled;
        self
    }

    /// Create a new config with default handlers enabled/disabled.
    pub fn with_default_handlers(mut self, enabled: bool) -> Self {
        self.enable_default_handlers = enabled;
        self
    }
}

/// Configuration validation utilities.
pub struct ConfigValidator;

impl ConfigValidator {
    /// Validate a complete watch configuration.
    pub fn validate_config(config: &WatchConfig) -> Result<(), ValidationError> {
        // Validate file watching config
        if let Some(ref file_watching) = config.file_watching {
            Self::validate_file_watching_config(file_watching)?;
        }

        // Validate watch profiles
        for (_name, profile) in &config.watch_profiles {
            Self::validate_watch_profile(profile)?;
        }

        // Validate default profile exists
        if let Some(ref default_profile) = config.default_profile {
            if !config.watch_profiles.contains_key(default_profile) {
                return Err(ValidationError::DefaultProfileNotFound(default_profile.clone()));
            }
        }

        Ok(())
    }

    /// Validate file watching configuration.
    fn validate_file_watching_config(config: &FileWatchingConfig) -> Result<(), ValidationError> {
        if config.max_concurrent_watchers.is_some() && config.max_concurrent_watchers.unwrap() == 0 {
            return Err(ValidationError::InvalidValue("max_concurrent_watchers".to_string(), "must be greater than 0".to_string()));
        }

        if let Some(ref event_processing) = config.event_processing {
            Self::validate_event_processing_config(event_processing)?;
        }

        Ok(())
    }

    /// Validate event processing configuration.
    fn validate_event_processing_config(config: &EventProcessingConfig) -> Result<(), ValidationError> {
        if let Some(capacity) = config.queue_capacity {
            if capacity == 0 {
                return Err(ValidationError::InvalidValue("queue_capacity".to_string(), "must be greater than 0".to_string()));
            }
        }

        if let Some(timeout) = config.handler_timeout_ms {
            if timeout == 0 {
                return Err(ValidationError::InvalidValue("handler_timeout_ms".to_string(), "must be greater than 0".to_string()));
            }
        }

        Ok(())
    }

    /// Validate watch profile configuration.
    fn validate_watch_profile(profile: &WatchProfile) -> Result<(), ValidationError> {
        if profile.paths.is_empty() {
            return Err(ValidationError::EmptyPathList(profile.name.clone()));
        }

        for path in &profile.paths {
            if path.path.is_empty() {
                return Err(ValidationError::InvalidPath("empty path".to_string()));
            }
        }

        Ok(())
    }
}

/// Configuration validation errors.
#[derive(Debug, Clone)]
pub enum ValidationError {
    /// Default profile not found
    DefaultProfileNotFound(String),
    /// Invalid configuration value
    InvalidValue(String, String),
    /// Empty path list
    EmptyPathList(String),
    /// Invalid path
    InvalidPath(String),
    /// Invalid backend configuration
    InvalidBackendConfig(String),
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::DefaultProfileNotFound(name) => write!(f, "Default profile '{}' not found", name),
            ValidationError::InvalidValue(field, reason) => write!(f, "Invalid value for {}: {}", field, reason),
            ValidationError::EmptyPathList(profile) => write!(f, "Profile '{}' has empty path list", profile),
            ValidationError::InvalidPath(path) => write!(f, "Invalid path: {}", path),
            ValidationError::InvalidBackendConfig(reason) => write!(f, "Invalid backend configuration: {}", reason),
        }
    }
}

impl std::error::Error for ValidationError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = WatchConfig::default();
        assert!(config.file_watching.is_some());
        assert_eq!(config.default_profile, Some("default".to_string()));
    }

    #[test]
    fn test_config_validation() {
        let mut config = WatchConfig::default();

        // Should be valid
        assert!(ConfigValidator::validate_config(&config).is_ok());

        // Add a profile with empty paths
        let profile = WatchProfile {
            name: "test".to_string(),
            description: None,
            paths: Vec::new(),
            backend: WatchBackend::Notify,
            mode: WatchModeConfig::Standard { debounce: None },
            filters: None,
            handlers: None,
            settings: None,
        };
        config.watch_profiles.insert("test".to_string(), profile);

        // Should be invalid
        assert!(ConfigValidator::validate_config(&config).is_err());
    }

    #[test]
    fn test_watch_manager_config() {
        let config = WatchManagerConfig::default()
            .with_queue_capacity(5000)
            .with_debounce_delay(Duration::from_millis(200))
            .with_monitoring(false);

        assert_eq!(config.queue_capacity, 5000);
        assert_eq!(config.debounce_delay, Duration::from_millis(200));
        assert!(!config.enable_monitoring);
    }
}
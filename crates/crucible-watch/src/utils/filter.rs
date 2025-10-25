//! Advanced filtering for file events.

#![allow(dead_code)]

use crate::{events::FileEvent, events::EventFilter};
use chrono::Timelike;
use std::time::Instant;

/// Advanced event filter with multiple filtering strategies.
pub struct AdvancedEventFilter {
    /// Base event filter
    base_filter: EventFilter,
    /// Custom filters
    custom_filters: Vec<Box<dyn EventFilterLogic + Send + Sync>>,
    /// Filter statistics
    stats: FilterStats,
    /// Whether to collect statistics
    collect_stats: bool,
}

/// Trait for custom event filtering logic.
pub trait EventFilterLogic: Send + Sync {
    /// Check if an event should be allowed through the filter.
    fn should_allow(&self, event: &FileEvent) -> bool;

    /// Get the name of this filter.
    fn name(&self) -> &'static str;
}

impl AdvancedEventFilter {
    /// Create a new advanced event filter.
    pub fn new(base_filter: EventFilter) -> Self {
        Self {
            base_filter,
            custom_filters: Vec::new(),
            stats: FilterStats::default(),
            collect_stats: false,
        }
    }

    /// Enable statistics collection.
    pub fn with_stats(mut self, enabled: bool) -> Self {
        self.collect_stats = enabled;
        self
    }

    /// Add a custom filter.
    pub fn add_custom_filter(mut self, filter: Box<dyn EventFilterLogic + Send + Sync>) -> Self {
        self.custom_filters.push(filter);
        self
    }

    /// Check if an event should be allowed through all filters.
    pub fn should_allow(&mut self, event: &FileEvent) -> bool {
        let start_time = if self.collect_stats {
            Some(Instant::now())
        } else {
            None
        };

        // Apply base filter first
        if !self.base_filter.matches(event) {
            if self.collect_stats {
                self.stats.record_filtered("base_filter", start_time);
            }
            return false;
        }

        // Apply custom filters
        for filter in &self.custom_filters {
            if !filter.should_allow(event) {
                if self.collect_stats {
                    self.stats.record_filtered(filter.name(), start_time);
                }
                return false;
            }
        }

        // Event passed all filters
        if self.collect_stats {
            self.stats.record_allowed(start_time);
        }

        true
    }

    /// Get filter statistics.
    pub fn get_stats(&self) -> &FilterStats {
        &self.stats
    }

    /// Reset statistics.
    pub fn reset_stats(&mut self) {
        self.stats = FilterStats::default();
    }
}

/// Statistics for event filtering.
#[derive(Debug, Clone, Default)]
pub struct FilterStats {
    /// Total events processed
    pub total_processed: u64,
    /// Events allowed through
    pub allowed: u64,
    /// Events filtered out
    pub filtered: u64,
    /// Breakdown of filtered events by filter name
    pub filtered_by: std::collections::HashMap<String, u64>,
    /// Total filtering time
    pub total_filtering_time_ns: u64,
}

impl FilterStats {
    /// Record that an event was allowed.
    fn record_allowed(&mut self, start_time: Option<Instant>) {
        self.total_processed += 1;
        self.allowed += 1;

        if let Some(start) = start_time {
            self.total_filtering_time_ns += start.elapsed().as_nanos() as u64;
        }
    }

    /// Record that an event was filtered.
    fn record_filtered(&mut self, filter_name: &str, start_time: Option<Instant>) {
        self.total_processed += 1;
        self.filtered += 1;

        *self.filtered_by.entry(filter_name.to_string()).or_insert(0) += 1;

        if let Some(start) = start_time {
            self.total_filtering_time_ns += start.elapsed().as_nanos() as u64;
        }
    }

    /// Get the filtering rate (0.0 to 1.0).
    pub fn filtering_rate(&self) -> f64 {
        if self.total_processed == 0 {
            0.0
        } else {
            self.filtered as f64 / self.total_processed as f64
        }
    }

    /// Get average filtering time in nanoseconds.
    pub fn avg_filtering_time_ns(&self) -> f64 {
        if self.total_processed == 0 {
            0.0
        } else {
            self.total_filtering_time_ns as f64 / self.total_processed as f64
        }
    }
}

/// Filter that excludes temporary files.
pub struct TempFileFilter;

impl EventFilterLogic for TempFileFilter {
    fn should_allow(&self, event: &FileEvent) -> bool {
        if let Some(file_name) = event.file_name() {
            // Common temporary file patterns
            !file_name.starts_with('.') &&
            !file_name.starts_with('~') &&
            !file_name.ends_with('~') &&
            !file_name.ends_with(".tmp") &&
            !file_name.ends_with(".temp") &&
            !file_name.ends_with(".swp") &&
            !file_name.ends_with(".swo") &&
            !file_name.ends_with(".bak") &&
            !file_name.contains("#") &&
            !file_name.starts_with("tmp")
        } else {
            true
        }
    }

    fn name(&self) -> &'static str {
        "temp_file_filter"
    }
}

/// Filter that excludes system files and directories.
pub struct SystemFileFilter;

impl EventFilterLogic for SystemFileFilter {
    fn should_allow(&self, event: &FileEvent) -> bool {
        if let Some(path_str) = event.path.to_str() {
            // Exclude common system directories and files
            let path_lower = path_str.to_lowercase();

            !path_lower.contains("/.git/") &&
            !path_lower.starts_with(".git/") &&
            !path_lower.ends_with("/.git") &&
            !path_lower.contains("/.svn/") &&
            !path_lower.starts_with(".svn/") &&
            !path_lower.ends_with("/.svn") &&
            !path_lower.contains("/node_modules/") &&
            !path_lower.starts_with("node_modules/") &&
            !path_lower.ends_with("/node_modules") &&
            !path_lower.contains("/target/") &&
            !path_lower.starts_with("target/") &&
            !path_lower.ends_with("/target") &&
            !path_lower.contains("/.vscode/") &&
            !path_lower.starts_with(".vscode/") &&
            !path_lower.ends_with("/.vscode") &&
            !path_lower.contains("/.idea/") &&
            !path_lower.starts_with(".idea/") &&
            !path_lower.ends_with("/.idea") &&
            !path_lower.ends_with(".ds_store") &&
            !path_lower.ends_with(".thumbs.db")
        } else {
            true
        }
    }

    fn name(&self) -> &'static str {
        "system_file_filter"
    }
}

/// Filter that limits events based on frequency.
pub struct FrequencyFilter {
    /// Events per time window
    max_events_per_window: usize,
    /// Time window duration
    window_duration: std::time::Duration,
    /// Event history
    event_history: std::collections::VecDeque<Instant>,
}

impl FrequencyFilter {
    /// Create a new frequency filter.
    pub fn new(max_events_per_window: usize, window_duration: std::time::Duration) -> Self {
        Self {
            max_events_per_window,
            window_duration,
            event_history: std::collections::VecDeque::new(),
        }
    }
}

impl EventFilterLogic for FrequencyFilter {
    fn should_allow(&self, _event: &FileEvent) -> bool {
        // Note: This would need to be made mutable for real usage
        // For now, just return true as a placeholder
        true
    }

    fn name(&self) -> &'static str {
        "frequency_filter"
    }
}

/// Filter that only allows events during specific hours.
pub struct TimeWindowFilter {
    /// Start hour (24-hour format)
    start_hour: u8,
    /// End hour (24-hour format)
    end_hour: u8,
    /// Timezone to use (UTC offset in hours)
    timezone_offset: i8,
}

impl TimeWindowFilter {
    /// Create a new time window filter.
    pub fn new(start_hour: u8, end_hour: u8) -> Self {
        Self {
            start_hour,
            end_hour,
            timezone_offset: 0, // UTC by default
        }
    }

    /// Set timezone offset.
    pub fn with_timezone(mut self, offset: i8) -> Self {
        self.timezone_offset = offset;
        self
    }

    /// Check if current time is within the allowed window.
    fn is_time_allowed(&self) -> bool {
        let now = chrono::Utc::now();
        let local_hour = ((now.hour() as i8 + self.timezone_offset).rem_euclid(24)) as u8;

        if self.start_hour <= self.end_hour {
            // Normal range (e.g., 9 to 17)
            local_hour >= self.start_hour && local_hour <= self.end_hour
        } else {
            // Overnight range (e.g., 22 to 6)
            local_hour >= self.start_hour || local_hour <= self.end_hour
        }
    }
}

impl EventFilterLogic for TimeWindowFilter {
    fn should_allow(&self, _event: &FileEvent) -> bool {
        self.is_time_allowed()
    }

    fn name(&self) -> &'static str {
        "time_window_filter"
    }
}

/// Filter that only allows events for files above a certain size.
pub struct SizeFilter {
    /// Minimum file size in bytes
    min_size: u64,
    /// Maximum file size in bytes
    max_size: Option<u64>,
}

impl SizeFilter {
    /// Create a new size filter.
    pub fn new(min_size: u64, max_size: Option<u64>) -> Self {
        Self { min_size, max_size }
    }
}

impl EventFilterLogic for SizeFilter {
    fn should_allow(&self, event: &FileEvent) -> bool {
        if let Some(metadata) = &event.metadata {
            if let Some(size) = metadata.size {
                if size < self.min_size {
                    return false;
                }
                if let Some(max) = self.max_size {
                    if size > max {
                        return false;
                    }
                }
            }
        }
        true
    }

    fn name(&self) -> &'static str {
        "size_filter"
    }
}

/// Builder for creating complex event filters.
pub struct EventFilterBuilder {
    filter: EventFilter,
    advanced_filters: Vec<Box<dyn EventFilterLogic + Send + Sync>>,
    collect_stats: bool,
}

impl EventFilterBuilder {
    /// Create a new filter builder.
    pub fn new() -> Self {
        Self {
            filter: EventFilter::new(),
            advanced_filters: Vec::new(),
            collect_stats: false,
        }
    }

    /// Add extension to include.
    pub fn include_extension(mut self, ext: impl Into<String>) -> Self {
        self.filter = self.filter.with_extension(ext);
        self
    }

    /// Add extension to exclude.
    pub fn exclude_extension(mut self, ext: impl Into<String>) -> Self {
        self.filter = self.filter.exclude_extension(ext);
        self
    }

    /// Include only files in the given directory.
    pub fn include_directory(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
        self.filter = self.filter.include_dir(dir);
        self
    }

    /// Exclude files in the given directory.
    pub fn exclude_directory(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
        self.filter = self.filter.exclude_dir(dir);
        self
    }

    /// Set size limits.
    pub fn with_size_limits(mut self, min: Option<u64>, max: Option<u64>) -> Self {
        self.filter = self.filter.with_size_limits(min, max);
        self
    }

    /// Add temporary file filter.
    pub fn exclude_temp_files(mut self) -> Self {
        self.advanced_filters.push(Box::new(TempFileFilter));
        self
    }

    /// Add system file filter.
    pub fn exclude_system_files(mut self) -> Self {
        self.advanced_filters.push(Box::new(SystemFileFilter));
        self
    }

    /// Add time window filter.
    pub fn allow_only_during_hours(mut self, start: u8, end: u8) -> Self {
        self.advanced_filters.push(Box::new(TimeWindowFilter::new(start, end)));
        self
    }

    /// Add size filter.
    pub fn with_size_filter(mut self, min: u64, max: Option<u64>) -> Self {
        self.advanced_filters.push(Box::new(SizeFilter::new(min, max)));
        self
    }

    /// Enable statistics collection.
    pub fn with_stats(mut self) -> Self {
        self.collect_stats = true;
        self
    }

    /// Build the final filter.
    pub fn build(self) -> AdvancedEventFilter {
        let mut advanced = AdvancedEventFilter::new(self.filter)
            .with_stats(self.collect_stats);

        for filter in self.advanced_filters {
            advanced = advanced.add_custom_filter(filter);
        }

        advanced
    }
}

impl Default for EventFilterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{FileEvent, FileEventKind};
    use std::path::PathBuf;

    #[test]
    fn test_temp_file_filter() {
        let filter = TempFileFilter;

        let allowed_event = FileEvent::new(FileEventKind::Created, PathBuf::from("document.txt"));
        let temp_event = FileEvent::new(FileEventKind::Created, PathBuf::from(".temp.txt"));
        let swp_event = FileEvent::new(FileEventKind::Created, PathBuf::from("file.swp"));

        assert!(filter.should_allow(&allowed_event));
        assert!(!filter.should_allow(&temp_event));
        assert!(!filter.should_allow(&swp_event));
    }

    #[test]
    fn test_system_file_filter() {
        let filter = SystemFileFilter;

        let allowed_event = FileEvent::new(FileEventKind::Created, PathBuf::from("src/main.rs"));
        let git_event = FileEvent::new(FileEventKind::Created, PathBuf::from(".git/config"));
        let node_modules_event = FileEvent::new(FileEventKind::Created, PathBuf::from("node_modules/package/index.js"));

        assert!(filter.should_allow(&allowed_event));
        assert!(!filter.should_allow(&git_event));
        assert!(!filter.should_allow(&node_modules_event));
    }

    #[test]
    fn test_time_window_filter() {
        let filter = TimeWindowFilter::new(9, 17); // 9 AM to 5 PM

        // This test would need to mock time or be more sophisticated
        // For now, just test the filter creation
        assert_eq!(filter.name(), "time_window_filter");
    }

    #[test]
    fn test_filter_builder() {
        let filter = EventFilterBuilder::new()
            .include_extension("md")
            .include_extension("txt")
            .exclude_temp_files()
            .exclude_system_files()
            .with_stats()
            .build();

        assert_eq!(filter.get_stats().total_processed, 0);
        assert_eq!(filter.custom_filters.len(), 2);
    }

    #[test]
    fn test_filter_stats() {
        let mut stats = FilterStats::default();

        stats.record_allowed(Some(Instant::now()));
        stats.record_filtered("test_filter", Some(Instant::now()));

        assert_eq!(stats.total_processed, 2);
        assert_eq!(stats.allowed, 1);
        assert_eq!(stats.filtered, 1);
        assert_eq!(stats.filtering_rate(), 0.5);
    }

    #[test]
    fn test_advanced_filter() {
        let base_filter = EventFilter::new()
            .with_extension("md");

        let mut advanced = AdvancedEventFilter::new(base_filter)
            .with_stats(true)
            .add_custom_filter(Box::new(TempFileFilter));

        let valid_event = FileEvent::new(FileEventKind::Created, PathBuf::from("document.md"));
        let temp_event = FileEvent::new(FileEventKind::Created, PathBuf::from("~document.md"));

        assert!(advanced.should_allow(&valid_event));
        assert!(!advanced.should_allow(&temp_event));

        let stats = advanced.get_stats();
        assert_eq!(stats.total_processed, 2);
        assert_eq!(stats.allowed, 1);
        assert_eq!(stats.filtered, 1);
    }
}
//! Memory profiler implementation for tracking detailed memory usage

use super::{MemoryMeasurement, ProfilerConfig, ProfilerState, MemoryTestError};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, trace, warn};

/// Memory profiler for detailed memory tracking
pub struct MemoryProfiler {
    /// Profiler configuration
    config: ProfilerConfig,
    /// Profiler state
    state: Arc<RwLock<ProfilerState>>,
    /// Baseline memory measurement
    baseline_memory: Arc<RwLock<Option<u64>>>,
    /// Memory tracking history
    history: Arc<RwLock<Vec<MemoryMeasurement>>>,
}

impl MemoryProfiler {
    /// Create a new memory profiler
    pub fn new(config: ProfilerConfig) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(ProfilerState {
                active: false,
                start_time: None,
                last_measurement: None,
            })),
            baseline_memory: Arc::new(RwLock::new(None)),
            history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Start profiling memory usage
    pub async fn start_profiling(&self) -> Result<(), MemoryTestError> {
        let mut state = self.state.write().await;

        if state.active {
            return Err(MemoryTestError::ProfilingError("Profiler already active".to_string()));
        }

        state.active = true;
        state.start_time = Some(Instant::now());
        state.last_measurement = None;

        // Take baseline measurement
        let baseline = self.get_current_memory_usage().await?;
        {
            let mut baseline_mem = self.baseline_memory.write().await;
            *baseline_mem = Some(baseline.total_memory_bytes);
        }

        debug!("Memory profiling started with baseline: {} bytes", baseline.total_memory_bytes);
        Ok(())
    }

    /// Stop profiling memory usage
    pub async fn stop_profiling(&self) -> Result<(), MemoryTestError> {
        let mut state = self.state.write().await;

        if !state.active {
            return Err(MemoryTestError::ProfilingError("Profiler not active".to_string()));
        }

        state.active = false;
        state.start_time = None;
        state.last_measurement = None;

        debug!("Memory profiling stopped");
        Ok(())
    }

    /// Take a memory measurement
    pub async fn take_measurement(&self) -> Result<MemoryMeasurement, MemoryTestError> {
        let state = self.state.read().await;

        if !state.active {
            return Err(MemoryTestError::ProfilingError("Profiler not active".to_string()));
        }

        let measurement = self.get_current_memory_usage().await?;

        // Store in history
        {
            let mut history = self.history.write().await;
            history.push(measurement.clone());
        }

        trace!("Memory measurement taken: {} bytes", measurement.total_memory_bytes);
        Ok(measurement)
    }

    /// Get current memory usage
    async fn get_current_memory_usage(&self) -> Result<MemoryMeasurement, MemoryTestError> {
        // Get total memory usage using system APIs
        let total_memory_bytes = self.get_process_memory_usage().await?;

        // Estimate heap memory (this is approximate)
        let heap_memory_bytes = self.estimate_heap_usage().await?;

        // Estimate stack memory
        let stack_memory_bytes = self.estimate_stack_usage().await?;

        // Get cache memory from various sources
        let cache_memory_bytes = self.get_cache_memory_usage().await?;

        // Get connection memory usage
        let connection_memory_bytes = self.get_connection_memory_usage().await?;

        // Get Arc/Mutex reference counts if enabled
        let arc_ref_count = if self.config.track_references {
            self.get_arc_reference_count().await?
        } else {
            0
        };

        // Get active task count
        let active_tasks = self.get_active_task_count().await?;

        // Get custom metrics
        let custom_metrics = self.get_custom_metrics().await?;

        Ok(MemoryMeasurement {
            timestamp: chrono::Utc::now(),
            total_memory_bytes,
            heap_memory_bytes,
            stack_memory_bytes,
            cache_memory_bytes,
            connection_memory_bytes,
            arc_ref_count,
            active_tasks,
            custom_metrics,
        })
    }

    /// Get process memory usage from the operating system
    async fn get_process_memory_usage(&self) -> Result<u64, MemoryTestError> {
        #[cfg(target_os = "linux")]
        {
            use std::fs;

            // Read from /proc/self/status
            let status = fs::read_to_string("/proc/self/status")
                .map_err(|e| MemoryTestError::ProfilingError(format!("Failed to read /proc/self/status: {}", e)))?;

            // Parse VmRSS (Resident Set Size) for actual memory usage
            for line in status.lines() {
                if line.starts_with("VmRSS:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let memory_kb: u64 = parts[1].parse()
                            .map_err(|_| MemoryTestError::ProfilingError("Failed to parse memory value".to_string()))?;
                        return Ok(memory_kb * 1024); // Convert KB to bytes
                    }
                }
            }

            // Fallback to VmSize if VmRSS not found
            for line in status.lines() {
                if line.starts_with("VmSize:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let memory_kb: u64 = parts[1].parse()
                            .map_err(|_| MemoryTestError::ProfilingError("Failed to parse memory value".to_string()))?;
                        return Ok(memory_kb * 1024);
                    }
                }
            }

            Err(MemoryTestError::ProfilingError("Could not find memory usage in /proc/self/status".to_string()))
        }

        #[cfg(target_os = "macos")]
        {
            use std::process::Command;

            // Use `ps` command to get memory usage on macOS
            let output = Command::new("ps")
                .args(&["-o", "rss=", "-p", &std::process::id().to_string()])
                .output()
                .map_err(|e| MemoryTestError::ProfilingError(format!("Failed to execute ps command: {}", e)))?;

            let memory_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let memory_kb: u64 = memory_str.parse()
                .map_err(|_| MemoryTestError::ProfilingError("Failed to parse memory value".to_string()))?;

            Ok(memory_kb * 1024) // Convert KB to bytes
        }

        #[cfg(target_os = "windows")]
        {
            use std::process::Command;

            // Use `wmic` command to get memory usage on Windows
            let output = Command::new("wmic")
                .args(&["process", "where", &format!("ProcessId={}", std::process::id()), "get", "WorkingSetSize", "/value"])
                .output()
                .map_err(|e| MemoryTestError::ProfilingError(format!("Failed to execute wmic command: {}", e)))?;

            let output_str = String::from_utf8_lossy(&output.stdout);

            for line in output_str.lines() {
                if line.starts_with("WorkingSetSize=") {
                    let memory_str = line.split('=').nth(1).unwrap_or("0");
                    let memory_bytes: u64 = memory_str.parse()
                        .map_err(|_| MemoryTestError::ProfilingError("Failed to parse memory value".to_string()))?;
                    return Ok(memory_bytes);
                }
            }

            Err(MemoryTestError::ProfilingError("Could not parse Windows memory usage".to_string()))
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            // Generic fallback - estimate based on available information
            warn!("Memory profiling not fully supported on this platform, using estimates");
            Ok(50 * 1024 * 1024) // 50MB estimate
        }
    }

    /// Estimate heap memory usage
    async fn estimate_heap_usage(&self) -> Result<u64, MemoryTestError> {
        // This is a rough estimation - in a real implementation,
        // you might use custom allocators or heap profiling libraries

        #[cfg(feature = "jemalloc")]
        {
            // Use jemalloc stats if available
            // This would require linking with jemalloc and using its API
            Ok(0) // Placeholder
        }

        #[cfg(not(feature = "jemalloc"))]
        {
            // Rough estimation based on total memory and estimated stack usage
            let total_memory = self.get_process_memory_usage().await?;
            let stack_memory = self.estimate_stack_usage().await?;
            Ok(total_memory.saturating_sub(stack_memory))
        }
    }

    /// Estimate stack memory usage
    async fn estimate_stack_usage(&self) -> Result<u64, MemoryTestError> {
        // Estimate based on number of active threads and typical stack size
        let active_tasks = self.get_active_task_count().await?;
        let estimated_stack_per_task = 2 * 1024 * 1024; // 2MB per task stack
        Ok(active_tasks as u64 * estimated_stack_per_task)
    }

    /// Get cache memory usage from various sources
    async fn get_cache_memory_usage(&self) -> Result<u64, MemoryTestError> {
        // This would need to be customized based on the actual caches in your services
        // For now, return an estimate based on custom metrics
        let custom_metrics = self.get_custom_metrics().await?;

        let mut cache_memory = 0u64;

        // Check for cache-related metrics
        if let Some(cache_size) = custom_metrics.get("cache_size") {
            cache_memory += *cache_size as u64;
        }

        if let Some(script_cache_size) = custom_metrics.get("script_cache_size") {
            cache_memory += *script_cache_size as u64;
        }

        if let Some(query_cache_size) = custom_metrics.get("query_cache_size") {
            cache_memory += *query_cache_size as u64;
        }

        Ok(cache_memory)
    }

    /// Get connection memory usage
    async fn get_connection_memory_usage(&self) -> Result<u64, MemoryTestError> {
        let custom_metrics = self.get_custom_metrics().await?;

        let mut connection_memory = 0u64;

        // Check for connection-related metrics
        if let Some(active_connections) = custom_metrics.get("active_connections") {
            // Estimate ~64KB per connection
            connection_memory += (*active_connections as u64) * 64 * 1024;
        }

        if let Some(session_count) = custom_metrics.get("session_count") {
            // Estimate ~32KB per session
            connection_memory += (*session_count as u64) * 32 * 1024;
        }

        Ok(connection_memory)
    }

    /// Get Arc reference count (approximation)
    async fn get_arc_reference_count(&self) -> Result<u32, MemoryTestError> {
        // This is difficult to measure accurately without custom instrumentation
        // For now, return an estimate based on known Arc usage in services

        let custom_metrics = self.get_custom_metrics().await?;

        let mut arc_count = 0;

        if let Some(cached_scripts) = custom_metrics.get("cached_scripts") {
            arc_count += *cached_scripts as u32;
        }

        if let Some(active_models) = custom_metrics.get("active_models") {
            arc_count += *active_models as u32;
        }

        if let Some(active_sessions) = custom_metrics.get("active_sessions") {
            arc_count += *active_sessions as u32;
        }

        Ok(arc_count)
    }

    /// Get active task count
    async fn get_active_task_count(&self) -> Result<u32, MemoryTestError> {
        // Use tokio runtime metrics if available
        // This is a simplified approach

        let custom_metrics = self.get_custom_metrics().await?;

        if let Some(active_tasks) = custom_metrics.get("active_tasks") {
            Ok(*active_tasks as u32)
        } else {
            // Fallback estimate
            Ok(10)
        }
    }

    /// Get custom metrics from services
    async fn get_custom_metrics(&self) -> Result<HashMap<String, f64>, MemoryTestError> {
        let mut metrics = HashMap::new();

        // This would integrate with actual service metrics in a real implementation
        // For now, provide some example metrics based on the service being tested

        for metric_name in &self.config.custom_metrics {
            match metric_name.as_str() {
                "cache_size" => {
                    metrics.insert("cache_size".to_string(), 1024.0 * 1024.0); // 1MB
                }
                "active_connections" => {
                    metrics.insert("active_connections".to_string(), 5.0);
                }
                "queue_size" => {
                    metrics.insert("queue_size".to_string(), 100.0);
                }
                "script_cache_size" => {
                    metrics.insert("script_cache_size".to_string(), 512.0 * 1024.0); // 512KB
                }
                "query_cache_size" => {
                    metrics.insert("query_cache_size".to_string(), 2.0 * 1024.0 * 1024.0); // 2MB
                }
                "cached_scripts" => {
                    metrics.insert("cached_scripts".to_string(), 50.0);
                }
                "active_models" => {
                    metrics.insert("active_models".to_string(), 3.0);
                }
                "active_sessions" => {
                    metrics.insert("active_sessions".to_string(), 10.0);
                }
                "active_tasks" => {
                    metrics.insert("active_tasks".to_string(), 15.0);
                }
                _ => {
                    // Default value for unknown metrics
                    metrics.insert(metric_name.clone(), 0.0);
                }
            }
        }

        Ok(metrics)
    }

    /// Get baseline memory usage
    pub async fn get_baseline_memory(&self) -> Option<u64> {
        let baseline = self.baseline_memory.read().await;
        *baseline
    }

    /// Get measurement history
    pub async fn get_history(&self) -> Vec<MemoryMeasurement> {
        let history = self.history.read().await;
        history.clone()
    }

    /// Clear measurement history
    pub async fn clear_history(&self) {
        let mut history = self.history.write().await;
        history.clear();
    }

    /// Check if profiler is active
    pub async fn is_active(&self) -> bool {
        let state = self.state.read().await;
        state.active
    }

    /// Get profiling duration
    pub async fn get_profiling_duration(&self) -> Option<Duration> {
        let state = self.state.read().await;
        state.start_time.map(|start| start.elapsed())
    }
}

/// Memory allocator statistics (if available)
#[cfg(feature = "jemalloc")]
pub mod allocator_stats {
    use super::*;

    /// Get jemalloc allocator statistics
    pub fn get_jemalloc_stats() -> Result<HashMap<String, u64>, MemoryTestError> {
        // This would use the jemalloc API to get detailed allocation statistics
        // Implementation depends on the jemalloc-sys crate

        let mut stats = HashMap::new();

        // Example stats (these would be actual jemalloc stats)
        stats.insert("allocated".to_string(), 0);
        stats.insert("active".to_string(), 0);
        stats.insert("metadata".to_string(), 0);
        stats.insert("resident".to_string(), 0);
        stats.insert("mapped".to_string(), 0);
        stats.insert("retained".to_string(), 0);

        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test;

    #[tokio::test]
    async fn test_profiler_creation() {
        let config = ProfilerConfig::default();
        let profiler = MemoryProfiler::new(config);

        assert!(!profiler.is_active().await);
        assert_eq!(profiler.get_baseline_memory().await, None);
        assert!(profiler.get_history().await.is_empty());
    }

    #[tokio::test]
    async fn test_profiling_lifecycle() {
        let profiler = MemoryProfiler::new(ProfilerConfig::default());

        // Start profiling
        profiler.start_profiling().await.unwrap();
        assert!(profiler.is_active().await);
        assert!(profiler.get_baseline_memory().await.is_some());

        // Take measurement
        let measurement = profiler.take_measurement().await.unwrap();
        assert!(measurement.total_memory_bytes > 0);

        // Stop profiling
        profiler.stop_profiling().await.unwrap();
        assert!(!profiler.is_active().await);

        // Should not be able to take measurements when stopped
        let result = profiler.take_measurement().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_memory_measurement() {
        let profiler = MemoryProfiler::new(ProfilerConfig::default());
        profiler.start_profiling().await.unwrap();

        let measurement = profiler.take_measurement().await.unwrap();

        assert!(measurement.total_memory_bytes > 0);
        assert!(measurement.timestamp > chrono::DateTime::from_timestamp(0, 0).unwrap());
        assert!(!measurement.custom_metrics.is_empty());

        profiler.stop_profiling().await.unwrap();
    }

    #[tokio::test]
    async fn test_custom_metrics() {
        let mut config = ProfilerConfig::default();
        config.custom_metrics = vec!["cache_size".to_string(), "active_connections".to_string()];

        let profiler = MemoryProfiler::new(config);
        profiler.start_profiling().await.unwrap();

        let measurement = profiler.take_measurement().await.unwrap();

        assert!(measurement.custom_metrics.contains_key("cache_size"));
        assert!(measurement.custom_metrics.contains_key("active_connections"));

        profiler.stop_profiling().await.unwrap();
    }

    #[tokio::test]
    async fn test_history_tracking() {
        let profiler = MemoryProfiler::new(ProfilerConfig::default());
        profiler.start_profiling().await.unwrap();

        // Take multiple measurements
        for _ in 0..3 {
            let _ = profiler.take_measurement().await.unwrap();
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let history = profiler.get_history().await;
        assert_eq!(history.len(), 3);

        profiler.clear_history().await;
        let history = profiler.get_history().await;
        assert!(history.is_empty());

        profiler.stop_profiling().await.unwrap();
    }
}
//! Factory implementations for creating file watcher backends.

use super::BackendRegistry;
use crate::{
    error::{Error, Result},
    WatchBackend,
};
use std::sync::Arc;

/// Extended factory registry with additional factory methods.
pub struct ExtendedBackendRegistry {
    inner: BackendRegistry,
}

impl ExtendedBackendRegistry {
    /// Create a new extended backend registry.
    pub fn new() -> Self {
        Self {
            inner: BackendRegistry::new(),
        }
    }

    /// Get the underlying registry.
    pub fn inner(&self) -> &BackendRegistry {
        &self.inner
    }

    /// Create a watcher with automatic backend selection.
    pub async fn create_optimal_watcher(
        &self,
        requirements: &WatcherRequirements,
    ) -> Result<Arc<dyn crate::traits::FileWatcher>> {
        let backend = self.select_optimal_backend(requirements)?;
        self.inner.create_watcher(backend).await.map(Arc::from)
    }

    /// Select the optimal backend based on requirements.
    pub fn select_optimal_backend(
        &self,
        requirements: &WatcherRequirements,
    ) -> Result<WatchBackend> {
        let available_backends = self.inner.available_backends();

        // Filter backends based on requirements
        let suitable_backends: Vec<_> = available_backends
            .into_iter()
            .filter(|backend| {
                if let Some(capabilities) = self.inner.get_capabilities(*backend) {
                    self.meets_requirements(&capabilities, requirements)
                } else {
                    false
                }
            })
            .collect();

        if suitable_backends.is_empty() {
            return Err(Error::BackendUnavailable(
                "No available backend meets requirements".to_string(),
            ));
        }

        // Select the best backend based on priority
        let best_backend = self.rank_backends(&suitable_backends, requirements);

        Ok(best_backend)
    }

    /// Check if backend capabilities meet requirements.
    fn meets_requirements(
        &self,
        capabilities: &crate::traits::BackendCapabilities,
        requirements: &WatcherRequirements,
    ) -> bool {
        // Check recursive requirement
        if requirements.recursive && !capabilities.recursive {
            return false;
        }

        // Check fine-grained events requirement
        if requirements.fine_grained_events && !capabilities.fine_grained_events {
            return false;
        }

        // Check multiple paths requirement
        if requirements.multiple_paths && !capabilities.multiple_paths {
            return false;
        }

        // Check hot reconfiguration requirement
        if requirements.hot_reconfig && !capabilities.hot_reconfig {
            return false;
        }

        // Check platform compatibility
        if !self.platform_compatible(&capabilities.platforms) {
            return false;
        }

        // Check performance requirements
        if let Some(max_latency) = requirements.max_latency_ms {
            // Different backends have different latency characteristics
            match capabilities.platforms.first().map(|s| s.as_str()) {
                Some("notify") => {
                    // Notify typically has low latency (< 50ms)
                    if max_latency < 50 {
                        return false;
                    }
                }
                Some("polling") => {
                    // Polling latency depends on interval
                    if max_latency < 1000 {
                        return false;
                    }
                }
                Some("editor") => {
                    // Editor integration is low frequency
                    if max_latency < 5000 {
                        return false;
                    }
                }
                _ => {}
            }
        }

        true
    }

    /// Check if platform is compatible.
    fn platform_compatible(&self, platforms: &[String]) -> bool {
        let current_platform = std::env::consts::OS;

        platforms
            .iter()
            .any(|platform| platform == "all" || platform == current_platform)
    }

    /// Rank backends by suitability for given requirements.
    fn rank_backends(
        &self,
        backends: &[WatchBackend],
        requirements: &WatcherRequirements,
    ) -> WatchBackend {
        // Define priority order based on typical use cases
        let priority_order = match requirements.use_case {
            WatcherUseCase::HighPerformance => vec![
                WatchBackend::Notify,
                WatchBackend::Polling,
                WatchBackend::Editor,
            ],
            WatcherUseCase::LowFrequency => vec![
                WatchBackend::Editor,
                WatchBackend::Polling,
                WatchBackend::Notify,
            ],
            WatcherUseCase::Compatibility => vec![
                WatchBackend::Polling, // Most compatible
                WatchBackend::Notify,
                WatchBackend::Editor,
            ],
            WatcherUseCase::EditorIntegration => vec![
                WatchBackend::Editor,
                WatchBackend::Notify,
                WatchBackend::Polling,
            ],
        };

        for backend in priority_order {
            if backends.contains(&backend) {
                return backend;
            }
        }

        // Fallback to first available backend
        backends.first().copied().unwrap_or(WatchBackend::Polling)
    }
}

impl Default for ExtendedBackendRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Requirements for selecting a file watcher backend.
#[derive(Debug, Clone)]
pub struct WatcherRequirements {
    /// Whether recursive watching is needed.
    pub recursive: bool,
    /// Whether fine-grained events are needed.
    pub fine_grained_events: bool,
    /// Whether multiple paths need to be watched.
    pub multiple_paths: bool,
    /// Whether hot reconfiguration is needed.
    pub hot_reconfig: bool,
    /// Maximum acceptable latency in milliseconds.
    pub max_latency_ms: Option<u64>,
    /// Use case for the watcher.
    pub use_case: WatcherUseCase,
    /// Priority for resource usage.
    pub resource_priority: ResourcePriority,
}

/// Use cases for file watching.
#[derive(Debug, Clone, PartialEq)]
pub enum WatcherUseCase {
    /// High-performance watching with minimal latency.
    HighPerformance,
    /// Low-frequency watching for background tasks.
    LowFrequency,
    /// Maximum compatibility across platforms.
    Compatibility,
    /// Integration with specific editors.
    EditorIntegration,
}

/// Priority for resource usage.
#[derive(Debug, Clone, PartialEq)]
pub enum ResourcePriority {
    /// Minimize CPU usage.
    LowCpu,
    /// Minimize memory usage.
    LowMemory,
    /// Balance CPU and memory usage.
    Balanced,
    /// Prioritize performance over resource usage.
    Performance,
}

impl Default for WatcherRequirements {
    fn default() -> Self {
        Self {
            recursive: true,
            fine_grained_events: true,
            multiple_paths: true,
            hot_reconfig: false,
            max_latency_ms: Some(100),
            use_case: WatcherUseCase::HighPerformance,
            resource_priority: ResourcePriority::Balanced,
        }
    }
}

impl WatcherRequirements {
    /// Create requirements for high-performance use case.
    pub fn high_performance() -> Self {
        Self {
            recursive: true,
            fine_grained_events: true,
            multiple_paths: true,
            hot_reconfig: false,
            max_latency_ms: Some(50),
            use_case: WatcherUseCase::HighPerformance,
            resource_priority: ResourcePriority::Performance,
        }
    }

    /// Create requirements for low-frequency use case.
    pub fn low_frequency() -> Self {
        Self {
            recursive: false,
            fine_grained_events: false,
            multiple_paths: true,
            hot_reconfig: true,
            max_latency_ms: Some(5000),
            use_case: WatcherUseCase::LowFrequency,
            resource_priority: ResourcePriority::LowCpu,
        }
    }

    /// Create requirements for editor integration use case.
    pub fn editor_integration() -> Self {
        Self {
            recursive: false,
            fine_grained_events: true,
            multiple_paths: true,
            hot_reconfig: true,
            max_latency_ms: Some(1000),
            use_case: WatcherUseCase::EditorIntegration,
            resource_priority: ResourcePriority::LowMemory,
        }
    }

    /// Create requirements for maximum compatibility.
    pub fn compatibility() -> Self {
        Self {
            recursive: true,
            fine_grained_events: false,
            multiple_paths: true,
            hot_reconfig: false,
            max_latency_ms: Some(1000),
            use_case: WatcherUseCase::Compatibility,
            resource_priority: ResourcePriority::Balanced,
        }
    }

    /// Set maximum latency requirement.
    pub fn with_max_latency(mut self, latency_ms: u64) -> Self {
        self.max_latency_ms = Some(latency_ms);
        self
    }

    /// Set resource priority.
    pub fn with_resource_priority(mut self, priority: ResourcePriority) -> Self {
        self.resource_priority = priority;
        self
    }
}

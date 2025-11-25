//! CLI component configuration
//!
//! Configuration for command-line interface and user interaction settings.
//! Features system-aware defaults that adapt to CPU, memory, and disk constraints.

use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::detection::{SystemCapabilities, DetectionError};

/// CLI component configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliComponentConfig {
    pub enabled: bool,
    pub paths: PathConfig,
    pub interface: InterfaceConfig,
    pub user_interaction: UserInteractionConfig,
    #[serde(default)]
    pub custom: std::collections::HashMap<String, serde_json::Value>,
}

/// Path configuration for CLI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathConfig {
    pub kiln_path: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
    pub cache_path: Option<PathBuf>,
}

/// Interface configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceConfig {
    pub command_timeout_seconds: u64,
    pub verbose: bool,
    pub show_progress: bool,
}

/// User interaction configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInteractionConfig {
    pub confirm_destructive: bool,
    pub show_hints: bool,
    pub auto_complete: bool,
}

impl Default for CliComponentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            paths: PathConfig::default(),
            interface: InterfaceConfig::default(),
            user_interaction: UserInteractionConfig::default(),
            custom: std::collections::HashMap::new(),
        }
    }
}

impl Default for PathConfig {
    fn default() -> Self {
        Self {
            kiln_path: None,
            config_path: None,
            cache_path: None,
        }
    }
}

impl Default for InterfaceConfig {
    fn default() -> Self {
        Self {
            command_timeout_seconds: 30,
            verbose: false,
            show_progress: true,
        }
    }
}

impl Default for UserInteractionConfig {
    fn default() -> Self {
        Self {
            confirm_destructive: true,
            show_hints: true,
            auto_complete: true, // Enable by default for better UX
        }
    }
}

impl CliComponentConfig {
    /// Create a CLI component configuration with system-aware defaults and graceful fallback
    pub fn system_aware() -> Result<Self, DetectionError> {
        let capabilities = SystemCapabilities::detect()?;
        Ok(Self::with_system_capabilities(&capabilities))
    }

    /// Create a CLI component configuration with fallback to safe defaults
    pub fn system_aware_with_fallback() -> Self {
        match Self::system_aware() {
            Ok(config) => config,
            Err(_) => {
                eprintln!("Warning: System detection failed for CLI config, using safe defaults");
                Self::fallback_defaults()
            }
        }
    }

    /// Create safe fallback defaults when system detection fails
    pub fn fallback_defaults() -> Self {
        let mut config = Self::default();
        config.interface.command_timeout_seconds = 120; // Conservative timeout
        config.interface.show_progress = false; // Disable to save resources
        config.custom.insert("memory_saver_mode".to_string(), serde_json::Value::Bool(true));
        config.custom.insert("cache_disabled".to_string(), serde_json::Value::Bool(true));
        config.custom.insert("max_concurrent_operations".to_string(), serde_json::Value::Number(1.into()));
        config.custom.insert("batch_size".to_string(), serde_json::Value::Number(5.into()));
        config
    }

    /// Create a CLI component configuration based on explicit system capabilities
    pub fn with_system_capabilities(capabilities: &SystemCapabilities) -> Self {
        let mut config = Self::default();

        // Apply system-aware optimizations
        config.optimize_for_system(capabilities);
        config
    }

    /// Optimize CLI configuration based on system capabilities
    fn optimize_for_system(&mut self, capabilities: &SystemCapabilities) {
        // CPU-aware command timeouts
        self.interface.command_timeout_seconds = self.calculate_cpu_optimized_timeout(capabilities);

        // Memory-aware settings
        let cache_size_mb = self.calculate_memory_aware_cache_size(capabilities);
        self.custom.insert("cache_size_mb".to_string(),
                           serde_json::Value::Number(cache_size_mb.into()));

        let batch_size = self.calculate_memory_aware_batch_size(capabilities);
        self.custom.insert("batch_size".to_string(),
                           serde_json::Value::Number(batch_size.into()));

        // Low memory mode detection and adjustment
        if self.is_low_memory_system(capabilities) {
            self.apply_low_memory_optimizations();
        }

        // Disk space awareness
        if self.should_disable_cache_due_to_disk(capabilities) {
            self.custom.insert("cache_disabled".to_string(),
                               serde_json::Value::Bool(true));
        }

        // CPU concurrency limits
        let max_concurrent = self.calculate_max_concurrent_operations(capabilities);
        self.custom.insert("max_concurrent_operations".to_string(),
                           serde_json::Value::Number(max_concurrent.into()));

        // Set reasonable default paths if not specified
        self.ensure_default_paths();
    }

    /// Calculate command timeout based on CPU performance
    fn calculate_cpu_optimized_timeout(&self, capabilities: &SystemCapabilities) -> u64 {
        // Faster CPUs get shorter timeouts (more responsive)
        // Slower CPUs get longer timeouts (more time to complete)
        if capabilities.cpu_info.logical_cores >= 8 {
            30  // Fast systems get shorter timeouts
        } else if capabilities.cpu_info.logical_cores >= 4 {
            60  // Mid-range systems get moderate timeouts
        } else {
            120 // Slow systems get generous timeouts
        }
    }

    /// Calculate memory-aware cache size
    fn calculate_memory_aware_cache_size(&self, capabilities: &SystemCapabilities) -> u64 {
        let total_gb = capabilities.total_memory_gb();
        if total_gb < 4.0 {
            50  // Conservative for low-memory systems
        } else if total_gb < 8.0 {
            100 // Moderate for mid-range systems
        } else {
            200 // Generous for high-memory systems
        }
    }

    /// Calculate memory-aware batch size
    fn calculate_memory_aware_batch_size(&self, capabilities: &SystemCapabilities) -> u64 {
        let available_gb = capabilities.available_memory_gb();
        if available_gb < 2.0 {
            5   // Very small batches for low-memory systems
        } else if available_gb < 4.0 {
            10  // Small batches for moderate systems
        } else {
            20  // Larger batches for memory-rich systems
        }
    }

    /// Check if this is a low-memory system
    fn is_low_memory_system(&self, capabilities: &SystemCapabilities) -> bool {
        capabilities.total_memory_gb() < 4.0 || capabilities.available_memory_gb() < 1.0
    }

    /// Apply optimizations for low-memory systems
    fn apply_low_memory_optimizations(&mut self) {
        self.interface.show_progress = false; // Disable progress to save memory
        self.custom.insert("memory_saver_mode".to_string(),
                           serde_json::Value::Bool(true));
    }

    /// Check if cache should be disabled due to low disk space
    fn should_disable_cache_due_to_disk(&self, capabilities: &SystemCapabilities) -> bool {
        capabilities.available_disk_gb() < 1.0
    }

    /// Calculate maximum concurrent operations based on CPU cores
    fn calculate_max_concurrent_operations(&self, capabilities: &SystemCapabilities) -> u64 {
        std::cmp::max(1, std::cmp::min(capabilities.cpu_info.logical_cores as u64, 4))
    }

    /// Ensure default paths are set to safe absolute paths
    fn ensure_default_paths(&mut self) {
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let config_dir = home_dir.join(".config").join("crucible");

        if self.paths.kiln_path.is_none() {
            // Default to ~/.config/crucible/kiln or ./kiln as fallback
            self.paths.kiln_path = Some(config_dir.join("kiln"));
        }
        if self.paths.cache_path.is_none() {
            // Default to ~/.config/crucible/cache or ./cache as fallback
            self.paths.cache_path = Some(config_dir.join("cache"));
        }
        if self.paths.config_path.is_none() {
            // Default to ~/.config/crucible/config.toml or ./config.toml as fallback
            self.paths.config_path = Some(config_dir.join("config.toml"));
        }
    }

    /// Get a custom configuration value
    pub fn get_custom<T>(&self, key: &str) -> Option<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        self.custom.get(key)
            .and_then(|value| serde_json::from_value(value.clone()).ok())
    }

    /// Set a custom configuration value
    pub fn set_custom<T>(&mut self, key: String, value: T) -> Result<(), serde_json::Error>
    where
        T: Serialize,
    {
        let json_value = serde_json::to_value(value)?;
        self.custom.insert(key, json_value);
        Ok(())
    }
}
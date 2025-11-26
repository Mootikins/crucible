use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

use crate::hardware::BackendType;

/// Configuration for Burn ML framework integration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurnConfig {
    pub default_backend: BackendConfig,
    pub model_dir: PathBuf,
    pub cache_dir: Option<PathBuf>,
    pub server: ServerConfig,
    pub benchmarks: BenchmarkConfig,
    pub hardware: HardwareConfig,
}

impl Default for BurnConfig {
    fn default() -> Self {
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"));
        let models_dir = home_dir.join("models");

        Self {
            default_backend: BackendConfig::Auto,
            model_dir: models_dir,
            cache_dir: Some(home_dir.join(".cache").join("crucible-burn")),
            server: ServerConfig::default(),
            benchmarks: BenchmarkConfig::default(),
            hardware: HardwareConfig::default(),
        }
    }
}

/// Backend configuration options
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackendConfig {
    /// Automatically detect and use the best available backend
    Auto,
    /// Force Vulkan backend with specific device
    Vulkan { device_id: usize },
    /// Force ROCm backend with specific device
    Rocm { device_id: usize },
    /// Force CPU backend with specific number of threads
    Cpu { num_threads: usize },
}

impl BackendConfig {
    /// Convert to BackendType
    pub fn to_backend_type(&self, cpu_threads: usize) -> BackendType {
        match self {
            BackendConfig::Auto => {
                // This will be resolved during runtime based on hardware detection
                BackendType::Cpu { num_threads: cpu_threads }
            }
            BackendConfig::Vulkan { device_id } => BackendType::Vulkan { device_id: *device_id },
            BackendConfig::Rocm { device_id } => BackendType::Rocm { device_id: *device_id },
            BackendConfig::Cpu { num_threads } => BackendType::Cpu { num_threads: *num_threads },
        }
    }
}

/// Server configuration for HTTP inference API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub max_request_size_mb: usize,
    pub enable_cors: bool,
    pub rate_limit: Option<RateLimitConfig>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
            max_request_size_mb: 100,
            enable_cors: true,
            rate_limit: Some(RateLimitConfig::default()),
        }
    }
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    pub requests_per_minute: u32,
    pub burst_size: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_minute: 1000,
            burst_size: 100,
        }
    }
}

/// Benchmarking configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    pub output_dir: PathBuf,
    pub generate_html_reports: bool,
    pub default_iterations: usize,
    pub warmup_iterations: usize,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"));

        Self {
            output_dir: home_dir.join(".cache").join("crucible-burn").join("benchmarks"),
            generate_html_reports: true,
            default_iterations: 100,
            warmup_iterations: 10,
        }
    }
}

/// Hardware-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareConfig {
    pub auto_detect: bool,
    pub memory_limit_gb: Option<usize>,
    pub prefer_rocm_in_container: bool,
    pub vulkan_validation: bool,
}

impl Default for HardwareConfig {
    fn default() -> Self {
        Self {
            auto_detect: true,
            memory_limit_gb: None,
            prefer_rocm_in_container: true,
            vulkan_validation: false,
        }
    }
}

impl BurnConfig {
    /// Load configuration from file or create default
    pub async fn load(config_path: Option<&Path>) -> Result<Self> {
        let config_path = if let Some(path) = config_path {
            path.to_path_buf()
        } else {
            get_default_config_path()?
        };

        debug!("Loading config from: {:?}", config_path);

        if config_path.exists() {
            let config_content = std::fs::read_to_string(&config_path)?;
            let config: BurnConfig = toml::from_str(&config_content)
                .map_err(|e| anyhow::anyhow!("Failed to parse config file {:?}: {}", config_path, e))?;

            debug!("Loaded configuration from file");
            Ok(config)
        } else {
            let config = BurnConfig::default();
            debug!("Using default configuration");

            // Try to create default config file
            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            if let Ok(config_str) = toml::to_string_pretty(&config) {
                if let Err(e) = std::fs::write(&config_path, config_str) {
                    warn!("Could not write default config file {:?}: {}", config_path, e);
                } else {
                    debug!("Created default config file at: {:?}", config_path);
                }
            }

            Ok(config)
        }
    }

    /// Save configuration to file
    pub async fn save(&self, config_path: Option<&Path>) -> Result<()> {
        let config_path = if let Some(path) = config_path {
            path.to_path_buf()
        } else {
            get_default_config_path()?
        };

        // Ensure directory exists
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let config_str = toml::to_string_pretty(self)?;
        std::fs::write(&config_path, config_str)?;

        debug!("Saved configuration to: {:?}", config_path);
        Ok(())
    }

    /// Get effective backend type (resolve Auto based on hardware detection)
    pub async fn get_effective_backend(&self) -> Result<BackendType> {
        match &self.default_backend {
            BackendConfig::Auto => {
                if self.hardware.auto_detect {
                    // Use hardware detection to determine best backend
                    let hardware_info = crate::hardware::HardwareInfo::detect().await?;
                    Ok(hardware_info.recommended_backend)
                } else {
                    // Fallback to CPU if auto-detection is disabled
                    Ok(BackendType::Cpu {
                        num_threads: num_cpus::get()
                    })
                }
            }
            other => Ok(other.to_backend_type(num_cpus::get())),
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        // Validate model directory
        if !self.model_dir.exists() {
            warn!("Model directory does not exist: {:?}", self.model_dir);
        }

        // Validate cache directory
        if let Some(cache_dir) = &self.cache_dir {
            if !cache_dir.exists() {
                if let Err(e) = std::fs::create_dir_all(cache_dir) {
                    warn!("Could not create cache directory {:?}: {}", cache_dir, e);
                }
            }
        }

        // Validate server configuration
        if self.server.port == 0 || self.server.port > 65535 {
            return Err(anyhow::anyhow!("Invalid server port: {}", self.server.port));
        }

        // Validate benchmark configuration
        if self.benchmarks.default_iterations == 0 {
            return Err(anyhow::anyhow!("Default iterations must be greater than 0"));
        }

        Ok(())
    }
}

/// Get the default configuration file path
fn get_default_config_path() -> Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?
        .join("crucible");

    Ok(config_dir.join("burn.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_default_config() {
        let config = BurnConfig::default();

        assert!(matches!(config.default_backend, BackendConfig::Auto));
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.benchmarks.default_iterations, 100);
    }

    #[tokio::test]
    async fn test_config_save_and_load() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("test_config.toml");

        let original_config = BurnConfig {
            default_backend: BackendConfig::Vulkan { device_id: 0 },
            model_dir: PathBuf::from("/test/models"),
            ..Default::default()
        };

        // Save configuration
        original_config.save(Some(&config_path)).await?;

        // Load configuration
        let loaded_config = BurnConfig::load(Some(&config_path)).await?;

        assert_eq!(
            original_config.model_dir,
            loaded_config.model_dir
        );

        match loaded_config.default_backend {
            BackendConfig::Vulkan { device_id } => assert_eq!(device_id, 0),
            _ => panic!("Expected Vulkan backend"),
        }

        Ok(())
    }

    #[test]
    fn test_backend_config_conversion() {
        let vulkan_config = BackendConfig::Vulkan { device_id: 1 };
        let backend_type = vulkan_config.to_backend_type(8);

        assert!(matches!(backend_type, BackendType::Vulkan { device_id: 1 }));

        let cpu_config = BackendConfig::Cpu { num_threads: 4 };
        let backend_type = cpu_config.to_backend_type(8);

        assert!(matches!(backend_type, BackendType::Cpu { num_threads: 4 }));
    }
}
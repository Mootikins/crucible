//! Unit tests for configuration module

use crucible_burn::config::{BurnConfig, BackendConfig, ServerConfig, HardwareConfig};
use std::path::PathBuf;
use tempfile::TempDir;
use tokio_test;

#[cfg(test)]
mod config_tests {
    use super::*;

    #[test]
    fn test_default_config_creation() {
        let config = BurnConfig::default();

        // Test default values
        assert!(matches!(config.default_backend, BackendConfig::Auto));
        assert!(!config.model_dir.as_os_str().is_empty());
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.benchmarks.default_iterations, 100);
        assert_eq!(config.benchmarks.warmup_iterations, 10);
        assert!(config.hardware.auto_detect);
        assert!(config.server.enable_cors);
    }

    #[test]
    fn test_backend_config_conversion() {
        let vulkan_config = BackendConfig::Vulkan { device_id: 1 };
        let backend_type = vulkan_config.to_backend_type(8);
        assert!(matches!(backend_type, crate::hardware::BackendType::Vulkan { device_id: 1 }));

        let rocm_config = BackendConfig::Rocm { device_id: 0 };
        let backend_type = rocm_config.to_backend_type(8);
        assert!(matches!(backend_type, crate::hardware::BackendType::Rocm { device_id: 0 }));

        let cpu_config = BackendConfig::Cpu { num_threads: 4 };
        let backend_type = cpu_config.to_backend_type(8);
        assert!(matches!(backend_type, crate::hardware::BackendType::Cpu { num_threads: 4 }));

        let auto_config = BackendConfig::Auto;
        let backend_type = auto_config.to_backend_type(8);
        assert!(matches!(backend_type, crate::hardware::BackendType::Cpu { num_threads: 8 }));
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

        // Verify file was created
        assert!(config_path.exists());

        // Load configuration
        let loaded_config = BurnConfig::load(Some(&config_path)).await?;

        assert_eq!(original_config.model_dir, loaded_config.model_dir);

        match loaded_config.default_backend {
            BackendConfig::Vulkan { device_id } => assert_eq!(device_id, 0),
            _ => panic!("Expected Vulkan backend"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_config_default_file_creation() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("auto_config.toml");

        // Loading non-existent config should create default
        let config = BurnConfig::load(Some(&config_path)).await?;

        // Should create the file
        assert!(config_path.exists());

        // Should have default values
        assert!(matches!(config.default_backend, BackendConfig::Auto));
        assert_eq!(config.server.port, 8080);

        Ok(())
    }

    #[tokio::test]
    async fn test_config_validation() {
        let mut config = BurnConfig::default();

        // Valid config should pass validation
        assert!(config.validate().is_ok());

        // Invalid port should fail
        config.server.port = 0;
        assert!(config.validate().is_err());

        config.server.port = 65536;
        assert!(config.validate().is_err());

        // Reset port and test other validations
        config.server.port = 8080;

        // Zero iterations should fail
        config.benchmarks.default_iterations = 0;
        assert!(config.validate().is_err());
    }

    #[tokio::test]
    async fn test_config_effective_backend() {
        let mut config = BurnConfig::default();

        // Auto backend should resolve based on hardware detection
        let effective = config.get_effective_backend().await;
        assert!(effective.is_ok());

        // Explicit backend should return that backend
        config.default_backend = BackendConfig::Cpu { num_threads: 4 };
        let effective = config.get_effective_backend().await.unwrap();
        assert!(matches!(effective, crate::hardware::BackendType::Cpu { num_threads: 4 }));

        // With auto-detection disabled, should fallback to CPU
        config.default_backend = BackendConfig::Auto;
        config.hardware.auto_detect = false;
        let effective = config.get_effective_backend().await.unwrap();
        assert!(matches!(effective, crate::hardware::BackendType::Cpu { .. }));
    }

    #[test]
    fn test_server_config_defaults() {
        let config = ServerConfig::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 8080);
        assert_eq!(config.max_request_size_mb, 100);
        assert!(config.enable_cors);
        assert!(config.rate_limit.is_some());
    }

    #[test]
    fn test_rate_limit_config_defaults() {
        let config = config::RateLimitConfig::default();
        assert_eq!(config.requests_per_minute, 1000);
        assert_eq!(config.burst_size, 100);
    }

    #[test]
    fn test_hardware_config_defaults() {
        let config = HardwareConfig::default();
        assert!(config.auto_detect);
        assert!(config.memory_limit_gb.is_none());
        assert!(config.prefer_rocm_in_container);
        assert!(!config.vulkan_validation);
    }

    #[tokio::test]
    async fn test_config_with_invalid_toml() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("invalid.toml");

        // Write invalid TOML
        fs::write(&config_path, "invalid = toml content [[[")?;

        // Should fail to parse
        let result = BurnConfig::load(Some(&config_path)).await;
        assert!(result.is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_config_model_search_paths() {
        let config = BurnConfig::default();

        // Should have default search paths
        assert!(!config.model_search_paths.is_empty());

        // Should include common model directories
        let search_paths_str: Vec<String> = config.model_search_paths
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();

        assert!(search_paths_str.iter().any(|p| p.contains("embeddings")));
        assert!(search_paths_str.iter().any(|p| p.contains("llm")));
        assert!(search_paths_str.iter().any(|p| p.contains("language")));
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let config = BurnConfig {
            default_backend: BackendConfig::Rocm { device_id: 0 },
            model_dir: PathBuf::from("/models"),
            model_search_paths: vec![
                PathBuf::from("/models/embeddings"),
                PathBuf::from("/models/llm"),
            ],
            cache_dir: Some(PathBuf::from("/cache")),
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 9000,
                ..Default::default()
            },
            ..Default::default()
        };

        // Serialize to TOML
        let toml_str = toml::to_string_pretty(&config).unwrap();

        // Deserialize back
        let deserialized: BurnConfig = toml::from_str(&toml_str).unwrap();

        assert_eq!(config.model_dir, deserialized.model_dir);
        assert_eq!(config.model_search_paths, deserialized.model_search_paths);
        assert_eq!(config.cache_dir, deserialized.cache_dir);
        assert_eq!(config.server.host, deserialized.server.host);
        assert_eq!(config.server.port, deserialized.server.port);
    }
}
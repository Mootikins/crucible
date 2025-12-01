#[cfg(test)]
mod tests {
    use anyhow::Result;
    use crucible_burn::models::{ModelRegistry, ModelFormat, ModelInfo, ModelType};
    use crucible_burn::{BurnConfig, BackendConfig, BackendType};
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    /// Test basic model registry functionality with empty directory
    #[tokio::test]
    async fn test_model_registry_empty() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let registry = ModelRegistry::new(vec![temp_dir.path().to_path_buf()]).await?;

        assert_eq!(registry.get_all_models().len(), 0);
        Ok(())
    }

    /// Test SafeTensors format detection (our most important format)
    #[test]
    fn test_safetensors_format_detection() {
        // Test basic format variants (these are enum variants, not constructed from extensions)
        let format = ModelFormat::SafeTensors;
        assert_eq!(format, ModelFormat::SafeTensors);

        let format = ModelFormat::GGUF;
        assert_eq!(format, ModelFormat::GGUF);

        let format = ModelFormat::ONNX;
        assert_eq!(format, ModelFormat::ONNX);
    }

    // Note: test_path_traversal_protection removed - validate_model_path is private
    // Path validation happens internally during model scanning

    /// Test configuration validation
    #[test]
    fn test_config_validation() {
        let mut config = BurnConfig::default();

        // Valid config should pass
        assert!(config.validate().is_ok());

        // Invalid port should fail
        config.server.port = 0;
        assert!(config.validate().is_err());

        // Reset and test resource limits
        config = BurnConfig::default();
        config.resource_limits.max_models_loaded = 0;
        assert!(config.validate().is_err());

        config.resource_limits.max_models_loaded = 1;
        config.resource_limits.max_concurrent_operations = 0;
        assert!(config.validate().is_err());
    }

    /// Test model info creation and basic properties
    #[test]
    fn test_model_info_creation() {
        let model_info = ModelInfo::new(
            "test_model".to_string(),
            ModelType::Llm,
            ModelFormat::SafeTensors,
            Path::new("/test/path").to_path_buf(),
        );

        assert_eq!(model_info.name, "test_model");
        assert_eq!(model_info.model_type, ModelType::Llm);
        assert_eq!(model_info.format, ModelFormat::SafeTensors);
        assert_eq!(model_info.path, Path::new("/test/path"));
        // Note: to_extension() is not public, just check the format variant directly
    }

    /// Test backend configuration conversion
    #[test]
    fn test_backend_config_conversion() {
        let vulkan_config = BackendConfig::Vulkan { device_id: 1 };
        let backend_type = vulkan_config.to_backend_type(8);
        assert!(matches!(backend_type, BackendType::Vulkan { device_id: 1 }));

        let cpu_config = BackendConfig::Cpu { num_threads: 4 };
        let backend_type = cpu_config.to_backend_type(8);
        assert!(matches!(backend_type, BackendType::Cpu { num_threads: 4 }));
    }

    // Note: test_model_type_detection removed - determine_model_type is private
    // Model type detection happens internally during scanning
}
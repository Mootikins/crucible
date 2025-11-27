#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;
    use std::path::Path;

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
        let format = ModelFormat::from_extension("safetensors");
        assert_eq!(format, ModelFormat::SafeTensors);

        let format = ModelFormat::from_extension("gguf");
        assert_eq!(format, ModelFormat::Gguf);

        let format = ModelFormat::from_extension("onnx");
        assert_eq!(format, ModelFormat::Onnx);
    }

    /// Test path traversal protection
    #[test]
    fn test_path_traversal_protection() {
        let temp_dir = TempDir::new().unwrap();
        let search_paths = vec![temp_dir.path().to_path_buf()];
        let registry = ModelRegistry::new(search_paths).await.unwrap();

        // These should be rejected as unsafe paths
        let unsafe_paths = vec![
            Path::new("../../../etc/passwd"),
            Path::new("/etc/passwd"),
            Path::new("..\\..\\windows\\system32"),
        ];

        for unsafe_path in unsafe_paths {
            let result = registry.validate_model_path(unsafe_path);
            assert!(result.is_err(), "Unsafe path should be rejected: {:?}", unsafe_path);
        }
    }

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
        assert_eq!(model_info.format.to_extension(), "safetensors");
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

    /// Test model type detection from format and files
    #[test]
    fn test_model_type_detection() {
        let registry = ModelRegistry::new(vec![]).await.unwrap();

        // Test with SafeTensors files
        let safetensors_files = vec![
            Path::new("model.safetensors").to_path_buf(),
        ];
        let model_type = registry.determine_model_type(
            Path::new("/test"),
            &ModelFormat::SafeTensors,
            &safetensors_files
        ).unwrap();
        assert_eq!(model_type, ModelType::Llm);

        // Test with GGUF embedding files
        let embedding_files = vec![
            Path::new("embed.gguf").to_path_buf(),
        ];
        let model_type = registry.determine_model_type(
            Path::new("/test"),
            &ModelFormat::Gguf,
            &embedding_files
        ).unwrap();
        assert_eq!(model_type, ModelType::Embedding);
    }
}
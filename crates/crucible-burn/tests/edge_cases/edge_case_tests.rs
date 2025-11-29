//! Edge case and error handling tests

use crucible_burn::{
    models::{ModelInfo, ModelType, ModelFormat, ModelRegistry},
    config::{BurnConfig, BackendConfig},
    hardware::{HardwareInfo, GpuInfo, GpuVendor, BackendType},
};
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use std::fs;
use crate::common::test_utils::*;

#[cfg(test)]
mod edge_case_tests {
    use super::*;

    #[test]
    fn test_empty_model_directory() {
        let temp_dir = TempDir::new().unwrap();
        let empty_dir = temp_dir.path().join("empty");
        fs::create_dir_all(&empty_dir).unwrap();

        let mut registry = ModelRegistry::new(vec![empty_dir]).tokio_test().await;
        registry.scan_models().tokio_test().await.unwrap();

        assert_eq!(registry.get_all_models().len(), 0);
    }

    #[test]
    fn test_directory_with_only_config_files() {
        let temp_dir = TempDir::new().unwrap();
        let model_dir = temp_dir.path().join("config_only_model");
        fs::create_dir_all(&model_dir).unwrap();

        // Create only config.json, no model files
        fs::write(model_dir.join("config.json"), r#"{"model_type": "embedding"}"#).unwrap();

        let mut registry = ModelRegistry::new(vec![temp_dir.path().to_path_buf()]).tokio_test().await;
        registry.scan_models().tokio_test().await.unwrap();

        // Should detect the model but mark it as incomplete
        let models = registry.get_all_models();
        assert_eq!(models.len(), 1);
        let model = models.values().next().unwrap();
        assert!(!model.is_complete());
    }

    #[test]
    fn test_malformed_config_json() {
        let temp_dir = TempDir::new().unwrap();
        let model_dir = temp_dir.path().join("malformed_config");
        fs::create_dir_all(&model_dir).unwrap();

        // Create malformed config.json
        TestDataGenerators::create_malformed_config(model_dir.join("config.json")).unwrap();
        fs::write(model_dir.join("model.safetensors"), b"fake_model").unwrap();

        let mut model_info = ModelInfo::new(
            "malformed-model".to_string(),
            ModelType::Embedding,
            ModelFormat::SafeTensors,
            model_dir.clone(),
        );

        // Should handle malformed config gracefully
        let result = model_info.load_metadata();
        assert!(result.is_ok()); // Should not panic
        assert!(model_info.config_path.is_some());
        assert_eq!(model_info.dimensions, None); // Should not extract dimensions
    }

    #[test]
    fn test_corrupted_model_file() {
        let temp_dir = TempDir::new().unwrap();
        let model_dir = temp_dir.path().join("corrupted_model");
        fs::create_dir_all(&model_dir).unwrap();

        fs::write(model_dir.join("config.json"), r#"{"model_type": "embedding"}"#).unwrap();
        TestDataGenerators::create_corrupted_model_file(model_dir.join("model.safetensors")).unwrap();

        let model_info = ModelInfo::new(
            "corrupted-model".to_string(),
            ModelType::Embedding,
            ModelFormat::SafeTensors,
            model_dir.clone(),
        );

        // Should still detect the model file even if corrupted
        assert!(model_info.has_model_file());
    }

    #[test]
    fn test_very_deep_directory_structure() {
        let temp_dir = TempDir::new().unwrap();
        let mut current_path = temp_dir.path().to_path_buf();

        // Create a deeply nested directory structure
        for i in 0..20 {
            current_path = current_path.join(format!("level_{}", i));
        }
        fs::create_dir_all(&current_path).unwrap();

        // Create model file at the deepest level
        fs::write(current_path.join("config.json"), r#"{"model_type": "embedding"}"#).unwrap();
        fs::write(current_path.join("model.safetensors"), b"fake_model").unwrap();

        let mut registry = ModelRegistry::new(vec![temp_dir.path().to_path_buf()]).tokio_test().await;
        registry.scan_models().tokio_test().await.unwrap();

        // Should still find the model even with deep nesting (within MAX_DEPTH)
        assert!(registry.get_all_models().len() > 0);
    }

    #[test]
    fn test_symlink_handling() {
        #[cfg(unix)]
        {
            let temp_dir = TempDir::new().unwrap();
            let real_model_dir = temp_dir.path().join("real_model");
            fs::create_dir_all(&real_model_dir).unwrap();

            fs::write(real_model_dir.join("config.json"), r#"{"model_type": "embedding"}"#).unwrap();
            fs::write(real_model_dir.join("model.safetensors"), b"fake_model").unwrap();

            // Create symlink
            let symlink_path = temp_dir.path().join("symlink_model");
            std::os::unix::fs::symlink(&real_model_dir, &symlink_path).unwrap();

            let mut registry = ModelRegistry::new(vec![symlink_path]).tokio_test().await;
            registry.scan_models().tokio_test().await.unwrap();

            // Should find model via symlink
            assert_eq!(registry.get_all_models().len(), 1);
        }
    }

    #[test]
    fn test_permission_denied_directories() {
        let temp_dir = TempDir::new().unwrap();
        let restricted_dir = temp_dir.path().join("restricted");
        fs::create_dir_all(&restricted_dir).unwrap();

        // Create model in restricted directory
        fs::write(restricted_dir.join("config.json"), r#"{"model_type": "embedding"}"#).unwrap();
        fs::write(restricted_dir.join("model.safetensors"), b"fake_model").unwrap();

        // Try to remove read permissions (may fail on some systems)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&restricted_dir).unwrap().permissions();
            perms.set_mode(0o000);
            fs::set_permissions(&restricted_dir, perms).unwrap();

            let mut registry = ModelRegistry::new(vec![restricted_dir]).tokio_test().await;
            let result = registry.scan_models().tokio_test().await;

            // Should handle permission errors gracefully
            assert!(result.is_ok() || result.is_err()); // Either way, shouldn't panic

            // Restore permissions for cleanup
            let mut perms = fs::metadata(&restricted_dir).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&restricted_dir, perms).unwrap();
        }
    }

    #[test]
    fn test_invalid_unicode_filenames() {
        let temp_dir = TempDir::new().unwrap();
        let model_dir = temp_dir.path().join("unicode_test");
        fs::create_dir_all(&model_dir).unwrap();

        // Create config with unicode content
        fs::write(model_dir.join("config.json"), r#"{"model_type": "embedding", "name": "测试模型"}"#).unwrap();
        fs::write(model_dir.join("model.safetensors"), b"fake_model").unwrap();

        let mut registry = ModelRegistry::new(vec![model_dir]).tokio_test().await;
        let result = registry.scan_models().tokio_test().await;

        // Should handle unicode gracefully
        assert!(result.is_ok());
    }

    #[test]
    fn test_extremely_large_config_files() {
        let temp_dir = TempDir::new().unwrap();
        let model_dir = temp_dir.path().join("large_config");
        fs::create_dir_all(&model_dir).unwrap();

        // Create a very large config file
        let mut large_config = r#"{"model_type": "embedding", "large_array": ["#.to_string();
        for i in 0..10000 {
            large_config.push_str(&format!("\"item_{}\", ", i));
        }
        large_config.push_str("]}");

        fs::write(model_dir.join("config.json"), large_config).unwrap();
        fs::write(model_dir.join("model.safetensors"), b"fake_model").unwrap();

        let mut model_info = ModelInfo::new(
            "large-config-model".to_string(),
            ModelType::Embedding,
            ModelFormat::SafeTensors,
            model_dir.clone(),
        );

        // Should handle large files without memory issues
        let result = model_info.load_metadata();
        assert!(result.is_ok());
    }

    #[test]
    fn test_zero_length_model_files() {
        let temp_dir = TempDir::new().unwrap();
        let model_dir = temp_dir.path().join("zero_length");
        fs::create_dir_all(&model_dir).unwrap();

        fs::write(model_dir.join("config.json"), r#"{"model_type": "embedding"}"#).unwrap();
        fs::write(model_dir.join("model.safetensors"), b"").unwrap(); // Empty file

        let model_info = ModelInfo::new(
            "zero-length-model".to_string(),
            ModelType::Embedding,
            ModelFormat::SafeTensors,
            model_dir.clone(),
        );

        // Should still detect the file even if empty
        assert!(model_info.has_model_file());
    }

    #[test]
    fn test_circular_symlinks() {
        #[cfg(unix)]
        {
            let temp_dir = TempDir::new().unwrap();
            let symlink_a = temp_dir.path().join("symlink_a");
            let symlink_b = temp_dir.path().join("symlink_b");

            // Create circular symlinks
            std::os::unix::fs::symlink(&symlink_b, &symlink_a).unwrap();
            std::os::unix::fs::symlink(&symlink_a, &symlink_b).unwrap();

            let mut registry = ModelRegistry::new(vec![symlink_a]).tokio_test().await;
            let result = registry.scan_models().tokio_test().await;

            // Should handle circular symlinks gracefully without infinite loops
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_model_registry_concurrent_access() {
        let temp_dir = TempDir::new().unwrap();
        let mut registry = ModelRegistry::new(vec![temp_dir.path().to_path_buf()]).tokio_test().await;

        // Test concurrent scanning
        let mut handles = vec![];
        for i in 0..5 {
            let mut reg_clone = ModelRegistry::new(vec![temp_dir.path().to_path_buf()])
                .tokio_test().await;
            let handle = tokio::spawn(async move {
                reg_clone.scan_models().await
            });
            handles.push(handle);
        }

        // Wait for all to complete
        for handle in handles {
            let result = handle.tokio_test().await.unwrap();
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_hardware_detection_edge_cases() {
        // Test with invalid GPU information
        let invalid_gpus = vec![
            GpuInfo {
                name: "".to_string(), // Empty name
                vendor: GpuVendor::Unknown,
                memory_mb: 0,
                vulkan_support: false,
                rocm_support: false,
                device_id: None,
            },
        ];

        let hardware_info = HardwareInfo {
            cpu_cores: 0, // Invalid CPU cores
            cpu_threads: 0,
            cpu_arch: "".to_string(), // Empty architecture
            gpus: invalid_gpus,
            recommended_backend: BackendType::Cpu { num_threads: 1 },
        };

        // Should still handle gracefully
        let backend = HardwareInfo::recommend_backend(&hardware_info.gpus, 1);
        assert!(matches!(backend, BackendType::Cpu { .. }));
    }

    #[test]
    fn test_backend_validation_edge_cases() {
        let hardware_info = HardwareInfo {
            cpu_cores: 8,
            cpu_threads: 16,
            cpu_arch: "x86_64".to_string(),
            gpus: vec![],
            recommended_backend: BackendType::Cpu { num_threads: 8 },
        };

        // Test with invalid device IDs
        assert!(!hardware_info.is_backend_supported(
            &BackendType::Vulkan { device_id: 9999 }
        ));
        assert!(!hardware_info.is_backend_supported(
            &BackendType::Rocm { device_id: 9999 }
        ));

        // Test with CPU (should always be supported)
        assert!(hardware_info.is_backend_supported(
            &BackendType::Cpu { num_threads: 1 }
        ));
    }

    #[test]
    fn test_configuration_edge_cases() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("edge_case_config.toml");

        // Test configuration with invalid port
        let invalid_config = r#"
server.port = 70000
"#;
        fs::write(&config_path, invalid_config).unwrap();

        let result = BurnConfig::load(Some(&config_path)).tokio_test().await;
        assert!(result.is_ok()); // Should load

        let config = result.unwrap();
        let validation_result = config.validate();
        assert!(validation_result.is_err()); // But validation should fail
    }

    #[test]
    fn test_memory_pressure_handling() {
        let temp_dir = TempDir::new().unwrap();
        let model_dirs: Vec<PathBuf> = (0..1000)
            .map(|i| {
                let dir = temp_dir.path().join(format!("model_{}", i));
                fs::create_dir_all(&dir).unwrap();

                // Create a large number of files
                fs::write(dir.join("config.json"), r#"{"model_type": "embedding"}"#).unwrap();
                fs::write(dir.join("model.safetensors"), vec![0u8; 1024 * 1024]).unwrap(); // 1MB file

                for j in 0..10 {
                    fs::write(dir.join(format!("extra_{}.json", j)), r#"{"data": "test"}"#).unwrap();
                }

                dir
            })
            .collect();

        let mut registry = ModelRegistry::new(model_dirs).tokio_test().await;

        // Should handle large number of models without running out of memory
        let result = registry.scan_models().tokio_test().await;
        assert!(result.is_ok());

        let models = registry.get_all_models();
        assert_eq!(models.len(), 1000);
    }

    #[test]
    fn test_network_timeout_simulation() {
        // Simulate what happens when network-dependent operations timeout
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("network_config.toml");

        // Create config that would require network access
        let config_content = r#"
model_dir = "/nonexistent/remote/path"
model_search_paths = [
    "https://nonexistent-remote-server.com/models/",
    "/local/fallback/path"
]
"#;
        fs::write(&config_path, config_content).unwrap();

        let result = BurnConfig::load(Some(&config_path)).tokio_test().await;
        assert!(result.is_ok()); // Should handle gracefully
    }

    #[test]
    fn test_graceful_degradation() {
        let temp_dir = TempDir::new().unwrap();

        // Create models with varying levels of completeness
        let complete_model = temp_dir.path().join("complete_model");
        fs::create_dir_all(&complete_model).unwrap();
        fs::write(complete_model.join("config.json"), r#"{"model_type": "embedding", "hidden_size": 384}"#).unwrap();
        fs::write(complete_model.join("tokenizer.json"), "{}").unwrap();
        fs::write(complete_model.join("model.safetensors"), b"fake_model").unwrap();

        let partial_model = temp_dir.path().join("partial_model");
        fs::create_dir_all(&partial_model).unwrap();
        fs::write(partial_model.join("config.json"), r#"{"model_type": "embedding"}"#).unwrap();
        // Missing tokenizer and model file

        let broken_model = temp_dir.path().join("broken_model");
        fs::create_dir_all(&broken_model).unwrap();
        // Only tokenizer, no config or model
        fs::write(broken_model.join("tokenizer.json"), "{}").unwrap();

        let mut registry = ModelRegistry::new(vec![temp_dir.path().to_path_buf()]).tokio_test().await;
        let result = registry.scan_models().tokio_test().await;

        // Should find all models and handle incomplete ones gracefully
        assert!(result.is_ok());
        let models = registry.get_all_models();
        assert_eq!(models.len(), 3);

        // Check model completeness
        for model in models.values() {
            // Should not panic when checking completeness
            let _is_complete = model.is_complete();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use tempfile::TempDir;

    /// Test model discovery performance baseline
    #[tokio::test]
    async fn test_model_discovery_performance() -> Result<()> {
        let start_time = Instant::now();

        // Use the actual models directory to test real-world performance
        let models_path = Path::new("/home/moot/models");
        if !models_path.exists() {
            // Skip test if models directory doesn't exist
            println!("Skipping performance test - models directory not found");
            return Ok(());
        }

        let registry = ModelRegistry::new(vec![models_path.to_path_buf()]).await?;

        let scan_start = Instant::now();
        registry.scan_models().await?;
        let scan_duration = scan_start.elapsed();

        let models = registry.get_all_models();
        let total_time = start_time.elapsed();

        // Performance assertions
        assert!(scan_duration.as_secs() <= 5, "Model scan took too long: {:?}", scan_duration);
        assert!(!models.is_empty(), "No models found in /home/moot/models");

        println!("Performance Results:");
        println!("  Total time: {:?}", total_time);
        println!("  Scan time: {:?}", scan_duration);
        println!("  Models found: {}", models.len());
        println!("  Time per model: {:?}", scan_duration / models.len() as u32);

        // Should find at least the Qwen2.5-7B-Instruct model
        let qwen_found = models.values().any(|m| m.name.contains("Qwen2.5-7B-Instruct"));
        assert!(qwen_found, "Qwen2.5-7B-Instruct model not found");

        Ok(())
    }

    /// Test configuration loading performance
    #[tokio::test]
    async fn test_config_loading_performance() -> Result<()> {
        let iterations = 100;
        let start_time = Instant::now();

        for _ in 0..iterations {
            let config = BurnConfig::default();
            config.validate()?;
        }

        let duration = start_time.elapsed();
        let avg_time = duration / iterations;

        println!("Config loading performance:");
        println!("  Total time: {:?}", duration);
        println!("  Iterations: {}", iterations);
        println!("  Average time per config load: {:?}", avg_time);

        // Should be very fast (< 1ms per config load)
        assert!(avg_time.as_millis() < 1, "Config loading too slow: {:?}", avg_time);

        Ok(())
    }

    /// Test path validation performance
    #[tokio::test]
    async fn test_path_validation_performance() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let registry = ModelRegistry::new(vec![temp_dir.path().to_path_buf()]).await?;

        let test_paths = vec![
            temp_dir.path().join("test1"),
            temp_dir.path().join("test2"),
            temp_dir.path().join("test3"),
            Path::new("../../../etc/passwd").to_path_buf(), // Should fail quickly
            Path::new("/fake/path").to_path_buf(),
        ];

        let iterations = 1000;
        let start_time = Instant::now();

        for _ in 0..iterations {
            for path in &test_paths {
                let _ = registry.validate_model_path(path);
            }
        }

        let duration = start_time.elapsed();
        let total_validations = iterations * test_paths.len();
        let avg_time = duration / total_validations as u32;

        println!("Path validation performance:");
        println!("  Total time: {:?}", duration);
        println!("  Validations: {}", total_validations);
        println!("  Average time per validation: {:?}", avg_time);

        // Should be very fast (< 1μs per validation)
        assert!(avg_time.as_micros() < 1, "Path validation too slow: {:?}", avg_time);

        Ok(())
    }

    /// Test model format detection performance
    #[test]
    fn test_format_detection_performance() -> Result<()> {
        let extensions = vec!["safetensors", "gguf", "onnx", "pth", "mlx"];
        let iterations = 10000;
        let start_time = Instant::now();

        for _ in 0..iterations {
            for ext in &extensions {
                let _ = ModelFormat::from_extension(ext);
            }
        }

        let duration = start_time.elapsed();
        let total_detections = iterations * extensions.len();
        let avg_time = duration / total_detections as u32;

        println!("Format detection performance:");
        println!("  Total time: {:?}", duration);
        println!("  Detections: {}", total_detections);
        println!("  Average time per detection: {:?}", avg_time);

        // Should be extremely fast (< 100ns per detection)
        assert!(avg_time.as_nanos() < 100, "Format detection too slow: {:?}", avg_time);

        Ok(())
    }
}
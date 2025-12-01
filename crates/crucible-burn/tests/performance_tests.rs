#[cfg(test)]
mod tests {
    use anyhow::Result;
    use crucible_burn::models::{ModelRegistry, ModelFormat};
    use crucible_burn::BurnConfig;
    use std::path::Path;
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

        let mut registry = ModelRegistry::new(vec![models_path.to_path_buf()]).await?;

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

    // Note: test_path_validation_performance removed - validate_model_path is private
    // Path validation happens internally and is tested through model scanning

    // Note: test_format_detection_performance removed - from_extension method doesn't exist
    // ModelFormat uses direct enum variants (SafeTensors, GGUF, etc.)

    /// Test basic format variant creation performance
    #[test]
    fn test_format_variant_performance() -> Result<()> {
        let iterations = 10000;
        let start_time = Instant::now();

        for _ in 0..iterations {
            // Create format variants
            let _f1 = ModelFormat::SafeTensors;
            let _f2 = ModelFormat::GGUF;
            let _f3 = ModelFormat::ONNX;
            let _f4 = ModelFormat::PTH;
            let _f5 = ModelFormat::MLX;
        }

        let duration = start_time.elapsed();
        let total_creations = iterations * 5;
        let avg_time = duration / total_creations as u32;

        println!("Format variant creation performance:");
        println!("  Total time: {:?}", duration);
        println!("  Creations: {}", total_creations);
        println!("  Average time per creation: {:?}", avg_time);

        // Should be extremely fast (< 100ns per creation)
        assert!(avg_time.as_nanos() < 100, "Format creation too slow: {:?}", avg_time);

        Ok(())
    }
}
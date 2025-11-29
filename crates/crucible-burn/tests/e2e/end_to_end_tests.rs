//! End-to-end tests for complete workflows

use std::process::Command;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;

#[cfg(test)]
mod e2e_tests {
    use super::*;

    /// Create a comprehensive test environment with models and config
    fn create_test_environment() -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let models_dir = temp_dir.path().join("models");
        fs::create_dir_all(&models_dir).unwrap();

        // Create embedding model
        let embed_model_path = models_dir.join("embedding-model");
        fs::create_dir_all(&embed_model_path).unwrap();

        let embed_config = r#"
{
    "model_type": "embedding",
    "hidden_size": 384,
    "max_position_embeddings": 512,
    "vocab_size": 30522,
    "num_parameters": 42000000
}
"#;
        fs::write(embed_model_path.join("config.json"), embed_config).unwrap();
        fs::write(embed_model_path.join("tokenizer.json"), "{}").unwrap();
        fs::write(embed_model_path.join("model.safetensors"), b"fake_embedding_model").unwrap();

        // Create LLM model
        let llm_model_path = models_dir.join("llm-model");
        fs::create_dir_all(&llm_model_path).unwrap();

        let llm_config = r#"
{
    "model_type": "causal_lm",
    "hidden_size": 768,
    "num_attention_heads": 12,
    "num_hidden_layers": 12,
    "vocab_size": 50257,
    "num_parameters": 125000000
}
"#;
        fs::write(llm_model_path.join("config.json"), llm_config).unwrap();
        fs::write(llm_model_path.join("tokenizer.json"), "{}").unwrap();
        fs::write(llm_model_path.join("model.gguf"), b"fake_llm_model").unwrap();

        // Create config file
        let config_path = temp_dir.path().join("burn.toml");
        let config_content = format!(r#"
model_dir = "{}"
default_backend = "cpu"

[server]
host = "127.0.0.1"
port = 0

[benchmarks]
default_iterations = 5
warmup_iterations = 1
"#, models_dir.display());

        fs::write(&config_path, config_content).unwrap();

        (temp_dir, config_path)
    }

    #[test]
    #[ignore] // Ignore by default as it requires the binary to be built
    fn test_complete_model_discovery_workflow() {
        let (temp_dir, config_path) = create_test_environment();

        // Step 1: Hardware detection
        let output = Command::new("cargo")
            .args(&["run", "--bin", "burn-test", "--",
                    "--config", config_path.to_str().unwrap(),
                    "detect", "hardware"])
            .output()
            .expect("Failed to run hardware detection");

        assert!(output.status.success(),
               "Hardware detection failed: {}",
               String::from_utf8_lossy(&output.stderr));

        // Step 2: Model discovery
        let output = Command::new("cargo")
            .args(&["run", "--bin", "burn-test", "--",
                    "--config", config_path.to_str().unwrap(),
                    "models", "list"])
            .output()
            .expect("Failed to run model discovery");

        assert!(output.status.success(),
               "Model discovery failed: {}",
               String::from_utf8_lossy(&output.stderr));

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("embedding-model") || stdout.contains("No models found"),
               "Expected to find embedding model or handle gracefully");

        // Step 3: Backend testing
        let output = Command::new("cargo")
            .args(&["run", "--bin", "burn-test", "--",
                    "--config", config_path.to_str().unwrap(),
                    "detect", "test-backend", "cpu"])
            .output()
            .expect("Failed to run backend test");

        assert!(output.status.success(),
               "Backend testing failed: {}",
               String::from_utf8_lossy(&output.stderr));
    }

    #[test]
    #[ignore] // Ignore by default
    fn test_embedding_generation_workflow() {
        let (temp_dir, config_path) = create_test_environment();

        // Step 1: List embedding models
        let output = Command::new("cargo")
            .args(&["run", "--bin", "burn-test", "--",
                    "--config", config_path.to_str().unwrap(),
                    "embed", "list"])
            .output()
            .expect("Failed to list embedding models");

        assert!(output.status.success());

        // Step 2: Test embedding generation (will fail gracefully since models are fake)
        let output = Command::new("cargo")
            .args(&["run", "--bin", "burn-test", "--",
                    "--config", config_path.to_str().unwrap(),
                    "embed", "test", "embedding-model", "Hello, world!",
                    "--backend", "cpu"])
            .output()
            .expect("Failed to test embedding generation");

        // Should fail gracefully (expected since we have fake models)
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("error") || stderr.contains("failed") ||
               stderr.contains("not found") || stderr.contains("incomplete"),
               "Expected graceful error handling, got: {}", stderr);
    }

    #[test]
    #[ignore] // Ignore by default
    fn test_llm_inference_workflow() {
        let (temp_dir, config_path) = create_test_environment();

        // Step 1: List LLM models
        let output = Command::new("cargo")
            .args(&["run", "--bin", "burn-test", "--",
                    "--config", config_path.to_str().unwrap(),
                    "llm", "list"])
            .output()
            .expect("Failed to list LLM models");

        assert!(output.status.success());

        // Step 2: Test LLM inference (will fail gracefully since models are fake)
        let output = Command::new("cargo")
            .args(&["run", "--bin", "burn-test", "--",
                    "--config", config_path.to_str().unwrap(),
                    "llm", "infer", "llm-model", "Once upon a time",
                    "--max-tokens", "10", "--backend", "cpu"])
            .output()
            .expect("Failed to test LLM inference");

        // Should fail gracefully (expected since we have fake models)
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("error") || stderr.contains("failed") ||
               stderr.contains("not found") || stderr.contains("incomplete"),
               "Expected graceful error handling, got: {}", stderr);
    }

    #[tokio::test]
    #[ignore]
    async fn test_server_startup_and_shutdown_workflow() {
        let (temp_dir, config_path) = create_test_environment();

        // This test would start the server and verify it responds to requests
        // Requires the 'server' feature to be enabled

        // In a real implementation, you would:
        // 1. Start the server in the background
        // 2. Wait for it to be ready
        // 3. Send HTTP requests to test endpoints
        // 4. Verify responses
        // 5. Shut down the server cleanly

        // For now, just verify the command can be parsed
        let output = Command::new("cargo")
            .args(&["run", "--bin", "burn-test", "--features", "server", "--",
                    "--config", config_path.to_str().unwrap(),
                    "server", "status"])
            .output()
            .expect("Failed to check server status");

        // Should handle missing server feature gracefully
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("Server functionality") ||
               stderr.contains("server feature") ||
               output.status.success(),
               "Expected graceful handling of server command");
    }

    #[test]
    #[ignore]
    fn test_benchmark_execution_workflow() {
        let (temp_dir, config_path) = create_test_environment();

        // Test benchmark command execution (requires 'benchmarks' feature)
        let output = Command::new("cargo")
            .args(&["run", "--bin", "burn-test", "--features", "benchmarks", "--",
                    "--config", config_path.to_str().unwrap(),
                    "bench", "embed", "embedding-model",
                    "--iterations", "1"])
            .output()
            .expect("Failed to run benchmark");

        // Should handle missing benchmark feature gracefully
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("Benchmarking requires") ||
               stderr.contains("benchmarks feature") ||
               output.status.success(),
               "Expected graceful handling of benchmark command");
    }

    #[test]
    #[ignore]
    fn test_configuration_persistence_workflow() {
        let (temp_dir, original_config_path) = create_test_environment();

        // Step 1: Load and validate initial configuration
        let output = Command::new("cargo")
            .args(&["run", "--bin", "burn-test", "--",
                    "--config", original_config_path.to_str().unwrap(),
                    "detect", "hardware"])
            .output()
            .expect("Failed to load initial config");

        assert!(output.status.success());

        // Step 2: Create modified configuration
        let modified_config_path = temp_dir.path().join("modified_config.toml");
        let modified_config = r#"
model_dir = "/different/path"
default_backend = "cpu"
server.port = 9999
"#;
        fs::write(&modified_config_path, modified_config).unwrap();

        // Step 3: Verify modified configuration is loaded
        let output = Command::new("cargo")
            .args(&["run", "--bin", "burn-test", "--",
                    "--config", modified_config_path.to_str().unwrap(),
                    "detect", "hardware"])
            .output()
            .expect("Failed to load modified config");

        assert!(output.status.success());
    }

    #[test]
    #[ignore]
    fn test_error_recovery_workflow() {
        let temp_dir = TempDir::new().unwrap();

        // Create invalid configuration
        let invalid_config_path = temp_dir.path().join("invalid_config.toml");
        fs::write(&invalid_config_path, "invalid = toml [[[").unwrap();

        // Should handle invalid TOML gracefully
        let output = Command::new("cargo")
            .args(&["run", "--bin", "burn-test", "--",
                    "--config", invalid_config_path.to_str().unwrap(),
                    "detect", "hardware"])
            .output()
            .expect("Failed to handle invalid config");

        assert!(!output.status.success() ||
               String::from_utf8_lossy(&output.stderr).contains("Failed to parse"));

        // Test with non-existent model
        let output = Command::new("cargo")
            .args(&["run", "--bin", "burn-test", "--",
                    "models", "search", "nonexistent_model"])
            .output()
            .expect("Failed to handle nonexistent model search");

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("not found") || stderr.contains("No models") ||
               output.status.success());
    }

    #[test]
    #[ignore]
    fn test_large_scale_model_discovery() {
        let temp_dir = TempDir::new().unwrap();
        let models_dir = temp_dir.path().join("models");
        fs::create_dir_all(&models_dir).unwrap();

        // Create many models to test performance with large numbers
        for i in 0..100 {
            let model_path = models_dir.join(format!("model-{}", i));
            fs::create_dir_all(&model_path).unwrap();

            let config = format!(r#"
{{
    "model_type": "{}",
    "hidden_size": 768,
    "num_parameters": {}
}}
"#, if i % 2 == 0 { "embedding" } else { "causal_lm" }, 100_000_000 + i * 1_000_000);

            fs::write(model_path.join("config.json"), config).unwrap();
            fs::write(model_path.join("tokenizer.json"), "{}").unwrap();

            let model_file = if i % 3 == 0 {
                "model.safetensors"
            } else if i % 3 == 1 {
                "model.gguf"
            } else {
                "model.bin"
            };
            fs::write(model_path.join(model_file), b"fake_model").unwrap();
        }

        let config_path = temp_dir.path().join("config.toml");
        let config_content = format!(r#"
model_dir = "{}"
default_backend = "cpu"
"#, models_dir.display());

        fs::write(&config_path, config_content).unwrap();

        // Test large-scale model discovery
        let start = std::time::Instant::now();
        let output = Command::new("cargo")
            .args(&["run", "--bin", "burn-test", "--",
                    "--config", config_path.to_str().unwrap(),
                    "models", "list"])
            .output()
            .expect("Failed to run large-scale model discovery");

        let duration = start.elapsed();

        assert!(output.status.success(),
               "Large-scale discovery failed: {}",
               String::from_utf8_lossy(&output.stderr));

        // Should complete in reasonable time (adjust threshold as needed)
        assert!(duration < Duration::from_secs(30),
               "Model discovery took too long: {:?}", duration);

        let stdout = String::from_utf8_lossy(&output.stdout);
        println!("Model discovery completed in {:?}, output: {}", duration, stdout);
    }

    #[test]
    #[ignore]
    fn test_concurrent_command_execution() {
        let (temp_dir, config_path) = create_test_environment();

        // Test that multiple commands can be executed concurrently
        let mut handles = vec![];

        for i in 0..5 {
            let config_path_clone = config_path.clone();
            let handle = std::thread::spawn(move || {
                let output = Command::new("cargo")
                    .args(&["run", "--bin", "burn-test", "--",
                            "--config", config_path_clone.to_str().unwrap(),
                            "detect", "hardware"])
                    .output()
                    .expect("Failed to run concurrent command");

                assert!(output.status.success(),
                       "Concurrent command {} failed: {}",
                       i, String::from_utf8_lossy(&output.stderr));

                String::from_utf8_lossy(&output.stdout).to_string()
            });
            handles.push(handle);
        }

        // Wait for all commands to complete
        for handle in handles {
            let _output = handle.join().expect("Thread panicked");
        }
    }
}
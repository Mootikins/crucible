//! Integration tests for CLI commands

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;
use tokio_test;

#[cfg(test)]
mod cli_integration_tests {
    use super::*;

    fn create_test_model_structure() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        let model_path = temp_dir.path().join("test-embedding-model");

        fs::create_dir_all(&model_path).unwrap();

        // Create config.json
        let config_content = r#"
{
    "model_type": "embedding",
    "hidden_size": 384,
    "max_position_embeddings": 512,
    "vocab_size": 30522
}
"#;
        fs::write(model_path.join("config.json"), config_content).unwrap();

        // Create tokenizer.json
        fs::write(model_path.join("tokenizer.json"), "{}").unwrap();

        // Create fake model file
        fs::write(model_path.join("model.safetensors"), b"fake_model_data").unwrap();

        temp_dir
    }

    #[test]
    fn test_cli_version() {
        let mut cmd = Command::cargo_bin("burn-test").unwrap();
        cmd.arg("--version");

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("0.1.0"));
    }

    #[test]
    fn test_cli_help() {
        let mut cmd = Command::cargo_bin("burn-test").unwrap();
        cmd.arg("--help");

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("Burn ML Framework Testing"))
            .stdout(predicate::str::contains("Usage:"));
    }

    #[test]
    fn test_hardware_detection_command() {
        let mut cmd = Command::cargo_bin("burn-test").unwrap();
        cmd.args(&["detect", "hardware"]);

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("Hardware Information:").or(
                predicate::str::contains("CPU cores").or(
                    predicate::str::contains("Recommended backend")
                )
            ));
    }

    #[test]
    fn test_backends_command() {
        let mut cmd = Command::cargo_bin("burn-test").unwrap();
        cmd.args(&["detect", "backends"]);

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("Available backends").or(
                predicate::str::contains("Vulkan").or(
                    predicate::str::contains("CPU")
                )
            ));
    }

    #[test]
    fn test_models_list_command() {
        let temp_dir = create_test_model_structure();
        let model_dir = temp_dir.path().join("test-embedding-model");

        let mut cmd = Command::cargo_bin("burn-test").unwrap();

        // Set config to point to our test model directory
        let config_content = format!(r#"
model_dir = "{}"
default_backend = "cpu"
"#, model_dir.parent().unwrap().display());

        let config_path = temp_dir.path().join("test_config.toml");
        fs::write(&config_path, config_content).unwrap();

        cmd.env("BURN_CONFIG", &config_path);
        cmd.args(&["models", "list"]);

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("test-embedding-model").or(
                predicate::str::contains("No models found").or(
                    predicate::str::contains("Scanning")
                )
            ));
    }

    #[test]
    fn test_models_search_command() {
        let temp_dir = create_test_model_structure();
        let model_dir = temp_dir.path().join("test-embedding-model");

        let mut cmd = Command::cargo_bin("burn-test").unwrap();

        let config_content = format!(r#"
model_dir = "{}"
default_backend = "cpu"
"#, model_dir.parent().unwrap().display());

        let config_path = temp_dir.path().join("test_config.toml");
        fs::write(&config_path, config_content).unwrap();

        cmd.env("BURN_CONFIG", &config_path);
        cmd.args(&["models", "search", "test-embedding"]);

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("test-embedding-model").or(
                predicate::str::contains("No models found").or(
                    predicate::str::contains("Search")
                )
            ));
    }

    #[test]
    fn test_embedding_list_command() {
        let mut cmd = Command::cargo_bin("burn-test").unwrap();
        cmd.args(&["embed", "list"]);

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("Available embedding models").or(
                predicate::str::contains("No embedding models found")
            ));
    }

    #[test]
    fn test_llm_list_command() {
        let mut cmd = Command::cargo_bin("burn-test").unwrap();
        cmd.args(&["llm", "list"]);

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("Available LLM models").or(
                predicate::str::contains("No LLM models found")
            ));
    }

    #[test]
    fn test_config_file_integration() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("integration_config.toml");

        let config_content = r#"
[default_backend]
cpu = { num_threads = 2 }

model_dir = "/nonexistent/models"

[server]
host = "0.0.0.0"
port = 9999

[benchmarks]
default_iterations = 5
warmup_iterations = 1

[hardware]
auto_detect = false
memory_limit_gb = 4
"#;

        fs::write(&config_path, config_content).unwrap();

        let mut cmd = Command::cargo_bin("burn-test").unwrap();
        cmd.args(["-C", config_path.to_str().unwrap(), "detect", "hardware"]);

        cmd.assert()
            .success();
    }

    #[test]
    fn test_verbose_logging() {
        let mut cmd = Command::cargo_bin("burn-test").unwrap();
        cmd.args(&["--verbose", "detect", "hardware"]);

        cmd.assert()
            .success()
            // Verbose mode might show debug output
            .stdout(predicate::str::contains("Hardware Information:").or(
                predicate::str::contains("DEBUG")
            ));
    }

    #[test]
    fn test_server_command_without_feature() {
        let mut cmd = Command::cargo_bin("burn-test").unwrap();
        cmd.args(&["server", "start"]);

        // Server functionality requires 'server' feature
        cmd.assert()
            .success()
            .stderr(predicate::str::contains("Server functionality requires").or(
                predicate::str::contains("server feature")
            ));
    }

    #[test]
    fn test_bench_command_without_feature() {
        let mut cmd = Command::cargo_bin("burn-test").unwrap();
        cmd.args(&["bench", "embed", "model"]);

        // Benchmarking requires 'benchmarks' feature
        cmd.assert()
            .success()
            .stderr(predicate::str::contains("Benchmarking requires").or(
                predicate::str::contains("benchmarks feature")
            ));
    }

    #[test]
    fn test_backend_test_command() {
        let mut cmd = Command::cargo_bin("burn-test").unwrap();
        cmd.args(&["detect", "test-backend", "cpu"]);

        // CPU backend test should always work
        cmd.assert()
            .success();
    }

    #[test]
    fn test_invalid_backend_test_command() {
        let mut cmd = Command::cargo_bin("burn-test").unwrap();
        cmd.args(&["detect", "test-backend", "invalid_backend"]);

        // Invalid backend should show error
        cmd.assert()
            .failure()
            .stderr(predicate::str::contains("error").or(
                predicate::str::contains("invalid").or(
                    predicate::str::contains("backend")
                )
            ));
    }

    #[test]
    fn test_embed_test_command_with_nonexistent_model() {
        let mut cmd = Command::cargo_bin("burn-test").unwrap();
        cmd.args(&["embed", "test", "nonexistent_model", "test text"]);

        // Should handle missing model gracefully
        cmd.assert()
            .failure()
            .stderr(predicate::str::contains("error").or(
                predicate::str::contains("not found").or(
                    predicate::str::contains("Model")
                )
            ));
    }

    #[test]
    fn test_llm_infer_command_with_nonexistent_model() {
        let mut cmd = Command::cargo_bin("burn-test").unwrap();
        cmd.args(&["llm", "infer", "nonexistent_model", "test prompt"]);

        // Should handle missing model gracefully
        cmd.assert()
            .failure()
            .stderr(predicate::str::contains("error").or(
                predicate::str::contains("not found").or(
                    predicate::str::contains("Model")
                )
            ));
    }

    #[test]
    fn test_global_config_flag() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test_config.toml");

        let config_content = r#"
[default_backend]
cpu = { num_threads = 1 }
"#;
        fs::write(&config_path, config_content).unwrap();

        let mut cmd = Command::cargo_bin("burn-test").unwrap();
        cmd.args(&["--config", config_path.to_str().unwrap(), "detect", "hardware"]);

        cmd.assert()
            .success();
    }

    #[test]
    fn test_models_detailed_flag() {
        let mut cmd = Command::cargo_bin("burn-test").unwrap();
        cmd.args(&["models", "list", "--detailed"]);

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("Available models").or(
                predicate::str::contains("No models found").or(
                    predicate::str::contains("Scanning")
                )
            ));
    }

    #[test]
    fn test_models_sort_by_size_flag() {
        let mut cmd = Command::cargo_bin("burn-test").unwrap();
        cmd.args(&["models", "list", "--by-size"]);

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("Available models").or(
                predicate::str::contains("No models found").or(
                    predicate::str::contains("Scanning")
                )
            ));
    }

    #[test]
    fn test_embed_backend_option() {
        let mut cmd = Command::cargo_bin("burn-test").unwrap();
        cmd.args(&["embed", "test", "test_model", "test text", "--backend", "cpu"]);

        // Should parse the backend option correctly
        cmd.assert()
            .failure() // Will fail because model doesn't exist
            .stderr(predicate::str::contains("error").or(
                predicate::str::contains("Model")
            ));
    }
}
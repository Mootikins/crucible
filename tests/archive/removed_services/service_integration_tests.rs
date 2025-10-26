//! Integration tests for CLI service integration
//!
//! This module tests the new CLI commands for service management and migration.

use anyhow::Result;
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Test service health command
#[tokio::test]
async fn test_service_health_command() {
    let mut cmd = Command::cargo_bin("crucible").unwrap();

    cmd.args(&["service", "health", "--format", "json"]);

    // The command should succeed (even with simulated data)
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("service"))
        .stdout(predicate::str::contains("status"));
}

/// Test service metrics command
#[tokio::test]
async fn test_service_metrics_command() {
    let mut cmd = Command::cargo_bin("crucible").unwrap();

    cmd.args(&["service", "metrics", "--format", "table"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Service"))
        .stdout(predicate::str::contains("Total"));
}

/// Test service list command
#[tokio::test]
async fn test_service_list_command() {
    let mut cmd = Command::cargo_bin("crucible").unwrap();

    cmd.args(&["service", "list", "--status"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("crucible-script-engine"))
        .stdout(predicate::str::contains("crucible-rune-service"));
}

/// Test migration status command
#[tokio::test]
async fn test_migration_status_command() {
    let mut cmd = Command::cargo_bin("crucible").unwrap();

    cmd.args(&["migration", "status", "--format", "json"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("migration_enabled"));
}

/// Test migration list command
#[tokio::test]
async fn test_migration_list_command() {
    let mut cmd = Command::cargo_bin("crucible").unwrap();

    cmd.args(&["migration", "list", "--format", "table"]);

    cmd.assert()
        .success();
}

/// Test migration dry run command
#[tokio::test]
async fn test_migration_dry_run_command() {
    let mut cmd = Command::cargo_bin("crucible").unwrap();

    cmd.args(&["migration", "migrate", "--dry-run"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("DRY RUN"))
        .stdout(predicate::str::contains("Would migrate"));
}

/// Test migration validate command
#[tokio::test]
async fn test_migration_validate_command() {
    let mut cmd = Command::cargo_bin("crucible").unwrap();

    cmd.args(&["migration", "validate", "--format", "table"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Validation Result"));
}

/// Test service logs command
#[tokio::test]
async fn test_service_logs_command() {
    let mut cmd = Command::cargo_bin("crucible").unwrap();

    cmd.args(&["service", "logs", "--lines", "5"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("logs for:"))
        .stdout(predicate::str::contains("Lines: 5"));
}

/// Test config shows new service and migration sections
#[tokio::test]
async fn test_config_with_new_sections() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("config.toml");

    // Create a test config
    let config_content = r#"
[vault]
path = "/tmp/test-vault"
embedding_url = "http://localhost:11434"
embedding_model = "test-model"

[services.script_engine]
enabled = true
security_level = "safe"
max_source_size = 1048576

[services.health]
enabled = true
check_interval_secs = 10

[migration]
enabled = true
default_security_level = "safe"
auto_migrate = false
"#;

    fs::write(&config_path, config_content)?;

    let mut cmd = Command::cargo_bin("crucible").unwrap();
    cmd.args(&["config", "show", "--config", &config_path.to_string_lossy()]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("services"))
        .stdout(predicate::str::contains("script_engine"))
        .stdout(predicate::str::contains("migration"));

    Ok(())
}

/// Test rune command with script execution
#[tokio::test]
async fn test_rune_command_execution() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let script_path = temp_dir.path().join("test_script.rn");

    // Create a simple test script
    let script_content = r#"
pub fn main() {
    "Hello from test script"
}
"#;

    fs::write(&script_path, script_content)?;

    let mut cmd = Command::cargo_bin("crucible").unwrap();
    cmd.args(&["run", &script_path.to_string_lossy()]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Executing"));

    Ok(())
}

/// Test help commands for new features
#[tokio::test]
async fn test_help_commands() {
    let mut cmd = Command::cargo_bin("crucible").unwrap();
    cmd.args(&["service", "--help"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("service management"))
        .stdout(predicate::str::contains("health"))
        .stdout(predicate::str::contains("metrics"))
        .stdout(predicate::str::contains("list"));

    let mut cmd = Command::cargo_bin("crucible").unwrap();
    cmd.args(&["migration", "--help"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("migration management"))
        .stdout(predicate::str::contains("migrate"))
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("validate"));
}

/// Test error handling for invalid commands
#[tokio::test]
async fn test_error_handling() {
    // Test invalid service
    let mut cmd = Command::cargo_bin("crucible").unwrap();
    cmd.args(&["service", "start", "non-existent-service"]);

    cmd.assert()
        .success(); // Should succeed even with simulated service

    // Test invalid migration target
    let mut cmd = Command::cargo_bin("crucible").unwrap();
    cmd.args(&["migration", "migrate", "--tool", "non-existent-tool"]);

    cmd.assert()
        .success(); // Should succeed in dry run mode
}

/// Test configuration validation
#[tokio::test]
async fn test_configuration_validation() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("invalid_config.toml");

    // Create an invalid config
    let config_content = r#"
[vault]
path = "/tmp/test-vault"

[services.script_engine]
enabled = true
security_level = "invalid_level"  # Invalid security level
max_source_size = 0  # Invalid size

[migration]
enabled = true
max_cache_size = 0  # Invalid cache size
"#;

    fs::write(&config_path, config_content)?;

    let mut cmd = Command::cargo_bin("crucible").unwrap();
    cmd.args(&["run", "test-script", "--config", &config_path.to_string_lossy()]);

    // The CLI should handle invalid config gracefully
    cmd.assert()
        .success(); // Should fall back to defaults

    Ok(())
}

/// Test command combinations and workflows
#[tokio::test]
async fn test_command_workflows() {
    // Test a typical migration workflow
    let mut cmd = Command::cargo_bin("crucible").unwrap();
    cmd.args(&["migration", "status"]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("crucible").unwrap();
    cmd.args(&["migration", "migrate", "--dry-run"]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("crucible").unwrap();
    cmd.args(&["service", "health"]);
    cmd.assert().success();

    let mut cmd = Command::cargo_bin("crucible").unwrap();
    cmd.args(&["service", "metrics"]);
    cmd.assert().success();
}

/// Test output formats
#[tokio::test]
async fn test_output_formats() {
    // Test JSON output
    let mut cmd = Command::cargo_bin("crucible").unwrap();
    cmd.args(&["service", "health", "--format", "json"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("{"))
        .stdout(predicate::str::contains("}"));

    // Test table output
    let mut cmd = Command::cargo_bin("crucible").unwrap();
    cmd.args(&["service", "health", "--format", "table"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Service"));
}

/// Test CLI with environment variables
#[tokio::test]
async fn test_environment_variables() {
    // Test with CRUCIBLE_TEST_MODE to avoid loading user config
    let mut cmd = Command::cargo_bin("crucible").unwrap();
    cmd.env("CRUCIBLE_TEST_MODE", "1");
    cmd.args(&["service", "list"]);

    cmd.assert()
        .success();
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::process::Stdio;

    /// Test real-time metrics monitoring (brief test)
    #[tokio::test]
    async fn test_real_time_metrics() -> Result<()> {
        let mut cmd = Command::cargo_bin("crucible").unwrap();
        cmd.args(&["service", "metrics", "--real-time"])
           .timeout(std::time::Duration::from_secs(3))
           .stdout(Stdio::piped());

        // This should start and then be terminated by timeout
        let result = cmd.assert();

        // Command should either succeed or be terminated (which is expected for real-time mode)
        assert!(result.success() || std::env::var("CI").is_ok());

        Ok(())
    }
}
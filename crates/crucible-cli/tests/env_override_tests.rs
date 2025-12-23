//! Tests for environment variable overrides

#![allow(clippy::field_reassign_with_default)]
#![allow(deprecated)] // cargo_bin is deprecated but still functional

//!
//! These tests verify that environment variables properly override config file values
//! and CLI arguments. Tests are run serially to avoid environment pollution.

use assert_cmd::Command;
use predicates::prelude::*;
use serial_test::serial;
use std::env;
use std::fs;
use tempfile::TempDir;

/// Helper to clean up environment variables before and after tests
fn clean_env_vars() {
    env::remove_var("CRUCIBLE_KILN_PATH");
    env::remove_var("CRUCIBLE_EMBEDDING_PROVIDER");
    env::remove_var("CRUCIBLE_EMBEDDING_MODEL");
    env::remove_var("CRUCIBLE_EMBEDDING_URL");
    env::remove_var("CRUCIBLE_EMBEDDING_MAX_CONCURRENT");
}

#[test]
#[serial]
fn test_comprehensive_env_overrides() {
    clean_env_vars();

    // Note: Tests marked #[serial] ensure no race conditions

    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("config.toml");

    // Create a base config file
    fs::write(
        &config_path,
        r#"
kiln_path = "/file/kiln"
[embedding]
provider = "fastembed"
model = "file-model"
api_url = "https://file-url.com"
batch_size = 16
"#,
    )
    .unwrap();

    // Set up HOME to point to our config
    let config_dir = temp.path().join(".config");
    fs::create_dir_all(&config_dir).unwrap();
    let default_config_path = config_dir.join("crucible").join("config.toml");
    fs::create_dir_all(default_config_path.parent().unwrap()).unwrap();
    fs::copy(&config_path, &default_config_path).unwrap();

    // Set all environment variables at once
    env::set_var("CRUCIBLE_KILN_PATH", "/env/kiln");
    env::set_var("CRUCIBLE_EMBEDDING_PROVIDER", "OpenAI");
    env::set_var("CRUCIBLE_EMBEDDING_MODEL", "env-model");
    env::set_var("CRUCIBLE_EMBEDDING_URL", "https://env-url.com");
    env::set_var("CRUCIBLE_EMBEDDING_MAX_CONCURRENT", "64");

    // Test with CLI flags (highest priority)
    let mut cmd = Command::cargo_bin("cru").unwrap();
    // Use CRUCIBLE_CONFIG_DIR for isolation on Windows
    cmd.env("CRUCIBLE_CONFIG_DIR", config_dir.join("crucible"))
        .arg("config")
        .arg("show")
        .arg("--format")
        .arg("toml")
        .env("CRUCIBLE_KILN_PATH", temp.path()); // Override with CLI via env

    cmd.assert()
        .success()
        // Should use CLI/CLI env override for kiln_path
        .stdout(predicate::str::contains(
            temp.path().to_string_lossy().to_string(),
        ))
        // Should use environment overrides (note: provider is lowercase in output)
        .stdout(predicate::str::contains("provider = \"openai\""))
        .stdout(predicate::str::contains("model = \"env-model\""))
        .stdout(predicate::str::contains("https://env-url.com"))
        .stdout(predicate::str::contains("max_concurrent = 64"));

    // Test with sources flag to verify tracking works
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.env("CRUCIBLE_CONFIG_DIR", config_dir.join("crucible"))
        .arg("config")
        .arg("show")
        .arg("--sources");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("# from: environment (CRUCIBLE_"))
        .stdout(predicate::str::contains("OpenAI"))
        .stdout(predicate::str::contains("env-model"));
}

#[test]
#[serial]
fn test_config_show_with_mixed_sources() {
    clean_env_vars();

    // Note: Tests marked #[serial] ensure no race conditions

    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("config.toml");

    // Create a base config (profiles not implemented yet)
    fs::write(
        &config_path,
        r#"
kiln_path = "/base/kiln"
agent_directories = ["/base/agents"]

[embedding]
provider = "anthropic"
batch_size = 32

[acp]
default_agent = "base-agent"
session_timeout_minutes = 120
"#,
    )
    .unwrap();

    // Set up HOME
    let config_dir = temp.path().join(".config");
    fs::create_dir_all(&config_dir).unwrap();
    let default_config_path = config_dir.join("crucible").join("config.toml");
    fs::create_dir_all(default_config_path.parent().unwrap()).unwrap();
    fs::copy(&config_path, &default_config_path).unwrap();

    env::set_var("HOME", temp.path());
    // Override some values with env
    env::set_var("CRUCIBLE_EMBEDDING_MODEL", "env-override-model");
    env::set_var("CRUCIBLE_EMBEDDING_PROVIDER", "OpenAI");

    let mut cmd = Command::cargo_bin("cru").unwrap();
    // Use CRUCIBLE_CONFIG_DIR for isolation
    cmd.env("CRUCIBLE_CONFIG_DIR", config_dir.join("crucible"))
        .arg("config")
        .arg("show");

    cmd.assert()
        .success()
        // From file
        .stdout(predicate::str::contains("/base/kiln"))
        .stdout(predicate::str::contains("batch_size = 32"))
        // From environment (overrides file)
        .stdout(predicate::str::contains("provider = \"openai\""))
        .stdout(predicate::str::contains("model = \"env-override-model\""))
        // From file (not overridden)
        .stdout(predicate::str::contains("session_timeout_minutes = 120"));
}

#[test]
#[serial]
fn test_invalid_env_var_values() {
    clean_env_vars();

    // Note: Tests marked #[serial] ensure no race conditions

    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("config.toml");

    fs::write(
        &config_path,
        r#"
kiln_path = "/test/kiln"
[embedding]
provider = "fastembed"
"#,
    )
    .unwrap();

    let config_dir = temp.path().join(".config");
    fs::create_dir_all(&config_dir).unwrap();
    let default_config_path = config_dir.join("crucible").join("config.toml");
    fs::create_dir_all(default_config_path.parent().unwrap()).unwrap();
    fs::copy(&config_path, &default_config_path).unwrap();

    env::set_var("HOME", temp.path());

    // Test invalid provider (should fall back to default)
    env::set_var("CRUCIBLE_EMBEDDING_PROVIDER", "invalid-provider");

    let mut cmd = Command::cargo_bin("cru").unwrap();
    // Use CRUCIBLE_CONFIG_DIR for isolation
    cmd.env("CRUCIBLE_CONFIG_DIR", config_dir.join("crucible"))
        .arg("config")
        .arg("show");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("/test/kiln"))
        .stdout(predicate::str::contains("provider = \"fastembed\"")); // Should fallback to default
}

#[test]
#[serial]
fn test_json_format_with_sources() {
    clean_env_vars();

    // Note: Tests marked #[serial] ensure no race conditions
    let temp = TempDir::new().unwrap(); // Need temp dir for isolation
    let config_dir = temp.path().join(".config");
    fs::create_dir_all(&config_dir).unwrap();
    // We need a minimal config file or empty dir so it doesn't try global
    // Actually just pointing to empty dir is enough if we rely on env vars

    // Test multiple overrides in JSON format
    env::set_var("CRUCIBLE_KILN_PATH", "/json-test/kiln");
    env::set_var("CRUCIBLE_EMBEDDING_PROVIDER", "Ollama");
    env::set_var("CRUCIBLE_EMBEDDING_MODEL", "json-model");

    let mut cmd = Command::cargo_bin("cru").unwrap();
    // Use CRUCIBLE_CONFIG_DIR for isolation
    cmd.env("CRUCIBLE_CONFIG_DIR", config_dir.join("crucible"))
        .arg("config")
        .arg("show")
        .arg("--format")
        .arg("json")
        .arg("--sources");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(r#""kiln_path""#))
        .stdout(predicate::str::contains(r#""value""#))
        .stdout(predicate::str::contains(r#""source""#))
        .stdout(predicate::str::contains(r#""embedding""#))
        .stdout(predicate::str::contains(r#""cli""#));
}

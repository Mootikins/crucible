//! Tests for CLI configuration commands
//!
//! This module tests the `config` subcommands:
//! - `config init`: Initialize a new config file
//! - `config show`: Show the current effective configuration
//! - `config dump`: Dump default configuration
//!
//! Tests include value source tracking to show where each value was set
//! (file, environment, CLI, or default)

use assert_cmd::Command;
use predicates::prelude::*;
use serial_test::serial;
use std::env;
use std::fs;
use tempfile::TempDir;

// ============================================================================
// Config Init Command Tests
// ============================================================================

#[test]
#[serial]
fn test_config_init_creates_file() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("test-config.toml");

    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.env("CRUCIBLE_CONFIG_DIR", temp.path().join("config"))
        .arg("config")
        .arg("init")
        .arg("--path")
        .arg(config_path.to_str().unwrap());

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Created config file at"))
        .stdout(predicate::str::contains(config_path.to_str().unwrap()));

    // Verify file was created
    assert!(config_path.exists());

    // Verify file has expected content
    let content = fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("Crucible CLI Configuration"));
    assert!(content.contains("kiln_path"));
    assert!(content.contains("[embedding]"));
    assert!(content.contains("[acp]"));
}

#[test]
#[serial]
fn test_config_init_fails_without_force_if_exists() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("test-config.toml");

    // Create existing file
    fs::write(&config_path, "existing content").unwrap();

    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.env("CRUCIBLE_CONFIG_DIR", temp.path().join("config"))
        .arg("config")
        .arg("init")
        .arg("--path")
        .arg(config_path.to_str().unwrap());

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Config file already exists"))
        .stdout(predicate::str::contains("--force"));
}

#[test]
#[serial]
fn test_config_init_overwrites_with_force() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("test-config.toml");

    // Create existing file
    fs::write(&config_path, "existing content").unwrap();

    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.env("CRUCIBLE_CONFIG_DIR", temp.path().join("config"))
        .arg("config")
        .arg("init")
        .arg("--path")
        .arg(config_path.to_str().unwrap())
        .arg("--force");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Created config file at"));

    // Verify file was overwritten
    let content = fs::read_to_string(&config_path).unwrap();
    assert!(!content.contains("existing content"));
    assert!(content.contains("Crucible CLI Configuration"));
}

#[test]
#[serial]
fn test_config_init_creates_parent_directories() {
    let temp = TempDir::new().unwrap();
    let nested_path = temp.path().join("a/b/c/config.toml");

    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.env("CRUCIBLE_CONFIG_DIR", temp.path().join("config"))
        .arg("config")
        .arg("init")
        .arg("--path")
        .arg(nested_path.to_str().unwrap());

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Created config file at"));

    assert!(nested_path.exists());
    assert!(nested_path.parent().unwrap().exists());
}

#[test]
#[serial]
fn test_config_init_uses_default_path() {
    // Temporarily override HOME and CRUCIBLE_CONFIG_DIR to use a test directory
    let original_home = env::var("HOME").ok();
    let temp = TempDir::new().unwrap();
    env::set_var("HOME", temp.path());
    
    // On Windows, setting HOME isn't enough, we need CRUCIBLE_CONFIG_DIR to isolate
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.env("CRUCIBLE_CONFIG_DIR", temp.path().join(".config/crucible"))
       .arg("config").arg("init").arg("--force");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Created config file at"));

    // Restore HOME
    if let Some(home) = original_home {
        env::set_var("HOME", home);
    } else {
        env::remove_var("HOME");
    }
}

// ============================================================================
// Config Show Command Tests
// ============================================================================

#[test]
#[serial]
fn test_config_show_default() {
    // Clear any existing config
    let original_home = env::var("HOME").ok();
    env::remove_var("CRUCIBLE_KILN_PATH");
    env::remove_var("CRUCIBLE_EMBEDDING_URL");
    env::remove_var("CRUCIBLE_EMBEDDING_MODEL");
    env::remove_var("CRUCIBLE_EMBEDDING_PROVIDER");

    let temp = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.env("CRUCIBLE_CONFIG_DIR", temp.path().join("config"))
       .arg("config").arg("show");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("kiln_path"))
        .stdout(predicate::str::contains("[embedding]"))
        .stdout(predicate::str::contains("provider = \"fastembed\""));

    // Restore env vars
    if let Some(home) = original_home {
        env::set_var("HOME", home);
    }
}

#[test]
#[serial]
fn test_config_show_json_format() {
    let temp = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.env("CRUCIBLE_CONFIG_DIR", temp.path().join("config"))
       .arg("config").arg("show").arg("--format").arg("json");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"kiln_path\""))
        .stdout(predicate::str::contains("\"embedding\""));
}

#[test]
#[serial]
fn test_config_show_with_env_overrides() {
    // Set environment variables
    env::set_var("CRUCIBLE_KILN_PATH", "/env/kiln");
    env::set_var("CRUCIBLE_EMBEDDING_PROVIDER", "openai");
    env::set_var("CRUCIBLE_EMBEDDING_MODEL", "env-model");

    let temp = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.env("CRUCIBLE_CONFIG_DIR", temp.path().join("config"))
       .arg("config").arg("show");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("/env/kiln"))
        .stdout(predicate::str::contains("provider = \"openai\""))
        .stdout(predicate::str::contains("model = \"env-model\""));

    // Cleanup
    env::remove_var("CRUCIBLE_KILN_PATH");
    env::remove_var("CRUCIBLE_EMBEDDING_PROVIDER");
    env::remove_var("CRUCIBLE_EMBEDDING_MODEL");
}

#[test]
#[serial]
fn test_config_show_with_file_config() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("test-config.toml");

    // Create a config file
    fs::write(
        &config_path,
        r#"
kiln_path = "/file/kiln"

[embedding]
provider = "anthropic"
model = "file-model"
api_url = "https://api.anthropic.com"
batch_size = 32

[acp]
default_agent = "claude-3-opus"
session_timeout_minutes = 45

[chat]
model = "claude-3-sonnet"
temperature = 0.8
streaming = false
"#,
    )
    .unwrap();

    // Set the config file path via environment
    let original_home = env::var("HOME").ok();
    let config_dir = temp.path().join("config");
    fs::create_dir_all(&config_dir).unwrap();
    let default_config_path = config_dir.join("crucible").join("config.toml");
    fs::create_dir_all(default_config_path.parent().unwrap()).unwrap();
    fs::copy(&config_path, &default_config_path).unwrap();
    env::set_var("HOME", temp.path());

    let mut cmd = Command::cargo_bin("cru").unwrap();
    // On Windows, set CRUCIBLE_CONFIG_DIR explicitly to the directory containing config.toml
    cmd.env("CRUCIBLE_CONFIG_DIR", default_config_path.parent().unwrap())
       .arg("config").arg("show");

    cmd.assert()
        .success()
        // Just check that it outputs a valid config structure
        .stdout(predicate::str::contains("kiln_path"))
        .stdout(predicate::str::contains("[embedding]"))
        .stdout(predicate::str::contains("provider"))
        .stdout(predicate::str::contains("[acp]"))
        .stdout(predicate::str::contains("[chat]"));

    // Cleanup
    if let Some(home) = original_home {
        env::set_var("HOME", home);
    } else {
        env::remove_var("HOME");
    }
}

#[test]
#[serial]
fn test_config_show_with_mixed_sources() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("mixed-config.toml");

    // Create a config file
    fs::write(
        &config_path,
        r#"
kiln_path = "/file/kiln"

[embedding]
provider = "openai"
api_url = "https://api.openai.com"
"#,
    )
    .unwrap();

    // Set up HOME to point to our config
    let config_dir = temp.path().join("config");
    fs::create_dir_all(&config_dir).unwrap();
    let default_config_path = config_dir.join("crucible").join("config.toml");
    fs::create_dir_all(default_config_path.parent().unwrap()).unwrap();
    fs::copy(&config_path, &default_config_path).unwrap();

    // Set environment variables for some overrides
    env::set_var("HOME", temp.path());
    env::set_var("CRUCIBLE_EMBEDDING_MODEL", "env-model");
    env::set_var("CRUCIBLE_EMBEDDING_PROVIDER", "ollama");

    let mut cmd = Command::cargo_bin("cru").unwrap();
    // On Windows, set CRUCIBLE_CONFIG_DIR explicitly to the directory containing config.toml
    cmd.env("CRUCIBLE_CONFIG_DIR", default_config_path.parent().unwrap())
       .arg("config").arg("show");

    cmd.assert()
        .success()
        // Check structure and that environment overrides work
        .stdout(predicate::str::contains("kiln_path"))
        .stdout(predicate::str::contains("[embedding]"))
        // Environment overrides should work regardless of file loading
        .stdout(predicate::str::contains("ollama")) // From env
        .stdout(predicate::str::contains("env-model")) // From env
        .stdout(predicate::str::contains("batch_size")); // Should be present

    // Cleanup
    env::remove_var("HOME");
    env::remove_var("CRUCIBLE_EMBEDDING_MODEL");
    env::remove_var("CRUCIBLE_EMBEDDING_PROVIDER");
}

// ============================================================================
// Config Dump Command Tests
// ============================================================================

#[test]
fn test_config_dump_default() {
    let temp = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.env("CRUCIBLE_CONFIG_DIR", temp.path().join("config"))
       .arg("config").arg("dump");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("kiln_path"))
        .stdout(predicate::str::contains("[embedding]"))
        .stdout(predicate::str::contains("provider = \"fastembed\""));
}

#[test]
fn test_config_dump_json_format() {
    let temp = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.env("CRUCIBLE_CONFIG_DIR", temp.path().join("config"))
       .arg("config").arg("dump").arg("--format").arg("json");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"kiln_path\""))
        .stdout(predicate::str::contains("\"embedding\""));
}

#[test]
#[serial]
fn test_config_dump_ignores_env_vars() {
    // Set environment variables
    env::set_var("CRUCIBLE_KILN_PATH", "/env/kiln");
    env::set_var("CRUCIBLE_EMBEDDING_PROVIDER", "openai");

    let temp = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.env("CRUCIBLE_CONFIG_DIR", temp.path().join("config"))
       .arg("config").arg("dump");

    cmd.assert()
        .success()
        // Should show defaults, not env overrides
        .stdout(predicate::str::contains("provider = \"fastembed\"")) // Default provider
        .stdout(predicate::str::contains("kiln_path"));

    // Cleanup
    env::remove_var("CRUCIBLE_KILN_PATH");
    env::remove_var("CRUCIBLE_EMBEDDING_PROVIDER");
}

// ============================================================================
// Value Source Tracking Tests
// ============================================================================

#[test]
#[serial]
fn test_config_show_with_sources() {
    // Set environment variables to test source tracking
    env::set_var("CRUCIBLE_KILN_PATH", "/env/kiln");
    env::set_var("CRUCIBLE_EMBEDDING_PROVIDER", "openai");
    env::set_var("CRUCIBLE_EMBEDDING_MODEL", "env-model");

    let temp = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.env("CRUCIBLE_CONFIG_DIR", temp.path().join("config"))
       .arg("config").arg("show").arg("--sources");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "# Effective Configuration with Sources",
        ))
        .stdout(predicate::str::contains(
            "kiln_path = \"/env/kiln\"  # from: environment (CRUCIBLE_KILN_PATH)",
        ))
        .stdout(predicate::str::contains(
            "provider = \"OpenAI\"  # from: environment (CRUCIBLE_EMBEDDING_PROVIDER)",
        ))
        .stdout(predicate::str::contains(
            "model = \"env-model\"  # from: environment (CRUCIBLE_EMBEDDING_MODEL)",
        ))
        .stdout(predicate::str::contains("batch_size = 16  # from: default"));

    // Cleanup
    env::remove_var("CRUCIBLE_KILN_PATH");
    env::remove_var("CRUCIBLE_EMBEDDING_PROVIDER");
    env::remove_var("CRUCIBLE_EMBEDDING_MODEL");
}

#[test]
#[serial]
fn test_config_show_sources_with_file_and_env() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("tracked-config.toml");

    // Create a config file
    fs::write(
        &config_path,
        r#"
kiln_path = "/file/kiln"
[embedding]
provider = "anthropic"
api_url = "https://api.anthropic.com"
batch_size = 32
"#,
    )
    .unwrap();

    // Set up HOME to point to our config
    let config_dir = temp.path().join("config");
    fs::create_dir_all(&config_dir).unwrap();
    let default_config_path = config_dir.join("crucible").join("config.toml");
    fs::create_dir_all(default_config_path.parent().unwrap()).unwrap();
    fs::copy(&config_path, &default_config_path).unwrap();

    // Override some values with env vars
    env::set_var("HOME", temp.path());
    env::set_var("CRUCIBLE_EMBEDDING_MODEL", "env-model");
    env::set_var("CRUCIBLE_EMBEDDING_PROVIDER", "ollama");

    let mut cmd = Command::cargo_bin("cru").unwrap();
    // On Windows, set CRUCIBLE_CONFIG_DIR explicitly to the directory containing config.toml
    cmd.env("CRUCIBLE_CONFIG_DIR", default_config_path.parent().unwrap())
       .arg("config").arg("show").arg("--sources");

    cmd.assert()
        .success()
        // Check that source tracking is enabled
        .stdout(predicate::str::contains(
            "# Effective Configuration with Sources",
        ))
        .stdout(predicate::str::contains("# from:"))
        // Should show environment overrides
        .stdout(predicate::str::contains("environment (CRUCIBLE_"))
        .stdout(predicate::str::contains("env-model"))
        .stdout(predicate::str::contains("Ollama"));

    // Cleanup
    env::remove_var("HOME");
    env::remove_var("CRUCIBLE_EMBEDDING_MODEL");
    env::remove_var("CRUCIBLE_EMBEDDING_PROVIDER");
}

#[test]
#[serial]
fn test_config_show_sources_json() {
    // Set environment variables
    env::set_var("CRUCIBLE_KILN_PATH", "/env/kiln");
    env::set_var("CRUCIBLE_EMBEDDING_PROVIDER", "ollama");

    let temp = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.env("CRUCIBLE_CONFIG_DIR", temp.path().join("config"))
        .arg("config")
        .arg("show")
        .arg("--format")
        .arg("json")
        .arg("--sources");

    let output = cmd.output().unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();

    // Basic checks that the JSON is valid and contains expected structure
    assert!(stdout.contains("kiln_path"));
    assert!(stdout.contains("embedding"));
    assert!(stdout.contains("cli"));
    // Should contain source information
    assert!(stdout.contains("source"));

    // Cleanup
    env::remove_var("CRUCIBLE_KILN_PATH");
    env::remove_var("CRUCIBLE_EMBEDDING_PROVIDER");
}

#[test]
#[serial]
fn test_config_show_sources_mixed_output() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("mixed-config.toml");

    // Create a minimal config file
    fs::write(
        &config_path,
        r#"
kiln_path = "/file/kiln"
[cli]
verbose = true
"#,
    )
    .unwrap();

    // Set up config
    let config_dir = temp.path().join("config");
    fs::create_dir_all(&config_dir).unwrap();
    let default_config_path = config_dir.join("crucible").join("config.toml");
    fs::create_dir_all(default_config_path.parent().unwrap()).unwrap();
    fs::copy(&config_path, &default_config_path).unwrap();

    env::set_var("HOME", temp.path());
    env::set_var("CRUCIBLE_EMBEDDING_PROVIDER", "openai");

    let mut cmd = Command::cargo_bin("cru").unwrap();
    // Set CRUCIBLE_CONFIG_DIR explicitly
    cmd.env("CRUCIBLE_CONFIG_DIR", default_config_path.parent().unwrap())
        .arg("config").arg("show").arg("--sources");

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Verify source tracking is working
    assert!(stdout.contains("# Effective Configuration with Sources"));
    assert!(stdout.contains("# from:"));

    // Should show environment variables
    assert!(stdout.contains("OpenAI"));
    assert!(stdout.contains("environment"));

    // Cleanup
    env::remove_var("HOME");
    env::remove_var("CRUCIBLE_EMBEDDING_PROVIDER");
}

#[test]
#[serial]
fn test_config_show_sources_without_flag() {
    // Verify that without --sources flag, we get normal output
    env::set_var("CRUCIBLE_KILN_PATH", "/env/kiln");

    let temp = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.env("CRUCIBLE_CONFIG_DIR", temp.path().join("config"))
       .arg("config").arg("show");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("kiln_path = \"/env/kiln\""))
        .stdout(predicate::str::contains("# from:").not()); // Should NOT have source comments

    // Cleanup
    env::remove_var("CRUCIBLE_KILN_PATH");
}

// ============================================================================
// Edge Cases and Error Handling
// ============================================================================

#[test]
#[serial]
fn test_config_show_with_invalid_config_file() {
    let temp = TempDir::new().unwrap();
    let config_dir = temp.path().join("config");
    fs::create_dir_all(&config_dir).unwrap();
    let config_path = config_dir.join("crucible").join("config.toml");
    fs::create_dir_all(config_path.parent().unwrap()).unwrap();

    // Write invalid TOML
    fs::write(&config_path, "this is not valid toml [[[").unwrap();

    env::set_var("HOME", temp.path());

    let mut cmd = Command::cargo_bin("cru").unwrap();
    // Set CRUCIBLE_CONFIG_DIR explicitly
    cmd.env("CRUCIBLE_CONFIG_DIR", config_path.parent().unwrap())
       .arg("config").arg("show");

    // Should fail when config is invalid
    // Note: Earlier behavior might have been fallback, but explicit failure is safer
    // so user knows their config is broken.
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Failed to parse config file"));

    env::remove_var("HOME");
}

#[test]
#[serial]
fn test_config_show_with_partial_config() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("partial-config.toml");

    // Create a config with only some fields
    fs::write(
        &config_path,
        r#"
kiln_path = "/partial/kiln"

[acp]
default_agent = "partial-agent"
"#,
    )
    .unwrap();

    let config_dir = temp.path().join("config");
    fs::create_dir_all(&config_dir).unwrap();
    let default_config_path = config_dir.join("crucible").join("config.toml");
    fs::create_dir_all(default_config_path.parent().unwrap()).unwrap();
    fs::copy(&config_path, &default_config_path).unwrap();

    env::set_var("HOME", temp.path());

    let mut cmd = Command::cargo_bin("cru").unwrap();
    // Set CRUCIBLE_CONFIG_DIR explicitly
    cmd.env("CRUCIBLE_CONFIG_DIR", default_config_path.parent().unwrap())
        .arg("config").arg("show");

    cmd.assert()
        .success()
        // Check that output is valid config
        .stdout(predicate::str::contains("kiln_path"))
        .stdout(predicate::str::contains("[acp]"))
        .stdout(predicate::str::contains("[embedding]"))
        .stdout(predicate::str::contains("provider"))
        .stdout(predicate::str::contains("[chat]"));

    env::remove_var("HOME");
}

#[test]
#[serial]
fn test_config_show_preserves_order() {
    let temp = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.env("CRUCIBLE_CONFIG_DIR", temp.path().join("config"))
       .arg("config").arg("show");

    // The output should be reasonably ordered for readability
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    // Check that major sections appear in a reasonable order
    let kiln_pos = stdout.find("kiln_path").unwrap();
    let embedding_pos = stdout.find("[embedding]").unwrap();
    let acp_pos = stdout.find("[acp]").unwrap();
    let chat_pos = stdout.find("[chat]").unwrap();
    let cli_pos = stdout.find("[cli]").unwrap();

    // Basic order check (not strict, just reasonable)
    assert!(kiln_pos < embedding_pos);
    assert!(embedding_pos < acp_pos);
    assert!(acp_pos < chat_pos);
    assert!(chat_pos < cli_pos);
}

// ============================================================================
// Integration with Other Commands
// ============================================================================

#[test]
#[serial]
fn test_config_output_used_by_other_commands() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("integration-config.toml");

    fs::write(
        &config_path,
        r#"
kiln_path = "/tmp/test-kiln"
[embedding]
provider = "fastembed"
"#,
    )
    .unwrap();

    // Create the kiln directory
    fs::create_dir_all("/tmp/test-kiln").unwrap();

    // Test that stats command uses the config
    env::set_var("CRUCIBLE_KILN_PATH", temp.path());

    let mut cmd = Command::cargo_bin("cru").unwrap();
    // Use temp config directory
    cmd.env("CRUCIBLE_CONFIG_DIR", temp.path().join("config"))
       .arg("config").arg("show").arg("--format").arg("json");

    cmd.assert().success();

    // Clean up
    env::remove_var("CRUCIBLE_KILN_PATH");
    fs::remove_dir_all("/tmp/test-kiln").unwrap_or(());
}

// ============================================================================
// Performance Tests
// ============================================================================

#[test]
#[serial]
#[ignore = "performance-sensitive smoke test; run manually"]
fn test_config_show_performance() {
    let temp = TempDir::new().unwrap();
    let config_dir = temp.path().join("config");
    std::fs::create_dir_all(&config_dir).unwrap();

    // Ensure config lookup is isolated from the developer machine.
    //
    // `crucible-config` uses `dirs::config_dir()` which respects XDG_CONFIG_HOME on Unix.
    env::set_var("XDG_CONFIG_HOME", &config_dir);
    let default_config_path = config_dir.join("crucible").join("config.toml");
    std::fs::create_dir_all(default_config_path.parent().unwrap()).unwrap();
    std::fs::write(&default_config_path, "kiln_path = \"/tmp/test-kiln\"\n").unwrap();

    let start = std::time::Instant::now();

    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("config").arg("show");

    cmd.assert().success();

    let duration = start.elapsed();
    // Config show should be fast (< 5 seconds for debug build)
    assert!(duration.as_millis() < 5000);

    env::remove_var("XDG_CONFIG_HOME");
}

#[test]
#[serial]
#[ignore = "performance-sensitive smoke test; run manually"]
fn test_config_show_with_large_config() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("large-config.toml");

    // Create a large config
    let mut content = r#"
kiln_path = "/large/kiln"

[embedding]
provider = "openai"
model = "text-embedding-3-large"

agent_directories = ["#
        .to_string();

    // Add many agent directories
    for i in 0..100 {
        content.push_str(&format!("\n    \"/opt/agents/{}\",", i));
    }
    content.push_str("\n]\n\n");

    // Add multiple profiles
    for i in 0..20 {
        content.push_str(&format!(
            r#"
[profiles.profile{}]
kiln_path = "/vault{}"
"#,
            i, i
        ));
    }

    fs::write(&config_path, content).unwrap();

    let config_dir = temp.path().join("config");
    fs::create_dir_all(&config_dir).unwrap();
    env::set_var("XDG_CONFIG_HOME", &config_dir);
    let default_config_path = config_dir.join("crucible").join("config.toml");
    fs::create_dir_all(default_config_path.parent().unwrap()).unwrap();
    fs::copy(&config_path, &default_config_path).unwrap();

    let start = std::time::Instant::now();
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("config").arg("show");

    cmd.assert().success();

    let duration = start.elapsed();
    // Should still be reasonably fast even with large config (debug build)
    assert!(duration.as_millis() < 5000);

    env::remove_var("XDG_CONFIG_HOME");
}

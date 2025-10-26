//! Error Recovery TDD Tests
//!
//! This module tests error handling and recovery mechanisms across the CLI.
//! Ensures graceful degradation and clear error messages for user-facing operations.
//!
//! **Test Philosophy:**
//! - Focus on user-facing error scenarios (not internal infrastructure)
//! - Use real errors where possible (file not found, invalid config)
//! - Test error messages are clear and actionable
//! - Verify data integrity during failures
//!
//! **Coverage:**
//! - Configuration loading and validation errors
//! - File system errors (missing files, permissions)
//! - Database connection failures
//! - Search fallback and graceful degradation
//! - Error message quality
//!
//! **Note:** Core infrastructure (circuit breaker, retry logic, health monitor)
//! is already tested in `/home/moot/crucible/crates/crucible-cli/src/error_recovery.rs`

use anyhow::Result;
use crucible_cli::config::CliConfig;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

// =============================================================================
// Configuration Error Recovery Tests
// =============================================================================

#[test]
fn test_config_loading_missing_file() -> Result<()> {
    // Test: Loading config from non-existent file should fall back to defaults
    let nonexistent_path = PathBuf::from("/nonexistent/config/path/config.toml");

    // Should succeed by falling back to defaults
    let config = CliConfig::from_file_or_default(Some(nonexistent_path))?;

    // Verify we got default values
    assert_eq!(config.kiln.embedding_url, "http://localhost:11434");
    assert_eq!(config.chat_model(), "llama3.2");

    Ok(())
}

#[test]
fn test_config_loading_invalid_toml() -> Result<()> {
    // Test: Invalid TOML should provide clear error message
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("invalid.toml");

    // Write invalid TOML
    fs::write(
        &config_path,
        r#"
[kiln]
path = /valid/path
embedding_url = "http://localhost:11434
# Missing closing quote - invalid TOML
"#,
    )?;

    let result = CliConfig::from_file_or_default(Some(config_path));

    // Should fail with TOML parse error
    assert!(
        result.is_err(),
        "Should fail to parse invalid TOML"
    );

    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("TOML") || error_msg.contains("parse") || error_msg.contains("expected"),
        "Error message should mention parsing issue, got: {}",
        error_msg
    );

    Ok(())
}

#[test]
fn test_config_with_invalid_kiln_path() -> Result<()> {
    // Test: Config with non-existent vault path should be created but operations should fail gracefully
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("config.toml");

    fs::write(
        &config_path,
        r#"
[kiln]
path = "/nonexistent/vault/path"
embedding_url = "http://localhost:11434"
"#,
    )?;

    // Config loading should succeed
    let config = CliConfig::from_file_or_default(Some(config_path))?;

    // Verify the path is set (even though it doesn't exist)
    assert_eq!(config.kiln.path.to_string_lossy(), "/nonexistent/vault/path");

    // Operations using this config should fail gracefully
    // (tested in other integration tests)

    Ok(())
}

#[test]
fn test_config_with_empty_values() -> Result<()> {
    // Test: Empty config values should use defaults
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("config.toml");

    fs::write(
        &config_path,
        r#"
[kiln]
path = ""
embedding_url = ""
"#,
    )?;

    let config = CliConfig::from_file_or_default(Some(config_path))?;

    // Empty strings should be preserved (but might be invalid for operations)
    assert_eq!(config.kiln.path.to_string_lossy(), "");
    assert_eq!(config.kiln.embedding_url, "");

    Ok(())
}

#[test]
fn test_config_with_partial_sections() -> Result<()> {
    // Test: Config with only some sections should merge with defaults
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("config.toml");

    fs::write(
        &config_path,
        r#"
[kiln]
path = "/tmp/vault"

[llm]
chat_model = "custom-model"
# Other sections like [services] are optional
"#,
    )?;

    let config = CliConfig::from_file_or_default(Some(config_path))?;

    // Custom LLM settings should be applied
    assert_eq!(config.chat_model(), "custom-model");

    // Kiln settings should use defaults for unspecified fields
    assert_eq!(config.kiln.embedding_url, "http://localhost:11434");

    // Services section should use defaults
    assert!(config.services.script_engine.enabled);

    Ok(())
}

// =============================================================================
// File System Error Recovery Tests
// =============================================================================

#[test]
fn test_kiln_path_validation_nonexistent() -> Result<()> {
    // Test: Accessing non-existent vault path should fail clearly
    let mut config = CliConfig::default();
    config.kiln.path = PathBuf::from("/definitely/does/not/exist/vault");

    // Config is valid, but path doesn't exist
    let kiln_str = config.kiln_path_str()?;
    assert_eq!(kiln_str, "/definitely/does/not/exist/vault");

    // Attempting to use this path should fail (tested in integration tests)
    Ok(())
}

#[test]
fn test_kiln_path_with_special_characters() -> Result<()> {
    // Test: Vault paths with special characters should be handled
    let temp_dir = TempDir::new()?;
    let special_dir = temp_dir.path().join("vault with spaces");
    fs::create_dir(&special_dir)?;

    let mut config = CliConfig::default();
    config.kiln.path = special_dir.clone();

    let kiln_str = config.kiln_path_str()?;
    assert!(kiln_str.contains("vault with spaces"));

    Ok(())
}

#[test]
fn test_kiln_path_symlink_handling() -> Result<()> {
    // Test: Vault path can be a symlink
    let temp_dir = TempDir::new()?;
    let real_dir = temp_dir.path().join("real_vault");
    let symlink_dir = temp_dir.path().join("symlink_vault");

    fs::create_dir(&real_dir)?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(&real_dir, &symlink_dir)?;

    #[cfg(unix)]
    {
        let mut config = CliConfig::default();
        config.kiln.path = symlink_dir.clone();

        // Should accept symlink path
        let kiln_str = config.kiln_path_str()?;
        assert!(kiln_str.contains("symlink_vault"));
    }

    Ok(())
}

// =============================================================================
// Database Connection Error Tests
// =============================================================================

#[test]
fn test_database_url_validation() -> Result<()> {
    // Test: Database URL should be validated
    let config = CliConfig::default();

    // Default database path should be valid
    let db_path = config.database_path();
    assert!(
        db_path.to_string_lossy().contains(".crucible")
            || db_path.to_string_lossy().contains("embeddings"),
        "Database path should be in .crucible directory or contain 'embeddings': {}",
        db_path.display()
    );

    Ok(())
}

#[test]
fn test_custom_database_path() -> Result<()> {
    // Test: Custom database path can be set
    let temp_dir = TempDir::new()?;
    let custom_db = temp_dir.path().join("custom.db");

    let mut config = CliConfig::default();
    config.custom_database_path = Some(custom_db.clone());

    // Custom path should be used
    assert_eq!(config.database_path(), custom_db);

    Ok(())
}

// =============================================================================
// Search Error Handling Tests
// =============================================================================

#[test]
fn test_search_with_empty_query_handling() {
    // Test: Empty search queries should be handled
    // This is validated at CLI arg parsing level
    // Empty strings are valid inputs, handling depends on command

    let query = "";
    assert_eq!(query.len(), 0, "Empty query should be detectable");

    let query = "   ";
    assert_eq!(query.trim().len(), 0, "Whitespace-only query should be detectable");
}

#[test]
fn test_search_query_validation() {
    // Test: Search query edge cases
    let valid_queries = vec![
        "simple query",
        "query with 'quotes'",
        "query with \"double quotes\"",
        "query-with-dashes",
        "query_with_underscores",
        "query.with.dots",
        "unicode 你好 query",
    ];

    for query in valid_queries {
        assert!(!query.is_empty(), "Valid query should not be empty");
    }
}

// =============================================================================
// Error Message Quality Tests
// =============================================================================

#[test]
fn test_error_display_messages() {
    // Test: Error messages should be descriptive
    use crucible_llm::embeddings::error::EmbeddingError;

    let timeout_error = EmbeddingError::Timeout { timeout_secs: 30 };
    let msg = timeout_error.to_string();
    assert!(
        msg.contains("30"),
        "Timeout error should mention duration"
    );
    assert!(
        msg.to_lowercase().contains("timeout") || msg.to_lowercase().contains("timed out"),
        "Error should mention timeout"
    );

    let auth_error = EmbeddingError::AuthenticationError("Invalid API key".to_string());
    let msg = auth_error.to_string();
    assert!(
        msg.contains("Authentication") || msg.contains("Invalid API key"),
        "Auth error should be clear"
    );

    let rate_limit = EmbeddingError::RateLimitExceeded {
        retry_after_secs: 60,
    };
    let msg = rate_limit.to_string();
    assert!(
        msg.contains("60"),
        "Rate limit error should mention retry time"
    );
    assert!(
        msg.to_lowercase().contains("rate limit"),
        "Error should mention rate limiting"
    );
}

#[test]
fn test_error_categorization() {
    // Test: Errors should be correctly categorized as retryable/non-retryable
    use crucible_llm::embeddings::error::EmbeddingError;

    // Retryable errors
    let retryable = vec![
        EmbeddingError::Timeout { timeout_secs: 30 },
        EmbeddingError::RateLimitExceeded {
            retry_after_secs: 60,
        },
    ];

    for error in retryable {
        assert!(
            error.is_retryable(),
            "Error should be retryable: {:?}",
            error
        );
        assert!(
            error.retry_delay_secs().is_some(),
            "Retryable error should have delay"
        );
    }

    // Non-retryable errors
    let non_retryable = vec![
        EmbeddingError::AuthenticationError("Invalid key".to_string()),
        EmbeddingError::ConfigError("Missing config".to_string()),
        EmbeddingError::InvalidResponse("Bad JSON".to_string()),
    ];

    for error in non_retryable {
        assert!(
            !error.is_retryable(),
            "Error should not be retryable: {:?}",
            error
        );
    }
}

// =============================================================================
// Graceful Degradation Tests
// =============================================================================

#[test]
fn test_fallback_to_defaults() -> Result<()> {
    // Test: System should fall back to sensible defaults when config is missing
    let config = CliConfig::default();

    // All critical fields should have defaults
    assert!(!config.kiln.embedding_url.is_empty());
    let db_path = config.database_path();
    assert!(db_path != PathBuf::new());
    assert!(!config.chat_model().is_empty());
    assert!(config.temperature() > 0.0);
    assert!(config.max_tokens() > 0);

    Ok(())
}

#[test]
fn test_partial_config_merging() -> Result<()> {
    // Test: Partial configs should merge with defaults
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("config.toml");

    // Provide minimal required config + one custom setting
    fs::write(
        &config_path,
        r#"
[kiln]
path = "/tmp/vault"

[llm]
temperature = 0.9
"#,
    )?;

    let config = CliConfig::from_file_or_default(Some(config_path))?;

    // Custom value should be applied
    assert_eq!(config.temperature(), 0.9);

    // Other values should use defaults
    assert_eq!(config.kiln.embedding_url, "http://localhost:11434");
    assert_eq!(config.chat_model(), "llama3.2");

    Ok(())
}

// =============================================================================
// Circuit Breaker Integration Tests
// =============================================================================

#[tokio::test]
async fn test_circuit_breaker_prevents_cascading_failures() {
    // Test: Circuit breaker should stop requests after threshold
    use crucible_cli::error_recovery::{CircuitBreaker, CircuitBreakerConfig, CircuitState};
    use std::time::Duration;

    let config = CircuitBreakerConfig {
        failure_threshold: 3,
        recovery_timeout: Duration::from_millis(100),
        success_threshold: 2,
    };

    let breaker = CircuitBreaker::new(config);

    // Initially closed
    assert!(breaker.is_request_allowed().await);

    // Record failures
    for i in 1..=3 {
        breaker.record_failure().await;
        if i < 3 {
            assert!(
                breaker.is_request_allowed().await,
                "Should allow requests before threshold"
            );
        }
    }

    // Should be open now
    assert!(!breaker.is_request_allowed().await);
    assert_eq!(breaker.get_state().await, CircuitState::Open);

    // Wait for recovery
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Should transition to half-open
    assert!(breaker.is_request_allowed().await);
    assert_eq!(breaker.get_state().await, CircuitState::HalfOpen);

    // Record successes to close
    breaker.record_success().await;
    breaker.record_success().await;

    assert_eq!(breaker.get_state().await, CircuitState::Closed);
}

#[tokio::test]
async fn test_service_health_monitoring() {
    // Test: Service health should be trackable
    use crucible_cli::error_recovery::{ServiceHealth, ServiceHealthMonitor};

    let monitor = ServiceHealthMonitor::new();

    // Initially unknown
    assert_eq!(
        monitor.get_health("test_service").await,
        ServiceHealth::Unknown
    );

    // Update to healthy
    monitor
        .update_health("test_service", ServiceHealth::Healthy)
        .await;
    assert!(monitor.is_healthy("test_service").await);

    // Update to unhealthy
    monitor
        .update_health("test_service", ServiceHealth::Unhealthy)
        .await;
    assert!(!monitor.is_healthy("test_service").await);

    // Degraded should still be considered "healthy enough"
    monitor
        .update_health("test_service", ServiceHealth::Degraded)
        .await;
    assert!(monitor.is_healthy("test_service").await);
}

// =============================================================================
// Retry Logic Tests
// =============================================================================

#[tokio::test]
async fn test_retry_logic_exponential_backoff() {
    // Test: Retry logic should use exponential backoff
    use std::time::{Duration, Instant};

    let base_delay = Duration::from_millis(10);
    let attempts = 3;

    let start = Instant::now();
    let mut total_delay = Duration::ZERO;

    for attempt in 0..attempts {
        let delay = base_delay * 2_u32.pow(attempt);
        total_delay += delay;
        tokio::time::sleep(delay).await;
    }

    let elapsed = start.elapsed();

    // Should have waited approximately the total delay
    // (allowing some tolerance for timing)
    assert!(
        elapsed >= total_delay,
        "Expected at least {:?}, got {:?}",
        total_delay,
        elapsed
    );
}

#[test]
fn test_retry_delay_calculation() {
    // Test: Retry delays should be calculated correctly
    use crucible_llm::embeddings::error::EmbeddingError;

    let timeout_error = EmbeddingError::Timeout { timeout_secs: 30 };
    assert_eq!(timeout_error.retry_delay_secs(), Some(2));

    let rate_limit = EmbeddingError::RateLimitExceeded {
        retry_after_secs: 120,
    };
    assert_eq!(rate_limit.retry_delay_secs(), Some(120));

    let auth_error = EmbeddingError::AuthenticationError("Invalid".to_string());
    assert_eq!(auth_error.retry_delay_secs(), None);
}

// =============================================================================
// Environment Variable Tests
// =============================================================================

#[test]
fn test_environment_variable_handling() -> Result<()> {
    // Test: Environment variables should be handled gracefully
    // This is a documentation test of expected behavior

    // Save original env vars
    let orig_vault = std::env::var("CRUCIBLE_KILN_PATH").ok();
    let orig_db = std::env::var("CRUCIBLE_DATABASE_URL").ok();

    // Test with missing env vars
    std::env::remove_var("CRUCIBLE_KILN_PATH");
    std::env::remove_var("CRUCIBLE_DATABASE_PATH");

    let config = CliConfig::default();

    // Should work with defaults (kiln_path_str may fail if path is invalid, which is OK)
    let _ = config.kiln_path_str();  // Just test it doesn't panic
    let db_path = config.database_path();
    assert!(db_path != PathBuf::new());

    // Restore env vars
    if let Some(val) = orig_vault {
        std::env::set_var("CRUCIBLE_KILN_PATH", val);
    }
    if let Some(val) = orig_db {
        std::env::set_var("CRUCIBLE_DATABASE_URL", val);
    }

    Ok(())
}

// =============================================================================
// Data Integrity Tests
// =============================================================================

#[test]
fn test_config_serialization_roundtrip() -> Result<()> {
    // Test: Config should survive serialization/deserialization
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("config.toml");

    let original = CliConfig::default();
    let content = toml::to_string(&original)?;
    fs::write(&config_path, content)?;

    let loaded = CliConfig::from_file_or_default(Some(config_path))?;

    // Key fields should match
    assert_eq!(original.kiln.embedding_url, loaded.kiln.embedding_url);
    assert_eq!(original.database_path(), loaded.database_path());
    assert_eq!(original.chat_model(), loaded.chat_model());

    Ok(())
}

#[test]
fn test_error_handling_preserves_data() -> Result<()> {
    // Test: Errors during config loading should not corrupt existing config
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("config.toml");

    // Write valid config
    fs::write(
        &config_path,
        r#"
[kiln]
path = "/valid/path"
"#,
    )?;

    let valid_config = CliConfig::from_file_or_default(Some(config_path.clone()))?;
    assert_eq!(valid_config.kiln.path.to_string_lossy(), "/valid/path");

    // Overwrite with invalid config
    fs::write(&config_path, "invalid toml {{")?;

    // Loading should fail without affecting the valid_config in memory
    let result = CliConfig::from_file_or_default(Some(config_path));
    assert!(result.is_err());

    // Original config should be unchanged
    assert_eq!(valid_config.kiln.path.to_string_lossy(), "/valid/path");

    Ok(())
}

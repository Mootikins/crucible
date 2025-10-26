//! TDD RED Phase: Vault Processing Integration Tests
//!
//! This test suite demonstrates the configuration integration gaps in vault processing.
//! These tests are designed to FAIL initially (RED phase) to drive proper implementation
//! of CLI configuration flow to vault processing.
//!
//! **Key Issues Demonstrated:**
//! 1. Current vault processing uses `EmbeddingConfig::default()` instead of CLI configuration
//! 2. CLI embedding configuration (embedding_url, embedding_model) is ignored during processing
//! 3. Embedding generation may use mock providers instead of real configured providers
//! 4. Configuration flow from CLI arguments to vault processing is broken
//!
//! **Expected Test Results (RED Phase):**
//! - All tests should FAIL initially
//! - Test failures should clearly demonstrate the configuration integration gaps
//! - Test failures should provide specification for implementing proper configuration flow

// Import CLI and vault processing components
use crate::test_utilities::{
    AssertUtils, MemoryUsage, PerformanceMeasurement, TestContext, TestDataGenerator,
};
use anyhow::Result;
use crucible_cli::config::CliConfig;
use crucible_surrealdb::{
    embedding_pool::EmbeddingThreadPool,
    vault_integration::{self, get_database_stats},
    vault_processor::process_vault_files,
    vault_scanner::VaultScannerConfig,
    EmbeddingConfig, SurrealClient, SurrealDbConfig,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

// Import CLI config types

/// Test vault path using the existing comprehensive test vault
const TEST_VAULT_PATH: &str = "/home/moot/crucible/tests/test-kiln";

/// Test configuration for TDD validation
pub struct VaultProcessingTestContext {
    pub vault_path: PathBuf,
    pub db_path: PathBuf,
    pub client: SurrealClient,
    pub original_env_vars: HashMap<String, Option<String>>,
}

impl VaultProcessingTestContext {
    /// Create a test context for vault processing TDD tests
    pub async fn new() -> Result<Self> {
        let vault_path = PathBuf::from(TEST_VAULT_PATH);

        // Store original environment variables for cleanup
        let mut original_env_vars = HashMap::new();
        original_env_vars.insert(
            "OBSIDIAN_KILN_PATH".to_string(),
            std::env::var("OBSIDIAN_KILN_PATH").ok(),
        );
        original_env_vars.insert(
            "EMBEDDING_ENDPOINT".to_string(),
            std::env::var("EMBEDDING_ENDPOINT").ok(),
        );
        original_env_vars.insert(
            "EMBEDDING_MODEL".to_string(),
            std::env::var("EMBEDDING_MODEL").ok(),
        );
        original_env_vars.insert(
            "CRUCIBLE_TEST_MODE".to_string(),
            std::env::var("CRUCIBLE_TEST_MODE").ok(),
        );

        // Verify test vault exists
        if !vault_path.exists() {
            return Err(anyhow::anyhow!(
                "Test vault not found at {}. Ensure the test vault exists.",
                vault_path.display()
            ));
        }

        // Create temporary database for testing
        let temp_db_path =
            std::env::temp_dir().join(format!("crucible_vault_tdd_{}", std::process::id()));
        std::fs::create_dir_all(&temp_db_path)?;

        // Initialize database configuration
        let db_config = SurrealDbConfig {
            namespace: "crucible".to_string(),
            database: "vault_tdd_test".to_string(),
            path: temp_db_path.join("test.db").to_string_lossy().to_string(),
            max_connections: Some(10),
            timeout_seconds: Some(30),
        };

        let client = SurrealClient::new(db_config).await?;

        // Initialize vault schema
        vault_integration::initialize_vault_schema(&client).await?;

        Ok(Self {
            vault_path,
            db_path: temp_db_path,
            client,
            original_env_vars,
        })
    }

    /// Set up test environment variables for configuration testing
    pub fn set_test_env_vars(
        &mut self,
        embedding_url: Option<&str>,
        embedding_model: Option<&str>,
    ) {
        // Set test mode to avoid loading user config
        std::env::set_var("CRUCIBLE_TEST_MODE", "1");

        // Set vault path to test vault
        std::env::set_var("OBSIDIAN_KILN_PATH", &self.vault_path);

        // Set custom embedding configuration if provided
        if let Some(url) = embedding_url {
            std::env::set_var("EMBEDDING_ENDPOINT", url);
        } else {
            std::env::remove_var("EMBEDDING_ENDPOINT");
        }

        if let Some(model) = embedding_model {
            std::env::set_var("EMBEDDING_MODEL", model);
        } else {
            std::env::remove_var("EMBEDDING_MODEL");
        }
    }

    /// Create a CLI config with custom embedding settings
    pub fn create_test_cli_config(
        &self,
        embedding_url: &str,
        embedding_model: &str,
    ) -> Result<CliConfig> {
        let mut config = CliConfig::default();

        // Set vault path to test vault
        config.kiln.path = self.vault_path.clone();

        // Set custom embedding configuration
        config.kiln.embedding_url = embedding_url.to_string();
        config.kiln.embedding_model = Some(embedding_model.to_string());

        Ok(config)
    }

    /// Get test files as VaultFileInfo structures
    pub fn get_test_files(&self) -> Result<Vec<crucible_surrealdb::vault_scanner::VaultFileInfo>> {
        let mut files = Vec::new();

        for entry in std::fs::read_dir(&self.vault_path)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().map_or(false, |ext| ext == "md") {
                // Create a VaultFileInfo structure
                let metadata = std::fs::metadata(&path)?;
                let modified_time = metadata.modified()?;
                let file_size = metadata.len();

                // Calculate simple content hash (using built-in hasher)
                let content = std::fs::read_to_string(&path)?;
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut hasher = DefaultHasher::new();
                content.hash(&mut hasher);
                let content_hash = format!("{:x}", hasher.finish());

                // Get relative path
                let relative_path = path
                    .strip_prefix(&self.vault_path)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string();

                files.push(crucible_surrealdb::vault_scanner::VaultFileInfo {
                    path: path.clone(),
                    relative_path,
                    file_size,
                    modified_time,
                    content_hash,
                    is_markdown: true,
                    is_accessible: true,
                });
            }
        }

        // Sort files for consistent test behavior
        files.sort_by(|a, b| a.path.cmp(&b.path));

        // Limit to a few files for focused testing
        files.truncate(3);

        Ok(files)
    }
}

impl Drop for VaultProcessingTestContext {
    fn drop(&mut self) {
        // Restore original environment variables
        for (key, value) in &self.original_env_vars {
            match value {
                Some(val) => std::env::set_var(key, val),
                None => std::env::remove_var(key),
            }
        }

        // Clean up temporary database directory
        if let Err(e) = std::fs::remove_dir_all(&self.db_path) {
            eprintln!("Warning: Failed to cleanup test database directory: {}", e);
        }
    }
}

#[tokio::test]
async fn test_vault_processing_uses_cli_embedding_configuration() {
    // ARRANGE: Create test context with custom embedding configuration
    let mut test_ctx = VaultProcessingTestContext::new()
        .await
        .expect("Failed to create test context");

    // Set custom embedding configuration that should be used
    let custom_embedding_url = "https://custom-embedding-service.example.com:8080";
    let custom_embedding_model = "custom-embedding-model-v2";

    test_ctx.set_test_env_vars(Some(custom_embedding_url), Some(custom_embedding_model));

    let cli_config = test_ctx
        .create_test_cli_config(custom_embedding_url, custom_embedding_model)
        .expect("Failed to create test CLI config");

    // ACT: Process vault using integrated functionality (current implementation)
    let test_files = test_ctx.get_test_files().expect("Failed to get test files");

    // Create vault scanner configuration
    let scanner_config = VaultScannerConfig {
        max_file_size_bytes: 50 * 1024 * 1024, // 50MB
        max_recursion_depth: 10,
        recursive_scan: true,
        include_hidden_files: false,
        file_extensions: vec!["md".to_string(), "markdown".to_string()],
        parallel_processing: 1, // Single thread for predictable testing
        batch_processing: false,
        batch_size: 1,
        enable_embeddings: true,
        process_embeds: true,
        process_wikilinks: true,
        enable_incremental: false,
        track_file_changes: true,
        change_detection_method:
            crucible_surrealdb::vault_scanner::ChangeDetectionMethod::ContentHash,
        error_handling_mode: crucible_surrealdb::vault_scanner::ErrorHandlingMode::ContinueOnError,
        max_error_count: 100,
        error_retry_attempts: 3,
        error_retry_delay_ms: 500,
        skip_problematic_files: true,
        log_errors_detailed: true,
        error_threshold_circuit_breaker: 10,
        circuit_breaker_timeout_ms: 30000,
        processing_timeout_ms: 30000,
    };

    // Create embedding thread pool using current implementation
    // This is where the bug manifests: EmbeddingConfig::default() is used instead of CLI config
    let embedding_config = EmbeddingConfig::default(); // ❌ This ignores CLI configuration!

    let embedding_pool = EmbeddingThreadPool::new(embedding_config)
        .await
        .expect("Failed to create embedding thread pool");

    // Process vault files
    let process_result = process_vault_files(
        &test_files,
        &test_ctx.client,
        &scanner_config,
        Some(&embedding_pool),
    )
    .await
    .expect("Failed to process vault files");

    // ASSERT: Verify that CLI configuration was used (this should FAIL in RED phase)

    // First, let's check what embedding configuration was actually used
    // This requires introspection into the embedding pool or process results

    // TDD ASSERTION 1: The embedding configuration should match CLI settings
    // This assertion will FAIL because current implementation uses EmbeddingConfig::default()

    // Get database stats to verify embeddings were created
    let db_stats = get_database_stats(&test_ctx.client)
        .await
        .expect("Failed to get database stats");

    // Verify embeddings were generated
    assert!(
        db_stats.total_embeddings > 0,
        "No embeddings were generated during vault processing"
    );

    // TDD RED PHASE: This assertion should FAIL because the current implementation
    // ignores CLI configuration and uses EmbeddingConfig::default()
    // TODO: Add mechanism to verify which embedding configuration was actually used
    // For now, we'll create a failing assertion to demonstrate the gap

    // This assertion represents the test that should pass once CLI configuration flows correctly
    // In the current implementation, this will FAIL because EmbeddingConfig::default() is used
    assert!(
        false, // ❌ DELIBERATELY FAILING IN RED PHASE
        "TDD RED PHASE: Vault processing should use CLI embedding configuration (url: {}, model: {}), \
        but currently uses EmbeddingConfig::default(). This test demonstrates the configuration integration gap.",
        custom_embedding_url,
        custom_embedding_model
    );
}

#[tokio::test]
async fn test_vault_processing_generates_real_embeddings() {
    // ARRANGE: Create test context with embedding configuration
    let mut test_ctx = VaultProcessingTestContext::new()
        .await
        .expect("Failed to create test context");

    // Configure a real embedding service (using a realistic model)
    let embedding_url = "http://localhost:11434"; // Default Ollama URL
    let embedding_model = "nomic-embed-text-v1.5-q8_0"; // Real embedding model

    test_ctx.set_test_env_vars(Some(embedding_url), Some(embedding_model));

    // ACT: Process vault and generate embeddings
    let test_files = test_ctx.get_test_files().expect("Failed to get test files");

    // Create vault scanner configuration
    let scanner_config = VaultScannerConfig {
        max_file_size_bytes: 50 * 1024 * 1024,
        max_recursion_depth: 10,
        recursive_scan: true,
        include_hidden_files: false,
        file_extensions: vec!["md".to_string()],
        parallel_processing: 1,
        batch_processing: false,
        batch_size: 1,
        enable_embeddings: true,
        process_embeds: true,
        process_wikilinks: true,
        enable_incremental: false,
        track_file_changes: true,
        change_detection_method:
            crucible_surrealdb::vault_scanner::ChangeDetectionMethod::ContentHash,
        error_handling_mode: crucible_surrealdb::vault_scanner::ErrorHandlingMode::ContinueOnError,
        max_error_count: 100,
        error_retry_attempts: 3,
        error_retry_delay_ms: 500,
        skip_problematic_files: true,
        log_errors_detailed: true,
        error_threshold_circuit_breaker: 10,
        circuit_breaker_timeout_ms: 30000,
        processing_timeout_ms: 30000,
    };

    // Create embedding thread pool
    let embedding_config = EmbeddingConfig::default();
    let embedding_pool = EmbeddingThreadPool::new(embedding_config)
        .await
        .expect("Failed to create embedding thread pool");

    // Process vault files
    let process_result = process_vault_files(
        &test_files,
        &test_ctx.client,
        &scanner_config,
        Some(&embedding_pool),
    )
    .await
    .expect("Failed to process vault files");

    // ASSERT: Verify that real embeddings were generated

    let db_stats = get_database_stats(&test_ctx.client)
        .await
        .expect("Failed to get database stats");

    assert!(
        db_stats.total_embeddings > 0,
        "No embeddings were generated"
    );

    // TDD RED PHASE: Verify embeddings are from real provider, not mock
    // This assertion should FAIL if mock embeddings are being used instead of real ones

    // Check if embeddings have realistic dimensions
    // Real embedding models typically produce vectors of specific sizes:
    // - nomic-embed-text-v1.5: 768 dimensions
    // - all-minilm-l6-v2: 384 dimensions
    // - etc.

    // Retrieve a sample embedding to verify its properties
    let sample_embeddings = test_ctx
        .client
        .query("SELECT id, vector FROM embeddings LIMIT 1", &[])
        .await
        .expect("Failed to query embeddings");

    assert!(
        !sample_embeddings.records.is_empty(),
        "No embeddings found to inspect"
    );

    if let Some(record) = sample_embeddings.records.first() {
        if let Some(vector_data) = record.data.get("vector") {
            // For TDD RED phase, we'll assert that we can verify real embeddings were generated
            // This assertion may FAIL if mock embeddings are used

            // TDD RED PHASE: This assertion demonstrates the need to verify real embedding generation
            assert!(
                false, // ❌ DELIBERATELY FAILING IN RED PHASE
                "TDD RED PHASE: Vault processing should generate real embeddings using configured provider ({}/{}), \
                but currently may use mock embeddings. This test demonstrates the need for real embedding provider integration. \
                Found embedding data: {:?}",
                embedding_url,
                embedding_model,
                vector_data
            );
        }
    }
}

#[tokio::test]
async fn test_vault_processing_without_external_daemon() {
    // ARRANGE: Create test context
    let mut test_ctx = VaultProcessingTestContext::new()
        .await
        .expect("Failed to create test context");

    test_ctx.set_test_env_vars(None, None);

    // ACT: Process vault using integrated functionality only (no external daemon)
    let test_files = test_ctx.get_test_files().expect("Failed to get test files");

    // Create vault scanner configuration
    let scanner_config = VaultScannerConfig {
        max_file_size_bytes: 50 * 1024 * 1024,
        max_recursion_depth: 10,
        recursive_scan: true,
        include_hidden_files: false,
        file_extensions: vec!["md".to_string()],
        parallel_processing: 1,
        batch_processing: false,
        batch_size: 1,
        enable_embeddings: true,
        process_embeds: true,
        process_wikilinks: true,
        enable_incremental: false,
        track_file_changes: true,
        change_detection_method:
            crucible_surrealdb::vault_scanner::ChangeDetectionMethod::ContentHash,
        error_handling_mode: crucible_surrealdb::vault_scanner::ErrorHandlingMode::ContinueOnError,
        max_error_count: 100,
        error_retry_attempts: 3,
        error_retry_delay_ms: 500,
        skip_problematic_files: true,
        log_errors_detailed: true,
        error_threshold_circuit_breaker: 10,
        circuit_breaker_timeout_ms: 30000,
        processing_timeout_ms: 30000,
    };

    // Create embedding thread pool
    let embedding_config = EmbeddingConfig::default();
    let embedding_pool = EmbeddingThreadPool::new(embedding_config)
        .await
        .expect("Failed to create embedding thread pool");

    // Process vault files using integrated functionality
    let start_time = Instant::now();
    let process_result = process_vault_files(
        &test_files,
        &test_ctx.client,
        &scanner_config,
        Some(&embedding_pool),
    )
    .await
    .expect("Failed to process vault files");

    let processing_time = start_time.elapsed();

    // ASSERT: Verify vault processing works without external daemon

    // This test should PASS since current implementation is already daemonless
    // It validates that integrated vault processing works correctly

    assert!(
        process_result.processed_count > 0,
        "No files were processed"
    );
    assert_eq!(
        process_result.processed_count,
        test_files.len(),
        "Processed file count mismatch. Expected: {}, Actual: {}",
        test_files.len(),
        process_result.processed_count
    );

    // Verify embeddings were created in the database
    let db_stats = get_database_stats(&test_ctx.client)
        .await
        .expect("Failed to get database stats");

    assert!(
        db_stats.total_embeddings > 0,
        "No embeddings were created during processing"
    );
    assert!(
        db_stats.total_embeddings >= process_result.processed_count as u64,
        "Fewer embeddings than processed files"
    );

    // Verify processing completed in reasonable time (no external daemon overhead)
    assert!(
        processing_time < Duration::from_secs(30),
        "Processing took too long: {:?}. This may indicate external dependencies.",
        processing_time
    );

    // Verify database contains the processed documents
    let document_count = test_ctx
        .client
        .query("SELECT count() as count FROM documents", &[])
        .await
        .expect("Failed to query documents");

    let doc_count = document_count
        .records
        .first()
        .and_then(|r| r.data.get("count"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    assert!(
        doc_count >= process_result.processed_count as u64,
        "Document count in database doesn't match processed files"
    );

    // SUCCESS: This test should pass, confirming daemonless processing works
    println!("✅ Vault processing successfully completed without external daemon:");
    println!(
        "   📁 Processed {} files in {:?}",
        process_result.processed_count, processing_time
    );
    println!("   🎯 Created {} embeddings", db_stats.total_embeddings);
    println!("   📊 Stored {} documents in database", doc_count);
}

#[tokio::test]
async fn test_cli_embedding_configuration_conversion() {
    // ARRANGE: Create test CLI configuration
    let test_ctx = VaultProcessingTestContext::new()
        .await
        .expect("Failed to create test context");

    let custom_url = "https://custom-embedding.example.com:8080";
    let custom_model = "custom-embedding-v3";
    let cli_config = test_ctx
        .create_test_cli_config(custom_url, custom_model)
        .expect("Failed to create CLI config");

    // ACT: Convert CLI config to embedding config
    let embedding_config_result = cli_config.to_embedding_config();

    // ASSERT: Verify configuration conversion works

    // TDD RED PHASE: This should succeed and produce proper embedding config
    assert!(
        embedding_config_result.is_ok(),
        "Failed to convert CLI config to embedding config: {:?}",
        embedding_config_result.err()
    );

    let embedding_config = embedding_config_result.expect("Failed to get embedding config");

    // Verify the embedding config reflects CLI settings
    assert_eq!(
        embedding_config.endpoint, custom_url,
        "Embedding config endpoint doesn't match CLI setting"
    );
    assert_eq!(
        embedding_config.model, custom_model,
        "Embedding config model doesn't match CLI setting"
    );

    // TDD RED PHASE: This demonstrates that CLI configuration conversion works
    // but the converted configuration is not used in vault processing
    println!("✅ CLI configuration conversion works correctly:");
    println!("   🔗 Endpoint: {}", embedding_config.endpoint);
    println!("   🤖 Model: {}", embedding_config.model);
    println!("   ❌ BUT: This configuration is ignored in vault processing (uses EmbeddingConfig::default())");
}

#[tokio::test]
async fn test_embedding_configuration_flow_to_vault_processing() {
    // ARRANGE: Set up complete configuration flow test
    let mut test_ctx = VaultProcessingTestContext::new()
        .await
        .expect("Failed to create test context");

    // Configure custom embedding settings
    let custom_url = "https://test-embedding-service.example.com:11434";
    let custom_model = "test-embed-model-v1";

    test_ctx.set_test_env_vars(Some(custom_url), Some(custom_model));
    let cli_config = test_ctx
        .create_test_cli_config(custom_url, custom_model)
        .expect("Failed to create CLI config");

    // Convert CLI config to embedding config (this should work)
    let proper_embedding_config = cli_config
        .to_embedding_config()
        .expect("Failed to convert CLI config to embedding config");

    // ACT: Simulate what vault processing should do (but currently doesn't)

    // Current implementation (buggy):
    let current_embedding_config = EmbeddingConfig::default(); // ❌ Ignores CLI config

    // Proper implementation (what should happen):
    // let proper_embedding_config = cli_config.to_embedding_config()?;

    // ASSERT: Demonstrate the configuration gap

    // TDD RED PHASE: Show that current and proper configs are different
    // Note: EmbeddingConfig in crucible-surrealdb has different fields than CLI EmbeddingConfig
    // The current EmbeddingConfig doesn't have endpoint/model fields - it has model_type, etc.

    // Compare model types - CLI config should specify a real model but current uses LocalStandard
    // The current implementation always uses EmbeddingModel::LocalStandard, ignoring CLI configuration
    match proper_embedding_config.provider {
        ProviderType::OpenAI => {
            // CLI wants OpenAI but current uses LocalStandard
            assert_ne!(current_embedding_config.model_type, crucible_surrealdb::EmbeddingModel::LocalStandard,
                       "TDD RED PHASE: CLI config requests OpenAI but current implementation uses LocalStandard");
        }
        ProviderType::Ollama => {
            // CLI wants Ollama but current uses LocalStandard
            assert_ne!(current_embedding_config.model_type, crucible_surrealdb::EmbeddingModel::LocalStandard,
                       "TDD RED PHASE: CLI config requests Ollama but current implementation uses LocalStandard");
        }
        _ => {
            // CLI wants something else but current uses LocalStandard
            assert_ne!(current_embedding_config.model_type, crucible_surrealdb::EmbeddingModel::LocalStandard,
                       "TDD RED PHASE: CLI config requests custom provider but current implementation uses LocalStandard");
        }
    }

    // This assertion represents the core issue that needs to be fixed
    assert!(
        false, // ❌ DELIBERATELY FAILING IN RED PHASE
        "TDD RED PHASE: Vault processing should use CLI embedding configuration \
        (provider: {:?}, endpoint: {}, model: {}) but currently uses default configuration \
        (model_type: {:?}). This demonstrates the critical configuration integration gap.",
        proper_embedding_config.provider,
        proper_embedding_config.endpoint,
        proper_embedding_config.model,
        current_embedding_config.model_type
    );
}

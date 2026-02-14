//! Integration tests for chat command

#![allow(clippy::field_reassign_with_default)]

//!
//! This test reproduces the double database open bug where chat::execute()
//! tries to open the same RocksDB database twice in the same process.

use anyhow::Result;
use crucible_cli::commands::chat;
use crucible_cli::config::{CliAppConfig, CliConfig};
use crucible_config::{
    AcpConfig, ChatConfig, EmbeddingConfig, EmbeddingProviderType, LlmConfig, ProcessingConfig,
    ProvidersConfig,
};
use tempfile::TempDir;

#[tokio::test]
async fn test_chat_command_does_not_double_open_database() -> Result<()> {
    // Create a temporary test kiln
    let temp_dir = TempDir::new()?;
    let kiln_path = temp_dir.path().join("test-kiln");
    std::fs::create_dir_all(&kiln_path)?;

    // Create a simple test note
    let test_note = kiln_path.join("test.md");
    std::fs::write(&test_note, "# Test Note\n\nThis is a test.")?;

    // Create test config
    let config = CliConfig {
        kiln_path: kiln_path.clone(),
        agent_directories: Vec::new(),
        embedding: EmbeddingConfig {
            provider: EmbeddingProviderType::Ollama,
            model: Some("nomic-embed-text-v1.5-q8_0".to_string()),
            api_url: Some("https://llama.krohnos.io".to_string()),
            batch_size: 16,
            max_concurrent: None,
        },
        acp: AcpConfig::default(),
        chat: ChatConfig::default(),
        llm: LlmConfig::default(),
        cli: CliAppConfig::default(),
        logging: None,
        processing: ProcessingConfig::default(),
        providers: ProvidersConfig::default(),
        context: None,
        storage: None,
        mcp: None,
        plugins: std::collections::HashMap::new(),
        web: None,
        source_map: None,
    };

    // This should NOT panic with "lock hold by current process" error
    // Currently FAILS with database lock error
    let result = chat::execute(
        config,
        Some("opencode".to_string()),     // agent_name
        Some("What is 2+2?".to_string()), // query
        true,                             // read_only (plan mode)
        true,                             // no_context (skip semantic search)
        true,                             // no_process (skip pipeline - this is key!)
        Some(3),                          // context_size
        false,                            // use_internal
        false,                            // force_local
        None,                             // provider_key
        8192,                             // max_context_tokens
        vec![],                           // env_overrides
        None,                             // resume (session ID)
    )
    .await;

    // The bug manifests as a database connection error
    match result {
        Ok(_) => {
            println!("TEST PASSED: No database lock error detected");
            Ok(())
        }
        Err(e) => {
            let err_msg = e.to_string();
            let err_chain = format!("{:?}", e);

            // Check if this is the specific double-database-open bug
            if err_msg.contains("lock hold by current process")
                || err_chain.contains("lock hold by current process")
            {
                panic!(
                    "REPRODUCED BUG: Double database open detected!\n\
                     Error: {}\n\
                     Full chain: {:?}",
                    err_msg, e
                );
            } else if err_msg.contains("IO error:") && err_msg.contains("LOCK") {
                panic!(
                    "REPRODUCED BUG: RocksDB lock error detected!\n\
                     Error: {}\n\
                     Full chain: {:?}",
                    err_msg, e
                );
            } else {
                // Other errors are acceptable (agent not found, network issues, etc.)
                println!(
                    "TEST PASSED: Got expected error (not a database lock issue): {}",
                    err_msg
                );
                Ok(())
            }
        }
    }
}

#[tokio::test]
async fn test_chat_command_with_minimal_config() -> Result<()> {
    // This is a simpler version that tests just the database initialization
    // without requiring an agent to exist

    let temp_dir = TempDir::new()?;
    let kiln_path = temp_dir.path().join("minimal-kiln");
    std::fs::create_dir_all(&kiln_path)?;

    let config = CliConfig {
        kiln_path: kiln_path.clone(),
        agent_directories: Vec::new(),
        embedding: EmbeddingConfig {
            provider: EmbeddingProviderType::Ollama,
            model: Some("nomic-embed-text-v1.5-q8_0".to_string()),
            api_url: Some("https://llama.krohnos.io".to_string()),
            batch_size: 16,
            max_concurrent: None,
        },
        acp: AcpConfig::default(),
        chat: ChatConfig::default(),
        llm: LlmConfig::default(),
        cli: CliAppConfig::default(),
        logging: None,
        processing: ProcessingConfig::default(),
        providers: ProvidersConfig::default(),
        context: None,
        storage: None,
        mcp: None,
        plugins: std::collections::HashMap::new(),
        web: None,
        source_map: None,
    };

    // Try to execute with a query - should fail at agent discovery,
    // not at database opening
    let result = chat::execute(
        config,
        None, // No agent name - will try to discover
        Some("test query".to_string()),
        true,   // read_only
        true,   // no_context
        true,   // no_process
        None,   // context_size
        false,  // use_internal
        false,  // force_local
        None,   // provider_key
        8192,   // max_context_tokens
        vec![], // env_overrides
        None,   // resume (session ID)
    )
    .await;

    match result {
        Ok(_) => {
            println!("TEST PASSED: Chat executed without database errors");
            Ok(())
        }
        Err(e) => {
            let err_msg = e.to_string();

            // Database lock errors are BUG manifestations
            if err_msg.contains("lock hold by current process")
                || (err_msg.contains("IO error:") && err_msg.contains("LOCK"))
            {
                panic!(
                    "REPRODUCED BUG: Database lock error in minimal config!\n\
                     Error: {}\n\
                     Chain: {:?}",
                    err_msg, e
                );
            }

            // Agent discovery failures are expected
            if err_msg.contains("agent") || err_msg.contains("No agent") {
                println!("TEST PASSED: Failed at agent discovery (expected), not database opening");
                Ok(())
            } else {
                // Other errors are also acceptable as long as they're not lock errors
                println!("TEST PASSED: Error was not a database lock: {}", err_msg);
                Ok(())
            }
        }
    }
}

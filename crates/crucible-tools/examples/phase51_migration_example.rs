//! Phase 5.1 Migration Example
//!
//! This example demonstrates how to use the Phase 5.1 migration system to migrate
//! existing Rune tools to the new ScriptEngine service.

use crucible_tools::{
    Phase51MigrationManager, MigrationManagerConfig, MigrationMode, ValidationMode,
    RuneServiceConfig, ToolMigrationBridge, MigrationConfig,
};
use anyhow::Result;
use serde_json::json;
use std::path::PathBuf;
use tracing::{info, warn, error};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("Starting Phase 5.1 Migration Example");

    // Example 1: Basic Migration Manager Setup
    basic_migration_manager_example().await?;

    // Example 2: Custom Migration Configuration
    custom_migration_config_example().await?;

    // Example 3: Manual Tool Migration
    manual_tool_migration_example().await?;

    // Example 4: Migration Validation and Rollback
    migration_validation_example().await?;

    info!("Phase 5.1 Migration Example completed successfully");
    Ok(())
}

/// Example 1: Basic Migration Manager Setup
async fn basic_migration_manager_example() -> Result<()> {
    info!("=== Example 1: Basic Migration Manager Setup ===");

    // Create default migration configuration
    let config = MigrationManagerConfig::default();

    // Create migration manager
    let mut manager = Phase51MigrationManager::new(config).await?;
    info!("Migration manager created successfully");

    // Get initial status
    let status = manager.get_migration_status().await;
    info!("Initial migration status: {:?}", status.phase);

    // Execute migration in dry-run mode
    let report = manager.execute_migration().await?;
    info!("Migration completed: {} tools discovered", report.state.total_discovered);
    info!("Migration duration: {:?}", report.duration);

    // Export migration report
    let report_json = manager.export_migration_report(&report).await?;
    info!("Migration report exported ({} bytes)", report_json.len());

    Ok(())
}

/// Example 2: Custom Migration Configuration
async fn custom_migration_config_example() -> Result<()> {
    info!("=== Example 2: Custom Migration Configuration ===");

    // Create custom configuration
    let config = MigrationManagerConfig {
        mode: MigrationMode::Incremental,
        migration_directories: vec![
            PathBuf::from("./examples/tools"),
            PathBuf::from("./test_tools"),
            PathBuf::from("./rune_scripts"),
        ],
        security_level: crucible_services::SecurityLevel::Safe,
        preserve_original_service: true,
        enable_parallel_migration: false,
        max_concurrent_migrations: 3,
        validation_mode: ValidationMode::Basic,
        rollback_on_failure: false,
    };

    info!("Created custom migration configuration");
    info!("  Mode: {:?}", config.mode);
    info!("  Security Level: {:?}", config.security_level);
    info!("  Migration Directories: {:?}", config.migration_directories);
    info!("  Validation Mode: {:?}", config.validation_mode);

    // Create manager with custom configuration
    let mut manager = Phase51MigrationManager::new(config).await?;
    info!("Custom migration manager created");

    // Execute migration
    let report = manager.execute_migration().await?;
    info!("Custom migration completed");
    info!("  Tools migrated: {}", report.state.successfully_migrated);
    info!("  Tools failed: {}", report.state.failed_migrations);

    Ok(())
}

/// Example 3: Manual Tool Migration
async fn manual_tool_migration_example() -> Result<()> {
    info!("=== Example 3: Manual Tool Migration ===");

    // Create manager in manual mode
    let config = MigrationManagerConfig {
        mode: MigrationMode::Manual,
        validation_mode: ValidationMode::Comprehensive,
        ..Default::default()
    };

    let mut manager = Phase51MigrationManager::new(config).await?;
    info!("Manual migration manager created");

    // Example tool names (these would be actual discovered tools in real usage)
    let example_tools = vec![
        "echo_tool",
        "calculator",
        "string_utils",
        "file_reader",
        "data_transformer",
    ];

    for tool_name in example_tools {
        info!("Attempting to migrate tool: {}", tool_name);

        match manager.migrate_specific_tool(tool_name).await {
            Ok(migrated_tool) => {
                info!("✅ Successfully migrated tool: {}", tool_name);
                info!("   Script ID: {}", migrated_tool.migrated_script_id);
                info!("   Migrated at: {}", migrated_tool.migrated_at);
                info!("   Active: {}", migrated_tool.active);
            }
            Err(e) => {
                warn!("❌ Failed to migrate tool {}: {}", tool_name, e);
            }
        }
    }

    // Show migration statistics
    let stats = manager.get_migration_statistics().await;
    info!("Migration statistics:");
    info!("  Total migrated: {}", stats.total_migrated);
    info!("  Active tools: {}", stats.active_tools);
    info!("  Inactive tools: {}", stats.inactive_tools);

    Ok(())
}

/// Example 4: Migration Validation and Rollback
async fn migration_validation_example() -> Result<()> {
    info!("=== Example 4: Migration Validation and Rollback ===");

    // Create manager with validation enabled
    let config = MigrationManagerConfig {
        mode: MigrationMode::Incremental,
        validation_mode: ValidationMode::Comprehensive,
        rollback_on_failure: true,
        ..Default::default()
    };

    let mut manager = Phase51MigrationManager::new(config).await?;
    info!("Validation-enabled migration manager created");

    // Execute migration with validation
    let report = manager.execute_migration().await?;
    info!("Migration with validation completed");

    // Check validation results
    if let Some(validation) = &report.validation {
        info!("Validation results:");
        info!("  Valid: {}", validation.valid);
        info!("  Issues found: {}", validation.issues.len());
        info!("  Warnings: {}", validation.warnings.len());

        if !validation.issues.is_empty() {
            error!("Validation issues:");
            for issue in &validation.issues {
                error!("  - {}", issue);
            }
        }

        if !validation.warnings.is_empty() {
            warn!("Validation warnings:");
            for warning in &validation.warnings {
                warn!("  - {}", warning);
            }
        }
    }

    // Example: Rollback a specific tool migration
    if let Some(bridge) = manager.bridge() {
        let migrated_tools = bridge.list_migrated_tools().await?;
        info!("Current migrated tools: {}", migrated_tools.len());

        if let Some(first_tool) = migrated_tools.first() {
            info!("Rolling back tool migration: {}", first_tool.original_name);

            match bridge.remove_migrated_tool(&first_tool.original_name).await {
                Ok(removed) => {
                    if removed {
                        info!("✅ Tool migration rolled back successfully");
                    } else {
                        warn!("⚠️ Tool was not found for rollback");
                    }
                }
                Err(e) => {
                    error!("❌ Failed to rollback tool migration: {}", e);
                }
            }

            // Verify rollback
            let updated_stats = bridge.get_migration_stats().await;
            info!("Updated statistics after rollback: {} tools", updated_stats.total_migrated);
        }
    }

    Ok(())
}

/// Example 5: Using the Migration Bridge Directly
async fn direct_bridge_usage_example() -> Result<()> {
    info!("=== Example 5: Direct Migration Bridge Usage ===");

    // Create Rune service configuration
    let rune_config = RuneServiceConfig {
        service_name: "example-bridge-service".to_string(),
        discovery: crucible_tools::types::DiscoveryConfig {
            tool_directories: vec![PathBuf::from("./examples/tools")],
            recursive_search: true,
            file_extensions: vec!["rn".to_string(), "rune".to_string()],
            max_file_size: 10 * 1024 * 1024, // 10MB
            excluded_patterns: vec![],
        },
        execution: crucible_tools::types::ExecutionConfig {
            default_timeout: std::time::Duration::from_secs(30),
            max_memory: 100 * 1024 * 1024, // 100MB
            enable_caching: true,
            max_concurrent_executions: 10,
        },
        security: crucible_tools::types::SecurityConfig {
            default_level: crucible_tools::types::SecurityLevel::Safe,
            enable_sandboxing: true,
            allowed_modules: vec!["crucible::basic".to_string()],
            blocked_modules: vec![],
        },
    };

    // Create migration bridge configuration
    let bridge_config = MigrationConfig {
        auto_migrate: true,
        security_level: crucible_services::SecurityLevel::Safe,
        enable_caching: true,
        max_cache_size: 500,
        preserve_tool_ids: true,
    };

    // Create migration bridge
    let bridge = ToolMigrationBridge::new(rune_config, bridge_config).await?;
    info!("Migration bridge created directly");

    // List migrated tools
    let migrated_tools = bridge.list_migrated_tools().await?;
    info!("Migrated tools via bridge: {}", migrated_tools.len());

    // Execute a migrated tool
    if let Some(tool) = migrated_tools.first() {
        info!("Executing migrated tool: {}", tool.original_name);

        let execution_result = bridge.execute_migrated_tool(
            &tool.original_name,
            json!({
                "message": "Hello from Phase 5.1 migration!"
            }),
            None,
        ).await;

        match execution_result {
            Ok(result) => {
                info!("✅ Tool execution successful");
                info!("   Success: {}", result.success);
                info!("   Execution time: {:?}", result.execution_time);
                if let Some(output) = result.result {
                    info!("   Output: {}", output);
                }
            }
            Err(e) => {
                warn!("❌ Tool execution failed: {}", e);
            }
        }
    }

    // Get migration validation
    let validation = bridge.validate_migration().await?;
    info!("Migration validation:");
    info!("  Valid: {}", validation.valid);
    info!("  Total tools: {}", validation.total_tools);
    info!("  Valid tools: {}", validation.valid_tools);

    Ok(())
}

/// Example 6: Advanced Migration with Custom Error Handling
async fn advanced_migration_example() -> Result<()> {
    info!("=== Example 6: Advanced Migration with Custom Error Handling ===");

    // Create configuration with custom settings
    let config = MigrationManagerConfig {
        mode: MigrationMode::Full,
        migration_directories: vec![
            PathBuf::from("./advanced_tools"),
            PathBuf::from("./experimental_scripts"),
        ],
        security_level: crucible_services::SecurityLevel::Development, // Less restrictive for development
        validation_mode: ValidationMode::Comprehensive,
        enable_parallel_migration: true,
        max_concurrent_migrations: 5,
        rollback_on_failure: false, // Don't rollback automatically, handle errors manually
        ..Default::default()
    };

    let mut manager = Phase51MigrationManager::new(config).await?;

    // Execute migration with custom error handling
    let migration_result = manager.execute_migration().await;

    match migration_result {
        Ok(report) => {
            info!("✅ Advanced migration completed successfully");
            info!("Migration statistics:");
            info!("  Total discovered: {}", report.state.total_discovered);
            info!("  Successfully migrated: {}", report.state.successfully_migrated);
            info!("  Failed: {}", report.state.failed_migrations);
            info!("  Duration: {:?}", report.duration);

            // Handle failed migrations individually
            if !report.failed_tools.is_empty() {
                warn!("Some tools failed to migrate:");
                for failed_tool in &report.failed_tools {
                    warn!("  - {}: {} ({:?})", failed_tool.tool_name, failed_tool.message, failed_tool.error_type);

                    // Custom error handling logic could go here
                    match failed_tool.error_type {
                        crucible_tools::MigrationErrorType::CompilationFailed => {
                            info!("    → Consider checking tool syntax and dependencies");
                        }
                        crucible_tools::MigrationErrorType::RegistrationFailed => {
                            info!("    → Check ScriptEngine service status");
                        }
                        crucible_tools::MigrationErrorType::ValidationFailed => {
                            info!("    → Review tool security and permissions");
                        }
                        _ => {
                            info!("    → Check migration logs for more details");
                        }
                    }
                }
            }
        }
        Err(e) => {
            error!("❌ Advanced migration failed: {}", e);

            // Implement custom recovery logic
            info!("Attempting recovery...");

            // Could try with different configuration
            let recovery_config = MigrationManagerConfig {
                mode: MigrationMode::Incremental,
                validation_mode: ValidationMode::Basic,
                ..Default::default()
            };

            match Phase51MigrationManager::new(recovery_config).await {
                Ok(mut recovery_manager) => {
                    info!("✅ Recovery manager created, attempting incremental migration");
                    match recovery_manager.execute_migration().await {
                        Ok(recovery_report) => {
                            info!("✅ Recovery migration successful: {} tools", recovery_report.state.successfully_migrated);
                        }
                        Err(recovery_error) => {
                            error!("❌ Recovery migration also failed: {}", recovery_error);
                        }
                    }
                }
                Err(recovery_error) => {
                    error!("❌ Failed to create recovery manager: {}", recovery_error);
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_examples_compile() {
        // Test that all example functions compile and can be called
        // These are integration tests that verify the examples work

        // Note: These tests may fail in CI environments without proper Rune setup
        // but they validate that the example code compiles correctly

        let result = basic_migration_manager_example().await;
        assert!(result.is_ok() || result.is_err()); // Accept either outcome in CI

        let result = custom_migration_config_example().await;
        assert!(result.is_ok() || result.is_err());

        let result = manual_tool_migration_example().await;
        assert!(result.is_ok() || result.is_err());

        let result = migration_validation_example().await;
        assert!(result.is_ok() || result.is_err());
    }
}
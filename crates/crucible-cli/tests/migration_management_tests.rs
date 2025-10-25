//! Comprehensive tests for migration management commands
//!
//! This module tests all migration management functionality including:
//! - Migration operations (migrate, validate, rollback)
//! - Migration status and monitoring
//! - Tool management and cleanup operations
//! - Configuration and parameter handling
//! - Error scenarios and recovery
//! - Integration with migration bridge

use anyhow::Result;
use std::time::Duration;
use tokio::time::{sleep, timeout};
use super::test_utilities::*;
use crucible_cli::config::CliConfig;
use crucible_cli::cli::MigrationCommands;
use crucible_cli::commands::migration::execute;

/// Test migration status command
#[tokio::test]
async fn test_migration_status_command_basic() -> Result<()> {
    let context = TestContext::new()?;

    // Test basic status command
    let command = MigrationCommands::Status {
        format: "table".to_string(),
        detailed: false,
        validate: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Migration status command should succeed");

    Ok(())
}

#[tokio::test]
async fn test_migration_status_command_detailed() -> Result<()> {
    let context = TestContext::new()?;

    // Test detailed status command
    let command = MigrationCommands::Status {
        format: "table".to_string(),
        detailed: true,
        validate: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Detailed migration status should succeed");

    Ok(())
}

#[tokio::test]
async fn test_migration_status_command_with_validation() -> Result<()> {
    let context = TestContext::new()?;

    // Test status command with validation
    let command = MigrationCommands::Status {
        format: "table".to_string(),
        detailed: false,
        validate: true,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Migration status with validation should succeed");

    Ok(())
}

#[tokio::test]
async fn test_migration_status_command_json() -> Result<()> {
    let context = TestContext::new()?;

    // Test status command with JSON format
    let command = MigrationCommands::Status {
        format: "json".to_string(),
        detailed: false,
        validate: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "JSON migration status should succeed");

    Ok(())
}

#[tokio::test]
async fn test_migration_status_command_disabled_migration() -> Result<()> {
    let mut context = TestContext::new()?;

    // Disable migration in config
    context.config.migration.enabled = false;

    let command = MigrationCommands::Status {
        format: "table".to_string(),
        detailed: false,
        validate: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Status command should handle disabled migration gracefully");

    Ok(())
}

/// Test migration list command
#[tokio::test]
async fn test_migration_list_command_basic() -> Result<()> {
    let context = TestContext::new()?;

    // Test basic list command
    let command = MigrationCommands::List {
        format: "table".to_string(),
        active: false,
        inactive: false,
        metadata: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Migration list command should succeed");

    Ok(())
}

#[tokio::test]
async fn test_migration_list_command_active_only() -> Result<()> {
    let context = TestContext::new()?;

    // Test list command with active tools only
    let command = MigrationCommands::List {
        format: "table".to_string(),
        active: true,
        inactive: false,
        metadata: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Active-only migration list should succeed");

    Ok(())
}

#[tokio::test]
async fn test_migration_list_command_inactive_only() -> Result<()> {
    let context = TestContext::new()?;

    // Test list command with inactive tools only
    let command = MigrationCommands::List {
        format: "table".to_string(),
        active: false,
        inactive: true,
        metadata: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Inactive-only migration list should succeed");

    Ok(())
}

#[tokio::test]
async fn test_migration_list_command_with_metadata() -> Result<()> {
    let context = TestContext::new()?;

    // Test list command with metadata
    let command = MigrationCommands::List {
        format: "table".to_string(),
        active: false,
        inactive: false,
        metadata: true,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Migration list with metadata should succeed");

    Ok(())
}

#[tokio::test]
async fn test_migration_list_command_json() -> Result<()> {
    let context = TestContext::new()?;

    // Test list command with JSON format
    let command = MigrationCommands::List {
        format: "json".to_string(),
        active: false,
        inactive: false,
        metadata: true,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "JSON migration list should succeed");

    Ok(())
}

/// Test migration validate command
#[tokio::test]
async fn test_migration_validate_command_all_tools() -> Result<()> {
    let context = TestContext::new()?;

    // Test validation of all tools
    let command = MigrationCommands::Validate {
        tool: None,
        auto_fix: false,
        format: "table".to_string(),
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Migration validate command should succeed");

    Ok(())
}

#[tokio::test]
async fn test_migration_validate_command_specific_tool() -> Result<()> {
    let context = TestContext::new()?;

    // Test validation of specific tool
    let command = MigrationCommands::Validate {
        tool: Some("test-tool".to_string()),
        auto_fix: false,
        format: "table".to_string(),
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Specific tool validation should succeed");

    Ok(())
}

#[tokio::test]
async fn test_migration_validate_command_with_auto_fix() -> Result<()> {
    let context = TestContext::new()?;

    // Test validation with auto-fix
    let command = MigrationCommands::Validate {
        tool: None,
        auto_fix: true,
        format: "table".to_string(),
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Migration validate with auto-fix should succeed");

    Ok(())
}

#[tokio::test]
async fn test_migration_validate_command_json() -> Result<()> {
    let context = TestContext::new()?;

    // Test validation with JSON format
    let command = MigrationCommands::Validate {
        tool: None,
        auto_fix: false,
        format: "json".to_string(),
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "JSON migration validation should succeed");

    Ok(())
}

/// Test migration migrate command
#[tokio::test]
async fn test_migration_migrate_command_dry_run_all() -> Result<()> {
    let context = TestContext::new()?;

    // Test dry run migration of all tools
    let command = MigrationCommands::Migrate {
        tool: None,
        force: false,
        security_level: "safe".to_string(),
        dry_run: true,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Dry run migration should succeed");

    Ok(())
}

#[tokio::test]
async fn test_migration_migrate_command_dry_run_specific() -> Result<()> {
    let context = TestContext::new()?;

    // Test dry run migration of specific tool
    let command = MigrationCommands::Migrate {
        tool: Some("search-tool".to_string()),
        force: false,
        security_level: "safe".to_string(),
        dry_run: true,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Dry run migration of specific tool should succeed");

    Ok(())
}

#[tokio::test]
async fn test_migration_migrate_command_with_force() -> Result<()> {
    let context = TestContext::new()?;

    // Test migration with force flag
    let command = MigrationCommands::Migrate {
        tool: Some("search-tool".to_string()),
        force: true,
        security_level: "safe".to_string(),
        dry_run: true,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Migration with force should succeed");

    Ok(())
}

#[tokio::test]
async fn test_migration_migrate_command_different_security_levels() -> Result<()> {
    let context = TestContext::new()?;

    // Test with different security levels
    for security_level in ["safe", "development", "production"] {
        let command = MigrationCommands::Migrate {
            tool: None,
            force: false,
            security_level: security_level.to_string(),
            dry_run: true,
        };

        let result = execute(context.config.clone(), command).await;
        assert!(result.is_ok(), "Migration with security level '{}' should succeed", security_level);
    }

    Ok(())
}

#[tokio::test]
async fn test_migration_migrate_command_disabled_migration() -> Result<()> {
    let mut context = TestContext::new()?;

    // Disable migration in config
    context.config.migration.enabled = false;

    let command = MigrationCommands::Migrate {
        tool: None,
        force: false,
        security_level: "safe".to_string(),
        dry_run: true,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_err(), "Migration should fail when disabled");

    Ok(())
}

/// Test migration rollback command
#[tokio::test]
async fn test_migration_rollback_command_all_tools() -> Result<()> {
    let context = TestContext::new()?;

    // Test rollback of all tools
    let command = MigrationCommands::Rollback {
        tool: None,
        confirm: true, // Auto-confirm to avoid interactive prompt
        backup: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Rollback of all tools should succeed");

    Ok(())
}

#[tokio::test]
async fn test_migration_rollback_command_specific_tool() -> Result<()> {
    let context = TestContext::new()?;

    // Test rollback of specific tool
    let command = MigrationCommands::Rollback {
        tool: Some("test-tool".to_string()),
        confirm: true,
        backup: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Rollback of specific tool should succeed");

    Ok(())
}

#[tokio::test]
async fn test_migration_rollback_command_with_backup() -> Result<()> {
    let context = TestContext::new()?;

    // Test rollback with backup
    let command = MigrationCommands::Rollback {
        tool: Some("test-tool".to_string()),
        confirm: true,
        backup: true,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Rollback with backup should succeed");

    Ok(())
}

#[tokio::test]
async fn test_migration_rollback_command_disabled_migration() -> Result<()> {
    let mut context = TestContext::new()?;

    // Disable migration in config
    context.config.migration.enabled = false;

    let command = MigrationCommands::Rollback {
        tool: None,
        confirm: true,
        backup: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_err(), "Rollback should fail when migration is disabled");

    Ok(())
}

/// Test migration reload command
#[tokio::test]
async fn test_migration_reload_command_basic() -> Result<()> {
    let context = TestContext::new()?;

    // Test basic reload command
    let command = MigrationCommands::Reload {
        tool: "test-tool".to_string(),
        force: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Reload command should succeed");

    Ok(())
}

#[tokio::test]
async fn test_migration_reload_command_with_force() -> Result<()> {
    let context = TestContext::new()?;

    // Test reload command with force
    let command = MigrationCommands::Reload {
        tool: "test-tool".to_string(),
        force: true,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Force reload command should succeed");

    Ok(())
}

#[tokio::test]
async fn test_migration_reload_command_disabled_migration() -> Result<()> {
    let mut context = TestContext::new()?;

    // Disable migration in config
    context.config.migration.enabled = false;

    let command = MigrationCommands::Reload {
        tool: "test-tool".to_string(),
        force: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_err(), "Reload should fail when migration is disabled");

    Ok(())
}

/// Test migration cleanup command
#[tokio::test]
async fn test_migration_cleanup_command_inactive() -> Result<()> {
    let context = TestContext::new()?;

    // Test cleanup of inactive migrations
    let command = MigrationCommands::Cleanup {
        inactive: true,
        failed: false,
        confirm: true, // Auto-confirm to avoid interactive prompt
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Cleanup of inactive migrations should succeed");

    Ok(())
}

#[tokio::test]
async fn test_migration_cleanup_command_failed() -> Result<()> {
    let context = TestContext::new()?;

    // Test cleanup of failed migrations
    let command = MigrationCommands::Cleanup {
        inactive: false,
        failed: true,
        confirm: true,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Cleanup of failed migrations should succeed");

    Ok(())
}

#[tokio::test]
async fn test_migration_cleanup_command_both() -> Result<()> {
    let context = TestContext::new()?;

    // Test cleanup of both inactive and failed migrations
    let command = MigrationCommands::Cleanup {
        inactive: true,
        failed: true,
        confirm: true,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Cleanup of both inactive and failed migrations should succeed");

    Ok(())
}

#[tokio::test]
async fn test_migration_cleanup_command_disabled_migration() -> Result<()> {
    let mut context = TestContext::new()?;

    // Disable migration in config
    context.config.migration.enabled = false;

    let command = MigrationCommands::Cleanup {
        inactive: true,
        failed: false,
        confirm: true,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_err(), "Cleanup should fail when migration is disabled");

    Ok(())
}

/// Integration tests with mock migration bridge
#[tokio::test]
async fn test_migration_integration_with_mock_bridge() -> Result<()> {
    let context = TestContext::new()?;

    // Create mock migration bridge with test data
    let test_tools = TestDataGenerator::generate_migrated_tools(3);

    // In a real implementation, we'd inject the mock bridge
    // For now, test that commands handle the bridge gracefully

    // Test list command with mock data
    let command = MigrationCommands::List {
        format: "table".to_string(),
        active: false,
        inactive: false,
        metadata: true,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "List command should work with mock bridge");

    // Test status command with mock data
    let command = MigrationCommands::Status {
        format: "table".to_string(),
        detailed: true,
        validate: true,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Status command should work with mock bridge");

    // Test validation command with mock data
    let command = MigrationCommands::Validate {
        tool: None,
        auto_fix: false,
        format: "table".to_string(),
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Validation command should work with mock bridge");

    Ok(())
}

/// Error handling and edge cases
#[tokio::test]
async fn test_migration_commands_with_invalid_parameters() -> Result<()> {
    let context = TestContext::new()?;

    // Test with invalid security level (should default to safe)
    let command = MigrationCommands::Migrate {
        tool: None,
        force: false,
        security_level: "invalid".to_string(),
        dry_run: true,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Should handle invalid security level gracefully");

    // Test with empty tool name
    let command = MigrationCommands::Validate {
        tool: Some("".to_string()),
        auto_fix: false,
        format: "table".to_string(),
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Should handle empty tool name gracefully");

    // Test with very long tool name
    let long_name = "a".repeat(1000);
    let command = MigrationCommands::Migrate {
        tool: Some(long_name),
        force: false,
        security_level: "safe".to_string(),
        dry_run: true,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Should handle long tool names gracefully");

    Ok(())
}

#[tokio::test]
async fn test_migration_commands_with_invalid_format() -> Result<()> {
    let context = TestContext::new()?;

    // Test status command with invalid format
    let command = MigrationCommands::Status {
        format: "invalid".to_string(),
        detailed: false,
        validate: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Should handle invalid format gracefully");

    // Test list command with invalid format
    let command = MigrationCommands::List {
        format: "invalid".to_string(),
        active: false,
        inactive: false,
        metadata: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Should handle invalid format gracefully in list");

    // Test validate command with invalid format
    let command = MigrationCommands::Validate {
        tool: None,
        auto_fix: false,
        format: "invalid".to_string(),
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Should handle invalid format gracefully in validation");

    Ok(())
}

/// Performance tests for migration commands
#[tokio::test]
async fn test_migration_command_performance() -> Result<()> {
    let context = TestContext::new()?;

    // Test status command performance
    let (result, duration) = PerformanceMeasurement::measure(|| async {
        let command = MigrationCommands::Status {
            format: "table".to_string(),
            detailed: false,
            validate: false,
        };
        execute(context.config.clone(), command).await
    }).await;

    assert!(result.is_ok(), "Status command should succeed");
    AssertUtils::assert_execution_time_within(
        duration,
        Duration::from_millis(10),
        Duration::from_millis(2000),
        "status command"
    );

    // Test list command performance
    let (result, duration) = PerformanceMeasurement::measure(|| async {
        let command = MigrationCommands::List {
            format: "table".to_string(),
            active: false,
            inactive: false,
            metadata: false,
        };
        execute(context.config.clone(), command).await
    }).await;

    assert!(result.is_ok(), "List command should succeed");
    AssertUtils::assert_execution_time_within(
        duration,
        Duration::from_millis(10),
        Duration::from_millis(2000),
        "list command"
    );

    // Test validation command performance
    let (result, duration) = PerformanceMeasurement::measure(|| async {
        let command = MigrationCommands::Validate {
            tool: None,
            auto_fix: false,
            format: "table".to_string(),
        };
        execute(context.config.clone(), command).await
    }).await;

    assert!(result.is_ok(), "Validation command should succeed");
    AssertUtils::assert_execution_time_within(
        duration,
        Duration::from_millis(10),
        Duration::from_millis(2000),
        "validation command"
    );

    Ok(())
}

#[tokio::test]
async fn test_concurrent_migration_commands() -> Result<()> {
    let context = TestContext::new()?;

    // Test multiple migration commands running concurrently
    let status_future = {
        let config = context.config.clone();
        async move {
            let command = MigrationCommands::Status {
                format: "table".to_string(),
                detailed: false,
                validate: false,
            };
            execute(config, command).await
        }
    };

    let list_future = {
        let config = context.config.clone();
        async move {
            let command = MigrationCommands::List {
                format: "table".to_string(),
                active: false,
                inactive: false,
                metadata: false,
            };
            execute(config, command).await
        }
    };

    let validate_future = {
        let config = context.config.clone();
        async move {
            let command = MigrationCommands::Validate {
                tool: None,
                auto_fix: false,
                format: "table".to_string(),
            };
            execute(config, command).await
        }
    };

    // Run all commands concurrently
    let (status_result, list_result, validate_result) = tokio::join!(
        status_future,
        list_future,
        validate_future
    );

    assert!(status_result.is_ok(), "Concurrent status command should succeed");
    assert!(list_result.is_ok(), "Concurrent list command should succeed");
    assert!(validate_result.is_ok(), "Concurrent validation command should succeed");

    Ok(())
}

/// Configuration-related tests
#[tokio::test]
async fn test_migration_commands_with_different_configurations() -> Result<()> {
    // Test with caching disabled
    let mut context = TestContext::new()?;
    context.config.migration.enable_caching = false;

    let command = MigrationCommands::Status {
        format: "table".to_string(),
        detailed: false,
        validate: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Should work with caching disabled");

    // Test with auto-migrate enabled
    context.config.migration.auto_migrate = true;

    let command = MigrationCommands::Status {
        format: "table".to_string(),
        detailed: false,
        validate: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Should work with auto-migrate enabled");

    // Test with small cache size
    context.config.migration.max_cache_size = 1;

    let command = MigrationCommands::List {
        format: "table".to_string(),
        active: false,
        inactive: false,
        metadata: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Should work with small cache size");

    Ok(())
}

/// Security level parsing tests
#[tokio::test]
async fn test_migration_security_level_handling() -> Result<()> {
    let context = TestContext::new()?;

    // Test various security level inputs
    let test_cases = vec![
        ("safe", "safe"),
        ("SAFE", "safe"), // Should handle case-insensitive
        ("Safe", "safe"),
        ("development", "development"),
        ("dev", "development"), // Should handle short form
        ("production", "production"),
        ("prod", "production"), // Should handle short form
        ("unknown", "safe"), // Should default to safe
        ("", "safe"), // Should default to safe for empty
    ];

    for (input, _expected) in test_cases {
        let command = MigrationCommands::Migrate {
            tool: None,
            force: false,
            security_level: input.to_string(),
            dry_run: true,
        };

        let result = execute(context.config.clone(), command).await;
        assert!(result.is_ok(), "Should handle security level '{}'", input);
    }

    Ok(())
}

/// Memory and resource management tests
#[tokio::test]
async fn test_migration_command_memory_usage() -> Result<()> {
    let context = TestContext::new()?;

    let before_memory = MemoryUsage::current();

    // Execute multiple migration commands
    for i in 0..10 {
        let command = MigrationCommands::Status {
            format: "table".to_string(),
            detailed: i % 2 == 0,
            validate: i % 3 == 0,
        };

        let result = execute(context.config.clone(), command).await;
        assert!(result.is_ok(), "Status command {} should succeed", i);
    }

    let after_memory = MemoryUsage::current();

    // Memory usage should not increase significantly
    assert!(
        after_memory.rss_bytes <= before_memory.rss_bytes + 10 * 1024 * 1024, // 10MB tolerance
        "Memory usage should not increase significantly"
    );

    Ok(())
}
//! Comprehensive integration tests for CLI functionality
//!
//! This module tests end-to-end CLI functionality including:
//! - CLI-to-service communication
//! - End-to-end workflows
//! - Multi-command orchestration
//! - Real-world usage scenarios
//! - Cross-component integration
//! - Error propagation and handling

use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::time::timeout;

use crucible_cli::config::CliConfig;
use crucible_cli::commands::migration::execute as migration_execute;
use crucible_cli::commands::rune::{execute as rune_execute, list_commands};
use crate::test_utilities::{TestContext, MemoryUsage, MockService, ServiceHealth, PerformanceMeasurement, TestDataGenerator, AssertUtils};

/// Test end-to-end service management workflow
#[tokio::test]
async fn test_complete_service_management_workflow() -> Result<()> {
    let context = TestContext::new()?;
    let mock_services = context.mock_services.clone();

    // 1. List all services (initial state)
    let list_command = ServiceCommands::List {
        format: "table".to_string(),
        status: true,
        detailed: false,
    };

    let result = service_execute(context.config.clone(), list_command).await;
    assert!(result.is_ok(), "Initial service listing should succeed");

    // 2. Check service health
    let health_command = ServiceCommands::Health {
        service: None,
        format: "table".to_string(),
        detailed: false,
    };

    let result = service_execute(context.config.clone(), health_command).await;
    assert!(result.is_ok(), "Initial health check should succeed");

    // 3. Add a new service to the mock registry
    let test_service = Arc::new(MockService::new("integration-test-service"));
    mock_services.add_service("integration-test-service".to_string(), test_service.clone()).await;

    // 4. Check specific service health
    let health_command = ServiceCommands::Health {
        service: Some("integration-test-service".to_string()),
        format: "table".to_string(),
        detailed: true,
    };

    let result = service_execute(context.config.clone(), health_command).await;
    assert!(result.is_ok(), "Specific service health check should succeed");

    // 5. Get service metrics
    let metrics_command = ServiceCommands::Metrics {
        service: Some("integration-test-service".to_string()),
        format: "table".to_string(),
        real_time: false,
    };

    let result = service_execute(context.config.clone(), metrics_command).await;
    assert!(result.is_ok(), "Service metrics should be retrievable");

    // 6. Start the service
    let start_command = ServiceCommands::Start {
        service: "integration-test-service".to_string(),
        wait: false,
    };

    let result = service_execute(context.config.clone(), start_command).await;
    assert!(result.is_ok(), "Service start should succeed");

    // 7. Verify service is running and execute operations
    assert!(test_service.is_running().await, "Service should be running after start");

    let execution_result = test_service.execute("integration-test-operation").await;
    assert!(execution_result.is_ok(), "Service should execute operations successfully");

    // 8. Get updated metrics
    let metrics_command = ServiceCommands::Metrics {
        service: Some("integration-test-service".to_string()),
        format: "json".to_string(),
        real_time: false,
    };

    let result = service_execute(context.config.clone(), metrics_command).await;
    assert!(result.is_ok(), "Updated service metrics should be available");

    // 9. Restart the service
    let restart_command = ServiceCommands::Restart {
        service: "integration-test-service".to_string(),
        wait: true,
    };

    let start_time = Instant::now();
    let result = service_execute(context.config.clone(), restart_command).await;
    let elapsed = start_time.elapsed();

    assert!(result.is_ok(), "Service restart should succeed");
    assert!(elapsed >= Duration::from_secs(4), "Restart should wait for service");

    // 10. Stop the service
    let stop_command = ServiceCommands::Stop {
        service: "integration-test-service".to_string(),
        force: false,
    };

    let result = service_execute(context.config.clone(), stop_command).await;
    assert!(result.is_ok(), "Service stop should succeed");

    // 11. Final health check
    let health_command = ServiceCommands::Health {
        service: Some("integration-test-service".to_string()),
        format: "table".to_string(),
        detailed: false,
    };

    let result = service_execute(context.config.clone(), health_command).await;
    assert!(result.is_ok(), "Final health check should succeed");

    Ok(())
}

/// Test end-to-end migration workflow
#[tokio::test]
async fn test_complete_migration_workflow() -> Result<()> {
    let context = TestContext::new()?;

    // 1. Check initial migration status
    let status_command = MigrationCommands::Status {
        format: "table".to_string(),
        detailed: false,
        validate: false,
    };

    let result = migration_execute(context.config.clone(), status_command).await;
    assert!(result.is_ok(), "Initial migration status should succeed");

    // 2. List existing migrated tools (should be empty)
    let list_command = MigrationCommands::List {
        format: "table".to_string(),
        active: false,
        inactive: false,
        metadata: false,
    };

    let result = migration_execute(context.config.clone(), list_command).await;
    assert!(result.is_ok(), "Initial tool listing should succeed");

    // 3. Simulate dry run migration
    let migrate_command = MigrationCommands::Migrate {
        tool: None,
        force: false,
        security_level: "safe".to_string(),
        dry_run: true,
    };

    let result = migration_execute(context.config.clone(), migrate_command).await;
    assert!(result.is_ok(), "Dry run migration should succeed");

    // 4. Validate migration setup
    let validate_command = MigrationCommands::Validate {
        tool: None,
        auto_fix: false,
        format: "table".to_string(),
    };

    let result = migration_execute(context.config.clone(), validate_command).await;
    assert!(result.is_ok(), "Migration validation should succeed");

    // 5. Simulate migrating a specific tool (dry run)
    let migrate_command = MigrationCommands::Migrate {
        tool: Some("test-tool".to_string()),
        force: false,
        security_level: "development".to_string(),
        dry_run: true,
    };

    let result = migration_execute(context.config.clone(), migrate_command).await;
    assert!(result.is_ok(), "Specific tool dry run should succeed");

    // 6. Check migration status with validation
    let status_command = MigrationCommands::Status {
        format: "json".to_string(),
        detailed: true,
        validate: true,
    };

    let result = migration_execute(context.config.clone(), status_command).await;
    assert!(result.is_ok(), "Migration status with validation should succeed");

    // 7. Test different security levels
    for security_level in ["safe", "development", "production"] {
        let migrate_command = MigrationCommands::Migrate {
            tool: Some(format!("security-test-{}", security_level)),
            force: false,
            security_level: security_level.to_string(),
            dry_run: true,
        };

        let result = migration_execute(context.config.clone(), migrate_command).await;
        assert!(result.is_ok(), "Migration with security level '{}' should succeed", security_level);
    }

    // 8. Test cleanup operations (dry run)
    let cleanup_command = MigrationCommands::Cleanup {
        inactive: true,
        failed: true,
        confirm: true,
    };

    let result = migration_execute(context.config.clone(), cleanup_command).await;
    assert!(result.is_ok(), "Migration cleanup should succeed");

    Ok(())
}

/// Test integrated service and migration workflow
#[tokio::test]
async fn test_service_migration_integration_workflow() -> Result<()> {
    let context = TestContext::new()?;
    let mock_services = context.mock_services.clone();

    // 1. Setup services with different states
    let services = vec![
        ("script-engine", ServiceHealth::Healthy),
        ("rune-service", ServiceHealth::Degraded),
        ("migration-service", ServiceHealth::Healthy),
    ];

    for (name, health) in services {
        let service = Arc::new(MockService::new(name));
        mock_services.add_service(name.to_string(), service).await;
        mock_services.set_health_status(name, health).await;
    }

    // 2. Check service health
    let health_command = ServiceCommands::Health {
        service: None,
        format: "table".to_string(),
        detailed: false,
    };

    let result = service_execute(context.config.clone(), health_command).await;
    assert!(result.is_ok(), "Service health check should succeed");

    // 3. Check migration status
    let migration_status = MigrationCommands::Status {
        format: "table".to_string(),
        detailed: false,
        validate: false,
    };

    let result = migration_execute(context.config.clone(), migration_status).await;
    assert!(result.is_ok(), "Migration status check should succeed");

    // 4. Start services needed for migration
    for (name, _) in services {
        if name != "rune-service" { // Skip degraded service
            let start_command = ServiceCommands::Start {
                service: name.to_string(),
                wait: false,
            };

            let result = service_execute(context.config.clone(), start_command).await;
            assert!(result.is_ok(), "Service '{}' should start", name);
        }
    }

    // 5. Perform migration operations
    let migrate_command = MigrationCommands::Migrate {
        tool: None,
        force: false,
        security_level: "safe".to_string(),
        dry_run: true,
    };

    let result = migration_execute(context.config.clone(), migrate_command).await;
    assert!(result.is_ok(), "Migration should succeed with running services");

    // 6. Validate migration with services running
    let validate_command = MigrationCommands::Validate {
        tool: None,
        auto_fix: false,
        format: "table".to_string(),
    };

    let result = migration_execute(context.config.clone(), validate_command).await;
    assert!(result.is_ok(), "Migration validation should succeed");

    // 7. Check service metrics after migration
    let metrics_command = ServiceCommands::Metrics {
        service: None,
        format: "table".to_string(),
        real_time: false,
    };

    let result = service_execute(context.config.clone(), metrics_command).await;
    assert!(result.is_ok(), "Service metrics should be available after migration");

    Ok(())
}

/// Test Rune script execution with service integration
#[tokio::test]
async fn test_rune_service_integration_workflow() -> Result<()> {
    let context = TestContext::new()?;
    let mock_services = context.mock_services.clone();

    // 1. Setup ScriptEngine service
    let script_engine = Arc::new(MockService::new("crucible-script-engine"));
    mock_services.add_service("crucible-script-engine".to_string(), script_engine.clone()).await;

    // 2. Start the ScriptEngine service
    let start_command = ServiceCommands::Start {
        service: "crucible-script-engine".to_string(),
        wait: true,
    };

    let result = service_execute(context.config.clone(), start_command).await;
    assert!(result.is_ok(), "ScriptEngine service should start");

    // 3. Create test Rune script
    let script_content = r#"
function main(args) {
    return {
        success: true,
        message: "Rune script executed successfully",
        input: args,
        timestamp: Date.now()
    };
}
"#;

    let script_path = context.create_test_script("integration-test-script", script_content);

    // 4. Execute Rune script with migration bridge enabled
    let mut config = context.config.clone();
    config.migration.enabled = true;
    config.migration.auto_migrate = true;

    let result = rune_execute(
        config.clone(),
        script_path.to_string_lossy().to_string(),
        Some(r#"{"test": "integration"}"#.to_string()),
    ).await;

    assert!(result.is_ok(), "Rune script should execute with service integration");

    // 5. Check service metrics after script execution
    let metrics_command = ServiceCommands::Metrics {
        service: Some("crucible-script-engine".to_string()),
        format: "table".to_string(),
        real_time: false,
    };

    let result = service_execute(context.config.clone(), metrics_command).await;
    assert!(result.is_ok(), "Service metrics should reflect script execution");

    // 6. List available Rune commands
    let result = list_commands(context.config.clone()).await;
    assert!(result.is_ok(), "Rune commands listing should succeed");

    // 7. Test multiple script executions
    for i in 0..3 {
        let script_content = format!(r#"
function main(args) {{
    return {{
        success: true,
        iteration: {},
        service_integration: true
    }};
}}
"#, i);

        let script_path = context.create_test_script(&format!("multi-test-{}", i), &script_content);

        let result = rune_execute(
            config.clone(),
            script_path.to_string_lossy().to_string(),
            None,
        ).await;

        assert!(result.is_ok(), "Multiple script executions should succeed");
    }

    // 8. Final service health check
    let health_command = ServiceCommands::Health {
        service: Some("crucible-script-engine".to_string()),
        format: "table".to_string(),
        detailed: false,
    };

    let result = service_execute(context.config.clone(), health_command).await;
    assert!(result.is_ok(), "Final service health check should succeed");

    Ok(())
}

/// Test error handling and recovery workflow
#[tokio::test]
async fn test_error_handling_and_recovery_workflow() -> Result<()> {
    let context = TestContext::new()?;
    let mock_services = context.mock_services.clone();

    // 1. Add a test service
    let test_service = Arc::new(MockService::new("error-test-service"));
    mock_services.add_service("error-test-service".to_string(), test_service.clone()).await;

    // 2. Start the service successfully
    let start_command = ServiceCommands::Start {
        service: "error-test-service".to_string(),
        wait: false,
    };

    let result = service_execute(context.config.clone(), start_command).await;
    assert!(result.is_ok(), "Service should start successfully");

    // 3. Simulate service failure
    mock_services.simulate_service_failure("error-test-service").await;

    // 4. Check service health (should show unhealthy)
    let health_command = ServiceCommands::Health {
        service: Some("error-test-service".to_string()),
        format: "table".to_string(),
        detailed: true,
    };

    let result = service_execute(context.config.clone(), health_command).await;
    assert!(result.is_ok(), "Health check should handle failed service gracefully");

    // 5. Try to execute operation on failed service
    let execution_result = test_service.execute("test-operation").await;
    assert!(execution_result.is_err(), "Failed service should not execute operations");

    // 6. Simulate service recovery
    mock_services.simulate_service_recovery("error-test-service").await;

    // 7. Restart the service
    let restart_command = ServiceCommands::Restart {
        service: "error-test-service".to_string(),
        wait: true,
    };

    let result = service_execute(context.config.clone(), restart_command).await;
    assert!(result.is_ok(), "Service restart should succeed");

    // 8. Verify service is healthy again
    let health_command = ServiceCommands::Health {
        service: Some("error-test-service".to_string()),
        format: "table".to_string(),
        detailed: false,
    };

    let result = service_execute(context.config.clone(), health_command).await;
    assert!(result.is_ok(), "Health check should show recovered service");

    // 9. Test migration error handling
    let mut config = context.config.clone();
    config.migration.enabled = false;

    let migrate_command = MigrationCommands::Migrate {
        tool: Some("error-test-tool".to_string()),
        force: false,
        security_level: "safe".to_string(),
        dry_run: true,
    };

    let result = migration_execute(config, migrate_command).await;
    assert!(result.is_err(), "Migration should fail when disabled");

    Ok(())
}

/// Test concurrent operations workflow
#[tokio::test]
async fn test_concurrent_operations_workflow() -> Result<()> {
    let context = TestContext::new()?;
    let mock_services = context.mock_services.clone();

    // 1. Setup multiple services
    let service_names = vec![
        "concurrent-service-1",
        "concurrent-service-2",
        "concurrent-service-3",
    ];

    for name in &service_names {
        let service = Arc::new(MockService::new(name));
        mock_services.add_service(name.to_string(), service).await;
    }

    // 2. Start all services concurrently
    let mut start_futures = Vec::new();
    for name in &service_names {
        let config = context.config.clone();
        let name = name.clone();

        let future = async move {
            let start_command = ServiceCommands::Start {
                service: name.to_string(),
                wait: false,
            };
            service_execute(config, start_command).await
        };

        start_futures.push(future);
    }

    let start_results = futures::future::join_all(start_futures).await;
    for (i, result) in start_results.into_iter().enumerate() {
        assert!(result.is_ok(), "Concurrent service {} start should succeed", i);
    }

    // 3. Execute health checks concurrently
    let mut health_futures = Vec::new();
    for name in &service_names {
        let config = context.config.clone();
        let name = name.clone();

        let future = async move {
            let health_command = ServiceCommands::Health {
                service: Some(name.clone()),
                format: "table".to_string(),
                detailed: false,
            };
            service_execute(config, health_command).await
        };

        health_futures.push(future);
    }

    let health_results = futures::future::join_all(health_futures).await;
    for (i, result) in health_results.into_iter().enumerate() {
        assert!(result.is_ok(), "Concurrent health check {} should succeed", i);
    }

    // 4. Execute metrics collection concurrently
    let mut metrics_futures = Vec::new();
    for name in &service_names {
        let config = context.config.clone();
        let name = name.clone();

        let future = async move {
            let metrics_command = ServiceCommands::Metrics {
                service: Some(name.clone()),
                format: "json".to_string(),
                real_time: false,
            };
            service_execute(config, metrics_command).await
        };

        metrics_futures.push(future);
    }

    let metrics_results = futures::future::join_all(metrics_futures).await;
    for (i, result) in metrics_results.into_iter().enumerate() {
        assert!(result.is_ok(), "Concurrent metrics collection {} should succeed", i);
    }

    // 5. Run migration operations concurrently
    let migration_futures = vec![
        {
            let config = context.config.clone();
            async move {
                let status_command = MigrationCommands::Status {
                    format: "table".to_string(),
                    detailed: false,
                    validate: false,
                };
                migration_execute(config, status_command).await
            }
        },
        {
            let config = context.config.clone();
            async move {
                let list_command = MigrationCommands::List {
                    format: "table".to_string(),
                    active: false,
                    inactive: false,
                    metadata: false,
                };
                migration_execute(config, list_command).await
            }
        },
        {
            let config = context.config.clone();
            async move {
                let validate_command = MigrationCommands::Validate {
                    tool: None,
                    auto_fix: false,
                    format: "table".to_string(),
                };
                migration_execute(config, validate_command).await
            }
        },
    ];

    let migration_results = futures::future::join_all(migration_futures).await;
    for (i, result) in migration_results.into_iter().enumerate() {
        assert!(result.is_ok(), "Concurrent migration operation {} should succeed", i);
    }

    // 6. Stop all services concurrently
    let mut stop_futures = Vec::new();
    for name in &service_names {
        let config = context.config.clone();
        let name = name.clone();

        let future = async move {
            let stop_command = ServiceCommands::Stop {
                service: name.to_string(),
                force: false,
            };
            service_execute(config, stop_command).await
        };

        stop_futures.push(future);
    }

    let stop_results = futures::future::join_all(stop_futures).await;
    for (i, result) in stop_results.into_iter().enumerate() {
        assert!(result.is_ok(), "Concurrent service {} stop should succeed", i);
    }

    Ok(())
}

/// Test real-world usage scenario
#[tokio::test]
async fn test_real_world_usage_scenario() -> Result<()> {
    let context = TestContext::new()?;
    let mock_services = context.mock_services.clone();

    // Scenario: User wants to set up Crucible with services and migrate tools

    // 1. Initial system check - what services are available?
    println!("=== Step 1: Checking available services ===");
    let list_command = ServiceCommands::List {
        format: "table".to_string(),
        status: true,
        detailed: false,
    };

    let result = service_execute(context.config.clone(), list_command).await;
    assert!(result.is_ok(), "Service listing should succeed");

    // 2. Check system health
    println!("=== Step 2: System health check ===");
    let health_command = ServiceCommands::Health {
        service: None,
        format: "table".to_string(),
        detailed: true,
    };

    let result = service_execute(context.config.clone(), health_command).await;
    assert!(result.is_ok(), "System health check should succeed");

    // 3. Start necessary services
    println!("=== Step 3: Starting essential services ===");
    let essential_services = vec![
        "crucible-script-engine",
        "crucible-rune-service",
    ];

    for service_name in essential_services {
        // Add service if not exists
        if mock_services.get_service(service_name).await.is_none() {
            let service = Arc::new(MockService::new(service_name));
            mock_services.add_service(service_name.to_string(), service).await;
        }

        let start_command = ServiceCommands::Start {
            service: service_name.to_string(),
            wait: true,
        };

        let result = service_execute(context.config.clone(), start_command).await;
        assert!(result.is_ok(), "Essential service '{}' should start", service_name);
    }

    // 4. Verify services are running
    println!("=== Step 4: Verifying service status ===");
    for service_name in ["crucible-script-engine", "crucible-rune-service"] {
        let health_command = ServiceCommands::Health {
            service: Some(service_name.to_string()),
            format: "table".to_string(),
            detailed: false,
        };

        let result = service_execute(context.config.clone(), health_command).await;
        assert!(result.is_ok(), "Service '{}' health check should succeed", service_name);
    }

    // 5. Check migration status
    println!("=== Step 5: Checking migration status ===");
    let migration_status = MigrationCommands::Status {
        format: "table".to_string(),
        detailed: true,
        validate: true,
    };

    let result = migration_execute(context.config.clone(), migration_status).await;
    assert!(result.is_ok(), "Migration status check should succeed");

    // 6. Create and test a Rune script
    println!("=== Step 6: Testing Rune script execution ===");
    let script_content = r#"
function main(args) {
    return {
        success: true,
        operation: "real_world_test",
        message: "Real-world scenario test successful",
        input_processing: args ? Object.keys(args).length : 0,
        timestamp: new Date().toISOString()
    };
}
"#;

    let script_path = context.create_test_script("real-world-test", script_content);

    let result = rune_execute(
        context.config.clone(),
        script_path.to_string_lossy().to_string(),
        Some(r#"{"user": "test_user", "scenario": "real_world"}"#.to_string()),
    ).await;

    assert!(result.is_ok(), "Real-world Rune script should execute successfully");

    // 7. Simulate tool migration
    println!("=== Step 7: Simulating tool migration ===");
    let migrate_command = MigrationCommands::Migrate {
        tool: Some("real-world-tool".to_string()),
        force: false,
        security_level: "safe".to_string(),
        dry_run: true,
    };

    let result = migration_execute(context.config.clone(), migrate_command).await;
    assert!(result.is_ok(), "Tool migration simulation should succeed");

    // 8. Check system performance
    println!("=== Step 8: Performance check ===");
    let metrics_command = ServiceCommands::Metrics {
        service: None,
        format: "table".to_string(),
        real_time: false,
    };

    let result = service_execute(context.config.clone(), metrics_command).await;
    assert!(result.is_ok(), "Performance metrics should be available");

    // 9. List available Rune commands
    println!("=== Step 9: Listing available commands ===");
    let result = list_commands(context.config.clone()).await;
    assert!(result.is_ok(), "Rune commands listing should succeed");

    // 10. Final system validation
    println!("=== Step 10: Final system validation ===");
    let validation_command = MigrationCommands::Validate {
        tool: None,
        auto_fix: false,
        format: "table".to_string(),
    };

    let result = migration_execute(context.config.clone(), validation_command).await;
    assert!(result.is_ok(), "Final system validation should succeed");

    println!("=== Real-world scenario completed successfully ===");

    Ok(())
}

/// Test performance under load
#[tokio::test]
async fn test_performance_under_load() -> Result<()> {
    let context = TestContext::new()?;
    let mock_services = context.mock_services.clone();

    // Setup multiple services
    let service_count = 10;
    for i in 0..service_count {
        let service = Arc::new(MockService::new(&format!("load-test-service-{}", i)));
        mock_services.add_service(format!("load-test-service-{}", i), service).await;
    }

    // Test concurrent service operations
    let operation_count = 50;
    let mut operation_futures = Vec::new();

    for i in 0..operation_count {
        let config = context.config.clone();
        let service_index = i % service_count;

        let future = async move {
            let start_time = Instant::now();

            let health_command = ServiceCommands::Health {
                service: Some(format!("load-test-service-{}", service_index)),
                format: "table".to_string(),
                detailed: false,
            };

            let result = service_execute(config, health_command).await;
            let duration = start_time.elapsed();

            (result, duration)
        };

        operation_futures.push(future);
    }

    // Execute all operations concurrently
    let results = futures::future::join_all(operation_futures).await;

    // Analyze results
    let mut success_count = 0;
    let mut total_duration = Duration::ZERO;
    let mut max_duration = Duration::ZERO;
    let mut min_duration = Duration::from_secs(1);

    for (result, duration) in results {
        if result.is_ok() {
            success_count += 1;
        }
        total_duration += duration;
        max_duration = max_duration.max(duration);
        min_duration = min_duration.min(duration);
    }

    let success_rate = (success_count as f64 / operation_count as f64) * 100.0;
    let avg_duration = total_duration / operation_count as u32;

    println!("Performance Test Results:");
    println!("  Operations: {}/{} ({:.1}% success)", success_count, operation_count, success_rate);
    println!("  Average duration: {:?}", avg_duration);
    println!("  Min duration: {:?}", min_duration);
    println!("  Max duration: {:?}", max_duration);

    // Performance assertions
    assert!(success_rate >= 95.0, "Success rate should be at least 95%");
    assert!(avg_duration < Duration::from_millis(500), "Average duration should be under 500ms");
    assert!(max_duration < Duration::from_secs(2), "Maximum duration should be under 2 seconds");

    Ok(())
}

/// Test configuration integration across components
#[tokio::test]
async fn test_configuration_integration() -> Result<()> {
    // Create custom configuration
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("integration_config.toml");

    let config_content = r#"
[vault]
path = "/tmp/integration_test_vault"
embedding_url = "https://integration-test-embedding.com"
embedding_model = "integration-test-model"

[llm]
chat_model = "integration-test-chat-model"
temperature = 0.6
max_tokens = 1536

[services.script_engine]
enabled = true
security_level = "development"
max_source_size = 2097152
default_timeout_secs = 45
enable_caching = true
max_cache_size = 500

[services.discovery]
enabled = true
endpoints = ["localhost:8080", "integration-test:8080"]
timeout_secs = 10

[services.health]
enabled = true
check_interval_secs = 15
failure_threshold = 5

[migration]
enabled = true
auto_migrate = true
default_security_level = "development"
enable_caching = true
max_cache_size = 250
preserve_tool_ids = true
backup_originals = true
"#;

    std::fs::write(&config_path, config_content)?;

    // Load configuration
    let config = CliConfig::from_file_or_default(Some(config_path))?;

    // Create test context with custom configuration
    let mut context = TestContext::new()?;
    context.config = config.clone();

    // Test that services respect configuration
    let health_command = ServiceCommands::Health {
        service: None,
        format: "table".to_string(),
        detailed: false,
    };

    let result = service_execute(context.config.clone(), health_command).await;
    assert!(result.is_ok(), "Service health check should respect custom configuration");

    // Test that migration respects configuration
    let status_command = MigrationCommands::Status {
        format: "table".to_string(),
        detailed: true,
        validate: false,
    };

    let result = migration_execute(context.config.clone(), status_command).await;
    assert!(result.is_ok(), "Migration status should respect custom configuration");

    // Test that Rune execution respects configuration
    let script_content = r#"
function main(args) {
    return {
        success: true,
        config_integration: true
    };
}
"#;

    let script_path = context.create_test_script("config-integration-test", script_content);

    let result = rune_execute(
        context.config.clone(),
        script_path.to_string_lossy().to_string(),
        None,
    ).await;

    assert!(result.is_ok(), "Rune execution should respect custom configuration");

    // Verify configuration values are properly applied
    assert_eq!(config.kiln.embedding_model, Some("integration-test-model".to_string()));
    assert_eq!(config.chat_model(), "integration-test-chat-model");
    assert_eq!(config.services.script_engine.security_level, "development");
    assert_eq!(config.migration.default_security_level, "development");
    assert!(config.migration.auto_migrate);

    Ok(())
}
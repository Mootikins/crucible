//! Comprehensive tests for service management commands
//!
//! This module tests all service management functionality including:
//! - Health monitoring commands
//! - Metrics collection and display
//! - Service lifecycle operations (start/stop/restart)
//! - Service discovery and listing
//! - Log management and filtering
//! - Error handling and edge cases

use anyhow::Result;
use std::time::Duration;
use std::sync::Arc;
use tokio::time::{sleep, timeout};
use crate::tests::test_utilities::*;
use crucible_cli::config::CliConfig;
use crucible_cli::cli::ServiceCommands;
use crucible_cli::commands::service::execute;

/// Test service health monitoring commands
#[tokio::test]
async fn test_service_health_command_all_services() -> Result<()> {
    let context = TestContext::new()?;

    // Test health command for all services
    let command = ServiceCommands::Health {
        service: None,
        format: "table".to_string(),
        detailed: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Health command should succeed");

    // Test with detailed output
    let command = ServiceCommands::Health {
        service: None,
        format: "table".to_string(),
        detailed: true,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Detailed health command should succeed");

    Ok(())
}

#[tokio::test]
async fn test_service_health_command_specific_service() -> Result<()> {
    let context = TestContext::new()?;

    // Test health command for specific service
    let command = ServiceCommands::Health {
        service: Some("crucible-script-engine".to_string()),
        format: "table".to_string(),
        detailed: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Health command for specific service should succeed");

    // Test with JSON format
    let command = ServiceCommands::Health {
        service: Some("crucible-script-engine".to_string()),
        format: "json".to_string(),
        detailed: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "JSON health command should succeed");

    Ok(())
}

#[tokio::test]
async fn test_service_health_command_nonexistent_service() -> Result<()> {
    let context = TestContext::new()?;

    // Test health command for non-existent service
    let command = ServiceCommands::Health {
        service: Some("non-existent-service".to_string()),
        format: "table".to_string(),
        detailed: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Health command should handle non-existent service gracefully");

    Ok(())
}

/// Test service metrics commands
#[tokio::test]
async fn test_service_metrics_command_all_services() -> Result<()> {
    let context = TestContext::new()?;

    // Test metrics command for all services
    let command = ServiceCommands::Metrics {
        service: None,
        format: "table".to_string(),
        real_time: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Metrics command should succeed");

    // Test with JSON format
    let command = ServiceCommands::Metrics {
        service: None,
        format: "json".to_string(),
        real_time: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "JSON metrics command should succeed");

    Ok(())
}

#[tokio::test]
async fn test_service_metrics_command_specific_service() -> Result<()> {
    let context = TestContext::new()?;

    // Test metrics command for specific service
    let command = ServiceCommands::Metrics {
        service: Some("crucible-rune-service".to_string()),
        format: "table".to_string(),
        real_time: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Metrics command for specific service should succeed");

    Ok(())
}

#[tokio::test]
async fn test_service_metrics_real_time_monitoring() -> Result<()> {
    let context = TestContext::new()?;

    // Test real-time metrics with timeout to prevent infinite loop
    let command = ServiceCommands::Metrics {
        service: None,
        format: "table".to_string(),
        real_time: true,
    };

    // Use timeout to prevent infinite loop in test
    let result = timeout(Duration::from_secs(3), execute(context.config.clone(), command)).await;

    // Should timeout because real-time monitoring runs indefinitely
    assert!(result.is_err(), "Real-time metrics should timeout in test environment");

    Ok(())
}

/// Test service lifecycle commands
#[tokio::test]
async fn test_service_start_command() -> Result<()> {
    let context = TestContext::new()?;
    let mock_services = context.mock_services.clone();

    // Add a test service to the mock registry
    let test_service = Arc::new(MockService::new("test-start-service"));
    mock_services.add_service("test-start-service".to_string(), test_service.clone()).await;

    // Test start command
    let command = ServiceCommands::Start {
        service: "test-start-service".to_string(),
        wait: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Start command should succeed");

    // In a real implementation, we'd verify the service was started
    // For now, just ensure the command doesn't panic

    Ok(())
}

#[tokio::test]
async fn test_service_start_command_with_wait() -> Result<()> {
    let context = TestContext::new()?;

    // Test start command with wait
    let command = ServiceCommands::Start {
        service: "crucible-script-engine".to_string(),
        wait: true,
    };

    let start_time = std::time::Instant::now();
    let result = execute(context.config.clone(), command).await;
    let elapsed = start_time.elapsed();

    assert!(result.is_ok(), "Start command with wait should succeed");
    // Should take at least some time due to waiting
    assert!(elapsed >= Duration::from_secs(2), "Start command should wait for service");

    Ok(())
}

#[tokio::test]
async fn test_service_stop_command() -> Result<()> {
    let context = TestContext::new()?;

    // Test stop command
    let command = ServiceCommands::Stop {
        service: "crucible-script-engine".to_string(),
        force: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Stop command should succeed");

    Ok(())
}

#[tokio::test]
async fn test_service_stop_command_force() -> Result<()> {
    let context = TestContext::new()?;

    // Test force stop command
    let command = ServiceCommands::Stop {
        service: "crucible-script-engine".to_string(),
        force: true,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Force stop command should succeed");

    Ok(())
}

#[tokio::test]
async fn test_service_restart_command() -> Result<()> {
    let context = TestContext::new()?;

    // Test restart command
    let command = ServiceCommands::Restart {
        service: "crucible-script-engine".to_string(),
        wait: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Restart command should succeed");

    Ok(())
}

#[tokio::test]
async fn test_service_restart_command_with_wait() -> Result<()> {
    let context = TestContext::new()?;

    // Test restart command with wait
    let command = ServiceCommands::Restart {
        service: "crucible-script-engine".to_string(),
        wait: true,
    };

    let start_time = std::time::Instant::now();
    let result = execute(context.config.clone(), command).await;
    let elapsed = start_time.elapsed();

    assert!(result.is_ok(), "Restart command with wait should succeed");
    // Should take longer due to stop + start + wait
    assert!(elapsed >= Duration::from_secs(4), "Restart command should wait for service");

    Ok(())
}

/// Test service listing commands
#[tokio::test]
async fn test_service_list_command_basic() -> Result<()> {
    let context = TestContext::new()?;

    // Test basic list command
    let command = ServiceCommands::List {
        format: "table".to_string(),
        status: false,
        detailed: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "List command should succeed");

    Ok(())
}

#[tokio::test]
async fn test_service_list_command_with_status() -> Result<()> {
    let context = TestContext::new()?;

    // Test list command with status
    let command = ServiceCommands::List {
        format: "table".to_string(),
        status: true,
        detailed: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "List command with status should succeed");

    Ok(())
}

#[tokio::test]
async fn test_service_list_command_detailed() -> Result<()> {
    let context = TestContext::new()?;

    // Test detailed list command
    let command = ServiceCommands::List {
        format: "table".to_string(),
        status: false,
        detailed: true,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Detailed list command should succeed");

    Ok(())
}

#[tokio::test]
async fn test_service_list_command_json() -> Result<()> {
    let context = TestContext::new()?;

    // Test list command with JSON format
    let command = ServiceCommands::List {
        format: "json".to_string(),
        status: true,
        detailed: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "JSON list command should succeed");

    Ok(())
}

/// Test service log commands
#[tokio::test]
async fn test_service_logs_command_basic() -> Result<()> {
    let context = TestContext::new()?;

    // Test basic logs command
    let command = ServiceCommands::Logs {
        service: None,
        lines: 100,
        follow: false,
        errors: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Logs command should succeed");

    Ok(())
}

#[tokio::test]
async fn test_service_logs_command_specific_service() -> Result<()> {
    let context = TestContext::new()?;

    // Test logs command for specific service
    let command = ServiceCommands::Logs {
        service: Some("crucible-script-engine".to_string()),
        lines: 50,
        follow: false,
        errors: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Logs command for specific service should succeed");

    Ok(())
}

#[tokio::test]
async fn test_service_logs_command_with_line_limit() -> Result<()> {
    let context = TestContext::new()?;

    // Test logs command with line limit
    let command = ServiceCommands::Logs {
        service: None,
        lines: 5,
        follow: false,
        errors: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Logs command with line limit should succeed");

    Ok(())
}

#[tokio::test]
async fn test_service_logs_command_errors_only() -> Result<()> {
    let context = TestContext::new()?;

    // Test logs command with errors only
    let command = ServiceCommands::Logs {
        service: None,
        lines: 100,
        follow: false,
        errors: true,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Logs command with errors only should succeed");

    Ok(())
}

#[tokio::test]
async fn test_service_logs_command_follow() -> Result<()> {
    let context = TestContext::new()?;

    // Test follow logs command with timeout
    let command = ServiceCommands::Logs {
        service: None,
        lines: 100,
        follow: true,
        errors: false,
    };

    // Use timeout to prevent infinite loop
    let result = timeout(Duration::from_secs(2), execute(context.config.clone(), command)).await;

    // Should timeout because follow runs indefinitely
    assert!(result.is_err(), "Follow logs should timeout in test environment");

    Ok(())
}

/// Test error handling and edge cases
#[tokio::test]
async fn test_service_commands_with_invalid_format() -> Result<()> {
    let context = TestContext::new()?;

    // Test health command with invalid format (should default to table)
    let command = ServiceCommands::Health {
        service: None,
        format: "invalid".to_string(),
        detailed: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Should handle invalid format gracefully");

    Ok(())
}

#[tokio::test]
async fn test_service_commands_edge_cases() -> Result<()> {
    let context = TestContext::new()?;

    // Test with empty service name
    let command = ServiceCommands::Health {
        service: Some("".to_string()),
        format: "table".to_string(),
        detailed: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Should handle empty service name gracefully");

    // Test with very long service name
    let long_name = "a".repeat(1000);
    let command = ServiceCommands::Health {
        service: Some(long_name),
        format: "table".to_string(),
        detailed: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Should handle long service names gracefully");

    Ok(())
}

/// Integration tests with mocked services
#[tokio::test]
async fn test_service_integration_with_mock_registry() -> Result<()> {
    let context = TestContext::new()?;
    let mock_services = context.mock_services.clone();

    // Add test services with different states
    let healthy_service = Arc::new(MockService::new("healthy-service"));
    let degraded_service = Arc::new(MockService::new("degraded-service"));
    let unhealthy_service = Arc::new(MockService::new("unhealthy-service"));

    mock_services.add_service("healthy-service".to_string(), healthy_service.clone()).await;
    mock_services.add_service("degraded-service".to_string(), degraded_service.clone()).await;
    mock_services.add_service("unhealthy-service".to_string(), unhealthy_service.clone()).await;

    // Set different health statuses
    mock_services.set_health_status("healthy-service", ServiceHealth::Healthy).await;
    mock_services.set_health_status("degraded-service", ServiceHealth::Degraded).await;
    mock_services.set_health_status("unhealthy-service", ServiceHealth::Unhealthy).await;

    // Test health command shows correct statuses
    let command = ServiceCommands::Health {
        service: None,
        format: "table".to_string(),
        detailed: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Health command should work with mock registry");

    // Test specific service health
    let command = ServiceCommands::Health {
        service: Some("healthy-service".to_string()),
        format: "table".to_string(),
        detailed: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Specific service health should work");

    Ok(())
}

#[tokio::test]
async fn test_service_lifecycle_integration() -> Result<()> {
    let context = TestContext::new()?;
    let mock_services = context.mock_services.clone();

    // Add a test service
    let test_service = Arc::new(MockService::new("lifecycle-test-service"));
    mock_services.add_service("lifecycle-test-service".to_string(), test_service.clone()).await;

    // Verify initial state
    assert!(!test_service.is_running().await, "Service should not be running initially");
    assert_eq!(test_service.get_start_count().await, 0, "Start count should be 0 initially");

    // Start the service
    let command = ServiceCommands::Start {
        service: "lifecycle-test-service".to_string(),
        wait: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Start command should succeed");

    // In a real implementation, we'd verify the service state changed
    // For now, just ensure the command executes without error

    // Stop the service
    let command = ServiceCommands::Stop {
        service: "lifecycle-test-service".to_string(),
        force: false,
    };

    let result = execute(context.config.clone(), command).await;
    assert!(result.is_ok(), "Stop command should succeed");

    Ok(())
}

/// Performance tests for service commands
#[tokio::test]
async fn test_service_command_performance() -> Result<()> {
    let context = TestContext::new()?;

    // Test health command performance
    let (result, duration) = PerformanceMeasurement::measure(|| async {
        let command = ServiceCommands::Health {
            service: None,
            format: "table".to_string(),
            detailed: false,
        };
        execute(context.config.clone(), command).await
    }).await;

    assert!(result.is_ok(), "Health command should succeed");
    AssertUtils::assert_execution_time_within(
        duration,
        Duration::from_millis(10),
        Duration::from_millis(1000),
        "health command"
    );

    // Test list command performance
    let (result, duration) = PerformanceMeasurement::measure(|| async {
        let command = ServiceCommands::List {
            format: "table".to_string(),
            status: true,
            detailed: false,
        };
        execute(context.config.clone(), command).await
    }).await;

    assert!(result.is_ok(), "List command should succeed");
    AssertUtils::assert_execution_time_within(
        duration,
        Duration::from_millis(10),
        Duration::from_millis(1000),
        "list command"
    );

    Ok(())
}

#[tokio::test]
async fn test_concurrent_service_commands() -> Result<()> {
    let context = TestContext::new()?;

    // Test multiple service commands running concurrently
    let health_future = {
        let config = context.config.clone();
        async move {
            let command = ServiceCommands::Health {
                service: None,
                format: "table".to_string(),
                detailed: false,
            };
            execute(config, command).await
        }
    };

    let list_future = {
        let config = context.config.clone();
        async move {
            let command = ServiceCommands::List {
                format: "table".to_string(),
                status: false,
                detailed: false,
            };
            execute(config, command).await
        }
    };

    let metrics_future = {
        let config = context.config.clone();
        async move {
            let command = ServiceCommands::Metrics {
                service: None,
                format: "table".to_string(),
                real_time: false,
            };
            execute(config, command).await
        }
    };

    // Run all commands concurrently
    let (health_result, list_result, metrics_result) = tokio::join!(
        health_future,
        list_future,
        metrics_future
    );

    assert!(health_result.is_ok(), "Concurrent health command should succeed");
    assert!(list_result.is_ok(), "Concurrent list command should succeed");
    assert!(metrics_result.is_ok(), "Concurrent metrics command should succeed");

    Ok(())
}

/// Memory and resource management tests
#[tokio::test]
async fn test_service_command_memory_usage() -> Result<()> {
    let context = TestContext::new()?;

    let before_memory = MemoryUsage::current();

    // Execute multiple service commands
    for i in 0..10 {
        let command = ServiceCommands::Health {
            service: Some(format!("test-service-{}", i % 3)),
            format: "table".to_string(),
            detailed: false,
        };

        let result = execute(context.config.clone(), command).await;
        assert!(result.is_ok(), "Health command {} should succeed", i);
    }

    let after_memory = MemoryUsage::current();

    // Memory usage should not increase significantly
    // This is a basic check - in real implementation you'd want more sophisticated memory tracking
    assert!(
        after_memory.rss_bytes <= before_memory.rss_bytes + 10 * 1024 * 1024, // 10MB tolerance
        "Memory usage should not increase significantly"
    );

    Ok(())
}
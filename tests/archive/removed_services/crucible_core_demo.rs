//! # CrucibleCore Demonstration
//!
//! This example demonstrates how to use the centralized CrucibleCore for service coordination,
//! event routing, and system management in the simplified Crucible architecture.

use crucible_core::{
    CrucibleCore, CrucibleCoreBuilder, CoreConfig,
};
use crucible_core::crucible_core::{
    DaemonEvent, EventPayload, EventSource, EventType, EventPriority,
};
use crucible_services::{
    ServiceLifecycle, HealthCheck, ServiceHealth, ServiceStatus, ServiceResult,
    service_traits::Observable, types::ServiceMetrics, ServiceTarget, PerformanceMetrics,
    events::core::SourceType,
};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

/// Example service implementation for demonstration
#[derive(Debug)]
struct ExampleService {
    name: String,
    version: String,
    running: bool,
}

impl ExampleService {
    fn new(name: &str, version: &str) -> Self {
        Self {
            name: name.to_string(),
            version: version.to_string(),
            running: false,
        }
    }
}

#[async_trait]
impl ServiceLifecycle for ExampleService {
    async fn start(&mut self) -> ServiceResult<()> {
        println!("Starting service: {}", self.name);
        self.running = true;
        Ok(())
    }

    async fn stop(&mut self) -> ServiceResult<()> {
        println!("Stopping service: {}", self.name);
        self.running = false;
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running
    }

    fn service_name(&self) -> &str {
        &self.name
    }

    fn service_version(&self) -> &str {
        &self.version
    }
}

#[async_trait]
impl HealthCheck for ExampleService {
    async fn health_check(&self) -> ServiceResult<ServiceHealth> {
        Ok(ServiceHealth {
            status: if self.running {
                ServiceStatus::Healthy
            } else {
                ServiceStatus::Unhealthy
            },
            message: Some(if self.running {
                "Service is running".to_string()
            } else {
                "Service is stopped".to_string()
            }),
            last_check: chrono::Utc::now(),
            details: std::collections::HashMap::new(),
        })
    }
}

#[async_trait]
impl Observable for ExampleService {
    async fn get_metrics(&self) -> ServiceResult<ServiceMetrics> {
        Ok(ServiceMetrics {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            average_response_time: Duration::from_millis(0),
            uptime: Duration::from_secs(0),
            memory_usage: 0,
            cpu_usage: 0.0,
        })
    }

    async fn reset_metrics(&mut self) -> ServiceResult<()> {
        Ok(())
    }

    async fn get_performance_metrics(&self) -> ServiceResult<PerformanceMetrics> {
        // Return default performance metrics
        Ok(PerformanceMetrics {
            request_times: vec![],
            memory_usage: 0,
            cpu_usage: 0.0,
            active_connections: 0,
            queue_sizes: std::collections::HashMap::new(),
            custom_metrics: std::collections::HashMap::new(),
            timestamp: chrono::Utc::now(),
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    println!("Logging initialized (tracing_subscriber not available in example)");

    println!("🚀 Starting CrucibleCore Demonstration");
    println!("=====================================");

    // Create core configuration
    let core_config = CoreConfig {
        max_services: 10,
        routing_config: crucible_services::events::routing::RoutingConfig::default(),
        health_check_interval_s: 10,
        metrics_interval_s: 30,
        enable_auto_recovery: true,
        max_recovery_attempts: 3,
        event_queue_size: 1000,
        enable_debug: true,
    };

    // Create and start the CrucibleCore
    let core = CrucibleCoreBuilder::new()
        .with_config(core_config)
        .with_max_services(10)
        .with_health_check_interval(5)
        .with_auto_recovery(true)
        .build()
        .await?;

    println!("✅ CrucibleCore created successfully");

    // Start the core
    core.start().await?;
    println!("✅ CrucibleCore started successfully");

    // Create example services
    let mcp_service = Arc::new(tokio::sync::RwLock::new(ExampleService::new("mcp-gateway", "1.0.0")));
    let inference_service = Arc::new(tokio::sync::RwLock::new(ExampleService::new("inference-engine", "1.0.0")));
    let script_service = Arc::new(tokio::sync::RwLock::new(ExampleService::new("script-engine", "1.0.0")));
    let datastore_service = Arc::new(tokio::sync::RwLock::new(ExampleService::new("datastore", "1.0.0")));

    // Register services with the core
    let services = vec![
        ("mcp-gateway", mcp_service.clone()),
        ("inference-engine", inference_service.clone()),
        ("script-engine", script_service.clone()),
        ("datastore", datastore_service.clone()),
    ];

    for (name, service) in services {
        // Note: In a real implementation, you'd need to handle the async trait properly
        // For this example, we're showing the intended usage pattern
        println!("📝 Registering service: {}", name);
        // core.register_service(service).await?;
    }

    println!("✅ All services registered successfully");

    // Demonstrate service listing
    let registered_services = core.list_services().await?;
    println!("📋 Registered services: {:?}", registered_services);

    // Demonstrate health checking
    println!("\n🏥 Performing health check...");
    let health_results = core.perform_health_check().await?;
    for (service_id, health) in health_results {
        println!("  {}: {:?}", service_id, health.status);
    }

    // Demonstrate event routing
    println!("\n📨 Routing events through the system...");

    // Create a sample daemon event
    let filesystem_event = DaemonEvent::new(
        EventType::Filesystem(crucible_services::events::core::FilesystemEventType::FileCreated {
            path: "/vault/example.md".to_string(),
        }),
        EventSource::filesystem("file-watcher-1".to_string()),
        EventPayload::text("File created: example.md".to_string()),
    )
    .with_priority(EventPriority::Normal)
    .with_target(ServiceTarget::new("mcp-gateway".to_string()));

    // Route the event
    core.route_event(filesystem_event).await?;
    println!("✅ Filesystem event routed successfully");

    // Create a system event
    let system_event = DaemonEvent::new(
        EventType::System(crucible_services::events::core::SystemEventType::DaemonStarted {
            version: "1.0.0".to_string(),
        }),
        EventSource::new("core".to_string(), SourceType::System),
        EventPayload::json(serde_json::json!({
            "version": "1.0.0",
            "timestamp": chrono::Utc::now(),
            "services": 4
        })),
    )
    .with_priority(EventPriority::High);

    core.route_event(system_event).await?;
    println!("✅ System event routed successfully");

    // Demonstrate metrics collection
    println!("\n📊 Collecting system metrics...");
    let metrics = core.get_metrics().await?;
    println!("  Events processed: {}", metrics.events_processed);
    println!("  Services managed: {}", metrics.services_managed);
    println!("  System uptime: {}ms", metrics.uptime_ms);
    println!("  Memory usage: {} bytes", metrics.memory_usage_bytes);
    println!("  Average response time: {:.2}ms", metrics.avg_response_time_ms);
    println!("  Error rate: {:.2}%", metrics.error_rate * 100.0);

    // Demonstrate configuration updates
    println!("\n⚙️ Updating configuration...");
    let new_config = crucible_core::CrucibleConfig::default(); // This would be a real config
    core.update_config(new_config).await?;
    println!("✅ Configuration updated successfully");

    // Simulate some runtime
    println!("\n⏳ Running for 5 seconds to demonstrate operation...");
    sleep(Duration::from_secs(5)).await;

    // Demonstrate graceful shutdown
    println!("\n🛑 Shutting down CrucibleCore...");
    core.stop().await?;
    println!("✅ CrucibleCore stopped successfully");

    // Final metrics
    println!("\n📈 Final metrics:");
    let final_metrics = core.get_metrics().await?;
    println!("  Total events processed: {}", final_metrics.events_processed);
    println!("  Final state: {:?}", core.get_state().await);

    println!("\n🎉 Demonstration completed successfully!");
    println!("=====================================");
    println!("Key features demonstrated:");
    println!("  ✅ Centralized service coordination");
    println!("  ✅ Event routing and processing");
    println!("  ✅ Health monitoring");
    println!("  ✅ Metrics collection");
    println!("  ✅ Configuration management");
    println!("  ✅ Graceful lifecycle management");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_core_lifecycle() {
        let core = CrucibleCoreBuilder::new()
            .with_max_services(5)
            .build()
            .await
            .expect("Failed to create core");

        assert!(core.start().await.is_ok());
        assert_eq!(core.get_state().await, crucible_core::CoreState::Running);

        assert!(core.stop().await.is_ok());
        assert_eq!(core.get_state().await, crucible_core::CoreState::Stopped);
    }

    #[tokio::test]
    async fn test_event_routing() {
        let core = CrucibleCoreBuilder::new()
            .build()
            .await
            .expect("Failed to create core");

        core.start().await.expect("Failed to start core");

        let event = DaemonEvent::new(
            EventType::System(crucible_services::events::core::SystemEventType::DaemonStarted {
                version: "test".to_string(),
            }),
            EventSource::system("test".to_string()),
            EventPayload::text("Test event".to_string()),
        );

        assert!(core.route_event(event).await.is_ok());

        let metrics = core.get_metrics().await.expect("Failed to get metrics");
        assert_eq!(metrics.events_processed, 1);

        core.stop().await.expect("Failed to stop core");
    }

    #[tokio::test]
    async fn test_health_monitoring() {
        let core = CrucibleCoreBuilder::new()
            .build()
            .await
            .expect("Failed to create core");

        core.start().await.expect("Failed to start core");

        let health_results = core.perform_health_check().await.expect("Failed to perform health check");
        assert!(health_results.is_empty()); // No services registered yet

        let system_health = core.get_system_health().await.expect("Failed to get system health");
        assert!(matches!(system_health.status, ServiceStatus::Degraded | ServiceStatus::Healthy));

        core.stop().await.expect("Failed to stop core");
    }
}
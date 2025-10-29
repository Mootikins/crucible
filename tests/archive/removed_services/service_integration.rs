//! # Service Integration Example
//!
//! This example demonstrates how to implement and integrate services with the CrucibleCore.
//! It shows proper trait implementations and service lifecycle management.

use crucible_core::{
    CrucibleCore, CrucibleCoreBuilder, CoreConfig,
    DaemonEvent, EventPayload, EventSource, EventType, EventPriority,
};
use crucible_services::{
    service_traits::{
        ServiceLifecycle, HealthCheck, Configurable, Observable, EventDriven, ResourceManager,
        McpGateway, InferenceEngine, ScriptEngine, DataStore,
    },
    types::{ServiceHealth, ServiceStatus, ServiceMetrics, ResourceUsage, ResourceLimits},
    errors::ServiceResult,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

// ============================================================================
// MCP GATEWAY SERVICE IMPLEMENTATION
// ============================================================================

#[derive(Debug)]
struct MockMcpGateway {
    name: String,
    version: String,
    running: bool,
    tools: HashMap<String, crucible_services::types::tool::ToolDefinition>,
    config: McpGatewayConfig,
    metrics: ServiceMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct McpGatewayConfig {
    max_tools: usize,
    timeout_ms: u64,
    enable_logging: bool,
}

impl Default for McpGatewayConfig {
    fn default() -> Self {
        Self {
            max_tools: 100,
            timeout_ms: 30000,
            enable_logging: true,
        }
    }
}

impl MockMcpGateway {
    fn new() -> Self {
        Self {
            name: "mcp-gateway".to_string(),
            version: "1.0.0".to_string(),
            running: false,
            tools: HashMap::new(),
            config: McpGatewayConfig::default(),
            metrics: ServiceMetrics {
                total_requests: 0,
                successful_requests: 0,
                failed_requests: 0,
                average_response_time: std::time::Duration::from_millis(0),
                uptime: std::time::Duration::from_secs(0),
                memory_usage: 0,
                cpu_usage: 0.0,
            },
        }
    }

    fn add_sample_tools(&mut self) {
        // Add sample tools for demonstration
        self.tools.insert("file-read".to_string(), crucible_services::types::tool::ToolDefinition {
            name: "file-read".to_string(),
            description: "Read a file from the filesystem".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "encoding": {"type": "string", "default": "utf-8"}
                },
                "required": ["path"]
            }),
            category: Some("filesystem".to_string()),
            version: Some("1.0.0".to_string()),
            author: Some("Crucible Team".to_string()),
            tags: vec!["file".to_string(), "read".to_string()],
            enabled: true,
            parameters: vec![],
        });
    }
}

#[async_trait]
impl ServiceLifecycle for MockMcpGateway {
    async fn start(&mut self) -> ServiceResult<()> {
        println!("🚀 Starting MCP Gateway service");
        self.running = true;
        self.add_sample_tools();
        println!("✅ MCP Gateway started with {} tools", self.tools.len());
        Ok(())
    }

    async fn stop(&mut self) -> ServiceResult<()> {
        println!("🛑 Stopping MCP Gateway service");
        self.running = false;
        self.tools.clear();
        println!("✅ MCP Gateway stopped");
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
impl HealthCheck for MockMcpGateway {
    async fn health_check(&self) -> ServiceResult<ServiceHealth> {
        Ok(ServiceHealth {
            status: if self.running {
                ServiceStatus::Healthy
            } else {
                ServiceStatus::Unhealthy
            },
            message: Some(format!(
                "MCP Gateway is {} with {} tools registered",
                if self.running { "running" } else { "stopped" },
                self.tools.len()
            )),
            last_check: chrono::Utc::now(),
            details: {
                let mut details = HashMap::new();
                details.insert("tools_registered".to_string(), self.tools.len().to_string());
                details.insert("max_tools".to_string(), self.config.max_tools.to_string());
                details.insert("timeout_ms".to_string(), self.config.timeout_ms.to_string());
                details
            },
        })
    }
}

#[async_trait]
impl Configurable for MockMcpGateway {
    type Config = McpGatewayConfig;

    async fn get_config(&self) -> ServiceResult<<Self as Configurable>::Config> {
        Ok(self.config.clone())
    }

    async fn update_config(&mut self, config: <Self as Configurable>::Config) -> ServiceResult<()> {
        println!("⚙️ Updating MCP Gateway configuration");
        self.config = config;
        println!("✅ MCP Gateway configuration updated");
        Ok(())
    }

    async fn validate_config(&self, config: &<Self as Configurable>::Config) -> ServiceResult<()> {
        if config.max_tools == 0 {
            return Err(crucible_services::errors::ServiceError::ValidationError(
                "max_tools must be greater than 0".to_string(),
            ));
        }
        if config.timeout_ms == 0 {
            return Err(crucible_services::errors::ServiceError::ValidationError(
                "timeout_ms must be greater than 0".to_string(),
            ));
        }
        Ok(())
    }

    async fn reload_config(&mut self) -> ServiceResult<()> {
        println!("🔄 Reloading MCP Gateway configuration");
        // In a real implementation, this would load from file or database
        Ok(())
    }
}

#[async_trait]
impl Observable for MockMcpGateway {
    async fn get_metrics(&self) -> ServiceResult<ServiceMetrics> {
        Ok(self.metrics.clone())
    }

    async fn reset_metrics(&mut self) -> ServiceResult<()> {
        self.metrics = ServiceMetrics {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            average_response_time: std::time::Duration::from_millis(0),
            uptime: std::time::Duration::from_secs(0),
            memory_usage: 0,
            cpu_usage: 0.0,
        };
        Ok(())
    }

    async fn get_performance_metrics(&self) -> ServiceResult<crucible_services::types::PerformanceMetrics> {
        Ok(crucible_services::types::PerformanceMetrics {
            response_times: vec![10.0, 15.0, 12.0, 8.0], // Sample response times
            throughput: 100.0, // requests per second
            error_rate: 0.02, // 2% error rate
            resource_usage: {
                let mut usage = HashMap::new();
                usage.insert("memory_mb".to_string(), 256.0);
                usage.insert("cpu_percent".to_string(), 15.0);
                usage
            },
        })
    }
}

// ============================================================================
// INFERENCE ENGINE SERVICE IMPLEMENTATION
// ============================================================================

#[derive(Debug)]
struct MockInferenceEngine {
    name: String,
    version: String,
    running: bool,
    models: HashMap<String, ModelInfo>,
    config: InferenceEngineConfig,
    metrics: ServiceMetrics,
}

#[derive(Debug, Clone)]
struct ModelInfo {
    id: String,
    name: String,
    size_mb: u64,
    loaded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InferenceEngineConfig {
    max_models: usize,
    default_model: String,
    max_tokens: u32,
    temperature: f32,
}

impl Default for InferenceEngineConfig {
    fn default() -> Self {
        Self {
            max_models: 5,
            default_model: "gpt-3.5-turbo".to_string(),
            max_tokens: 2048,
            temperature: 0.7,
        }
    }
}

impl MockInferenceEngine {
    fn new() -> Self {
        let mut models = HashMap::new();
        models.insert("gpt-3.5-turbo".to_string(), ModelInfo {
            id: "gpt-3.5-turbo".to_string(),
            name: "GPT-3.5 Turbo".to_string(),
            size_mb: 350,
            loaded: true,
        });
        models.insert("gpt-4".to_string(), ModelInfo {
            id: "gpt-4".to_string(),
            name: "GPT-4".to_string(),
            size_mb: 800,
            loaded: false,
        });

        Self {
            name: "inference-engine".to_string(),
            version: "1.0.0".to_string(),
            running: false,
            models,
            config: InferenceEngineConfig::default(),
            metrics: ServiceMetrics {
                total_requests: 0,
                successful_requests: 0,
                failed_requests: 0,
                average_response_time: std::time::Duration::from_millis(0),
                uptime: std::time::Duration::from_secs(0),
                memory_usage: 0,
                cpu_usage: 0.0,
            },
        }
    }
}

#[async_trait]
impl ServiceLifecycle for MockInferenceEngine {
    async fn start(&mut self) -> ServiceResult<()> {
        println!("🚀 Starting Inference Engine service");
        self.running = true;
        println!("✅ Inference Engine started with {} models available", self.models.len());
        Ok(())
    }

    async fn stop(&mut self) -> ServiceResult<()> {
        println!("🛑 Stopping Inference Engine service");
        self.running = false;
        for model in self.models.values_mut() {
            model.loaded = false;
        }
        println!("✅ Inference Engine stopped");
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
impl HealthCheck for MockInferenceEngine {
    async fn health_check(&self) -> ServiceResult<ServiceHealth> {
        let loaded_models = self.models.values().filter(|m| m.loaded).count();

        Ok(ServiceHealth {
            status: if self.running && loaded_models > 0 {
                ServiceStatus::Healthy
            } else if self.running {
                ServiceStatus::Degraded
            } else {
                ServiceStatus::Unhealthy
            },
            message: Some(format!(
                "Inference Engine is {} with {}/{} models loaded",
                if self.running { "running" } else { "stopped" },
                loaded_models,
                self.models.len()
            )),
            last_check: chrono::Utc::now(),
            details: {
                let mut details = HashMap::new();
                details.insert("total_models".to_string(), self.models.len().to_string());
                details.insert("loaded_models".to_string(), loaded_models.to_string());
                details.insert("default_model".to_string(), self.config.default_model.clone());
                details.insert("max_tokens".to_string(), self.config.max_tokens.to_string());
                details
            },
        })
    }
}

#[async_trait]
impl Configurable for MockInferenceEngine {
    type Config = InferenceEngineConfig;

    async fn get_config(&self) -> ServiceResult<<Self as Configurable>::Config> {
        Ok(self.config.clone())
    }

    async fn update_config(&mut self, config: <Self as Configurable>::Config) -> ServiceResult<()> {
        println!("⚙️ Updating Inference Engine configuration");
        self.config = config;
        println!("✅ Inference Engine configuration updated");
        Ok(())
    }

    async fn validate_config(&self, config: &<Self as Configurable>::Config) -> ServiceResult<()> {
        if config.max_models == 0 {
            return Err(crucible_services::errors::ServiceError::ValidationError(
                "max_models must be greater than 0".to_string(),
            ));
        }
        if config.max_tokens == 0 {
            return Err(crucible_services::errors::ServiceError::ValidationError(
                "max_tokens must be greater than 0".to_string(),
            ));
        }
        if !(0.0..=2.0).contains(&config.temperature) {
            return Err(crucible_services::errors::ServiceError::ValidationError(
                "temperature must be between 0.0 and 2.0".to_string(),
            ));
        }
        Ok(())
    }

    async fn reload_config(&mut self) -> ServiceResult<()> {
        println!("🔄 Reloading Inference Engine configuration");
        Ok(())
    }
}

#[async_trait]
impl Observable for MockInferenceEngine {
    async fn get_metrics(&self) -> ServiceResult<ServiceMetrics> {
        Ok(self.metrics.clone())
    }

    async fn reset_metrics(&mut self) -> ServiceResult<()> {
        self.metrics = ServiceMetrics {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            average_response_time: std::time::Duration::from_millis(0),
            uptime: std::time::Duration::from_secs(0),
            memory_usage: 0,
            cpu_usage: 0.0,
        };
        Ok(())
    }

    async fn get_performance_metrics(&self) -> ServiceResult<crucible_services::types::PerformanceMetrics> {
        Ok(crucible_services::types::PerformanceMetrics {
            response_times: vec![250.0, 300.0, 275.0, 320.0], // Sample response times
            throughput: 10.0, // requests per second
            error_rate: 0.01, // 1% error rate
            resource_usage: {
                let mut usage = HashMap::new();
                usage.insert("memory_mb".to_string(), 2048.0);
                usage.insert("cpu_percent".to_string(), 45.0);
                usage.insert("gpu_percent".to_string(), 60.0);
                usage
            },
        })
    }
}

// ============================================================================
// SERVICE REGISTRATION HELPER
// ============================================================================

struct ServiceRegistry {
    services: Vec<Box<dyn ServiceLifecycleWrapper>>,
}

#[async_trait]
trait ServiceLifecycleWrapper: Send + Sync {
    async fn start(&mut self) -> ServiceResult<()>;
    async fn stop(&mut self) -> ServiceResult<()>;
    fn is_running(&self) -> bool;
    fn service_name(&self) -> &str;
    fn service_version(&self) -> &str;
    async fn health_check(&self) -> ServiceResult<ServiceHealth>;
    async fn get_metrics(&self) -> ServiceResult<ServiceMetrics>;
}

impl<T> ServiceLifecycleWrapper for T
where
    T: ServiceLifecycle + HealthCheck + Observable + Send + Sync + 'static,
{
    async fn start(&mut self) -> ServiceResult<()> {
        <Self as ServiceLifecycle>::start(self).await
    }

    async fn stop(&mut self) -> ServiceResult<()> {
        <Self as ServiceLifecycle>::stop(self).await
    }

    fn is_running(&self) -> bool {
        <Self as ServiceLifecycle>::is_running(self)
    }

    fn service_name(&self) -> &str {
        <Self as ServiceLifecycle>::service_name(self)
    }

    fn service_version(&self) -> &str {
        <Self as ServiceLifecycle>::service_version(self)
    }

    async fn health_check(&self) -> ServiceResult<ServiceHealth> {
        <Self as HealthCheck>::health_check(self).await
    }

    async fn get_metrics(&self) -> ServiceResult<ServiceMetrics> {
        <Self as Observable>::get_metrics(self).await
    }
}

// ============================================================================
// MAIN DEMONSTRATION
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("🔧 Service Integration Demonstration");
    println!("==================================");

    // Create and start CrucibleCore
    let core = CrucibleCoreBuilder::new()
        .with_max_services(10)
        .with_health_check_interval(5)
        .with_auto_recovery(true)
        .build()
        .await?;

    println!("✅ CrucibleCore created successfully");

    core.start().await?;
    println!("✅ CrucibleCore started successfully");

    // Create service instances
    let mut mcp_gateway = MockMcpGateway::new();
    let mut inference_engine = MockInferenceEngine::new();

    // Start services individually
    println!("\n🚀 Starting individual services...");
    mcp_gateway.start().await?;
    println!("✅ MCP Gateway started");

    inference_engine.start().await?;
    println!("✅ Inference Engine started");

    // Demonstrate service health checks
    println!("\n🏥 Checking service health...");
    let mcp_health = mcp_gateway.health_check().await?;
    println!("  MCP Gateway: {:?} - {}", mcp_health.status, mcp_health.message.unwrap_or_default());

    let inference_health = inference_engine.health_check().await?;
    println!("  Inference Engine: {:?} - {}", inference_health.status, inference_health.message.unwrap_or_default());

    // Demonstrate service metrics
    println!("\n📊 Collecting service metrics...");
    let mcp_metrics = mcp_gateway.get_metrics().await?;
    println!("  MCP Gateway: {} requests, {}ms avg response",
             mcp_metrics.total_requests, mcp_metrics.average_response_time.as_millis());

    let inference_metrics = inference_engine.get_metrics().await?;
    println!("  Inference Engine: {} requests, {}ms avg response",
             inference_metrics.total_requests, inference_metrics.average_response_time.as_millis());

    // Demonstrate configuration management
    println!("\n⚙️ Demonstrating configuration management...");

    // Update MCP Gateway configuration
    let new_mcp_config = McpGatewayConfig {
        max_tools: 200,
        timeout_ms: 60000,
        enable_logging: true,
    };

    if let Err(e) = mcp_gateway.validate_config(&new_mcp_config).await {
        println!("❌ MCP Gateway config validation failed: {}", e);
    } else {
        mcp_gateway.update_config(new_mcp_config).await?;
        println!("✅ MCP Gateway configuration updated");
    }

    // Update Inference Engine configuration
    let new_inference_config = InferenceEngineConfig {
        max_models: 10,
        default_model: "gpt-4".to_string(),
        max_tokens: 4096,
        temperature: 0.5,
    };

    if let Err(e) = inference_engine.validate_config(&new_inference_config).await {
        println!("❌ Inference Engine config validation failed: {}", e);
    } else {
        inference_engine.update_config(new_inference_config).await?;
        println!("✅ Inference Engine configuration updated");
    }

    // Route some events through the core
    println!("\n📨 Routing events to services...");

    // Tool execution event
    let tool_event = DaemonEvent::new(
        EventType::Mcp(crucible_services::events::core::McpEventType::ToolCall {
            tool_name: "file-read".to_string(),
            parameters: serde_json::json!({"path": "/kiln/example.md"}),
        }),
        EventSource::external("api-client".to_string()),
        EventPayload::json(serde_json::json!({
            "tool": "file-read",
            "parameters": {"path": "/kiln/example.md"}
        })),
    ).with_target(crucible_services::events::core::ServiceTarget::new("mcp-gateway".to_string()));

    core.route_event(tool_event).await?;
    println!("✅ Tool execution event routed");

    // Model inference event
    let inference_event = DaemonEvent::new(
        EventType::External(crucible_services::events::core::ExternalEventType::DataReceived {
            source: "chat-interface".to_string(),
            data: serde_json::json!({
                "prompt": "Explain the architecture of Crucible",
                "model": "gpt-3.5-turbo"
            }),
        }),
        EventSource::external("chat-interface".to_string()),
        EventPayload::json(serde_json::json!({
            "type": "inference_request",
            "model": "gpt-3.5-turbo",
            "prompt": "Explain the architecture of Crucible"
        })),
    ).with_target(crucible_services::events::core::ServiceTarget::new("inference-engine".to_string()));

    core.route_event(inference_event).await?;
    println!("✅ Inference event routed");

    // Get system metrics
    println!("\n📈 System-wide metrics:");
    let system_metrics = core.get_metrics().await?;
    println!("  Events processed: {}", system_metrics.events_processed);
    println!("  Services managed: {}", system_metrics.services_managed);
    println!("  Uptime: {}ms", system_metrics.uptime_ms);
    println!("  Memory usage: {} bytes", system_metrics.memory_usage_bytes);

    // Perform comprehensive health check
    println!("\n🏥 Comprehensive health check:");
    let health_results = core.perform_health_check().await?;
    for (service_id, health) in health_results {
        println!("  {}: {:?} - {}", service_id, health.status,
                 health.message.unwrap_or_default());
    }

    // Simulate some runtime
    println!("\n⏳ Running for 3 seconds...");
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // Shutdown services
    println!("\n🛑 Shutting down services...");
    mcp_gateway.stop().await?;
    inference_engine.stop().await?;

    // Shutdown core
    core.stop().await?;
    println!("✅ All services and core shut down successfully");

    println!("\n🎉 Service integration demonstration completed!");
    println!("=============================================");
    println!("Features demonstrated:");
    println!("  ✅ Service lifecycle management");
    println!("  ✅ Health monitoring and checks");
    println!("  ✅ Metrics collection");
    println!("  ✅ Configuration management");
    println!("  ✅ Event routing");
    println!("  ✅ Service integration");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mcp_gateway_lifecycle() {
        let mut service = MockMcpGateway::new();

        assert!(!service.is_running());
        assert!(service.start().await.is_ok());
        assert!(service.is_running());

        let health = service.health_check().await.unwrap();
        assert!(matches!(health.status, ServiceStatus::Healthy));

        assert!(service.stop().await.is_ok());
        assert!(!service.is_running());
    }

    #[tokio::test]
    async fn test_inference_engine_lifecycle() {
        let mut service = MockInferenceEngine::new();

        assert!(!service.is_running());
        assert!(service.start().await.is_ok());
        assert!(service.is_running());

        let health = service.health_check().await.unwrap();
        assert!(matches!(health.status, ServiceStatus::Healthy));

        assert!(service.stop().await.is_ok());
        assert!(!service.is_running());
    }

    #[tokio::test]
    async fn test_configuration_validation() {
        let mut service = MockMcpGateway::new();

        // Valid configuration
        let valid_config = McpGatewayConfig {
            max_tools: 100,
            timeout_ms: 30000,
            enable_logging: true,
        };
        assert!(service.validate_config(&valid_config).await.is_ok());

        // Invalid configuration
        let invalid_config = McpGatewayConfig {
            max_tools: 0, // Invalid
            timeout_ms: 30000,
            enable_logging: true,
        };
        assert!(service.validate_config(&invalid_config).await.is_err());
    }

    #[tokio::test]
    async fn test_metrics_collection() {
        let service = MockMcpGateway::new();
        let metrics = service.get_metrics().await.unwrap();
        assert_eq!(metrics.total_requests, 0);

        let perf_metrics = service.get_performance_metrics().await.unwrap();
        assert!(!perf_metrics.response_times.is_empty());
        assert!(perf_metrics.throughput >= 0.0);
    }
}
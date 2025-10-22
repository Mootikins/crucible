# CrucibleCore Architecture Documentation

## Overview

CrucibleCore is the centralized coordinator for the simplified Crucible knowledge management system. It eliminates complex service orchestration by providing a single, cohesive interface for service management, event routing, configuration, and health monitoring.

## Core Principles

1. **Centralized Coordination**: Single point of coordination for all system services
2. **Event-Driven Architecture**: Comprehensive event routing and processing system
3. **Simplified Lifecycle Management**: Clean start/stop/restart operations for all services
4. **Health Monitoring**: Continuous health tracking with automatic recovery capabilities
5. **Configuration Integration**: Centralized configuration management with dynamic updates
6. **Metrics Collection**: Real-time performance monitoring and metrics aggregation

## Architecture Components

### 1. CrucibleCore Struct

The main coordinator that brings together all system components:

```rust
pub struct CrucibleCore {
    // Core components
    config: Arc<RwLock<CrucibleConfig>>,
    orchestrator: Arc<RwLock<SimpleServiceOrchestrator>>,
    event_router: Arc<DefaultEventRouter>,
    config_manager: Arc<RwLock<ConfigManager>>,
    master_controller: Arc<RwLock<MasterController>>,

    // Service management
    services: Arc<RwLock<HashMap<String, Arc<dyn ServiceLifecycle>>>>,

    // Event handling
    event_sender: mpsc::UnboundedSender<CoreEvent>,
    event_receiver: Arc<RwLock<Option<mpsc::UnboundedReceiver<CoreEvent>>>>,

    // Monitoring
    health_data: Arc<RwLock<CoreHealthData>>,
    metrics: Arc<RwLock<CoreMetrics>>,

    // State management
    state: Arc<RwLock<CoreState>>,
}
```

### 2. Service Registry

Manages all registered services and their lifecycles:

- **Registration**: Services register with the core using `register_service()`
- **Discovery**: Services can be retrieved using `get_service()` or listed with `list_services()`
- **Unregistration**: Services can be cleanly removed with `unregister_service()`
- **Lifecycle Management**: Automatic start/stop/restart operations

### 3. Event Routing Integration

Integrates with the comprehensive event system from `crucible-services`:

- **Daemon Events**: Routes system-wide events to appropriate services
- **Service Events**: Handles service-specific events and notifications
- **Load Balancing**: Distributes events across multiple service instances
- **Circuit Breaking**: Prevents cascade failures with automatic circuit breaking
- **Event Filtering**: Supports fine-grained event filtering and routing rules

### 4. Health Monitoring

Continuous health tracking for all services:

- **Service Health**: Individual service health status and metrics
- **System Health**: Overall system health aggregated from all services
- **Health History**: Historical health data for trend analysis
- **Auto-Recovery**: Automatic service recovery based on health status
- **Alerting**: System alerts for health changes and critical events

### 5. Metrics Collection

Real-time performance monitoring:

- **Event Metrics**: Event processing rates and latencies
- **Service Metrics**: Individual service performance data
- **System Metrics**: Overall system resource usage and performance
- **Historical Data**: Metrics history for analysis and optimization

## Service Types

CrucibleCore coordinates four main service types:

### 1. MCP Gateway Service
- **Purpose**: Handles MCP (Model Context Protocol) operations
- **Capabilities**: Tool registration, execution, protocol negotiation
- **Events**: Tool calls, responses, resource requests

### 2. Inference Engine Service
- **Purpose**: AI/LLM operations and model management
- **Capabilities**: Text generation, embeddings, model loading
- **Events**: Model requests, generation completions, inference metrics

### 3. Script Engine Service
- **Purpose**: Rune script execution and management
- **Capabilities**: Script compilation, execution, security policies
- **Events**: Script requests, execution results, compilation events

### 4. Data Store Service
- **Purpose**: Database operations and persistence
- **Capabilities**: CRUD operations, queries, transactions
- **Events**: Database changes, query results, schema updates

## Event System Integration

CrucibleCore uses the comprehensive event system from `crucible-services`:

### Event Types
- **Filesystem Events**: File creation, modification, deletion
- **Database Events**: Record changes, schema updates, transactions
- **External Events**: API calls, webhooks, notifications
- **MCP Events**: Tool calls, responses, resource operations
- **Service Events**: Health changes, configuration updates, lifecycle events
- **System Events**: Daemon lifecycle, configuration reloads, maintenance

### Event Routing
```rust
// Create and route an event
let event = DaemonEvent::new(
    EventType::Filesystem(FilesystemEventType::FileCreated { path: String }),
    EventSource::filesystem("watcher".to_string()),
    EventPayload::text("File created".to_string()),
).with_target(ServiceTarget::new("mcp-gateway".to_string()));

core.route_event(event).await?;
```

## Usage Examples

### Basic Setup
```rust
use crucible_core::{CrucibleCoreBuilder, CoreConfig};

// Create core with custom configuration
let core = CrucibleCoreBuilder::new()
    .with_max_services(100)
    .with_health_check_interval(30)
    .with_auto_recovery(true)
    .build()
    .await?;

// Start the core
core.start().await?;
```

### Service Registration
```rust
use crucible_services::service_traits::ServiceLifecycle;
use std::sync::Arc;

// Create a service
let service = Arc::new(MyService::new());

// Register with core
core.register_service(service).await?;

// List all services
let services = core.list_services().await?;
```

### Event Routing
```rust
use crucible_core::*;
use crucible_services::events::*;

// Create a daemon event
let event = DaemonEvent::new(
    EventType::System(SystemEventType::DaemonStarted { version: "1.0.0".to_string() }),
    EventSource::system("core".to_string()),
    EventPayload::json(serde_json::json!({ "status": "running" })),
);

// Route the event
core.route_event(event).await?;
```

### Health Monitoring
```rust
// Perform health check
let health_results = core.perform_health_check().await?;

// Get system health
let system_health = core.get_system_health().await?;

// Get metrics
let metrics = core.get_metrics().await?;
```

## Configuration

### Core Configuration
```rust
let config = CoreConfig {
    max_services: 100,
    routing_config: RoutingConfig::default(),
    health_check_interval_s: 30,
    metrics_interval_s: 60,
    enable_auto_recovery: true,
    max_recovery_attempts: 3,
    event_queue_size: 10000,
    enable_debug: false,
};
```

### Dynamic Configuration Updates
```rust
// Update configuration at runtime
let new_config = CrucibleConfig::default();
core.update_config(new_config).await?;
```

## Error Handling

CrucibleCore provides comprehensive error handling:

- **Service Errors**: Individual service failures are isolated
- **System Errors**: Critical system errors trigger appropriate responses
- **Recovery**: Automatic recovery attempts for transient failures
- **Circuit Breaking**: Prevents cascade failures
- **Logging**: Comprehensive error logging and debugging information

## Performance Considerations

### Memory Management
- **Arc/RwLock Pattern**: Shared state with concurrent access
- **Event Queueing**: Bounded queues prevent memory exhaustion
- **Metrics Limiting**: Historical data is automatically pruned

### Concurrency
- **Async/Await**: Full async/await support throughout
- **Tokio Integration**: Built on Tokio runtime for optimal performance
- **Lock-Free Operations**: Minimized locking for high-throughput operations

### Scalability
- **Service Limits**: Configurable limits on number of services
- **Event Throttling**: Configurable event processing limits
- **Resource Management**: Automatic resource cleanup and monitoring

## Integration Points

### With Daemon
CrucibleCore provides the coordination layer that the daemon uses to manage services:

```rust
// In daemon initialization
let core = CrucibleCore::new(core_config).await?;
core.start().await?;

// Register all daemon services
core.register_service(mcp_gateway).await?;
core.register_service(inference_engine).await?;
core.register_service(script_engine).await?;
core.register_service(datastore).await?;
```

### With Frontend
The core provides endpoints for frontend monitoring and control:

- **Health Status**: Real-time health information
- **Metrics**: Performance metrics and system statistics
- **Configuration**: Dynamic configuration updates
- **Service Control**: Service start/stop/restart operations

### With External Systems
External systems can interact with the core through:

- **Event APIs**: Submit events for processing
- **Service APIs**: Direct service interaction
- **Monitoring APIs**: Health and metrics information
- **Configuration APIs**: System configuration management

## Best Practices

### Service Design
1. **Async Traits**: Implement async traits for all service operations
2. **Error Handling**: Provide comprehensive error information
3. **Health Checks**: Implement meaningful health check logic
4. **Metrics**: Provide relevant performance metrics
5. **Configuration**: Support dynamic configuration updates

### Event Handling
1. **Event Validation**: Validate events before processing
2. **Error Isolation**: Isolate event processing errors
3. **Backpressure**: Handle event queue backpressure gracefully
4. **Filtering**: Use appropriate event filtering
5. **Correlation**: Use correlation IDs for event tracking

### Resource Management
1. **Cleanup**: Implement proper resource cleanup
2. **Limits**: Set appropriate resource limits
3. **Monitoring**: Monitor resource usage continuously
4. **Scaling**: Design for horizontal scaling
5. **Recovery**: Implement graceful recovery procedures

## Testing

The CrucibleCore includes comprehensive test coverage:

```rust
#[tokio::test]
async fn test_core_lifecycle() {
    let core = CrucibleCoreBuilder::new().build().await?;
    core.start().await?;
    assert_eq!(core.get_state().await, CoreState::Running);
    core.stop().await?;
    assert_eq!(core.get_state().await, CoreState::Stopped);
}
```

## Future Enhancements

Planned improvements to CrucibleCore:

1. **Service Discovery**: Automatic service discovery and registration
2. **Load Balancing**: Advanced load balancing algorithms
3. **Distributed Mode**: Multi-node deployment support
4. **Advanced Metrics**: More detailed metrics and analytics
5. **API Layer**: REST/gRPC API for external integration
6. **Security**: Enhanced security features and authentication
7. **Monitoring UI**: Built-in monitoring dashboard
8. **Backup/Restore**: System state backup and restore capabilities

## Conclusion

CrucibleCore provides a clean, simplified architecture for coordinating services in the Crucible knowledge management system. By centralizing coordination and leveraging a comprehensive event system, it eliminates the complexity of traditional service orchestration while providing all necessary functionality for robust, scalable operations.
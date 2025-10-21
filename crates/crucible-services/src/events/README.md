# Event System for Centralized Daemon Coordination

This module provides a comprehensive event system for coordinating services through the central daemon in the Crucible knowledge management system.

## Architecture Overview

The event system is designed around the following principles:

1. **Centralized Coordination**: All events flow through the central daemon
2. **Service-Based Architecture**: Events are routed to specific services based on type and rules
3. **Resilient Routing**: Built-in circuit breakers, retries, and error handling
4. **Performance Optimized**: Efficient routing with load balancing and deduplication
5. **Extensible Design**: Easy to add new event types and routing rules

## Core Components

### 1. Event Types (`core.rs`)

#### `DaemonEvent`
The main event structure that flows through the system:

```rust
pub struct DaemonEvent {
    pub id: Uuid,                    // Unique identifier
    pub event_type: EventType,        // Type of event
    pub priority: EventPriority,      // Processing priority
    pub source: EventSource,          // Who generated it
    pub targets: Vec<ServiceTarget>,  // Where it should go
    pub payload: EventPayload,        // Event data
    pub metadata: EventMetadata,      // Debug/monitoring info
    // ... additional fields for correlation, retries, scheduling
}
```

#### `EventType`
Enumerates all possible event categories:

- **Filesystem**: File creation, modification, deletion
- **Database**: Record changes, schema updates, transactions
- **External**: Webhooks, API calls, notifications
- **MCP**: Model Context Protocol events
- **Service**: Service coordination and health events
- **System**: Daemon lifecycle and maintenance events

#### Event Priorities
- `Critical`: System-critical events (shutdowns, security violations)
- `High`: Important events (service failures, configuration changes)
- `Normal`: Regular operational events (file changes, data updates)
- `Low`: Background events (metrics, maintenance)

### 2. Service-Specific Events (`service_events.rs`)

Each service has its own event types:

#### McpGateway Events
- Server connections/disconnections
- Tool calls and responses
- Resource requests
- Protocol errors
- Load balancing decisions

#### InferenceEngine Events
- Inference requests and completions
- Model loading/unloading
- Queue status updates
- Resource usage monitoring
- Batch processing results

#### ScriptEngine Events
- Script execution lifecycle
- Runtime management
- Sandbox operations
- Security violations
- Compilation results

#### DataStore Events
- Query operations
- Data mutations (CRUD)
- Schema changes
- Backup/restore operations
- Performance metrics

### 3. Event Routing (`routing.rs`)

#### `EventRouter` Trait
Defines the interface for event routing:

```rust
#[async_trait]
pub trait EventRouter: Send + Sync {
    async fn route_event(&self, event: DaemonEvent) -> EventResult<()>;
    async fn register_service(&self, service: ServiceRegistration) -> EventResult<()>;
    async fn update_service_health(&self, service_id: &str, health: ServiceHealth) -> EventResult<()>;
    // ... other methods
}
```

#### Load Balancing Strategies
- **RoundRobin**: Even distribution across services
- **LeastConnections**: Route to service with fewest active connections
- **WeightedRandom**: Random selection weighted by service capacity
- **HealthBased**: Prefer healthy services over degraded ones
- **PriorityBased**: Consider service and event priorities

#### Circuit Breaker Pattern
Protects against cascading failures:

```rust
struct CircuitBreaker {
    failure_count: u32,
    last_failure_time: DateTime<Utc>,
    state: CircuitBreakerState, // Closed, Open, HalfOpen
}
```

### 4. Error Handling (`errors.rs`)

Comprehensive error types for all failure scenarios:

- Serialization errors
- Routing failures
- Service not found
- Timeouts
- Queue full
- Circuit breaker open
- Rate limiting

## Usage Examples

### Creating and Routing Events

```rust
use crucible_services::events::*;

// Create a file system event
let event = DaemonEvent::new(
    EventType::Filesystem(FilesystemEventType::FileCreated {
        path: "/path/to/file.txt".to_string(),
    }),
    EventSource::filesystem("watcher-1".to_string()),
    EventPayload::json(serde_json::json!({
        "size": 1024,
        "type": "text/plain"
    })),
)
.with_priority(EventPriority::Normal)
.with_target(ServiceTarget::new("datastore".to_string()));

// Route the event
let router = DefaultEventRouter::new();
router.route_event(event).await?;
```

### Service-Specific Events

```rust
use crucible_services::events::service_events::*;

// Create MCP Gateway event
let mcp_event = McpGatewayEvent::tool_call_completed(
    "server-1".to_string(),
    "read_file".to_string(),
    "call-123".to_string(),
    ToolCallResult::Success {
        result: serde_json::json!({"content": "file content"}),
    },
    500, // duration_ms
);

// Convert to daemon event
let daemon_event = ServiceEventBuilder::new(EventSource::service("mcp-gateway".to_string()))
    .with_correlation(Uuid::new_v4())
    .mcp_gateway_event(mcp_event);
```

### Custom Routing Rules

```rust
// Create a routing rule for high-priority file events
let rule = RoutingRule {
    rule_id: "urgent-files".to_string(),
    name: "Urgent File Processing".to_string(),
    filter: EventFilter {
        event_types: vec!["filesystem".to_string()],
        priorities: vec![EventPriority::Critical, EventPriority::High],
        ..Default::default()
    },
    targets: vec![
        ServiceTarget::new("inference-engine".to_string())
            .with_priority(1)
    ],
    priority: 10,
    enabled: true,
    conditions: vec![],
};

router.add_routing_rule(rule).await?;
```

## Performance Considerations

### Memory Management
- Events use `serde_json::Value` for flexible payloads
- Payload size validation prevents memory exhaustion
- Deduplication cache with TTL to manage memory usage

### Concurrency
- Uses `DashMap` for lock-free concurrent access
- Async event processing with bounded queues
- Circuit breakers prevent resource exhaustion

### Load Balancing
- Multiple strategies for different use cases
- Health-aware routing to avoid failed services
- Priority-based routing for critical events

## Monitoring and Debugging

### Event Metadata
Each event includes comprehensive metadata:

```rust
pub struct EventMetadata {
    pub fields: HashMap<String, String>,     // Custom fields
    pub metrics: EventMetrics,               // Processing metrics
    pub debug: DebugInfo,                    // Debug information
}
```

### Routing Statistics
Track routing performance:

```rust
pub struct RoutingStats {
    pub total_events_routed: u64,
    pub events_routed_last_minute: u64,
    pub service_stats: HashMap<String, ServiceRoutingStats>,
    pub error_rate: f64,
    pub average_routing_time_ms: f64,
}
```

## Extensibility

### Adding New Event Types
1. Add variants to `EventType` enum
2. Create service-specific event types
3. Update routing logic if needed
4. Add tests for new events

### Custom Load Balancing
Implement the `LoadBalancingStrategy` enum and add logic to the routing implementation.

### Custom Filters
Extend the `EventFilter` struct with additional filtering criteria.

## Best Practices

1. **Event Design**: Keep events focused and single-purpose
2. **Error Handling**: Always handle routing failures gracefully
3. **Monitoring**: Track event flow and routing performance
4. **Testing**: Test routing rules and load balancing strategies
5. **Documentation**: Document custom event types and routing rules

## Testing

The event system includes comprehensive tests:

- Event serialization/deserialization
- Routing rule evaluation
- Load balancing strategies
- Circuit breaker behavior
- Error handling scenarios

Run tests with:
```bash
cargo test -p crucible-services
```

## Integration with Daemon

The event system is designed to integrate with the central daemon:

1. Daemon hosts the `EventRouter` instance
2. Services register with the daemon on startup
3. All inter-service communication goes through events
4. Daemon provides health monitoring and metrics

This design ensures loose coupling between services while maintaining centralized coordination and monitoring.
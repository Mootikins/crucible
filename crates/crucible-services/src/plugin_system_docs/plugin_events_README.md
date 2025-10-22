# Plugin Event Subscription System

A comprehensive event subscription system that enables plugins to subscribe to and receive events from the Crucible daemon event system with real-time delivery, reliability, and security.

## Overview

The plugin event subscription system provides a production-ready infrastructure for plugin event handling with the following key features:

- **Real-time Event Delivery**: Low-latency event delivery to subscribed plugins
- **Advanced Filtering**: Content-based and pattern-based event filtering
- **Reliability**: Guaranteed event delivery with retries and persistence
- **Security**: Plugin authorization and event access control
- **Performance**: Scalable architecture optimized for high throughput
- **Monitoring**: Comprehensive metrics and health monitoring
- **API Support**: REST API and WebSocket for subscription management

## Architecture

The system consists of several core components that work together:

### Core Components

1. **Subscription Manager** (`subscription_manager.rs`)
   - Central coordination of all subscription operations
   - Lifecycle management and health monitoring
   - Security enforcement and audit logging

2. **Subscription Registry** (`subscription_registry.rs`)
   - Storage and indexing of active subscriptions
   - Efficient event routing and matching
   - Performance statistics tracking

3. **Filter Engine** (`filter_engine.rs`)
   - Advanced content-based event filtering
   - Compiled filter expressions for performance
   - Custom filter function support

4. **Event Delivery System** (`delivery_system.rs`)
   - Reliable event delivery with acknowledgments
   - Backpressure handling and retry logic
   - Batch and priority delivery modes

5. **Event Bridge** (`event_bridge.rs`)
   - Integration with daemon event system
   - Event transformation and enrichment
   - Security filtering and audit logging

6. **Subscription API** (`subscription_api.rs`)
   - REST API for subscription management
   - WebSocket for real-time event streaming
   - Authentication and rate limiting

## Quick Start

### Basic Usage

```rust
use crucible_services::plugin_events::*;

// Create system configuration
let config = SubscriptionSystemConfig::default();

// Create and initialize the system
let mut event_system = PluginEventSystem::new(config)?;
let event_bus = Arc::new(EventBusImpl::new());
event_system.initialize(event_bus).await?;

// Create a subscription
let subscription_config = SubscriptionConfig::new(
    "my-plugin".to_string(),
    "important-events".to_string(),
    SubscriptionType::Realtime,
    AuthContext::new("my-plugin".to_string(), vec![]),
);

let subscription_id = event_system
    .subscription_manager()
    .create_subscription("my-plugin".to_string(), subscription_config)
    .await?;

println!("Created subscription: {}", subscription_id.as_string());
```

### Using the Builder Pattern

```rust
let event_system = PluginEventSystemBuilder::new()
    .with_config_file("config.toml")?
    .with_api_port(8080)
    .with_log_level("debug")
    .with_security_enabled(true)
    .build()?;
```

### Configuration

Create a `config.toml` file:

```toml
[system]
name = "crucible-plugin-events"
environment = "production"
data_dir = "./data"

[api]
enabled = true
port = 8080
enable_cors = true

[security]
enabled = true

[monitoring]
enabled = true
metrics.collection_interval_seconds = 60

[logging]
level = "info"
format = "json"
```

## Subscription Types

### Real-time Subscriptions
Events are delivered immediately as they occur:

```rust
let subscription = SubscriptionConfig::new(
    "plugin-id".to_string(),
    "realtime-events".to_string(),
    SubscriptionType::Realtime,
    auth_context,
);
```

### Batched Subscriptions
Events are collected and delivered in batches:

```rust
let subscription = SubscriptionConfig::new(
    "plugin-id".to_string(),
    "batched-events".to_string(),
    SubscriptionType::Batched {
        interval_seconds: 60,
        max_batch_size: 100,
    },
    auth_context,
);
```

### Persistent Subscriptions
Events are stored for offline plugins:

```rust
let subscription = SubscriptionConfig::new(
    "plugin-id".to_string(),
    "persistent-events".to_string(),
    SubscriptionType::Persistent {
        max_stored_events: 1000,
        ttl: Duration::from_secs(3600),
    },
    auth_context,
);
```

## Event Filtering

### Basic Filtering

```rust
let filter = EventFilter {
    event_types: vec!["filesystem".to_string(), "database".to_string()],
    sources: vec!["my-plugin".to_string()],
    ..Default::default()
};

let subscription = SubscriptionConfig::new(
    "plugin-id".to_string(),
    "filtered-events".to_string(),
    SubscriptionType::Realtime,
    auth_context,
).with_filter(filter);
```

### Advanced Filtering

```rust
// Filter by event content
let filter = EventFilter {
    expression: Some("priority = 1 AND payload.size > 1024".to_string()),
    ..Default::default()
};

// Regex pattern matching
let filter = EventFilter {
    expression: Some("source matches '.*-service'".to_string()),
    ..Default::default()
};
```

### Custom Filter Functions

```rust
// Register custom filter function
let custom_function = MyCustomFilter::new();
filter_engine.register_custom_function(custom_function).await?;

// Use in filter expression
let filter = EventFilter {
    expression: Some("custom_filter(payload)".to_string()),
    ..Default::default()
};
```

## Security

### Authorization

```rust
// Create authorization context with specific permissions
let auth_context = AuthContext::new(
    "plugin-id".to_string(),
    vec![
        EventPermission {
            scope: PermissionScope::Plugin,
            event_types: vec!["filesystem".to_string()],
            categories: vec![],
            sources: vec!["plugin-id".to_string()],
            max_priority: Some(EventPriority::High),
        },
    ],
);
```

### Access Control Lists

```rust
let acl = AccessControlListConfig {
    name: "plugin-acl".to_string(),
    entries: vec![
        AclEntryConfig {
            principal: "plugin-id".to_string(),
            event_pattern: "filesystem.*".to_string(),
            permission: "allow".to_string(),
            conditions: vec![],
            priority: 1,
        },
    ],
    default_action: "deny".to_string(),
    priority: 1,
};
```

## API Usage

### REST API

Create a subscription:
```bash
curl -X POST http://localhost:8080/api/v1/subscriptions \
  -H "Content-Type: application/json" \
  -d '{
    "plugin_id": "my-plugin",
    "name": "events-subscription",
    "subscription_type": "realtime",
    "filters": [
      {
        "event_types": ["filesystem", "database"]
      }
    ]
  }'
```

Get subscription:
```bash
curl http://localhost:8080/api/v1/subscriptions/{subscription-id}
```

### WebSocket API

Connect and subscribe:
```javascript
const ws = new WebSocket('ws://localhost:8080/ws');

// Subscribe to events
ws.send(JSON.stringify({
    type: 'Subscribe',
    subscription_id: 'subscription-id'
}));

// Handle events
ws.onmessage = (event) => {
    const message = JSON.parse(event.data);
    if (message.type === 'Event') {
        console.log('Received event:', message.event_data);
    }
};
```

## Monitoring

### Health Checks

```rust
let health_result = event_system.health_check().await;
println!("System status: {:?}", health_result.overall_status);
```

### Metrics

```rust
let system_stats = event_system.get_system_stats().await;
println!("Active subscriptions: {}", system_stats.manager_stats.active_subscriptions);
println!("Events processed: {}", system_stats.manager_stats.total_events_processed);
```

### Performance Metrics

```rust
let performance_metrics = event_system
    .subscription_manager()
    .get_performance_metrics()
    .await;

println!("Throughput: {:.2} events/sec", performance_metrics.events_per_second);
println!("Average latency: {:.2} ms", performance_metrics.avg_latency_ms);
```

## Configuration Reference

### System Configuration

- `system.name`: System identifier
- `system.environment`: Environment (development/staging/production)
- `system.data_dir`: Data storage directory
- `system.thread_pool`: Thread pool configuration
- `system.resource_limits`: Resource limits and constraints

### API Configuration

- `api.enabled`: Enable/disable API server
- `api.port`: API server port
- `api.enable_cors`: Enable CORS support
- `api.rate_limiting`: Rate limiting configuration
- `api.websocket`: WebSocket configuration

### Security Configuration

- `security.enabled`: Enable security features
- `security.authentication`: Authentication methods and settings
- `security.authorization`: Authorization and access control
- `security.encryption`: Encryption settings
- `security.audit`: Audit logging configuration

### Monitoring Configuration

- `monitoring.enabled`: Enable monitoring
- `monitoring.metrics`: Metrics collection settings
- `monitoring.health_checks`: Health check configuration
- `monitoring.alerting`: Alerting rules and notifications
- `monitoring.tracing`: Distributed tracing configuration

## Performance Considerations

### Throughput Optimization

- Use compiled filters for frequently used filter expressions
- Enable batch processing for high-volume subscriptions
- Configure appropriate buffer sizes for your workload

### Memory Usage

- Set appropriate retention periods for stored events
- Configure cache sizes based on available memory
- Monitor queue depths to prevent memory buildup

### Latency

- Use real-time subscriptions for low-latency requirements
- Enable compression for large events
- Configure appropriate timeouts for your environment

## Troubleshooting

### Common Issues

1. **High Memory Usage**
   - Check queue depths in delivery system
   - Review retention periods for persistent subscriptions
   - Monitor cache sizes and hit rates

2. **Slow Event Delivery**
   - Check filter complexity and compilation times
   - Review delivery system performance metrics
   - Verify network connectivity to plugins

3. **Connection Issues**
   - Check plugin connection manager status
   - Verify plugin authentication and authorization
   - Review API rate limiting configuration

### Debug Logging

Enable debug logging for detailed troubleshooting:

```toml
[logging]
level = "debug"
output.targets = [
    { name = "console", target_type = "console", level = "debug" },
    { name = "file", target_type = "file", level = "debug",
      config = { file_path = "./logs/debug.log" } }
]
```

## Testing

### Unit Tests

Run unit tests:
```bash
cargo test plugin_events
```

### Integration Tests

Run integration tests:
```bash
cargo test --test integration_tests plugin_events
```

### Performance Tests

Run performance benchmarks:
```bash
cargo test --release --features memory-testing plugin_events::tests::performance
```

## Examples

See the `examples/` directory for complete working examples:

- `basic_subscription.rs`: Basic subscription setup
- `advanced_filtering.rs`: Complex filtering examples
- `security_example.rs`: Security and authorization setup
- `monitoring_example.rs`: Metrics and health monitoring
- `api_client.rs`: REST API usage example
- `websocket_client.rs`: WebSocket client example

## Contributing

When contributing to the plugin event subscription system:

1. Follow the existing code style and patterns
2. Add comprehensive tests for new functionality
3. Update documentation for any API changes
4. Consider performance implications of changes
5. Ensure backward compatibility when possible

## License

This project is part of the Crucible knowledge management system and follows the same licensing terms.
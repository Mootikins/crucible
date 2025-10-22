# Crucible Plugin IPC Protocol

A comprehensive, high-performance Inter-Process Communication (IPC) protocol for process-based plugin communication in the Crucible knowledge management system.

## Features

### 🔐 Security
- **JWT Authentication**: Secure token-based authentication
- **End-to-End Encryption**: AES-256-GCM and ChaCha20-Poly1305 support
- **Capability-Based Authorization**: Fine-grained permission control
- **Sandboxing**: Process isolation with configurable security policies
- **Rate Limiting**: Protection against abuse and DoS attacks

### ⚡ Performance
- **Connection Pooling**: Efficient connection reuse and management
- **Multiplexing**: Multiple concurrent requests per connection
- **Compression**: LZ4 and Zstandard compression support
- **Zero-Copy Operations**: Optimized data transfer
- **Load Balancing**: Multiple strategies for request distribution

### 🛠️ Reliability
- **Automatic Retries**: Exponential backoff with jitter
- **Circuit Breaking**: Fail-fast for repeated failures
- **Health Monitoring**: Continuous health checking and metrics
- **Graceful Shutdown**: Clean connection and resource cleanup
- **Error Recovery**: Comprehensive error handling and logging

### 📊 Observability
- **Metrics Collection**: Performance and resource usage metrics
- **Distributed Tracing**: OpenTelemetry integration
- **Structured Logging**: JSON format with correlation IDs
- **Health Checks**: Liveness and readiness probes
- **Real-time Monitoring**: Export to Prometheus and other systems

## Quick Start

### Client Usage

```rust
use crucible_services::plugin_ipc::{IpcClientBuilder, config::Environment};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create client
    let client = IpcClientBuilder::new()
        .environment(Environment::Development)
        .enable_compression(true)
        .request_timeout(Duration::from_secs(30))
        .build()
        .await?;

    // Connect to plugin
    let connection = client.connect("my_plugin").await?;

    // Send request
    let result = connection.send_request(
        "process_data",
        serde_json::json!({
            "input": "Hello, World!",
            "options": { "format": "uppercase" }
        })
    ).await?;

    println!("Result: {}", result);

    // Send event
    connection.send_event(
        "user_action",
        serde_json::json!({
            "action": "click",
            "element": "submit_button"
        })
    ).await?;

    connection.close().await?;
    Ok(())
}
```

### Server Usage

```rust
use crucible_services::plugin_ipc::{IpcServerBuilder, RequestHandler};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create server
    let mut server = IpcServerBuilder::new()
        .environment(Environment::Production)
        .socket_path("/var/run/crucible/plugins")
        .max_concurrent_requests(100)
        .build()
        .await?;

    // Register custom handler
    server.register_handler("process_data", RequestHandler::new(|request| {
        Box::pin(async move {
            // Process the request
            if let crate::plugin_ipc::message::MessagePayload::Request(req) = request.payload {
                let input = req.parameters["input"].as_str().unwrap_or("");
                let processed = input.to_uppercase();

                let response = crate::plugin_ipc::message::ResponsePayload {
                    success: true,
                    data: Some(serde_json::json!({
                        "result": processed,
                        "processed_at": std::time::SystemTime::now()
                    })),
                    error: None,
                    execution_time_ms: 5,
                    resource_usage: crate::plugin_ipc::message::ResourceUsage::default(),
                    metadata: std::collections::HashMap::new(),
                };

                Ok(crate::plugin_ipc::message::IpcMessage::response(
                    request.header.correlation_id.unwrap_or_default(),
                    response,
                ))
            } else {
                Err(crucible_services::plugin_ipc::error::IpcError::Message {
                    message: "Expected request payload".to_string(),
                    code: crucible_services::plugin_ipc::error::MessageErrorCode::InvalidMessageFormat,
                    message_id: Some(request.header.message_id),
                })
            }
        })
    })).await;

    // Start server
    server.start().await?;

    // Server runs until shutdown
    tokio::signal::ctrl_c().await?;

    server.stop().await?;
    Ok(())
}
```

## Configuration

The IPC system supports comprehensive configuration for different environments:

```yaml
# ipc-config.yaml
transport:
  default_type: UnixDomainSocket
  socket_path: /var/run/crucible/plugins
  tcp_port_range: 9000-10000
  connection_pool:
    max_total_connections: 200
    max_connections_per_endpoint: 20
    connect_timeout_ms: 3000
    idle_timeout: 600s

security:
  auth:
    token_type: Jwt
    session_timeout_s: 7200
    token_expiry_s: 14400
    refresh_enabled: true
  encryption:
    algorithm: Aes256Gcm
    key_rotation_interval_s: 7200
    compression_enabled: true
  authorization:
    rbac_enabled: true
    abac_enabled: true
    policy_engine: production-policy

performance:
  enable_compression: true
  compression_level: 9
  enable_multiplexing: true
  max_concurrent_requests: 500
  request_timeout_ms: 60000

monitoring:
  metrics:
    export_enabled: true
    export_format: Prometheus
    collection_interval: 30s
  tracing:
    enabled: true
    level: warn
    sampling_rate: 0.01
    export_jaeger: true

plugins:
  auto_discovery: false
  plugin_directories:
    - /opt/crucible/plugins
  max_plugins: 1000
  resource_limits:
    max_memory_mb: 4096
    max_cpu_cores: 4.0
    max_disk_mb: 10240
  sandbox:
    enabled: true
    sandbox_type: Container
    isolated_filesystem: true
    network_access: false
```

## Architecture

### Message Flow

```
┌─────────────┐    1. Connect     ┌─────────────┐    2. Handshake    ┌─────────────┐
│   Client    │ ──────────────────▶│   Server    │ ──────────────────▶│   Security  │
│             │                   │             │                   │   Manager   │
└─────────────┘                   └─────────────┘                   └─────────────┘
        │ 3. Authenticated                               │
        │                                                │
        ▼                                                ▼
┌─────────────┐    4. Request     ┌─────────────┐    5. Process     ┌─────────────┐
│ Connection  │ ──────────────────▶│  Handler    │ ──────────────────▶│  Plugin     │
│    Pool     │                   │             │                   │             │
└─────────────┘                   └─────────────┘                   └─────────────┘
        │ 6. Response                                      │
        │                                                │
        ▼                                                ▼
┌─────────────┐    7. Response    ┌─────────────┐                   ┌─────────────┐
│   Client    │ ◀─────────────────│   Server    │ ◀─────────────────│  Plugin     │
│             │                   │             │                   │             │
└─────────────┘                   └─────────────┘                   └─────────────┘
```

### Core Components

1. **Protocol Handler**: Message framing, serialization, and protocol negotiation
2. **Transport Manager**: Connection management, load balancing, and transport abstraction
3. **Security Manager**: Authentication, authorization, and encryption
4. **Connection Pool**: Efficient connection reuse and lifecycle management
5. **Metrics Collector**: Performance monitoring and observability

## Message Types

### Core Messages

- **Handshake**: Initial connection setup and capability negotiation
- **Request**: Plugin operation requests with parameters and context
- **Response**: Operation results with success/failure status
- **Event**: Asynchronous notifications and status updates
- **Heartbeat**: Health checking and keepalive messages
- **Error**: Error reporting with detailed information

### Message Format

```
┌─────────────────┬──────────────────┬─────────────────┬─────────────────┐
│ Header (32 bytes) │ Payload Length   │ Payload         │ Checksum (8)    │
│                 │ (4 bytes)        │ (variable)      │ bytes           │
└─────────────────┴──────────────────┴─────────────────┴─────────────────┘
```

## Security

### Authentication

- **JWT Tokens**: Secure signed tokens with expiration
- **API Keys**: Simple key-based authentication
- **Certificates**: X.509 certificate-based authentication

### Encryption

- **AES-256-GCM**: High-performance authenticated encryption
- **ChaCha20-Poly1305**: Alternative cipher for different platforms
- **Key Rotation**: Automatic key rotation for enhanced security

### Authorization

- **Capability-Based**: Define what operations each plugin can perform
- **Resource Limits**: Memory, CPU, and network restrictions
- **Sandboxing**: Isolated execution environments

## Performance

### Benchmarks

| Metric | Value |
|--------|-------|
| Message Latency (p99) | < 1ms |
| Throughput | > 10,000 msg/s |
| Connection Setup | < 100ms |
| Memory Overhead | < 100MB |

### Optimization Features

- **Connection Multiplexing**: Multiple requests per connection
- **Compression**: Adaptive compression based on payload size
- **Batching**: Group multiple operations together
- **Caching**: Response caching for repeated requests

## Monitoring

### Metrics

- **Performance**: Latency, throughput, error rates
- **Resources**: Memory, CPU, connection usage
- **Business**: Plugin executions, success rates
- **Security**: Authentication attempts, authorization failures

### Tracing

- **Distributed Tracing**: OpenTelemetry integration
- **Request Correlation**: Trace IDs across process boundaries
- **Performance Profiling**: Identify bottlenecks

### Health Checks

- **Liveness**: Basic endpoint health
- **Readiness**: Service availability
- **Comprehensive**: Full system health assessment

## Error Handling

### Error Categories

- **Protocol Errors**: Malformed messages, version mismatches
- **Authentication Errors**: Invalid credentials, expired tokens
- **Connection Errors**: Network issues, timeouts
- **Plugin Errors**: Execution failures, resource exhaustion

### Recovery Strategies

- **Automatic Retries**: Exponential backoff with jitter
- **Circuit Breaking**: Fail-fast for repeated failures
- **Graceful Degradation**: Fallback to reduced functionality
- **Dead Letter Queues**: Failed message handling

## Development

### Building

```bash
cargo build --package crucible-services --features plugin-ipc
```

### Testing

```bash
# Run unit tests
cargo test --package crucible-services --lib plugin_ipc

# Run integration tests
cargo test --package crucible-services --test plugin_ipc_integration

# Run benchmarks
cargo bench --package crucible-services plugin_ipc
```

### Code Quality

```bash
# Format code
cargo fmt --package crucible-services

# Lint code
cargo clippy --package crucible-services -- -D warnings

# Security audit
cargo audit --package crucible-services
```

## Integration with Crucible

The IPC system integrates seamlessly with the existing Crucible architecture:

1. **Service Integration**: Works with the daemon's service manager
2. **Event System**: Integrates with the event routing system
3. **Configuration**: Uses the existing configuration framework
4. **Monitoring**: Integrates with the metrics and logging systems

## Migration Guide

### From Direct Function Calls

```rust
// Before (direct call)
let result = plugin.process_data(input).await?;

// After (IPC call)
let client = IpcClient::new(config).await?;
let connection = client.connect("plugin_name").await?;
let result = connection.send_request("process_data", input).await?;
```

### From Shared Memory IPC

The new IPC system provides better security, monitoring, and reliability compared to shared memory approaches:

- **Type Safety**: Strong typing and validation
- **Security**: Authentication and encryption
- **Monitoring**: Built-in metrics and tracing
- **Reliability**: Error handling and recovery

## Troubleshooting

### Common Issues

1. **Connection Refused**: Check if the server is running and the socket path is correct
2. **Authentication Failed**: Verify tokens and certificates
3. **Timeout Errors**: Increase timeout values or check plugin performance
4. **Memory Usage**: Monitor resource limits and adjust as needed

### Debug Mode

Enable debug logging for detailed troubleshooting:

```yaml
environment:
  debug_enabled: true
  log_level: debug
```

### Health Check

Use the built-in health check to verify system status:

```bash
curl http://localhost:9000/health
```

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests and documentation
5. Submit a pull request

### Development Guidelines

- Follow Rust best practices and conventions
- Add comprehensive tests for new features
- Update documentation for any API changes
- Ensure all tests pass before submitting

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Support

For support and questions:

- **Issues**: Create an issue on GitHub
- **Discussions**: Use GitHub Discussions for questions
- **Documentation**: Check the `/docs` directory for detailed guides

## Roadmap

See [roadmap.md](roadmap.md) for the detailed implementation plan and upcoming features.
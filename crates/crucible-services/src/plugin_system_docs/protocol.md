# Crucible Plugin IPC Protocol Specification

## Overview

This document specifies the Inter-Process Communication (IPC) protocol for process-based plugin communication in the Crucible knowledge management system. The protocol enables secure, high-performance communication between the daemon and isolated plugin processes.

## Design Goals

1. **Process Isolation**: Each plugin runs in its own isolated process for security and stability
2. **High Performance**: Low-latency communication with minimal overhead
3. **Type Safety**: Strong typing and validation for all messages
4. **Version Compatibility**: Graceful handling of different plugin versions
5. **Security**: Authentication, authorization, and sandboxing
6. **Observability**: Built-in monitoring and debugging capabilities
7. **Error Handling**: Robust error recovery and fault tolerance
8. **Scalability**: Support for many concurrent plugin instances

## Protocol Architecture

### Message Format

All messages follow a binary format using protobuf for serialization:

```
┌─────────────────┬──────────────────┬─────────────────┬─────────────────┐
│ Header (32 bytes) │ Payload Length   │ Payload         │ Checksum (8)    │
│                 │ (4 bytes)        │ (variable)      │ bytes           │
└─────────────────┴──────────────────┴─────────────────┴─────────────────┘
```

#### Header Structure

```
┌─────────────────┬─────────────────┬─────────────────┬─────────────────┐
│ Version (1)     │ Type (1)        │ Flags (2)       │ Reserved (4)    │
│ Message ID (8)  │ Session ID (8)   │ Timestamp (8)   │                 │
└─────────────────┴─────────────────┴─────────────────┴─────────────────┘
```

- **Version**: Protocol version (currently 1)
- **Type**: Message type (request, response, event, etc.)
- **Flags**: Message flags (compressed, encrypted, etc.)
- **Message ID**: Unique message identifier (UUID)
- **Session ID**: Plugin session identifier
- **Timestamp**: Unix timestamp in nanoseconds

### Transport Layer

#### Primary Transport: Unix Domain Sockets

- **Path**: `/tmp/crucible-plugins/<plugin-id>.sock`
- **Format**: Binary message framing
- **Advantages**: High performance, secure, filesystem-based

#### Fallback Transport: TCP

- **Port**: Dynamic allocation from range 9000-9999
- **Format**: Same binary protocol over TCP
- **Use case**: When Unix sockets aren't available

### Message Types

#### Core Messages

1. **Handshake**: Initial connection setup
2. **Heartbeat**: Health checking and keepalive
3. **Request**: Plugin execution requests
4. **Response**: Plugin execution responses
5. **Event**: Asynchronous notifications
6. **Error**: Error reporting
7. **Shutdown**: Graceful termination

#### Plugin-Specific Messages

1. **PluginRegister**: Register a new plugin
2. **PluginUnregister**: Unregister a plugin
3. **CapabilityQuery**: Query plugin capabilities
4. **ResourceRequest**: Request additional resources
5. **HealthCheck**: Plugin health status
6. **MetricsReport**: Performance metrics

## Security Model

### Authentication

1. **Token-based**: JWT tokens for plugin authentication
2. **Certificate-based**: X.509 certificates for daemon authentication
3. **Challenge-response**: Prevent replay attacks

### Authorization

1. **Capability-based**: Plugins declare required capabilities
2. **Resource limits**: Memory, CPU, and network restrictions
3. **Sandboxing**: Filesystem and network access controls

### Encryption

1. **TLS 1.3**: End-to-end encryption for TCP transport
2. **AES-256-GCM**: Payload encryption for sensitive data
3. **Forward secrecy**: Ephemeral key exchange

## Performance Optimizations

### Connection Management

1. **Connection pooling**: Reuse connections for multiple requests
2. **Multiplexing**: Multiple concurrent requests per connection
3. **Keep-alive**: Persistent connections with health checks

### Data Transfer

1. **Compression**: LZ4 compression for large payloads
2. **Streaming**: Chunked transfer for large data
3. **Zero-copy**: Memory mapping for large file transfers

### Caching

1. **Response caching**: Cache repeated requests
2. **Capability caching**: Cache plugin capabilities
3. **Connection caching**: Pool established connections

## Error Handling

### Error Categories

1. **Protocol Errors**: Malformed messages, version mismatches
2. **Authentication Errors**: Invalid credentials, expired tokens
3. **Authorization Errors**: Insufficient permissions
4. **Resource Errors**: Out of memory, CPU limits exceeded
5. **Plugin Errors**: Plugin execution failures
6. **System Errors**: Network issues, filesystem errors

### Recovery Strategies

1. **Retry Logic**: Exponential backoff with jitter
2. **Circuit Breaking**: Fail fast for repeated failures
3. **Graceful Degradation**: Fallback to reduced functionality
4. **Automatic Recovery**: Self-healing mechanisms

## Monitoring and Observability

### Metrics

1. **Message Latency**: Request/response timing
2. **Throughput**: Messages per second
3. **Error Rates**: Failed requests by category
4. **Resource Usage**: Memory, CPU, network
5. **Connection States**: Active, idle, failed connections

### Logging

1. **Structured Logging**: JSON format with correlation IDs
2. **Log Levels**: Debug, Info, Warning, Error, Critical
3. **Context Propagation**: Trace IDs across process boundaries
4. **Audit Logging**: Security-relevant events

### Tracing

1. **Distributed Tracing**: OpenTelemetry integration
2. **Span Correlation**: Link spans across processes
3. **Performance Profiling**: Identify bottlenecks
4. **Dependency Mapping**: Service topology visualization

## Version Compatibility

### Semantic Versioning

- **Major**: Breaking changes
- **Minor**: New features, backward compatible
- **Patch**: Bug fixes, fully compatible

### Compatibility Matrix

| Daemon Version | Plugin Version | Compatible |
|---------------|----------------|------------|
| 1.x           | 1.x            | ✓          |
| 1.x           | 2.x            | ✗          |
| 2.x           | 1.x            | ⚠          |
| 2.x           | 2.x            | ✓          |

### Migration Strategy

1. **Feature Flags**: Enable/disable new features
2. **Compatibility Mode**: Support older protocol versions
3. **Deprecation Warnings**: Notify of upcoming changes
4. **Automated Migration**: Tools for plugin updates

## Implementation Considerations

### Language Bindings

1. **Rust**: Primary implementation with tokio async runtime
2. **Python**: Plugin SDK with asyncio support
3. **JavaScript/TypeScript**: Node.js and browser support
4. **Go**: High-performance plugin implementation
5. **C/C++**: Maximum performance plugin interface

### Platform Support

1. **Linux**: Primary target with full feature support
2. **macOS**: Unix domain sockets and TCP support
3. **Windows**: Named pipes and TCP support
4. **Container**: Support for Docker and Kubernetes

### Testing

1. **Unit Tests**: Individual component testing
2. **Integration Tests**: End-to-end message flows
3. **Performance Tests**: Load testing and benchmarking
4. **Security Tests**: Penetration testing and vulnerability scanning
5. **Compatibility Tests**: Cross-version and cross-platform testing

## Reference Implementation

The reference implementation provides:

1. **Protocol Library**: Core message handling and serialization
2. **Client SDK**: Easy plugin development interface
3. **Server Library**: Daemon-side integration
4. **Examples**: Sample plugins and usage patterns
5. **Documentation**: API reference and tutorials
6. **Tools**: Debugging and testing utilities

## Future Enhancements

1. **HTTP/2 Support**: Web-based plugin communication
2. **gRPC Integration**: Alternative RPC framework
3. **WebSocket Support**: Real-time bidirectional communication
4. **Message Queuing**: Asynchronous message processing
5. **Service Mesh**: Advanced routing and load balancing
6. **Edge Computing**: Distributed plugin execution
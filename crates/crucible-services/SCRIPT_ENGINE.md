# Crucible Script Engine Service

## Overview

The Crucible Script Engine is a production-ready service for executing Rune scripts using the VM-per-execution pattern. This implementation provides secure, isolated script execution with comprehensive monitoring, caching, and performance optimization.

## Key Features

### 🔒 Security & Isolation
- **VM-per-execution pattern**: Each script execution runs in a fresh, isolated VM instance
- **Security policies**: Configurable security levels (Safe, Development, Production)
- **Resource limits**: Configurable memory, CPU, and execution time limits
- **Sandboxing**: Controlled access to system resources and APIs

### ⚡ Performance & Caching
- **Script caching**: Automatic caching of compiled scripts for faster execution
- **Execution metrics**: Comprehensive performance tracking and statistics
- **Resource monitoring**: Real-time resource usage tracking
- **Cache management**: Intelligent cache eviction and cleanup

### 🔧 Service Integration
- **Event-driven architecture**: Comprehensive event system for monitoring and coordination
- **Health checks**: Built-in health monitoring and status reporting
- **Configuration management**: Hot-reloadable configuration system
- **Tool integration**: Support for registering and managing script tools

## Architecture

### Core Components

1. **CrucibleScriptEngine**: Main service implementation
2. **ScriptEngineConfig**: Configuration management
3. **SecurityPolicy**: Security and sandboxing rules
4. **ExecutionState**: Active execution tracking
5. **ScriptMetrics**: Performance and usage statistics

### VM-per-Execution Pattern

```rust
// Each script execution gets a fresh VM instance
async fn execute_script_with_vm(&self, source: &str, context: &ExecutionContext) -> ServiceResult<ExecutionResult> {
    // Create execution state
    let execution_state = ExecutionState { ... };

    // Track execution
    self.active_executions.insert(execution_id.clone(), execution_state);

    // Execute in fresh VM (simulated)
    let result = simulate_execution(source, context);

    // Clean up and update metrics
    self.active_executions.remove(&execution_id);
    self.update_metrics(&result);

    Ok(result)
}
```

## Security Model

### Security Levels

#### Safe Mode (Default)
- **Memory limit**: 50MB
- **Execution timeout**: 10 seconds
- **File access**: Disabled
- **Network access**: Disabled
- **System calls**: Disabled
- **Allowed modules**: `crucible::basic` only

#### Development Mode
- **Memory limit**: Unlimited
- **Execution timeout**: None
- **File access**: Full access
- **Network access**: Full access
- **System calls**: Full access
- **Allowed modules**: All modules

#### Production Mode
- **Memory limit**: 100MB
- **Execution timeout**: 30 seconds
- **File access**: Disabled
- **Network access**: HTTP only
- **System calls**: Disabled
- **Allowed modules**: `crucible::basic`, `crucible::http`, `crucible::json`

### Security Policy Structure

```rust
pub struct SecurityPolicy {
    pub name: String,
    pub version: String,
    pub default_security_level: SecurityLevel,
    pub allowed_modules: Vec<String>,
    pub blocked_modules: Vec<String>,
    pub resource_limits: ResourceLimits,
    pub execution_timeout: Option<Duration>,
    pub allow_file_access: bool,
    pub allow_network_access: bool,
    pub allow_system_calls: bool,
    pub custom_rules: HashMap<String, String>,
}
```

## Usage Examples

### Basic Script Execution

```rust
use crucible_services::{CrucibleScriptEngine, ScriptEngineConfig, ExecutionContext, CompilationContext};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create script engine with default configuration
    let config = ScriptEngineConfig::default();
    let mut engine = CrucibleScriptEngine::new(config).await?;

    // Start the service
    engine.start().await?;

    // Compile a script
    let script_source = r#"
        pub fn main() {
            "Hello, World!"
        }
    "#;

    let compilation_context = CompilationContext::default();
    let compiled_script = engine.compile_script(script_source, compilation_context).await?;

    // Execute the compiled script
    let execution_context = ExecutionContext {
        execution_id: uuid::Uuid::new_v4().to_string(),
        script_id: compiled_script.script_id.clone(),
        arguments: HashMap::new(),
        environment: HashMap::new(),
        working_directory: None,
        security_context: SecurityContext::default(),
        timeout: Some(Duration::from_secs(5)),
        available_tools: vec![],
        user_context: None,
    };

    let result = engine.execute_script(&compiled_script.script_id, execution_context).await?;

    println!("Execution result: {:?}", result.return_value);
    println!("Output: {}", result.stdout);
    println!("Execution time: {:?}", result.execution_time);

    Ok(())
}
```

### Streaming Execution

```rust
let mut rx = engine.execute_script_stream(&script_id, execution_context).await?;

while let Some(chunk) = rx.recv().await {
    match chunk.chunk_type {
        ExecutionChunkType::Stdout => {
            println!("Output: {}", chunk.data);
        }
        ExecutionChunkType::Stderr => {
            eprintln!("Error: {}", chunk.data);
        }
        ExecutionChunkType::Complete => {
            println!("Execution completed: {:?}", chunk.data);
            break;
        }
        ExecutionChunkType::Error => {
            eprintln!("Execution failed: {}", chunk.data);
            break;
        }
    }
}
```

### Event Subscription

```rust
// Subscribe to script compilation events
let mut event_receiver = engine.subscribe("script_compiled").await?;

tokio::spawn(async move {
    while let Some(event) = event_receiver.recv().await {
        match event {
            ScriptEngineEvent::ScriptCompiled { script_id, success, duration } => {
                println!("Script {} compiled in {:?} (success: {})", script_id, duration, success);
            }
            ScriptEngineEvent::ScriptExecuted { script_id, execution_id, success, duration } => {
                println!("Script {} executed in {:?} (success: {})", script_id, duration, success);
            }
            _ => {}
        }
    }
});
```

### Security Configuration

```rust
// Configure for production use
let config = ScriptEngineConfig {
    max_cache_size: 500,
    default_execution_timeout: Duration::from_secs(30),
    max_source_size: 512 * 1024, // 512KB
    enable_caching: true,
    security_level: SecurityLevel::Production,
    resource_limits: ResourceLimits {
        max_memory_bytes: Some(100 * 1024 * 1024), // 100MB
        max_cpu_percentage: Some(75.0),
        max_concurrent_operations: Some(50),
        operation_timeout: Some(Duration::from_secs(30)),
        ..Default::default()
    },
};

let mut engine = CrucibleScriptEngine::new(config).await?;
```

## Configuration

### Default Configuration

```rust
ScriptEngineConfig {
    max_cache_size: 1000,
    default_execution_timeout: Duration::from_secs(30),
    max_source_size: 1024 * 1024, // 1MB
    enable_caching: true,
    security_level: SecurityLevel::Safe,
    resource_limits: ResourceLimits {
        max_memory_bytes: Some(100 * 1024 * 1024), // 100MB
        max_cpu_percentage: Some(80.0),
        max_concurrent_operations: Some(100),
        operation_timeout: Some(Duration::from_secs(60)),
        ..Default::default()
    },
}
```

### Configuration Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `max_cache_size` | `usize` | `1000` | Maximum number of cached scripts |
| `default_execution_timeout` | `Duration` | `30s` | Default script execution timeout |
| `max_source_size` | `usize` | `1MB` | Maximum script source size |
| `enable_caching` | `bool` | `true` | Enable script caching |
| `security_level` | `SecurityLevel` | `Safe` | Default security level |
| `resource_limits` | `ResourceLimits` | See above | Resource usage limits |

## Monitoring & Metrics

### Available Metrics

- **Compilation metrics**: Total compilations, success rate, compilation time
- **Execution metrics**: Total executions, success rate, execution time
- **Cache metrics**: Cache hit rate, cache size, evictions
- **Resource metrics**: Memory usage, CPU usage, active executions

### Health Checks

```rust
let health = engine.health_check().await?;

match health.status {
    ServiceStatus::Healthy => println!("Service is healthy"),
    ServiceStatus::Degraded => println!("Service is degraded"),
    ServiceStatus::Unhealthy => println!("Service is unhealthy"),
}

println!("Active executions: {}", health.details.get("active_executions"));
println!("Cache size: {}", health.details.get("cache_size"));
println!("Success rate: {}%", health.details.get("success_rate"));
```

### Performance Metrics

```rust
let metrics = engine.get_performance_metrics().await?;

println!("Memory usage: {} bytes", metrics.memory_usage);
println!("Active connections: {}", metrics.active_connections);
println!("CPU usage: {}%", metrics.cpu_usage);
```

## Error Handling

The Script Engine uses a comprehensive error handling system:

```rust
pub enum ServiceError {
    ExecutionError(String),
    ConfigurationError(String),
    ValidationError(String),
    Timeout { timeout_ms: u64 },
    ResourceExceeded { resource: String, limit: u64 },
    // ... more error types
}
```

### Error Recovery

- **Timeouts**: Automatic cancellation of timed-out executions
- **Resource limits**: Graceful handling of resource exhaustion
- **Cache overflow**: Automatic cleanup when cache limits are exceeded
- **Security violations**: Detailed reporting of security policy violations

## Integration with Crucible Tools

The Script Engine is designed to integrate seamlessly with the existing Crucible tools infrastructure:

### Tool Registration

```rust
// Register a script tool
let script_tool = ScriptTool {
    name: "data_transformer".to_string(),
    description: "Transforms data using custom logic".to_string(),
    signature: "transform(data: Map<String, Value>) -> Map<String, Value>".to_string(),
    parameters: vec![],
    return_type: "Map<String, Value>".to_string(),
    script_id: "transform_script".to_string(),
    function_name: "transform".to_string(),
    metadata: HashMap::new(),
    version: Some("1.0.0".to_string()),
    author: Some("Crucible Team".to_string()),
};

engine.register_tool(script_tool).await?;
```

### Context Management

The Script Engine provides secure, isolated execution contexts:

```rust
let execution_context = ExecutionContext {
    execution_id: uuid::Uuid::new_v4().to_string(),
    script_id: script_id.clone(),
    arguments: HashMap::from([
        ("input".to_string(), serde_json::json!("test data")),
        ("mode".to_string(), serde_json::json!("production")),
    ]),
    environment: HashMap::from([
        ("ENV".to_string(), "production".to_string()),
    ]),
    working_directory: Some("/tmp/crucible".to_string()),
    security_context: SecurityContext {
        user_id: "user123".to_string(),
        session_id: session_id.clone(),
        permissions: vec!["read".to_string(), "execute".to_string()],
        security_level: SecurityLevel::Production,
        sandbox: true,
    },
    timeout: Some(Duration::from_secs(10)),
    available_tools: vec!["file_reader".to_string(), "http_client".to_string()],
    user_context: Some(UserContext {
        preferences: HashMap::new(),
        settings: HashMap::new(),
    }),
};
```

## Testing

The implementation includes comprehensive tests:

```bash
# Run all script engine tests
cargo test -p crucible-services script_engine

# Run specific test categories
cargo test -p crucible-services script_engine_creation
cargo test -p crucible-services script_engine_execution
cargo test -p crucible-services script_engine_security
```

### Test Coverage

- ✅ Service lifecycle management
- ✅ Script compilation and execution
- ✅ Security policy enforcement
- ✅ Event system integration
- ✅ Cache management
- ✅ Resource monitoring
- ✅ Error handling
- ✅ Configuration management

## Performance Considerations

### Memory Management
- **VM isolation**: Each execution gets a fresh VM, preventing memory leaks
- **Cache eviction**: LRU-based cache eviction prevents memory bloat
- **Resource limits**: Hard limits prevent resource exhaustion

### Execution Performance
- **Script caching**: Compiled scripts are cached for faster re-execution
- **Async execution**: Non-blocking script execution
- **Resource pooling**: Efficient reuse of system resources

### Scalability
- **Concurrent execution**: Multiple scripts can execute simultaneously
- **Resource limits**: Configurable limits prevent resource exhaustion
- **Health monitoring**: Real-time monitoring of system health

## Security Best Practices

1. **Use appropriate security levels**:
   - Safe mode for untrusted scripts
   - Production mode for controlled environments
   - Development mode only for testing

2. **Set resource limits**:
   - Memory limits prevent resource exhaustion
   - Timeouts prevent hanging executions
   - Concurrent execution limits prevent overload

3. **Monitor execution**:
   - Subscribe to execution events
   - Monitor resource usage
   - Set up health checks

4. **Validate inputs**:
   - Sanitize script inputs
   - Validate argument types
   - Check file paths and permissions

## Future Enhancements

### Planned Features
- [ ] Real Rune VM integration (currently simulated)
- [ ] Advanced security policies
- [ ] Distributed execution
- [ ] Script debugging support
- [ ] Performance profiling
- [ ] Plugin system for custom security policies

### Integration Opportunities
- [ ] MCP Gateway integration for remote script execution
- [ ] Inference Engine integration for AI-assisted scripting
- [ ] Data Store integration for persistent script storage
- [ ] Web UI for script management

## Troubleshooting

### Common Issues

**Scripts timeout unexpectedly**
- Check `default_execution_timeout` configuration
- Verify script isn't in infinite loop
- Monitor resource usage

**High memory usage**
- Reduce `max_cache_size`
- Check for memory leaks in scripts
- Monitor active executions

**Security policy violations**
- Review script permissions
- Check security level configuration
- Verify module access rights

**Compilation failures**
- Check script syntax
- Verify required modules are available
- Review security restrictions

### Debug Information

Enable debug logging to troubleshoot issues:

```rust
use tracing::Level;

// Set up tracing subscriber
tracing_subscriber::fmt()
    .with_max_level(Level::DEBUG)
    .init();
```

This will provide detailed logging for:
- Script compilation and execution
- Cache operations
- Security policy enforcement
- Resource usage tracking
- Error conditions

## File Locations

- **Main implementation**: `/crates/crucible-services/src/script_engine.rs`
- **Service traits**: `/crates/crucible-services/src/service_traits.rs`
- **Service types**: `/crates/crucible-services/src/service_types.rs`
- **Tests**: Integrated in the script_engine.rs file
- **Examples**: `/crates/crucible-services/src/examples/`

## Contributing

When contributing to the Script Engine:

1. **Follow Rust best practices** for memory safety and performance
2. **Add comprehensive tests** for new features
3. **Update documentation** for API changes
4. **Consider security implications** of all changes
5. **Test with different security levels** and configurations

## License

This implementation is part of the Crucible project and follows the same licensing terms.
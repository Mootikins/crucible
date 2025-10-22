# ScriptEngine Service API Documentation

> Comprehensive API reference for the ScriptEngine service architecture

## Overview

The ScriptEngine service provides a production-ready API for secure script execution, service management, and migration operations. This document covers the complete API surface including service traits, types, and integration patterns.

## Core Service Traits

### ScriptService Trait

The primary interface for script execution services:

```rust
#[async_trait]
pub trait ScriptService: Send + Sync {
    /// Execute a script with the given context
    async fn execute_script(
        &self,
        script_id: &str,
        context: ExecutionContext,
    ) -> ServiceResult<ExecutionResult>;

    /// Compile a script for later execution
    async fn compile_script(
        &self,
        source: &str,
        context: CompilationContext,
    ) -> ServiceResult<CompiledScript>;

    /// Get service health status
    async fn health_check(&self) -> ServiceResult<HealthStatus>;

    /// Get performance metrics
    async fn get_metrics(&self) -> ServiceResult<PerformanceMetrics>;

    /// Start the service
    async fn start(&mut self) -> ServiceResult<()>;

    /// Stop the service
    async fn stop(&mut self) -> ServiceResult<()>;
}
```

### MigrationService Trait

Interface for tool migration operations:

```rust
#[async_trait]
pub trait MigrationService: Send + Sync {
    /// Migrate a tool to ScriptEngine
    async fn migrate_tool(
        &self,
        tool_name: &str,
        config: MigrationConfig,
    ) -> ServiceResult<MigrationResult>;

    /// Validate migration integrity
    async fn validate_migration(
        &self,
        tool_name: &str,
    ) -> ServiceResult<ValidationResult>;

    /// Rollback a migrated tool
    async fn rollback_tool(
        &self,
        tool_name: &str,
        config: RollbackConfig,
    ) -> ServiceResult<RollbackResult>;

    /// List migrated tools
    async fn list_migrations(
        &self,
        filter: MigrationFilter,
    ) -> ServiceResult<Vec<MigrationInfo>>;

    /// Get migration status
    async fn get_migration_status(
        &self,
    ) -> ServiceResult<MigrationStatus>;
}
```

### EventEmitter Trait

Interface for event-driven communication:

```rust
#[async_trait]
pub trait EventEmitter: Send + Sync {
    /// Emit an event
    async fn emit(&self, event: ServiceEvent) -> ServiceResult<()>;

    /// Subscribe to events
    async fn subscribe(
        &self,
        event_type: &str,
    ) -> ServiceResult<Box<dyn EventReceiver>>;

    /// Unsubscribe from events
    async fn unsubscribe(
        &self,
        subscription_id: &str,
    ) -> ServiceResult<()>;
}
```

## Core Types

### Execution Context

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionContext {
    /// Unique execution identifier
    pub execution_id: String,

    /// Script identifier
    pub script_id: String,

    /// Arguments passed to the script
    pub arguments: HashMap<String, Value>,

    /// Environment variables
    pub environment: HashMap<String, String>,

    /// Working directory (optional)
    pub working_directory: Option<String>,

    /// Security context
    pub security_context: SecurityContext,

    /// Execution timeout (optional)
    pub timeout: Option<Duration>,

    /// Available tools for the script
    pub available_tools: Vec<String>,

    /// User context information
    pub user_context: Option<UserContext>,
}
```

### Security Context

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityContext {
    /// User identifier
    pub user_id: String,

    /// Session identifier
    pub session_id: String,

    /// User permissions
    pub permissions: Vec<String>,

    /// Security level
    pub security_level: SecurityLevel,

    /// Sandbox mode enabled
    pub sandbox: bool,

    /// Additional security metadata
    pub metadata: HashMap<String, String>,
}
```

### Security Levels

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SecurityLevel {
    /// Safe mode - limited capabilities, sandboxed
    Safe,

    /// Development mode - full access for testing
    Development,

    /// Production mode - balanced security and functionality
    Production,
}
```

### Execution Result

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Execution identifier
    pub execution_id: String,

    /// Script identifier
    pub script_id: String,

    /// Return value from script
    pub return_value: Option<Value>,

    /// Standard output
    pub stdout: String,

    /// Standard error
    pub stderr: String,

    /// Execution duration
    pub execution_time: Duration,

    /// Success status
    pub success: bool,

    /// Error information (if failed)
    pub error: Option<ExecutionError>,

    /// Resource usage statistics
    pub resource_usage: ResourceUsage,

    /// Execution metadata
    pub metadata: HashMap<String, Value>,
}
```

### Resource Usage

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    /// Memory usage in bytes
    pub memory_bytes: u64,

    /// CPU usage percentage
    pub cpu_percentage: f64,

    /// Number of operations executed
    pub operations_count: u64,

    /// Network I/O bytes
    pub network_io_bytes: u64,

    /// File I/O bytes
    pub file_io_bytes: u64,
}
```

## Service Configuration

### ScriptEngine Configuration

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptEngineConfig {
    /// Maximum cache size for compiled scripts
    pub max_cache_size: usize,

    /// Default execution timeout
    pub default_execution_timeout: Duration,

    /// Maximum source code size
    pub max_source_size: usize,

    /// Enable script caching
    pub enable_caching: bool,

    /// Default security level
    pub security_level: SecurityLevel,

    /// Resource limits
    pub resource_limits: ResourceLimits,

    /// Service discovery settings
    pub discovery: DiscoveryConfig,

    /// Health check configuration
    pub health_check: HealthCheckConfig,
}
```

### Resource Limits

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum memory usage in bytes
    pub max_memory_bytes: Option<u64>,

    /// Maximum CPU percentage
    pub max_cpu_percentage: Option<f64>,

    /// Maximum concurrent operations
    pub max_concurrent_operations: Option<u64>,

    /// Operation timeout
    pub operation_timeout: Option<Duration>,

    /// Maximum script execution time
    pub max_execution_time: Option<Duration>,

    /// Maximum network requests
    pub max_network_requests: Option<u64>,
}
```

## Event System

### Service Events

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum ServiceEvent {
    /// Script compilation completed
    ScriptCompiled {
        script_id: String,
        success: bool,
        duration: Duration,
        error: Option<String>,
    },

    /// Script execution completed
    ScriptExecuted {
        script_id: String,
        execution_id: String,
        success: bool,
        duration: Duration,
        resource_usage: ResourceUsage,
    },

    /// Service health status changed
    HealthStatusChanged {
        service_id: String,
        status: ServiceStatus,
        timestamp: DateTime<Utc>,
    },

    /// Migration started
    MigrationStarted {
        tool_name: String,
        migration_id: String,
    },

    /// Migration completed
    MigrationCompleted {
        tool_name: String,
        migration_id: String,
        success: bool,
        result: MigrationResult,
    },

    /// Service registered
    ServiceRegistered {
        service_id: String,
        service_type: String,
        metadata: HashMap<String, Value>,
    },

    /// Service unregistered
    ServiceUnregistered {
        service_id: String,
        reason: String,
    },
}
```

### Service Status

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ServiceStatus {
    /// Service is healthy and operating normally
    Healthy,

    /// Service is degraded but still functional
    Degraded,

    /// Service is unhealthy and not functional
    Unhealthy,

    /// Service is starting up
    Starting,

    /// Service is shutting down
    Stopping,

    /// Service is stopped
    Stopped,
}
```

## Migration API

### Migration Configuration

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationConfig {
    /// Security level for migrated tool
    pub security_level: SecurityLevel,

    /// Force migration even if tool exists
    pub force: bool,

    /// Preserve original tool ID
    pub preserve_tool_id: bool,

    /// Create backup of original
    pub backup_original: bool,

    /// Validation settings
    pub validation: ValidationConfig,

    /// Migration metadata
    pub metadata: HashMap<String, String>,
}
```

### Migration Result

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationResult {
    /// Migration identifier
    pub migration_id: String,

    /// Tool name
    pub tool_name: String,

    /// Migration status
    pub status: MigrationStatus,

    /// New tool ID (if successful)
    pub new_tool_id: Option<String>,

    /// Migration duration
    pub duration: Duration,

    /// Validation results
    pub validation_results: Vec<ValidationResult>,

    /// Warnings generated during migration
    pub warnings: Vec<String>,

    /// Errors encountered during migration
    pub errors: Vec<String>,

    /// Migration metadata
    pub metadata: HashMap<String, Value>,
}
```

### Validation Result

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Validation identifier
    pub validation_id: String,

    /// Tool name
    pub tool_name: String,

    /// Overall validation status
    pub status: ValidationStatus,

    /// Specific validation checks
    pub checks: Vec<ValidationCheck>,

    /// Issues found (if any)
    pub issues: Vec<ValidationIssue>,

    /// Auto-fix results
    pub auto_fix_results: Option<AutoFixResult>,

    /// Validation timestamp
    pub timestamp: DateTime<Utc>,
}
```

## Error Handling

### Service Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("Execution error: {0}")]
    ExecutionError(String),

    #[error("Compilation error: {0}")]
    CompilationError(String),

    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Security violation: {0}")]
    SecurityViolation(String),

    #[error("Resource limit exceeded: {resource} limit: {limit}")]
    ResourceExceeded { resource: String, limit: u64 },

    #[error("Timeout error: operation timed out after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },

    #[error("Service not found: {service_id}")]
    ServiceNotFound { service_id: String },

    #[error("Script not found: {script_id}")]
    ScriptNotFound { script_id: String },

    #[error("Migration error: {0}")]
    MigrationError(String),

    #[error("Internal error: {0}")]
    InternalError(String),
}
```

### Service Result Type

```rust
pub type ServiceResult<T> = Result<T, ServiceError>;
```

## Usage Examples

### Basic Script Execution

```rust
use crucible_services::{CrucibleScriptEngine, ScriptEngineConfig, ExecutionContext};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create script engine
    let config = ScriptEngineConfig::default();
    let mut engine = CrucibleScriptEngine::new(config).await?;

    // Start the service
    engine.start().await?;

    // Create execution context
    let context = ExecutionContext {
        execution_id: uuid::Uuid::new_v4().to_string(),
        script_id: "example_script".to_string(),
        arguments: HashMap::from([
            ("input".to_string(), serde_json::json!("Hello, World!")),
        ]),
        environment: HashMap::new(),
        working_directory: None,
        security_context: SecurityContext::default(),
        timeout: Some(Duration::from_secs(30)),
        available_tools: vec![],
        user_context: None,
    };

    // Execute script
    let result = engine.execute_script("example_script", context).await?;

    println!("Execution successful: {}", result.success);
    println!("Output: {}", result.stdout);
    println!("Duration: {:?}", result.execution_time);

    Ok(())
}
```

### Service Health Monitoring

```rust
use crucible_services::{CrucibleScriptEngine, ScriptEngineConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ScriptEngineConfig::default();
    let mut engine = CrucibleScriptEngine::new(config).await?;
    engine.start().await?;

    // Check health
    let health = engine.health_check().await?;
    match health.status {
        ServiceStatus::Healthy => println!("Service is healthy"),
        ServiceStatus::Degraded => println!("Service is degraded"),
        ServiceStatus::Unhealthy => println!("Service is unhealthy"),
        _ => println!("Service status: {:?}", health.status),
    }

    // Get metrics
    let metrics = engine.get_metrics().await?;
    println!("Memory usage: {} bytes", metrics.memory_usage);
    println!("Active executions: {}", metrics.active_connections);
    println!("CPU usage: {}%", metrics.cpu_usage);

    Ok(())
}
```

### Event Subscription

```rust
use crucible_services::{CrucibleScriptEngine, ScriptEngineConfig, ServiceEvent};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ScriptEngineConfig::default();
    let mut engine = CrucibleScriptEngine::new(config).await?;
    engine.start().await?;

    // Subscribe to script execution events
    let mut event_receiver = engine.subscribe("script_executed").await?;

    // Handle events in background task
    tokio::spawn(async move {
        while let Some(event) = event_receiver.recv().await {
            match event {
                ServiceEvent::ScriptExecuted {
                    script_id,
                    execution_id,
                    success,
                    duration,
                    resource_usage
                } => {
                    println!("Script {} executed in {}ms (success: {})",
                        script_id, duration.as_millis(), success);
                    println!("Memory used: {} bytes", resource_usage.memory_bytes);
                }
                _ => {}
            }
        }
    });

    // Continue with other work...

    Ok(())
}
```

### Migration Operations

```rust
use crucible_services::{MigrationService, MigrationConfig, SecurityLevel};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let migration_service = get_migration_service(); // Assume this is implemented

    // Configure migration
    let config = MigrationConfig {
        security_level: SecurityLevel::Production,
        force: false,
        preserve_tool_id: true,
        backup_original: true,
        validation: ValidationConfig::default(),
        metadata: HashMap::new(),
    };

    // Migrate tool
    let result = migration_service.migrate_tool("search_tool", config).await?;

    if result.status == MigrationStatus::Completed {
        println!("Migration successful for tool: {}", result.tool_name);
        println!("New tool ID: {:?}", result.new_tool_id);

        // Validate migration
        let validation = migration_service.validate_migration("search_tool").await?;
        println!("Validation status: {:?}", validation.status);
    } else {
        println!("Migration failed: {:?}", result.errors);
    }

    Ok(())
}
```

## Integration Patterns

### Service Discovery

```rust
use crucible_services::{ServiceRegistry, ServiceInfo};

// Register a service
let service_info = ServiceInfo {
    service_id: "script-engine-1".to_string(),
    service_type: "ScriptEngine".to_string(),
    version: "1.0.0".to_string(),
    endpoint: "localhost:8080".to_string(),
    health_check_url: "http://localhost:8080/health".to_string(),
    metadata: HashMap::from([
        ("max_memory".to_string(), "1GB".to_string()),
        ("security_level".to_string(), "production".to_string()),
    ]),
};

service_registry.register_service(service_info).await?;

// Discover services
let script_engines = service_registry.discover_services("ScriptEngine").await?;
for service in script_engines {
    println!("Found service: {} at {}", service.service_id, service.endpoint);
}
```

### Configuration Management

```rust
use crucible_services::{ScriptEngineConfig, SecurityLevel, ResourceLimits};

// Load configuration from file
let config = ScriptEngineConfig::from_file("config.toml")?;

// Or create programmatically
let config = ScriptEngineConfig {
    max_cache_size: 1000,
    default_execution_timeout: Duration::from_secs(30),
    max_source_size: 1024 * 1024, // 1MB
    enable_caching: true,
    security_level: SecurityLevel::Production,
    resource_limits: ResourceLimits {
        max_memory_bytes: Some(100 * 1024 * 1024), // 100MB
        max_cpu_percentage: Some(80.0),
        max_concurrent_operations: Some(50),
        operation_timeout: Some(Duration::from_secs(60)),
        max_execution_time: Some(Duration::from_secs(300)),
        max_network_requests: Some(100),
    },
    discovery: DiscoveryConfig::default(),
    health_check: HealthCheckConfig::default(),
};
```

## Performance Considerations

### Caching Strategy

The ScriptEngine service provides intelligent caching for compiled scripts:

- **LRU Cache**: Automatically evicts least recently used scripts
- **Cache Size**: Configurable maximum cache size (default: 1000 scripts)
- **Cache Invalidation**: Automatic cache invalidation on script changes
- **Performance**: Cached scripts execute 10-100x faster than uncached ones

### Resource Management

- **Memory Limits**: Configurable memory limits prevent resource exhaustion
- **CPU Throttling**: CPU usage limits prevent system overload
- **Concurrent Execution**: Configurable limits on concurrent script executions
- **Timeout Protection**: Automatic termination of long-running scripts

### Monitoring and Metrics

- **Execution Metrics**: Track execution time, success rate, and resource usage
- **Health Monitoring**: Real-time health checks and status reporting
- **Event Tracking**: Comprehensive event system for debugging and monitoring
- **Performance Profiling**: Built-in performance profiling capabilities

## Security Features

### Security Levels

| Level | Memory Limit | Timeout | File Access | Network Access | Use Case |
|-------|-------------|---------|-------------|----------------|----------|
| Safe | 50MB | 10s | Disabled | Disabled | Untrusted scripts |
| Development | Unlimited | None | Full | Full | Testing and development |
| Production | 100MB | 30s | Disabled | HTTP only | Production workloads |

### Sandbox Features

- **VM Isolation**: Each script runs in a fresh, isolated VM
- **Resource Limits**: Hard limits on memory, CPU, and execution time
- **API Restrictions**: Controlled access to system APIs and resources
- **Network Filtering**: Configurable network access policies
- **File System Access**: Restricted file system access with permissions

## Testing

### Unit Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test;

    #[tokio::test]
    async fn test_script_execution() {
        let config = ScriptEngineConfig::default();
        let mut engine = CrucibleScriptEngine::new(config).await.unwrap();
        engine.start().await.unwrap();

        let context = ExecutionContext::default();
        let result = engine.execute_script("test_script", context).await.unwrap();

        assert!(result.success);
        assert!(result.execution_time.as_millis() > 0);
    }

    #[tokio::test]
    async fn test_service_health() {
        let config = ScriptEngineConfig::default();
        let mut engine = CrucibleScriptEngine::new(config).await.unwrap();
        engine.start().await.unwrap();

        let health = engine.health_check().await.unwrap();
        assert_eq!(health.status, ServiceStatus::Healthy);
    }
}
```

### Integration Testing

```rust
#[tokio::test]
async fn test_migration_workflow() {
    let migration_service = setup_test_migration_service().await;

    // Test migration
    let config = MigrationConfig::default();
    let result = migration_service.migrate_tool("test_tool", config).await.unwrap();
    assert_eq!(result.status, MigrationStatus::Completed);

    // Test validation
    let validation = migration_service.validate_migration("test_tool").await.unwrap();
    assert_eq!(validation.status, ValidationStatus::Passed);

    // Test rollback
    let rollback_config = RollbackConfig::default();
    let rollback = migration_service.rollback_tool("test_tool", rollback_config).await.unwrap();
    assert!(rollback.success);
}
```

## Best Practices

### Performance Optimization

1. **Enable Caching**: Always enable script caching for production workloads
2. **Set Appropriate Limits**: Configure resource limits based on your workload
3. **Monitor Metrics**: Regularly monitor performance metrics and health status
4. **Use Appropriate Security Levels**: Choose security levels based on trust requirements

### Security Best Practices

1. **Use Safe Mode**: Always use Safe mode for untrusted scripts
2. **Set Resource Limits**: Configure strict resource limits for production
3. **Monitor Security Events**: Subscribe to security-related events
4. **Regular Validation**: Regularly validate migrated tools and configurations

### Error Handling

1. **Handle All Errors**: Always handle ServiceError variants appropriately
2. **Use Timeouts**: Set appropriate timeouts for all operations
3. **Implement Retries**: Implement retry logic for transient failures
4. **Log Errors**: Log errors with sufficient context for debugging

---

For more detailed implementation examples, see the [ScriptEngine Service Documentation](../crates/crucible-services/SCRIPT_ENGINE.md) and the [CLI Service Integration Guide](../crates/crucible-cli/CLI_SERVICE_INTEGRATION.md).
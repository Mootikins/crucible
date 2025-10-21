# Crucible Services - Comprehensive Service Architecture

This crate provides a comprehensive service abstraction layer for the Crucible knowledge management system. It defines clean, extensible interfaces for the four core services in the simplified architecture, with proper async support, error handling, and performance monitoring.

## Architecture Overview

The service architecture is built around four main service components:

### Core Services

1. **McpGateway** - MCP (Model Context Protocol) protocol handling and tool management
2. **InferenceEngine** - AI/LLM inference, text generation, and reasoning
3. **ScriptEngine** - Rune script execution and tool runtime
4. **DataStore** - Database operations, persistence, and data management

### Common Infrastructure

All services implement common traits for:
- **Lifecycle Management** - Start, stop, restart operations
- **Health Checking** - Liveness and readiness probes
- **Configuration Management** - Dynamic configuration updates
- **Metrics & Monitoring** - Performance tracking and observability
- **Event Handling** - Pub/sub event communication
- **Resource Management** - Resource limits and cleanup

## Service Traits

### McpGateway

The MCP Gateway handles Model Context Protocol operations, providing tool registration, execution, and client session management.

```rust
use crucible_services::service_traits::*;

#[async_trait]
pub trait McpGateway: ServiceLifecycle + HealthCheck + Configurable + Observable + EventDriven + ResourceManager {
    // MCP Protocol Operations
    async fn initialize_connection(&self, client_id: &str, capabilities: McpCapabilities) -> ServiceResult<McpSession>;
    async fn close_connection(&self, session_id: &str) -> ServiceResult<()>;
    async fn list_connections(&self) -> ServiceResult<Vec<McpSession>>;

    // Tool Management
    async fn register_tool(&mut self, tool: ToolDefinition) -> ServiceResult<()>;
    async fn execute_tool(&self, request: McpToolRequest) -> ServiceResult<McpToolResponse>;
    async fn list_tools(&self) -> ServiceResult<Vec<ToolDefinition>>;

    // And many more methods...
}
```

**Key Capabilities:**
- Multi-client session management with timeout handling
- Tool registration and validation
- Async tool execution with cancellation support
- Capability negotiation with clients
- Resource usage monitoring and limits

### InferenceEngine

The Inference Engine provides AI/LLM capabilities including text generation, embeddings, and reasoning.

```rust
#[async_trait]
pub trait InferenceEngine: ServiceLifecycle + HealthCheck + Configurable + Observable + EventDriven + ResourceManager {
    // Model Management
    async fn load_model(&mut self, model_config: ModelConfig) -> ServiceResult<ModelInfo>;
    async fn list_models(&self) -> ServiceResult<Vec<ModelInfo>>;

    // Text Generation
    async fn generate_completion(&self, request: CompletionRequest) -> ServiceResult<CompletionResponse>;
    async fn generate_chat_completion(&self, request: ChatCompletionRequest) -> ServiceResult<ChatCompletionResponse>;

    // Embeddings
    async fn generate_embeddings(&self, request: EmbeddingRequest) -> ServiceResult<EmbeddingResponse>;

    // Advanced Inference
    async fn perform_reasoning(&self, request: ReasoningRequest) -> ServiceResult<ReasoningResponse>;
    async fn perform_tool_use(&self, request: ToolUseRequest) -> ServiceResult<ToolUseResponse>;

    // And many more methods...
}
```

**Key Capabilities:**
- Multiple model provider support (OpenAI, Anthropic, local models)
- Streaming text generation
- Embedding generation for semantic search
- Tool use and function calling
- Fine-tuning and model optimization

### ScriptEngine

The Script Engine provides secure Rune script execution with sandboxing and tool integration.

```rust
#[async_trait]
pub trait ScriptEngine: ServiceLifecycle + HealthCheck + Configurable + Observable + EventDriven + ResourceManager {
    // Script Compilation
    async fn compile_script(&mut self, source: &str, context: CompilationContext) -> ServiceResult<CompiledScript>;
    async fn get_compilation_errors(&self, script_id: &str) -> ServiceResult<Vec<CompilationError>>;

    // Script Execution
    async fn execute_script(&self, script_id: &str, context: ExecutionContext) -> ServiceResult<ExecutionResult>;
    async fn execute_script_stream(&self, script_id: &str, context: ExecutionContext) -> ServiceResult<mpsc::UnboundedReceiver<ExecutionChunk>>;

    // Tool Integration
    async fn register_tool(&mut self, tool: ScriptTool) -> ServiceResult<()>;
    async fn list_script_tools(&self) -> ServiceResult<Vec<ScriptTool>>;

    // Security and Sandboxing
    async fn set_security_policy(&mut self, policy: SecurityPolicy) -> ServiceResult<()>;
    async fn validate_script_security(&self, script_id: &str) -> ServiceResult<SecurityValidationResult>;

    // And many more methods...
}
```

**Key Capabilities:**
- Secure script compilation and execution
- Configurable security policies and sandboxing
- Tool registration and discovery
- Streaming script output
- Performance optimization with caching

### DataStore

The Data Store provides comprehensive database operations with support for documents, queries, transactions, and vector search.

```rust
#[async_trait]
pub trait DataStore: ServiceLifecycle + HealthCheck + Configurable + Observable + EventDriven + ResourceManager {
    // Database Operations
    async fn create_database(&mut self, name: &str, schema: Option<DatabaseSchema>) -> ServiceResult<DatabaseInfo>;
    async fn list_databases(&self) -> ServiceResult<Vec<DatabaseInfo>>;

    // CRUD Operations
    async fn create(&self, database: &str, data: DocumentData) -> ServiceResult<DocumentId>;
    async fn read(&self, database: &str, id: &str) -> ServiceResult<Option<DocumentData>>;
    async fn update(&self, database: &str, id: &str, data: DocumentData) -> ServiceResult<DocumentData>;
    async fn delete(&self, database: &str, id: &str) -> ServiceResult<bool>;

    // Query Operations
    async fn query(&self, database: &str, query: Query) -> ServiceResult<QueryResult>;
    async fn search(&self, database: &str, search_query: SearchQuery) -> ServiceResult<SearchResult>;
    async fn vector_search(&self, database: &str, vector: Vec<f32>, options: VectorSearchOptions) -> ServiceResult<VectorSearchResult>;

    // Transaction Support
    async fn begin_transaction(&self) -> ServiceResult<TransactionId>;
    async fn commit_transaction(&self, transaction_id: &str) -> ServiceResult<()>;
    async fn rollback_transaction(&self, transaction_id: &str) -> ServiceResult<()>;

    // And many more methods...
}
```

**Key Capabilities:**
- Full CRUD operations with schema validation
- Advanced querying with filters, sorting, and aggregation
- Full-text and semantic search
- ACID transactions
- Vector similarity search
- Backup and replication support

## Common Service Traits

### ServiceLifecycle

All services implement lifecycle management:

```rust
#[async_trait]
pub trait ServiceLifecycle: Send + Sync {
    async fn start(&mut self) -> ServiceResult<()>;
    async fn stop(&mut self) -> ServiceResult<()>;
    async fn restart(&mut self) -> ServiceResult<()>;
    fn is_running(&self) -> bool;
    fn service_name(&self) -> &str;
    fn service_version(&self) -> &str;
}
```

### HealthCheck

Health monitoring capabilities:

```rust
#[async_trait]
pub trait HealthCheck: Send + Sync {
    async fn health_check(&self) -> ServiceResult<ServiceHealth>;
    async fn liveness_check(&self) -> ServiceResult<bool>;
    async fn readiness_check(&self) -> ServiceResult<bool>;
}
```

### Configurable

Dynamic configuration management:

```rust
#[async_trait]
pub trait Configurable: Send + Sync {
    type Config: Clone + Send + Sync + Serialize + for<'de> Deserialize<'de>;

    async fn get_config(&self) -> ServiceResult<Self::Config>;
    async fn update_config(&mut self, config: Self::Config) -> ServiceResult<()>;
    async fn validate_config(&self, config: &Self::Config) -> ServiceResult<()>;
    async fn reload_config(&mut self) -> ServiceResult<()>;
}
```

### Observable

Metrics and performance monitoring:

```rust
#[async_trait]
pub trait Observable: Send + Sync {
    async fn get_metrics(&self) -> ServiceResult<ServiceMetrics>;
    async fn reset_metrics(&mut self) -> ServiceResult<()>;
    async fn get_performance_metrics(&self) -> ServiceResult<PerformanceMetrics>;
}
```

### EventDriven

Event communication between services:

```rust
#[async_trait]
pub trait EventDriven: Send + Sync {
    type Event: Clone + Send + Sync + Serialize + for<'de> Deserialize<'de>;

    async fn subscribe(&mut self, event_type: &str) -> ServiceResult<mpsc::UnboundedReceiver<Self::Event>>;
    async fn publish(&self, event: Self::Event) -> ServiceResult<()>;
    async fn handle_event(&mut self, event: Self::Event) -> ServiceResult<()>;
}
```

### ResourceManager

Resource limits and management:

```rust
#[async_trait]
pub trait ResourceManager: Send + Sync {
    async fn get_resource_usage(&self) -> ServiceResult<ResourceUsage>;
    async fn set_limits(&mut self, limits: ResourceLimits) -> ServiceResult<()>;
    async fn get_limits(&self) -> ServiceResult<ResourceLimits>;
    async fn cleanup_resources(&mut self) -> ServiceResult<()>;
}
```

## Error Handling

The service layer provides comprehensive error handling with specific error types:

```rust
use crucible_services::errors::*;

#[derive(Error, Debug)]
pub enum ServiceError {
    #[error("Tool service error: {message}")]
    ToolError { message: String },

    #[error("Database service error: {message}")]
    DatabaseError { message: String },

    #[error("LLM service error: {message}")]
    LLMError { message: String },

    #[error("Service configuration error: {message}")]
    ConfigurationError { message: String },

    #[error("Service operation timed out after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },

    // ... and more error variants
}
```

All service methods return `ServiceResult<T>` which is an alias for `Result<T, ServiceError>`.

## Usage Examples

### Basic Service Usage

```rust
use crucible_services::service_traits::*;
use crucible_services::examples::ExampleMcpGateway;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create and configure the service
    let config = McpGatewayConfig {
        max_sessions: 100,
        session_timeout: Duration::from_secs(3600),
        max_executions: 50,
        execution_timeout: Duration::from_secs(30),
    };

    let mut gateway = ExampleMcpGateway::new(config);

    // Start the service
    gateway.start().await?;

    // Check health
    let health = gateway.health_check().await?;
    println!("Service health: {:?}", health);

    // Register a tool
    let tool = ToolDefinition {
        name: "echo".to_string(),
        description: "Echoes the input text".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "text": {"type": "string"}
            },
            "required": ["text"]
        }),
        category: Some("utility".to_string()),
        version: Some("1.0.0".to_string()),
        author: Some("example".to_string()),
        tags: vec!["text".to_string(), "echo".to_string()],
        enabled: true,
        parameters: vec![],
    };

    gateway.register_tool(tool).await?;

    // Initialize a client connection
    let session = gateway.initialize_connection(
        "client_123",
        McpCapabilities::default()
    ).await?;

    // Execute a tool
    let tool_request = McpToolRequest {
        tool_name: "echo".to_string(),
        arguments: {
            let mut args = HashMap::new();
            args.insert("text".to_string(), serde_json::Value::String("Hello, World!".to_string()));
            args
        },
        session_id: session.session_id.clone(),
        request_id: "req_123".to_string(),
        timeout_ms: Some(5000),
    };

    let response = gateway.execute_tool(tool_request).await?;
    println!("Tool result: {:?}", response);

    // Stop the service
    gateway.stop().await?;

    Ok(())
}
```

### Service Composition

```rust
// Example of coordinating multiple services
async fn coordinate_services() -> ServiceResult<()> {
    // Create services
    let mut mcp_gateway = ExampleMcpGateway::new(mcp_config);
    // let mut inference_engine = ExampleInferenceEngine::new(inference_config);
    // let mut script_engine = ExampleScriptEngine::new(script_config);
    // let mut data_store = ExampleDataStore::new(datastore_config);

    // Start all services
    mcp_gateway.start().await?;
    // inference_engine.start().await?;
    // script_engine.start().await?;
    // data_store.start().await?;

    // Set up event communication
    let mut events = mcp_gateway.subscribe("tool_executed").await?;

    // Monitor health
    let health = mcp_gateway.health_check().await?;
    if health.status != ServiceStatus::Healthy {
        return Err(ServiceError::internal_error("Service not healthy"));
    }

    // Coordinate workflow
    // 1. Receive tool request via MCP Gateway
    // 2. Execute script in Script Engine
    // 3. Use Inference Engine for processing
    // 4. Store results in Data Store

    // Cleanup
    mcp_gateway.stop().await?;
    Ok(())
}
```

## Performance Considerations

The service architecture is designed with performance in mind:

### Async/Await Support
All service methods use async/await for non-blocking operations, allowing high concurrency.

### Resource Management
Services track memory usage, CPU usage, and other resources with configurable limits.

### Metrics and Monitoring
Comprehensive metrics collection for performance analysis and optimization.

### Event-Driven Communication
Services communicate via events to minimize coupling and improve scalability.

### Streaming Support
Many operations support streaming results to handle large data sets efficiently.

### Caching and Optimization
Services include built-in caching mechanisms and performance optimization options.

## Testing

The crate includes comprehensive tests and examples:

```bash
# Run tests
cargo test

# Run examples (if examples feature is enabled)
cargo run --features examples --example mcp_gateway_usage

# Run with logging
RUST_LOG=debug cargo test
```

## Feature Flags

- `examples`: Include implementation examples and usage patterns
- `full`: Enable all optional features

## Dependencies

The service layer uses these key dependencies:

- `async-trait`: Async trait support
- `serde`: Serialization/deserialization
- `thiserror`: Error handling
- `chrono`: Date/time handling
- `uuid`: Unique identifier generation
- `tokio`: Async runtime

## Contributing

When implementing services:

1. Follow the trait interfaces exactly
2. Implement proper error handling with `ServiceError`
3. Use async/await for all I/O operations
4. Provide comprehensive logging
5. Include resource limits and monitoring
6. Handle edge cases and cleanup properly
7. Write comprehensive tests

## License

This crate is part of the Crucible project and follows the same license terms.
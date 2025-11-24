# Advanced Tool Architecture Implementation Guide

## Overview

This guide provides a comprehensive roadmap for implementing advanced tool use capabilities in Crucible, inspired by Anthropic's advanced tool use patterns. The implementation focuses on **Rune-based execution** rather than Python to maintain security, performance, and integration consistency.

## Architecture Summary

### Key Components

1. **Enhanced ToolExecutor Trait** - Extended interface for advanced tool operations
2. **Rune Execution Engine** - Sandboxed scripting environment for dynamic tool execution
3. **MCP Schema Converter** - Automatic conversion of MCP tool definitions to Rune functions
4. **Tool Discovery System** - Search, filtering, and deferred loading capabilities
5. **Execution Context Bridge** - Secure communication between Rust and Rune environments

### Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| **Execution Environment** | Rune (not Python) | Better security, native integration, smaller attack surface |
| **Tool Discovery** | Deferred loading + search | 85% token reduction, efficient memory usage |
| **Schema Conversion** | MCP → Rune automatic conversion | Ecosystem compatibility, dynamic updates |
| **Sandboxing** | Resource limits + permissions | Security isolation without breaking functionality |

## Implementation Roadmap

### Phase 1: Core Infrastructure (Week 1-2)

#### 1.1 Enhanced ToolExecutor Trait
```rust
// crates/crucible-core/src/traits/tools.rs

#[async_trait]
pub trait ToolExecutor: Send + Sync {
    // Existing methods...

    // NEW: Advanced capabilities
    async fn search_tools(&self, query: &ToolSearchQuery) -> ToolResult<Vec<ToolDefinition>>;
    async fn execute_tools_batch(&self, requests: Vec<ToolExecutionRequest>) -> ToolResult<Vec<ToolExecutionResult>>;
    async fn get_tool_examples(&self, tool_name: &str) -> ToolResult<Vec<ToolExample>>;
    async fn validate_tool_parameters(&self, tool_name: &str, params: serde_json::Value) -> ToolResult<ValidationResult>;
    async fn defer_load_tool(&self, tool_name: &str) -> ToolResult<ToolDefinition>;
}
```

#### 1.2 Tool Search and Discovery
```rust
// crates/crucible-core/src/traits/tools.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSearchQuery {
    pub query: String,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub permissions: Vec<String>,
    pub limit: Option<usize>,
    pub exact_match: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionRequest {
    pub tool_name: String,
    pub parameters: serde_json::Value,
    pub context: ExecutionContext,
    pub timeout: Option<Duration>,
    pub priority: ExecutionPriority,
}
```

### Phase 2: Rune Execution Engine (Week 3-4)

#### 2.1 Enhanced Rune Runtime
```rust
// crates/crucible-plugins/src/runtime.rs

use rune::{Context, Diagnostics, Source, Unit, Vm};
use std::sync::Arc;
use tokio::sync::Semaphore;

pub struct AdvancedRuneRuntime {
    context: Arc<Context>,
    execution_semaphore: Arc<Semaphore>,
    resource_limits: ResourceLimits,
    tool_registry: Arc<ToolRegistry>,
}

#[derive(Debug, Clone)]
pub struct ResourceLimits {
    pub max_memory: usize,
    pub max_execution_time: Duration,
    pub max_instructions: u64,
    pub allowed_modules: HashSet<String>,
}

impl AdvancedRuneRuntime {
    pub async fn execute_tool_script(
        &self,
        script: &str,
        params: serde_json::Value,
        context: &ExecutionContext,
    ) -> Result<serde_json::Value, ExecutionError> {
        // 1. Validate script permissions
        // 2. Parse and compile script
        // 3. Execute with resource limits
        // 4. Handle results and errors
    }
}
```

#### 2.2 Security Sandbox
```rust
// crates/crucible-plugins/src/sandbox.rs

pub struct SandboxConfig {
    pub allow_filesystem: bool,
    pub allowed_paths: Vec<PathBuf>,
    pub allow_network: bool,
    pub allowed_domains: Vec<String>,
    pub max_memory: usize,
    pub max_cpu_time: Duration,
}

pub struct SandboxExecutor {
    config: SandboxConfig,
    metrics: ExecutionMetrics,
}
```

### Phase 3: MCP Schema Conversion (Week 5-6)

#### 3.1 Schema to Rune Converter
```rust
// crates/crucible-tools/src/schema_converter.rs

pub struct McpToRuneConverter {
    type_mapper: TypeMapper,
    function_generator: FunctionGenerator,
}

impl McpToRuneConverter {
    pub fn convert_tool_definition(&self, mcp_tool: &McpToolDefinition) -> Result<RuneFunction, ConversionError> {
        // 1. Parse JSON Schema parameters
        // 2. Generate Rune type definitions
        // 3. Create function wrapper with validation
        // 4. Add error handling and logging
    }

    pub fn generate_rune_function(&self, tool_def: &ToolDefinition) -> Result<String, GenerationError> {
        // Generate complete Rune function code
    }
}
```

#### 3.2 Automatic Function Injection
```rust
// crates/crucible-plugins/src/injection.rs

pub struct FunctionInjector {
    runtime: Arc<AdvancedRuneRuntime>,
    converter: Arc<McpToRuneConverter>,
}

impl FunctionInjector {
    pub async fn inject_mcp_tools(&self, mcp_tools: &[McpToolDefinition]) -> Result<InjectionResult, InjectionError> {
        for tool in mcp_tools {
            let rune_func = self.converter.convert_tool_definition(tool)?;
            self.runtime.register_function(rune_func).await?;
        }
        Ok(InjectionResult::new(mcp_tools.len()))
    }
}
```

### Phase 4: Tool Examples and Validation (Week 7-8)

#### 4.1 Examples Framework
```rust
// crates/crucible-core/src/examples.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExampleLibrary {
    examples: HashMap<String, Vec<ToolExample>>,
    validation_rules: HashMap<String, ValidationRule>,
}

impl ToolExampleLibrary {
    pub fn validate_parameters(&self, tool_name: &str, params: &serde_json::Value) -> ValidationResult {
        // 1. Check against known examples
        // 2. Apply validation rules
        // 3. Suggest corrections if invalid
    }

    pub fn suggest_improvements(&self, tool_name: &str, params: &serde_json::Value) -> Vec<Suggestion> {
        // ML-based parameter improvement suggestions
    }
}
```

#### 4.2 Parameter Coercion and Validation
```rust
// crates/crucible-core/src/validation.rs

pub struct ParameterValidator {
    schema_registry: Arc<SchemaRegistry>,
    coercion_rules: Vec<CoercionRule>,
}

impl ParameterValidator {
    pub fn coerce_and_validate(&self, params: serde_json::Value, schema: &JsonSchema) -> Result<ValidatedParameters, ValidationError> {
        // 1. Apply type coercion rules
        // 2. Validate against schema
        // 3. Fill in optional parameters with defaults
        // 4. Return validated, ready-to-use parameters
    }
}
```

### Phase 5: Enhanced MCP Server (Week 9-10)

#### 5.1 Advanced MCP Features
```rust
// crates/crucible-tools/src/advanced_mcp_server.rs

#[tool_router]
impl AdvancedMcpServer {
    #[tool(description = "Search available tools by query, category, or tags")]
    pub async fn search_tools(
        &self,
        params: Parameters<ToolSearchParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let results = self.tool_executor.search_tools(&params.into()).await?;
        Ok(CallToolResult::success(serde_json::to_value(results)?))
    }

    #[tool(description = "Execute multiple tools in parallel with batch optimization")]
    pub async fn execute_tools_batch(
        &self,
        params: Parameters<BatchExecutionParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let requests = params.into_inner().requests;
        let results = self.tool_executor.execute_tools_batch(requests).await?;
        Ok(CallToolResult::success(serde_json::to_value(results)?))
    }

    #[tool(description = "Get examples and usage patterns for a specific tool")]
    pub async fn get_tool_examples(
        &self,
        params: Parameters<ToolExamplesParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let examples = self.tool_executor.get_tool_examples(&params.tool_name).await?;
        Ok(CallToolResult::success(serde_json::to_value(examples)?))
    }
}
```

## Key Implementation Details

### 1. Security Considerations

#### Resource Limiting
```rust
pub const DEFAULT_RESOURCE_LIMITS: ResourceLimits = ResourceLimits {
    max_memory: 64 * 1024 * 1024, // 64MB
    max_execution_time: Duration::from_secs(30),
    max_instructions: 1_000_000,
    allowed_modules: HashSet::from(["std", "json", "collections"]),
};
```

#### Permission System
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum ToolPermission {
    Read,
    Write,
    Network,
    Filesystem { path: PathBuf },
    Admin,
}

pub struct PermissionContext {
    pub user_id: String,
    pub session_id: String,
    pub permissions: HashSet<ToolPermission>,
    pub resource_limits: ResourceLimits,
}
```

### 2. Performance Optimizations

#### Tool Caching
```rust
pub struct ToolCache {
    compiled_functions: LruCache<String, CompiledFunction>,
    execution_results: LruCache<String, CachedResult>,
    search_index: SearchIndex,
}

impl ToolCache {
    pub async fn get_or_compile_function(&mut self, tool_name: &str, source: &str) -> Result<CompiledFunction, CompilationError> {
        if let Some(compiled) = self.compiled_functions.get(tool_name) {
            return Ok(compiled.clone());
        }

        let compiled = self.compile_function(source).await?;
        self.compiled_functions.put(tool_name.to_string(), compiled.clone());
        Ok(compiled)
    }
}
```

#### Search Indexing
```rust
pub struct ToolSearchIndex {
    // BM25 for keyword search
    keyword_index: tantivy::Index,
    // Embedding index for semantic search
    embedding_index: Arc<dyn EmbeddingIndex>,
    // Category and tag indexes
    category_index: HashMap<String, Vec<String>>,
    tag_index: HashMap<String, Vec<String>>,
}
```

### 3. Error Handling and Recovery

#### Comprehensive Error Types
```rust
#[derive(Debug, thiserror::Error)]
pub enum ToolExecutionError {
    #[error("Tool not found: {tool_name}")]
    ToolNotFound { tool_name: String },

    #[error("Compilation failed: {source}")]
    CompilationFailed { source: String, errors: Vec<String> },

    #[error("Execution timeout after {duration}")]
    ExecutionTimeout { duration: Duration },

    #[error("Resource limit exceeded: {limit}")]
    ResourceLimitExceeded { limit: String },

    #[error("Permission denied: {permission}")]
    PermissionDenied { permission: String },

    #[error("Parameter validation failed: {errors}")]
    ValidationFailed { errors: Vec<ValidationError> },
}
```

## Testing Strategy

### 1. Unit Tests
- Tool executor interface compliance
- Schema conversion accuracy
- Parameter validation logic
- Resource limit enforcement

### 2. Integration Tests
- MCP server with advanced features
- Rune runtime security sandbox
- End-to-end tool execution workflows
- Performance benchmarking

### 3. Security Tests
- Sandbox escape attempts
- Resource limit exhaustion
- Permission boundary violations
- Malicious script execution

### 4. Performance Tests
- Large tool library loading (1000+ tools)
- Concurrent execution scaling
- Search performance with complex queries
- Memory usage under load

## Migration Guide

### From Basic MCP Server

1. **Incremental Migration**
   - Keep existing MCP server running
   - Deploy advanced server alongside
   - Gradually migrate client connections
   - Deprecate old server after validation

2. **Backward Compatibility**
   - Support existing MCP tool definitions
   - Maintain current authentication methods
   - Provide fallback for unsupported features

3. **Configuration Migration**
   ```yaml
   # Existing config
   mcp_server:
     enabled: true
     tools: ["notes", "search", "kiln"]

   # Advanced config
   advanced_mcp:
     enabled: true
     execution_engine: "rune"
     resource_limits:
       memory: "64MB"
       timeout: "30s"
     search:
       indexing: true
       semantic_search: true
   ```

## Success Metrics

### Performance Targets
- **Tool Discovery**: <100ms for searching 1000+ tools
- **Execution Start**: <50ms cold start, <10ms warm
- **Batch Execution**: 5x improvement over sequential calls
- **Memory Usage**: <200MB for 1000 loaded tools

### Quality Metrics
- **Parameter Validation Accuracy**: >95%
- **Example Coverage**: >80% of tools have examples
- **Security Incident Rate**: 0 sandbox escapes
- **Uptime**: >99.9% for execution engine

### Developer Experience
- **Tool Integration**: <30 minutes to add new tool
- **Documentation Coverage**: 100% for public APIs
- **Error Message Quality**: Actionable errors with suggestions
- **Debug Support**: Full execution tracing and logging

This comprehensive implementation guide provides everything needed to build advanced tool use capabilities in Crucible while maintaining security, performance, and developer productivity.
# Dependency Injection Architecture

## Overview

The Crucible project now implements proper **Dependency Inversion Principle (DIP)** with constructor-based dependency injection via the Builder pattern. This document describes the architecture, rationale, and usage patterns.

## Architecture Principles

### 1. Dependency Inversion Principle

**High-level modules do not depend on low-level modules. Both depend on abstractions.**

```
┌─────────────────┐
│   crucible-cli  │  (Composition Root - creates and wires dependencies)
└────────┬────────┘
         │ depends on
         ▼
┌─────────────────┐
│  crucible-core  │  (Defines trait abstractions)
└────────┬────────┘
         │ defines traits
         ▼
┌─────────────────────────┐
│  Storage, Parser, Tools │  (Trait abstractions)
└────────┬────────────────┘
         │ implemented by
         ▼
┌──────────────────────┐
│ crucible-surrealdb   │  (Concrete implementation)
└──────────────────────┘
```

### 2. Core as Orchestrator

`crucible-core` serves two purposes:
1. **Defines abstractions** - Traits for Storage, Parser, Tools, Agents
2. **Orchestrates business logic** - Coordinates between abstractions

This is NOT "Core as Interface-Only" - Core contains actual business logic but delegates to injected implementations.

### 3. Composition Root

The **CLI** is the composition root - it:
1. Creates concrete implementations (e.g., `SurrealClient`)
2. Injects them into `CrucibleCore` via the builder
3. Passes the configured `Arc<CrucibleCore>` to commands

## Code Structure

### Trait Definitions (`crucible-core/src/traits/`)

```rust
/// Storage abstraction - database operations
#[async_trait]
pub trait Storage: Send + Sync {
    async fn query(&self, sql: &str, params: &QueryParams) -> Result<QueryResult, String>;
    async fn get_stats(&self) -> Result<BTreeMap<String, serde_json::Value>, String>;
    async fn list_tables(&self) -> Result<Vec<String>, String>;
    async fn initialize_schema(&self) -> Result<(), String>;
}

/// Parser abstraction - markdown parsing
#[async_trait]
pub trait MarkdownParser: Send + Sync {
    async fn parse(&self, content: &str) -> Result<ParsedDocument, String>;
}

/// Tool execution abstraction
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute_tool(&self, name: &str, params: Value, context: ExecutionContext)
        -> Result<ToolResult, String>;
    fn list_tools(&self) -> Vec<ToolDefinition>;
}
```

### Core Implementation (`crucible-core/src/crucible_core.rs`)

```rust
pub struct CrucibleCore {
    storage: Arc<dyn Storage>,                    // Required
    parser: Option<Arc<dyn MarkdownParser>>,      // Optional
    tools: Option<Arc<dyn ToolExecutor>>,         // Optional
}

impl CrucibleCore {
    /// Entry point for dependency injection
    pub fn builder() -> CrucibleCoreBuilder {
        CrucibleCoreBuilder::new()
    }

    /// Delegates to storage implementation
    pub async fn query(&self, query: &str) -> Result<Vec<BTreeMap<String, serde_json::Value>>, String> {
        let result = self.storage.query(query, &[]).await?;
        // Convert and return...
    }
}
```

### Builder Pattern (`crucible-core/src/crucible_core.rs`)

```rust
pub struct CrucibleCoreBuilder {
    storage: Option<Arc<dyn Storage>>,
    parser: Option<Arc<dyn MarkdownParser>>,
    tools: Option<Arc<dyn ToolExecutor>>,
}

impl CrucibleCoreBuilder {
    pub fn with_storage<S: Storage + 'static>(mut self, storage: S) -> Self {
        self.storage = Some(Arc::new(storage));
        self
    }

    pub fn with_parser<P: MarkdownParser + 'static>(mut self, parser: P) -> Self {
        self.parser = Some(Arc::new(parser));
        self
    }

    pub fn with_tools<T: ToolExecutor + 'static>(mut self, tools: T) -> Self {
        self.tools = Some(Arc::new(tools));
        self
    }

    pub fn build(self) -> Result<CrucibleCore, String> {
        let storage = self.storage
            .ok_or_else(|| "Storage implementation is required".to_string())?;

        Ok(CrucibleCore {
            storage,
            parser: self.parser,
            tools: self.tools,
        })
    }
}
```

### Trait Implementation (`crucible-surrealdb/src/surreal_client.rs`)

```rust
use crucible_core::traits::Storage;

#[async_trait]
impl Storage for SurrealClient {
    async fn query(&self, sql: &str, params: &QueryParams) -> Result<QueryResult, String> {
        // SurrealDB-specific implementation
    }

    async fn get_stats(&self) -> Result<BTreeMap<String, serde_json::Value>, String> {
        // SurrealDB-specific implementation
    }

    // ... other trait methods
}
```

### Composition Root (`crucible-cli/src/main.rs`)

```rust
#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = CliConfig::load(/* ... */)?;

    // Create storage implementation
    let storage_config = SurrealDbConfig {
        path: config.database_path_str()?,
        namespace: "crucible".to_string(),
        database: "kiln".to_string(),
        max_connections: Some(10),
        timeout_seconds: Some(30),
    };

    let storage = SurrealClient::new(storage_config).await?;

    // Build Core with injected dependencies
    let core = Arc::new(
        CrucibleCore::builder()
            .with_storage(storage)
            .build()?
    );

    // Pass to commands
    match cli.command {
        None => commands::repl::execute(core, config, cli.non_interactive).await?,
        // ... other commands
    }

    Ok(())
}
```

## Benefits

### 1. Testability

Easy to mock dependencies for unit tests:

```rust
struct MockStorage;

#[async_trait]
impl Storage for MockStorage {
    async fn query(&self, _sql: &str, _params: &QueryParams) -> Result<QueryResult, String> {
        Ok(QueryResult { records: vec![], total_count: Some(0), execution_time_ms: None, has_more: false })
    }
    // ... other methods
}

#[test]
fn test_with_mock_storage() {
    let core = CrucibleCore::builder()
        .with_storage(MockStorage)
        .build()
        .unwrap();

    // Test Core logic without real database
}
```

### 2. Swappable Implementations

Change storage backend without modifying Core:

```rust
// Option 1: SurrealDB
let storage = SurrealClient::new(config).await?;

// Option 2: PostgreSQL (future)
let storage = PostgresClient::new(config).await?;

// Option 3: In-memory (testing)
let storage = InMemoryStorage::new();

// Core doesn't care which - all implement Storage trait
let core = CrucibleCore::builder()
    .with_storage(storage)
    .build()?;
```

### 3. Clear Dependency Graph

No circular dependencies:

```
crucible-cli  ──depends──>  crucible-core  <──implements──  crucible-surrealdb
                                  │
                                  └──> traits (Storage, Parser, Tools)
```

### 4. Single Responsibility

- **crucible-core**: Business logic + trait definitions
- **crucible-surrealdb**: SurrealDB implementation
- **crucible-cli**: Composition root + UI

## Usage Patterns

### Adding a New Storage Implementation

1. Create a new crate (e.g., `crucible-postgres`)
2. Depend on `crucible-core` for traits
3. Implement `Storage` trait
4. Use in CLI composition root

```rust
// crates/crucible-postgres/src/lib.rs
use crucible_core::traits::Storage;

pub struct PostgresClient { /* ... */ }

#[async_trait]
impl Storage for PostgresClient {
    // Implement all trait methods...
}

// crates/crucible-cli/src/main.rs
let storage = PostgresClient::new(config).await?;
let core = CrucibleCore::builder()
    .with_storage(storage)
    .build()?;
```

### Adding Parser Support

1. Implement `MarkdownParser` trait
2. Inject via builder

```rust
let parser = PulldownParser::new();
let core = CrucibleCore::builder()
    .with_storage(storage)
    .with_parser(parser)  // Now Core has parsing capabilities
    .build()?;
```

### Adding Tool Execution

1. Implement `ToolExecutor` trait
2. Inject via builder

```rust
let tools = RuneToolExecutor::new(tool_dir).await?;
let core = CrucibleCore::builder()
    .with_storage(storage)
    .with_tools(tools)  // Now Core can execute tools
    .build()?;
```

## Testing

All DI patterns are tested:

```bash
cargo test -p crucible-core --lib crucible_core
```

Tests verify:
- ✅ Builder pattern works with mock storage
- ✅ Builder requires storage (compilation error if missing)
- ✅ Core delegates to storage correctly
- ✅ Optional dependencies (parser, tools) work when not provided

## Future Enhancements

### Planned Additions

1. **Agent Providers** - Trait for pluggable AI agent implementations
2. **Event Bus** - Trait for pub/sub messaging between components
3. **Cache Layer** - Trait for pluggable caching strategies
4. **Authentication** - Trait for pluggable auth providers

### Multi-Backend Support

With this architecture, supporting multiple backends is straightforward:

```rust
// Database selection at runtime
let storage: Box<dyn Storage> = match config.db_type {
    DatabaseType::SurrealDB => Box::new(SurrealClient::new(config).await?),
    DatabaseType::PostgreSQL => Box::new(PostgresClient::new(config).await?),
    DatabaseType::SQLite => Box::new(SqliteClient::new(config).await?),
};

let core = CrucibleCore::builder()
    .with_storage(*storage)
    .build()?;
```

## References

- [Dependency Inversion Principle](https://en.wikipedia.org/wiki/Dependency_inversion_principle)
- [Builder Pattern in Rust](https://rust-unofficial.github.io/patterns/patterns/creational/builder.html)
- [Composition Root](https://blog.ploeh.dk/2011/07/28/CompositionRoot/)

---

**Last Updated**: 2025-11-02
**Status**: ✅ Implemented and Tested

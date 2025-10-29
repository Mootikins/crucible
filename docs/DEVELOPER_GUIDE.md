> **Note:** The `crucible-daemon` crate has been removed; references in this document remain for historical context.

# Crucible Developer Guide

> **Status**: Active Development Guide
> **Version**: 1.0.0
> **Date**: 2025-10-20
> **Purpose**: Comprehensive guide for developers working on the Crucible codebase

## Table of Contents

- [Development Environment Setup](#development-environment-setup)
- [Architecture Overview](#architecture-overview)
- [Crate Structure](#crate-structure)
- [Development Workflow](#development-workflow)
- [Creating New Features](#creating-new-features)
- [Testing Guidelines](#testing-guidelines)
- [Code Style and Conventions](#code-style-and-conventions)
- [Debugging and Profiling](#debugging-and-profiling)
- [Contributing Guidelines](#contributing-guidelines)

## Development Environment Setup

### Prerequisites

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Install dependencies
sudo apt install sqlite3 libsqlite3-dev pkg-config build-essential

# Install Node.js for frontend
curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
sudo apt install nodejs

# Install pnpm
npm install -g pnpm
```

### Initial Setup

```bash
# Clone repository
git clone https://github.com/matthewkrohn/crucible.git
cd crucible

# Install Rust toolchain
rustup update
rustup component add clippy rustfmt

# Install frontend dependencies
pnpm install

# Run development setup
./scripts/setup.sh
```

### IDE Configuration

#### VS Code
Install these extensions:
- Rust Analyzer
- Cargo
- ESLint
- Prettier
- Tailwind CSS IntelliSense

#### IntelliJ IDEA
- Install Rust plugin
- Configure Cargo.toml file associations
- Enable Tailwind CSS support

## Architecture Overview

### Service-Oriented Architecture

Crucible follows a service-oriented architecture with clear separation of concerns:

```
┌─────────────────────────────────────────────────────────────┐
│                     User Interfaces                         │
│  CLI/TUI  │  Desktop App  │  Web Interface  │  Mobile       │
└─────────────────────────────────────────────────────────────┘
                         ↓
┌─────────────────────────────────────────────────────────────┐
│                      Service Layer                          │
│  Search  │  Index  │  Agent  │  Tools  │  HotReload       │
│          │         │         │         │                  │
└─────────────────────────────────────────────────────────────┘
                         ↓
┌─────────────────────────────────────────────────────────────┐
│                    Core System                               │
│ crucible-core │ crucible-daemon (removed) │ crucible-tauri           │
└─────────────────────────────────────────────────────────────┘
                         ↓
┌─────────────────────────────────────────────────────────────┐
│                      Storage                                │
│    SurrealDB    │    DuckDB    │    File System           │
└─────────────────────────────────────────────────────────────┘
```

### Key Design Principles

1. **Service Layer**: Clean abstraction for system capabilities
2. **Plugin System**: Dynamic extensibility through Rune scripts
3. **Static Tools**: Compile-time tool generation with macros
4. **Async-First**: Non-blocking operations throughout the system
5. **Type Safety**: Strong typing with compile-time validation
6. **Performance**: Optimized for concurrent operations

## Crate Structure

### Foundation Layer

#### `crucible-core`
Core business logic and domain models
```bash
crates/crucible-core/
├── src/
│   ├── lib.rs              # Main library interface
│   ├── models/            # Domain models
│   ├── operations/        # Business operations
│   ├── agents/           # Agent definitions
│   └── error.rs          # Core error types
└── Cargo.toml
```

#### `crucible-config`
Configuration management and validation
```bash
crates/crucible-config/
├── src/
│   ├── lib.rs            # Configuration API
│   ├── models/           # Configuration models
│   ├── validation/       # Configuration validation
│   └── sources/          # Configuration sources
└── Cargo.toml
```

### Service Layer

> **Note:** The former `crucible-services` crate has been removed. Its lightweight service abstractions were folded into the CLI (`crates/crucible-cli`) and SurrealDB integration (`crates/crucible-surrealdb`). The directory layout below is retained for historical context only.

### Scripting & Tools Layer

#### `crucible-rune`
Dynamic tool execution with hot-reload
```bash
crates/crucible-rune/
├── src/
│   ├── lib.rs            # Rune runtime API
│   ├── loader/           # Script loading
│   ├── execution/        # Script execution
│   ├── hot_reload/       # Hot reload support
│   └── integration/      # Integration services
└── Cargo.toml
```

#### `crucible-tools`
Static system tools
```bash
crates/crucible-tools/
├── src/
│   ├── lib.rs            # Tool registry API
│   ├── search/           # Search tools
│   ├── metadata/         # Metadata extraction
│   ├── validation/       # Tool validation
│   └── registry/         # Tool registry
└── Cargo.toml
```

#### `crucible-rune-macros`
Procedural macros for tool generation
```bash
crates/crucible-rune-macros/
├── src/
│   ├── lib.rs            # Macro exports
│   ├── tool_macro.rs     # #[rune_tool] implementation
│   ├── metadata.rs       # Metadata extraction
│   └── schema.rs         # Schema generation
└── Cargo.toml
```

### Interface Layer

#### `crucible-cli`
Command-line interface and REPL
```bash
crates/crucible-cli/
├── src/
│   ├── main.rs          # CLI entry point
│   ├── repl/            # REPL implementation
│   ├── commands/        # CLI commands
│   ├── tui/             # Terminal UI
│   └── tools/           # CLI tools
├── tests/
│   └── integration.rs   # Integration tests
└── Cargo.toml
```

#### `crucible-tauri`
Desktop application backend
```bash
crates/crucible-tauri/
├── src/
│   ├── main.rs          # Tauri entry point
│   ├── commands/        # Tauri commands
│   ├── services/        # Desktop services
│   └── window/          # Window management
├── src-tauri/
│   ├── tauri.conf.json  # Tauri configuration
│   └── Cargo.toml       # Tauri Cargo.toml
└── Cargo.toml
```

### Storage Layer

#### `crucible-surrealdb`
SurrealDB integration
```bash
crates/crucible-surrealdb/
├── src/
│   ├── lib.rs          # SurrealDB service
│   ├── connection/     # Connection management
│   ├── queries/        # Query builders
│   └── migrations/     # Database migrations
└── Cargo.toml
```

#### `crucible-llm`
LLM service integration
```bash
crates/crucible-llm/
├── src/
│   ├── lib.rs          # LLM service
│   ├── providers/       # LLM providers
│   ├── embeddings/     # Embedding services
│   └── models/         # Model definitions
└── Cargo.toml
```

## Development Workflow

### Local Development

#### 1. Feature Development
```bash
# Create feature branch
git checkout -b feature/new-feature

# Run development server
pnpm dev

# Run tests
cargo test

# Run clippy
cargo clippy

# Format code
cargo fmt
```

#### 2. Testing
```bash
# Unit tests
cargo test

# Integration tests
cargo test --test integration

# All tests in specific crate
cargo test -p crucible-core

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_specific_function
```

#### 3. Building
```bash
# Build all crates
cargo build

# Build specific crate
cargo build -p crucible-cli

# Build with release profile
cargo build --release

# Run specific binary
cargo run -p crucible-cli -- stats
```

### CI/CD Pipeline

#### GitHub Actions Workflows
- **Continuous Integration**: Automated testing on PRs
- **Release Process**: Automated releases to crates.io
- **Documentation**: Auto-generated docs on pushes

#### Local CI Simulation
```bash
# Run all checks
./scripts/check.sh

# Run specific checks
./scripts/test.sh
./scripts/lint.sh
./scripts/format.sh
```

## Creating New Features

### 1. Adding a New Service

#### Step 1: Define Service Trait
```rust
// Legacy example (crate removed): crates/crucible-services/src/my_service.rs
use async_trait::async_trait;
use serde::Deserialize;

#[async_trait]
pub trait MyService: Send + Sync {
    async fn do_something(&self, input: Input) -> Result<Output, Error>;
}

#[derive(Debug, Deserialize)]
pub struct Input {
    pub param: String,
}

#[derive(Debug, Serialize)]
pub struct Output {
    pub result: String,
}
```

#### Step 2: Implement Service
```rust
// Legacy example (crate removed): crates/crucible-services/src/my_service.rs
pub struct MyServiceImpl {
    config: MyServiceConfig,
}

#[async_trait]
impl MyService for MyServiceImpl {
    async fn do_something(&self, input: Input) -> Result<Output, Error> {
        // Implementation
        Ok(Output { result: format!("Processed: {}", input.param) })
    }
}
```

#### Step 3: Register Service
```rust
// Legacy example (crate removed): crates/crucible-services/src/lib.rs
pub use my_service::{MyService, MyServiceImpl, MyServiceConfig};

// In service registry
impl ServiceRegistry {
    pub fn register_my_service(&mut self, config: MyServiceConfig) {
        let service = Box::new(MyServiceImpl::new(config));
        self.register("my_service", service);
    }
}
```

### 2. Creating a Static Tool

#### Step 1: Define Tool with Macro
```rust
// crates/crucible-tools/src/my_tool.rs
use crucible_rune_macros::rune_tool;
use crate::ToolResult;

#[rune_tool(
    desc = "Process input and return result",
    category = "utility",
    tags = ["processing", "transform"]
)]
pub fn process_input(input: String, options: Option<ProcessOptions>) -> ToolResult<String> {
    let options = options.unwrap_or_default();
    let result = match options.method {
        Method::Simple => format!("Simple: {}", input),
        Method::Advanced => format!("Advanced: {}", advanced_process(&input)),
    };

    ToolResult::Success(result)
}

#[derive(serde::Deserialize)]
pub struct ProcessOptions {
    pub method: Method,
    pub verbose: bool,
}

#[derive(serde::Deserialize)]
pub enum Method {
    Simple,
    Advanced,
}
```

#### Step 2: Register Tool
```rust
// crates/crucible-tools/src/lib.rs
pub use my_tool::{process_input, ProcessOptions, Method};

// In tool registry
impl ToolRegistry {
    pub fn register_my_tools(&mut self) {
        self.register_tool("process_input", process_input);
    }
}
```

### 3. Creating a Dynamic Tool

#### Step 1: Define Rune Script
```rune
// tools/my_tool.rn
pub fn analyze_data(data: array, options: map?) -> map {
    let analyzer = crucible_services::get_analysis_service();
    let result = analyzer.analyze(data, options.unwrap_or({}));

    {
        "status": "success",
        "analysis": result,
        "timestamp": crucible_rune::timestamp()
    }
}
```

#### Step 2: Enable Hot Reload
```yaml
# config/services.yaml
services:
  tools:
    hot_reload: true
    paths: ["./tools"]
```

### 4. Adding Tests

#### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_input() {
        let result = process_input(
            "test".to_string(),
            None,
        ).unwrap();

        assert_eq!(result, "Simple: test");
    }
}
```

#### Integration Tests
```rust
#[tokio::test]
async fn test_service_integration() {
    let service = MyServiceImpl::new(test_config());
    let result = service.do_something(Input {
        param: "test".to_string(),
    }).await.unwrap();

    assert_eq!(result.result, "Processed: test");
}
```

## Testing Guidelines

### Test Organization

#### 1. Unit Tests
- Place in `#[cfg(test)]` modules
- Test individual functions in isolation
- Mock external dependencies
- Focus on edge cases and error conditions

#### 2. Integration Tests
- Place in `tests/` directory
- Test multiple components together
- Use real dependencies when appropriate
- Test user-facing behavior

#### 3. Property Tests
```rust
#[cfg(test)]
mod prop_tests {
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_process_arbitrary_input(input in "\\PC*") {
            let result = process_input(input, None);
            assert!(result.is_ok());
            assert!(!result.unwrap().is_empty());
        }
    }
}
```

### Test Data

#### Fixtures
```rust
// tests/fixtures.rs
pub fn test_config() -> Config {
    Config {
        database_url: "sqlite::memory:".to_string(),
        // ... other config
    }
}

pub fn test_notes() -> Vec<Note> {
    vec![
        Note {
            id: "1".to_string(),
            title: "Test Note 1".to_string(),
            content: "Content 1".to_string(),
            // ... other fields
        },
        // ... more test data
    ]
}
```

### Test Utilities

#### Mock Services
```rust
// tests/mocks.rs
pub struct MockMyService {
    pub responses: HashMap<String, Result<Output, Error>>,
}

impl MockMyService {
    pub fn new() -> Self {
        Self { responses: HashMap::new() }
    }

    pub fn with_response(self, input: &str, response: Result<Output, Error>) -> Self {
        let mut responses = self.responses;
        responses.insert(input.to_string(), response);
        Self { responses }
    }
}

#[async_trait]
impl MyService for MockMyService {
    async fn do_something(&self, input: Input) -> Result<Output, Error> {
        self.responses.get(&input.param)
            .cloned()
            .unwrap_or_else(|| Err(Error::NoResponse))
    }
}
```

## Code Style and Conventions

### Rust Code Style

#### 1. Naming Conventions
```rust
// Functions: snake_case
pub fn process_document() {}

// Types: PascalCase
pub struct DocumentProcessor {}

// Constants: SCREAMING_SNAKE_CASE
pub const MAX_RETRIES: u32 = 3;

// Modules: snake_case
mod document_processing {}
```

#### 2. Error Handling
```rust
// Use appropriate error types
pub enum MyError {
    Io(std::io::Error),
    Validation(String),
    Service(String),
}

impl From<std::io::Error> for MyError {
    fn from(err: std::io::Error) -> Self {
        MyError::Io(err)
    }
}

// Return Result from functions
pub fn do_something() -> Result<MyResult, MyError> {
    // Implementation
}
```

#### 3. Async Patterns
```rust
// Use async/await properly
pub async fn process_async(input: String) -> Result<String, Error> {
    let result = tokio::spawn(async move {
        // Async work
        format!("Processed: {}", input)
    }).await?;

    Ok(result)
}

// Handle async errors properly
pub async fn handle_async_operation() -> Result<(), Error> {
    match some_async_operation().await {
        Ok(result) => {
            // Success handling
        }
        Err(e) => return Err(e),
    }
}
```

### TypeScript/Svelte Code Style

#### 1. Component Structure
```typescript
// components/MyComponent.svelte
<script lang="ts">
  import { onMount } from 'svelte';

  // Component state
  export let input: string = '';
  let result: string | null = null;

  // Lifecycle
  onMount(() => {
    // Initialization
  });

  // Functions
  async function process() {
    try {
      result = await myService.process(input);
    } catch (error) {
      console.error('Error:', error);
    }
  }
</script>

<!-- Template -->
<div class="my-component">
  <input bind:value={input} />
  <button on:click={process}>Process</button>
  {#if result}
    <div class="result">{result}</div>
  {/if}
</div>

<!-- Styles -->
<style>
  .my-component {
    /* Styles */
  }
</style>
```

#### 2. TypeScript Types
```typescript
// Define clear interfaces
interface ProcessOptions {
  method: 'simple' | 'advanced';
  verbose?: boolean;
}

interface ProcessResult {
  status: 'success' | 'error';
  data?: string;
  error?: string;
}

// Use proper typing
async function process(input: string, options: ProcessOptions = {}): Promise<ProcessResult> {
  // Implementation
}
```

## Debugging and Profiling

### 1. Debug Logging

#### Structured Logging
```rust
use tracing::{info, warn, error, debug};

pub fn process_document(doc: &Document) -> Result<(), Error> {
    info!("Processing document: {}", doc.id);

    match doc.validate() {
        Ok(_) => {
            debug!("Document validation passed");
            // Process document
        }
        Err(e) => {
            error!("Document validation failed: {}", e);
            return Err(Error::Validation(e));
        }
    }

    info!("Document processed successfully");
    Ok(())
}
```

#### Log Levels
- `TRACE`: Detailed tracing information
- `DEBUG`: Debug information for developers
- `INFO`: General information about system operation
- `WARN`: Warning conditions that should be handled
- `ERROR`: Error conditions that need attention

### 2. Debugging Tools

#### Debug Build
```bash
# Build with debug symbols
cargo build

# Run with debug info
RUST_LOG=debug cargo run -p crucible-cli

# Enable all tracing
RUST_LOG=trace cargo run -p crucible-cli
```

#### Memory Debugging
```rust
// Install debug tools
cargo add insta             # Snapshot testing
cargo add criterion        # Benchmarking
cargo add mem             # Memory tracking

// Use memory debugging
#[cfg(test)]
mod memory_tests {
    use super::*;

    #[test]
    fn test_memory_usage() {
        let start = memory_usage();
        // Run test
        let end = memory_usage();
        assert!(end - start < 1000); // Less than 1KB
    }
}
```

### 3. Performance Profiling

#### Benchmarking
```rust
#[cfg(test)]
mod benchmarks {
    use criterion::{black_box, criterion_group, criterion_main, Criterion};

    fn bench_search(c: &mut Criterion) {
        c.bench_function("search_notes", |b| {
            b.iter(|| {
                black_box(search_notes("test query".to_string()))
            })
        });
    }

    criterion_group!(benches, bench_search);
    criterion_main!(benches);
}
```

#### Flamegraphs
```bash
# Install profiling tools
cargo add flamegraph

# Generate flamegraph
cargo flamegraph --bin crucible-cli -- search "test"
```

## Contributing Guidelines

### 1. Code Review Process

#### Pull Request Checklist
- [ ] All tests pass
- [ ] Code follows style guidelines
- [ ] Documentation is updated
- [ ] Feature is tested
- [ ] Performance impact considered
- [ ] Security implications reviewed

#### Review Criteria
- **Correctness**: Code does what it's supposed to do
- **Performance**: No significant performance regressions
- **Maintainability**: Code is easy to understand and modify
- **Testability**: Code can be easily tested
- **Documentation**: Code is well documented

### 2. Commit Guidelines

#### Commit Message Format
```bash
# Feature addition
feat(service): add search service with hot-reload

# Bug fix
fix(tools): resolve metadata extraction bug

# Documentation
docs: update developer guide with new examples

# Performance
perf: optimize database query performance

# Breaking change
BREAKING CHANGE: remove deprecated MCP interface
```

#### Commit Best Practices
- Write clear, descriptive commit messages
- Keep commits focused on single changes
- Reference issues in commit messages
- Include tests for new features
- Run tests before committing

### 3. Release Process

#### Version Bumping
```bash
# Update version numbers
cargo bump patch  # Bug fix
cargo bump minor  # New feature
cargo bump major  # Breaking change
```

#### Release Checklist
- [ ] All tests pass
- [ ] Documentation updated
- [ ] Changelog updated
- [ ] Version numbers bumped
- [ ] Release notes written
- [ ] Published to crates.io
- [ ] Tags pushed to repository

### 4. Community Guidelines

#### Code of Conduct
- Be respectful and inclusive
- Focus on technical merits
- Welcome new contributors
- Provide constructive feedback
- Follow the project's values

#### Getting Help
- Join Discord community
- Ask questions on GitHub Discussions
- Check existing issues
- Review documentation
- Ask maintainers directly

---

*This developer guide will be updated as the project evolves. Check for the latest version in the documentation repository.*

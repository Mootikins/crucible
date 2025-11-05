# Project Context

## Purpose
Crucible is a high-performance knowledge management system that combines hierarchical organization, real-time collaboration, and AI agent integration. The system promotes **linked thinking** - the seamless connection and evolution of ideas across time and context - by routing every interface (CLI today, desktop/agent integrations tomorrow) through a shared `crucible-core` façade that orchestrates configuration, storage, agents, and tools behind the scenes.

**Key Goals:**
- Provide local-first knowledge management with advanced search capabilities
- Enable AI agents and human operators to share the same APIs and workflows
- Support real-time collaboration through CRDT-based document synchronization
- Deliver high performance with memory safety and comprehensive input validation
- Maintain a clean separation between presentation logic and core domain functionality

## Tech Stack

### Core Technologies
- **Rust** (1.75+): Core performance-critical components with tokio async runtime
- **SurrealDB**: Multi-model database with vector extensions and RocksDB backend
- **Yrs**: CRDT implementation for real-time collaboration
- **Tauri**: Desktop application framework with secure web-based UI

### Frontend Stack
- **Svelte 5**: Modern reactive frontend with TypeScript
- **Vite**: Build tool and development server
- **TypeScript** (5.3+): Type-safe frontend development

### CLI & Tools
- **Clap**: Command-line argument parsing with derive macros
- **Reedline**: Interactive terminal editing with syntax highlighting
- **Ratatui**: Terminal user interface components
- **Nucleo**: High-performance fuzzy search and picking

### AI & ML Integration
- **Transformers.js**: Local embedding generation with `@xenova/transformers`
- **OpenAI/Anthropic APIs**: Remote LLM integration through unified facade
- **Semantic Search**: Vector similarity with configurable embedding providers

### Development & Testing
- **Cargo Workspaces**: Multi-crate Rust project organization
- **Criterion**: Performance benchmarking with HTML reports
- **Mockito**: HTTP mocking for API testing
- **Tracing**: Structured logging and observability

## Project Conventions

### Code Style

#### Rust Code
- **Naming**: `snake_case` for functions/variables, `PascalCase` for types, `SCREAMING_SNAKE_CASE` for constants
- **Error Handling**: Comprehensive use of `Result<T, E>` with `anyhow` for application errors and `thiserror` for library errors
- **Async Patterns**: Consistent use of async/await with proper error propagation
- **Documentation**: Public APIs require `///` doc comments with examples
- **Testing**: Unit tests in same file, integration tests in `tests/` directory
- **Dependencies**: Minimal external dependencies, prefer built-in types where possible

#### TypeScript/Svelte Code
- **Naming**: `camelCase` for variables, `PascalCase` for components and types
- **Component Structure**: Svelte 5 composition API with `<script>` blocks using TypeScript
- **Error Boundaries**: Proper error handling with user-friendly error messages
- **Styling**: Utility-first CSS approach with consistent design tokens

### Architecture Patterns

#### Façade Pattern Implementation
The project uses a core-façade architecture where:
- **Presentation Layer** (CLI, Desktop UI) talks only to `crucible-core`
- **Core Orchestration** provides trait-based abstractions for storage, tools, and agents
- **Infrastructure Layer** implements concrete storage (SurrealDB), tools, and external integrations
- **Dependency Injection** through builder patterns enables testability and modularity

#### Domain-Centric Design
- **CRDT Documents**: Version-controlled document structures with conflict resolution
- **Knowledge Graph**: Relationship mapping between documents with metadata indexing
- **Tool Registry**: Extensible system for adding custom tools and agents
- **Configuration Management**: Centralized config with validation and hot-reloading

#### Storage Abstraction
```rust
// Storage trait enables multiple backends
pub trait Storage: Send + Sync {
    async fn query(&self, query: &str) -> Result<Vec<Document>>;
    async fn store(&self, doc: &Document) -> Result<()>;
}
```

### Testing Strategy

#### Testing Pyramid
- **Unit Tests**: Individual function and module testing with high coverage (>90%)
- **Integration Tests**: Cross-component interaction testing via `crates/integration-tests/`
- **End-to-End Tests**: Full workflow testing with realistic data
- **Performance Tests**: Benchmarking critical paths with Criterion

#### Test Organization
- **Fixtures**: Shared test data from `crucible_core::test_support`
- **Mocking**: Comprehensive mocking for external services and I/O
- **Property-Based Testing**: Edge case validation for parsing and validation
- **Continuous Integration**: Automated testing on all PRs with coverage reporting

#### Test Quality Standards
- **Test Naming**: Descriptive names that explain the scenario (`test_search_returns_relevant_results_sorted_by_relevance`)
- **Setup/Teardown**: Proper resource cleanup with RAII patterns
- **Deterministic Tests**: No reliance on external state or timing
- **Performance Guards**: Tests fail if performance degrades beyond thresholds

### Git Workflow

#### Branching Strategy
- **master**: Main development branch, always in deployable state
- **feat/***: Feature branches for new functionality
- **fix/***: Bug fix branches for production issues
- **refactor/***: Code improvement and cleanup branches
- **docs/***: Documentation updates and improvements

#### Commit Message Convention
```
type(scope): description with emoji

feat(cli): add interactive fuzzy search with keyboard shortcuts 🎯
fix(surrealdb): resolve concurrent access deadlock 🔒
refactor(core): extract storage trait for better testability 🏗️
docs(readme): update quick start installation steps 📚
```

**Commit Types:**
- `feat`: New features and capabilities
- `fix`: Bug fixes and error handling
- `refactor`: Code improvements without behavior changes
- `test`: Test additions and improvements
- `docs`: Documentation updates
- `style`: Code formatting and style improvements
- `perf`: Performance optimizations
- `chore`: Maintenance and dependency updates

#### Pull Request Process
- **Draft PRs**: For work in progress requiring discussion
- **Code Review**: All PRs require at least one review
- **Automated Checks**: All tests must pass, coverage requirements met
- **Squash Merging**: Clean commit history with conventional commit format

## Domain Context

### Knowledge Management Domain
- **Documents**: Markdown-based knowledge artifacts with frontmatter metadata
- **Links**: First-class relationships between documents with typed connections
- **Tags**: Hierarchical categorization system for content organization
- **Search**: Multi-modal search including fuzzy, semantic, and structured queries

### Collaboration Domain
- **CRDT**: Conflict-free replicated data types for real-time collaboration
- **Sessions**: Temporary collaboration contexts with participant management
- **Permissions**: Role-based access control for multi-user scenarios
- **Sync**: Cross-device synchronization with conflict resolution

### AI Agent Integration
- **Tools**: Extensible system for AI-powered operations and analysis
- **Agents**: Autonomous processes that can execute workflows on behalf of users
- **Embeddings**: Vector representations for semantic search and similarity
- **Pipelines**: Data processing workflows with step-by-step validation

## Important Constraints

### Performance Requirements
- **Startup Time**: CLI must start within 500ms on typical hardware
- **Search Latency**: Fuzzy search results within 100ms for typical queries
- **Memory Usage**: Respect system memory limits with streaming for large files
- **Concurrency**: Support multiple simultaneous operations without blocking

### Security Requirements
- **Input Validation**: All user inputs must be validated and sanitized
- **Path Traversal**: Prevent file system access outside configured boundaries
- **Memory Safety**: Protect against memory exhaustion from large files
- **Sandboxing**: Isolate agent execution with proper resource limits

### Compatibility Requirements
- **Rust Version**: Minimum supported Rust version 1.75
- **Node.js Version**: Minimum supported Node.js 18.0 for frontend development
- **Operating Systems**: Linux, macOS, and Windows support
- **Database**: SurrealDB with RocksDB backend for production, in-memory for testing

### Development Constraints
- **Breaking Changes**: Must be versioned and documented with migration guides
- **API Stability**: Public APIs maintain backward compatibility within major versions
- **License**: Proprietary license with appropriate attribution for open source dependencies
- **Dependencies**: Regular security audits and dependency updates

## External Dependencies

### Runtime Dependencies
- **SurrealDB**: Database layer with vector search capabilities
- **Embedding Providers**: OpenAI, Anthropic, or local models for semantic search
- **File System**: Local filesystem watching with notify crate
- **Network**: HTTP clients for external API integration

### Development Dependencies
- **Node.js/npm**: Frontend build tooling and package management
- **Rust Toolchain**: cargo, rustc, rustfmt, clippy for development
- **CI/CD**: GitHub Actions for automated testing and deployment
- **Documentation**: mdBook for documentation generation

### Optional Integrations
- **Obsidian Plugin**: Integration with Obsidian.md note-taking app
- **VSCode Extension**: Editor integration for seamless development
- **Web Interface**: Browser-based access to knowledge management features
- **API Server**: RESTful API for third-party integrations

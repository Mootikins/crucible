# 🔥 Crucible

> Where ideas transform through linked thinking

A high-performance knowledge management system that combines hierarchical organization, real-time collaboration, and AI agent integration. Crucible promotes **linked thinking** – the seamless connection and evolution of ideas across time and context – by routing every UI (CLI today, desktop/agent integrations tomorrow) through a shared `crucible-core` façade that orchestrates configuration, storage, agents, and tools behind the scenes.

> **Status Note (2025-10-30):** The project is pivoting away from the legacy “service” architecture toward a lightweight, local-first core. References to service orchestration below describe historical behavior and will be updated as the refactor progresses.

## ✨ Key Features

- 🔍 **Advanced Search**: Fuzzy search, semantic search with embeddings, and SurrealQL queries
- 🖥️ **Interactive REPL**: Full-featured terminal interface with syntax highlighting and auto-completion
- 🤖 **AI Agent Integration**: Multiple AI agents share the same core APIs as human operators
- 🔧 **Tool Orchestration**: Shared execution layer available to the CLI, agents, and future desktop UI
- 🔄 **Sync & Collaboration (roadmap)**: CRDT-backed document sync and multi-user sessions coordinated by the core
- 📊 **Operational Insights**: Core-level metrics, tooling diagnostics, and performance tracking
- ⚡ **High Performance**: Simplified architecture with 83% complexity reduction and 51% fewer dependencies
- 🛡️ **Security First**: Multiple security levels, sandboxed execution, and comprehensive validation
- 🔒 **Memory Safety**: Large file protection, UTF-8 safety, and input validation for search operations

## 🚀 Quick Start

```bash
# Clone the repository
git clone https://github.com/matthewkrohn/crucible.git
cd crucible

# Build and install CLI
cargo build -p crucible-cli

# Start interactive REPL (default behavior)
cru

# Show available commands
cru --help
```

## 🔁 Execution Flow (Baseline → Target)

**Baseline (2025-10-30)**
1. `main.rs` parses CLI args with Clap and loads `CliConfig`.
2. Global singletons spin up on demand (kiln watcher, `CrucibleToolManager`).
3. Commands hit filesystem/SurrealDB/tool layers directly.
4. REPL starts with its own copies of the same globals.

**Target (Roadmap)**
1. UI adapters parse input then hand control to a `CliApp` (or desktop equivalent) constructed from `crucible-core`.
2. The core façade exposes agent, tool, and storage traits; concrete implementations stay inside core.
3. Commands/REPL interact only with the façade, making orchestration identical across UIs.
4. Sync & collaboration piggyback on the same façade so multiple devices/users share state through CRDT updates coordinated by the core.

During the transition you may still encounter global managers or direct CLI → infrastructure calls; the roadmap tracks the steps that retire those paths.

## 🖥️ CLI Overview

The Crucible CLI (`cru`) provides comprehensive command-line tools for knowledge management, service orchestration, and AI integration:

### Core Commands
```bash
# Interactive REPL with SurrealQL support
cru

# Search operations (with built-in safety)
cru search "your query" --limit 20 --format table
cru fuzzy "concept" --content --tags --paths
cru semantic "machine learning concepts" --show-scores

# Search automatically handles:
# - Large files (>10MB skipped, >1MB content limited)
# - UTF-8 encoding errors (graceful recovery)
# - Input validation (2-1000 character queries)

# Note management
cru note create projects/research.md --edit
cru note get projects/research.md --format json
cru note list --format table
```

### Service Management (NEW)
```bash
# Service health and monitoring
crucible-cli service health --detailed
crucible-cli service metrics --real-time
crucible-cli service list --status

# Service lifecycle
crucible-cli service start crucible-script-engine --wait
crucible-cli service restart crucible-script-engine
crucible-cli service logs --follow --errors
```

### Migration Management (NEW)
```bash
# Migration operations
crucible-cli migration status --detailed --validate
crucible-cli migration migrate --security-level production --dry-run
crucible-cli migration migrate --security-level production
crucible-cli migration validate --auto-fix
crucible-cli migration list --active --metadata
```

### AI Integration
```bash
# AI chat with multiple agents
crucible-cli chat --agent researcher --start-message "Help me analyze my research notes"
crucible-cli chat --agent writer --temperature 0.7 --max-tokens 1000

# Run custom Rune scripts
crucible-cli run my-analysis-script.rn --args '{"query": "test", "limit": 10}'
crucible-cli commands  # List available commands
```

### REPL Commands
Inside the interactive REPL:
```sql
-- SurrealQL queries
SELECT * FROM notes ORDER BY created DESC LIMIT 10;
SELECT title, tags FROM notes WHERE tags CONTAINS '#project';

-- REPL built-in commands
:tools          # List available tools
:run search-tool "query"
:stats          # Show kiln statistics
:config         # Show configuration
:help           # Show help
```

## 🏗️ Architecture Highlights

### Core-Orchestrated Architecture
- **Domain-Centric Core**: `crucible-core` owns parsing, CRDTs, configuration, agent orchestration, and the traits that expose shared functionality to every UI.
- **Integrated Agents & Tools**: LLM agents and tool execution pipelines live inside the core layer so automated workflows and human operators share the same capabilities.
- **Infrastructure Behind Façade**: Storage (SurrealDB), embedding pipelines, and external runners are coordinated by the core; UI layers never talk to them directly.
- **Shared Test Fixtures**: `crucible_core::test_support` exports kiln/document builders so unit, integration, and UI tests exercise identical data.

## 🔧 Tech Stack

- **Core**: Rust + Tokio + SurrealDB orchestration façade
- **Frontend**: Svelte 5 + TypeScript
- **Database**: SurrealDB with vector extensions
- **Scripting**: Rune with security sandboxing
- **CRDT**: Yrs for real-time collaboration
- **CLI**: Clap-based with interactive REPL
- **Monitoring**: Comprehensive metrics and health checks

## 📚 Documentation

### User Documentation
- **[Architecture](./docs/ARCHITECTURE.md)** - Updated simplified system architecture
- **[CLI Reference](./docs/CLI_REFERENCE.md)** - Comprehensive CLI command documentation
- **[Migration Guide](./docs/MIGRATION_GUIDE.md)** - Tool migration and validation
- **[Examples and Tutorials](./docs/EXAMPLES_AND_TUTORIALS.md)** - Practical examples and tutorials
- **[Troubleshooting](./docs/TROUBLESHOOTING.md)** - Common issues and solutions
- **[FAQ](./docs/FAQ.md)** - Frequently asked questions
- **[System Requirements](./docs/SYSTEM_REQUIREMENTS.md)** - Hardware and software requirements

### Technical Documentation
- **[API Documentation](./docs/API_DOCUMENTATION.md)** - Complete API reference
- **[ScriptEngine API](./docs/SCRIPTENGINE_API.md)** - Legacy documentation for the pre-refactor service system
- **[Service Integration](./crates/crucible-cli/CLI_SERVICE_INTEGRATION.md)** - Legacy CLI/service notes (to be retired)
- **[Developer Guide](./docs/DEVELOPER_GUIDE.md)** - Development environment and workflow
- **[AI Agent Guide](./AGENTS.md)** - Instructions for AI agents working on the codebase

### Contributing
- **[Contributing Guidelines](./CONTRIBUTING.md)** - How to contribute to Crucible
- **[Documentation Audit Report](./PHASE8_5_DOCUMENTATION_AUDIT_REPORT.md)** - Latest documentation quality assessment

## 🔒 Safety & Performance Features

### Memory Protection
- **Large File Handling**: Automatically skips files >10MB to prevent memory exhaustion
- **Content Limits**: Enforces 1MB content limit with streaming reads for large files
- **Buffer Management**: 8KB streaming buffers for efficient memory usage
- **Performance**: Maintains speed while protecting system resources

### UTF-8 Safety
- **Encoding Recovery**: Gracefully handles UTF-8 encoding errors with character replacement
- **Error Resilience**: Continues processing even with corrupted text files
- **Character Safety**: Replaces invalid UTF-8 sequences safely
- **International Content**: Full support for international text and emoji

### Input Validation
- **Query Limits**: Search queries validated to 2-1000 characters for meaningful results
- **Whitespace Normalization**: Cleans up excessive whitespace automatically
- **Null Character Protection**: Blocks potentially harmful null characters
- **Helpful Errors**: Clear validation messages guide users to correct usage

### Testing & Quality
- **12/12 CLI Tests**: All integration tests passing with comprehensive coverage
- **91/91 Core Tests**: All core functionality tests passing in 0.06s
- **Zero Timeouts**: Eliminated all test timeout issues through dead code removal
- **Memory Testing**: Validated large file handling and memory limits

## 🔥 Roadmap Focus

### Core-Orchestrated Architecture (in progress)
- **UI → Core → Infra Flow**: CLI now targets the shared core façade before hitting storage/tools
- **Integrated Agents & Tools**: Agents, LLM utilities, and tool execution move behind the core façade so automated workflows and humans share the same APIs
- **Shared Fixtures**: `crucible_core::test_support` centralises kiln/document builders for every layer
- **Dependency Cleanup**: Roadmap phases focus on removing direct UI → infrastructure calls

### Enhanced CLI Capabilities
- **20+ Commands**: Search, notes, semantics, migrations, and diagnostics
- **Interactive REPL**: Syntax highlighting, auto-completion, and tool execution via the shared core façade
- **Search Safety**: Built-in memory protection and input validation

### Multi-Client & Collaboration (planned)
- **Sync Engine**: CRDT-powered document sync between devices through the core façade
- **Shared Sessions**: Core-managed collaboration channels so multiple users can edit the same knowledge base in real time
- **Agent Collaboration**: Agents consume the same APIs and tools as humans, enabling automated document curation and cross-device assistance

### Migration & Maintenance
- **Automation**: Tool migration helpers with validation and rollback paths
- **Legacy Docs**: ScriptEngine references live in `docs/SCRIPTENGINE_API.md` for historical context while the new architecture lands
- **Roadmap Driven**: See `ROADMAP.md` for the staged core-centric refactor

## License

Copyright (c) 2024 Crucible. All Rights Reserved.

This software is proprietary and may not be used, reproduced, or distributed without permission from Crucible.

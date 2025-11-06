# 🔥 Crucible

> A plaintext-first agent framework for metadata-rich knowledge graphs

Crucible is a high-performance knowledge management system built around a simple principle: **wikilinks define the knowledge graph, and agents explore it through simple CLI primitives.** By combining portable markdown files with block-level embeddings, graph traversal, and semantic search, Crucible enables testing whether metadata-rich knowledge graphs improve agent accuracy—with a clear path toward RL optimization, custom agent definitions, and markdown-based workflows.

> **Current MVP Focus (2025-11):** Validating that wikilink-based knowledge graphs, tags, and block-level embeddings enable better agent context discovery through agent-friendly commands (`cru semantic`, `cru query`). Future enhancements include reinforcement learning for context selection and definable workflows via markdown.

> **Architecture Note:** The system routes every interface (CLI today, desktop/agent integrations tomorrow) through a shared `crucible-core` façade. Markdown files remain the source of truth for portability and lock-in avoidance—the database is optional infrastructure for rich queries.

## ✨ Key Features

### Agent-First Knowledge Discovery
- 🧠 **Wikilink-Based Graph**: `[[Note Name]]` links define entities and relationships—no extraction needed
- 🎯 **Block-Level Granularity**: Semantic search and embeddings operate at paragraph/heading level for precise context
- 🔍 **Hybrid Search**: Combine semantic similarity, graph structure, tags, and fuzzy matching
- 🤖 **Agent-Friendly CLI**: Simple primitives (`cru semantic`, `cru query`) that agents call with native tool-calling

### Performance & Portability
- 📄 **Plaintext-First**: Markdown files are source of truth—works on devices without database
- ⚡ **Incremental Processing**: Only changed files are reprocessed for fast startup (Phase 1 in progress)
- 🗃️ **Optional Database**: SurrealDB provides rich queries (SurrealQL) when available, but system works file-only
- 🔒 **Memory Safety**: Large file protection, UTF-8 safety, and input validation

### Developer Experience
- 🖥️ **Interactive REPL**: Full-featured terminal interface with syntax highlighting and auto-completion
- 📊 **Operational Insights**: Core-level metrics, tooling diagnostics, and performance tracking
- 🔧 **Clean Architecture**: 83% complexity reduction, 51% fewer dependencies
- 🛡️ **Security First**: Multiple security levels, sandboxed execution, comprehensive validation

### Future Roadmap
- 🔄 **RL Optimization**: Reinforcement learning for context selection and agent accuracy tuning
- 📝 **Markdown Workflows**: Definable agent workflows and custom agent definitions via markdown
- 🤝 **Sync & Collaboration**: CRDT-backed document sync for multi-device, multi-user scenarios

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

# File Processing Options
cru --no-process search "query"           # Skip file processing for quick commands
cru --process-timeout 60 semantic "ml"   # Set custom processing timeout

# Search automatically handles:
# - Large files (>10MB skipped, >1MB content limited)
# - UTF-8 encoding errors (graceful recovery)
# - Input validation (2-1000 character queries)

# Note management
cru note create projects/research.md --edit
cru note get projects/research.md --format json
cru note list --format table
```

### Integrated File Processing

Crucible now processes files automatically on startup to ensure all data is up-to-date:

```bash
# Files are processed automatically when CLI starts
cru semantic "recent changes"    # Uses latest processed data

# Control file processing behavior
cru --no-process fuzzy "concept"         # Use existing data (faster)
cru --process-timeout 120 stats          # Custom timeout (seconds)
```

**What happens automatically:**
- ✅ Scans for new and modified files
- ✅ Updates embeddings for semantic search
- ✅ Processes only changed files (incremental)
- ✅ Shows progress and handles errors gracefully
- ✅ Continues even if processing fails (graceful degradation)

### Tooling & Automation
```bash
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
- **[Architecture](./docs/ARCHITECTURE.md)** - Updated system architecture and roadmap context
- **[CLI Reference](./docs/CLI_REFERENCE.md)** - Comprehensive CLI command documentation
- **[Troubleshooting](./docs/TROUBLESHOOTING.md)** - Common issues and solutions
- **[FAQ](./docs/FAQ.md)** - Frequently asked questions
- **[System Requirements](./docs/SYSTEM_REQUIREMENTS.md)** - Hardware and software requirements

### Technical Documentation
- **[API Documentation](./docs/API_DOCUMENTATION.md)** - Complete API reference
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

### Current MVP: Agent Context Accuracy Testing (2025-11)
**Goal**: Validate that metadata-rich knowledge graphs (wikilinks, tags, block embeddings) improve agent accuracy through simple CLI primitives agents can call with native tool-calling.

**What's Working**:
- ✅ Wikilink parsing and backlink queries (SurrealQL)
- ✅ Tag indexing and querying
- ✅ Block-level semantic search with embeddings
- ✅ Agent-friendly commands (`cru semantic`, `cru query`)
- ✅ Portable markdown-first architecture

**In Progress**:
- ⚙️ **Incremental File Processing** (optimize-data-flow): Make CLI startup sub-second for large vaults by processing only changed files
- ⚙️ **Architecture Refactoring**: Clean SOLID-compliant module boundaries for maintainability

**What This Enables**:
- Agents manually suggest relevant searches during conversations
- Agents explore vault content using their native `Read` tool
- Testing which metadata signals (graph structure, embeddings, tags) improve agent responses

### Future Enhancements (Post-MVP)
- **Reinforcement Learning**: Optimize context selection based on agent accuracy metrics
- **Custom Agent Definitions**: Define specialized agents and their behaviors via markdown
- **Markdown Workflows**: Declarative workflow automation in markdown files
- **Multi-Device Sync**: CRDT-backed sync for collaborative knowledge bases
- **Advanced Context Assembly**: Automated topic extraction and hybrid retrieval pipelines

### Core Architecture Evolution (In Progress)
- **UI → Core → Infra Flow**: CLI targets shared core façade before hitting storage/tools
- **Integrated Agents & Tools**: Agents and humans share the same APIs through the core
- **Shared Fixtures**: `crucible_core::test_support` centralizes kiln/document builders
- **Dependency Cleanup**: Removing direct UI → infrastructure calls for clean boundaries

## License

Copyright (c) 2024 Crucible. All Rights Reserved.

This software is proprietary and may not be used, reproduced, or distributed without permission from Crucible.

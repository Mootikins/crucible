# 🔥 Crucible

> A plaintext-first knowledge management system for metadata-rich knowledge graphs

Crucible is a high-performance knowledge management system built around a simple principle: **wikilinks define the knowledge graph, and applications explore it through a unified core API.** By combining portable markdown files with block-level embeddings, graph traversal, and semantic search, Crucible enables testing whether metadata-rich knowledge graphs improve agent accuracy—with a clear path toward RL optimization, custom agent definitions, and markdown-based workflows.

> **Current MVP Focus (2025-11):** Validating that wikilink-based knowledge graphs, tags, and block-level embeddings enable better agent context discovery through the Agent Context Protocol (ACP). The CLI provides a chat interface for testing, with future desktop and web interfaces planned.

> **Architecture Note:** The system routes every interface (CLI chat today, desktop/web/agent integrations tomorrow) through a shared `crucible-core` façade. Markdown files remain the source of truth for portability and lock-in avoidance—the database is optional infrastructure for rich queries.

## ✨ Key Features

### Agent-First Knowledge Discovery
- 🧠 **Wikilink-Based Graph**: `[[Note Name]]` links define entities and relationships—no extraction needed
- 🎯 **Block-Level Granularity**: Semantic search and embeddings operate at paragraph/heading level for precise context
- 🔍 **Hybrid Search**: Combine semantic similarity, graph structure, tags, and fuzzy matching
- 🤖 **Agent Context Protocol (ACP)**: Built on Zed's ACP implementation for standardized agent interactions

### Performance & Portability
- 📄 **Plaintext-First**: Markdown files are source of truth—works on devices without database
- ⚡ **Incremental Processing**: Only changed files are reprocessed for fast startup (Phase 1 in progress)
- 🗃️ **Optional Database**: SurrealDB provides rich queries (SurrealQL) when available, but system works file-only
- 🔒 **Memory Safety**: Large file protection, UTF-8 safety, and input validation

### Developer Experience
- 🖥️ **Multiple Interfaces**: CLI chat interface today, desktop and web UIs planned
- 📊 **Operational Insights**: Core-level metrics, tooling diagnostics, and performance tracking
- 🔧 **Clean Architecture**: SOLID principles with dependency injection, trait-based extensibility
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

# Build the system
cargo build --release

# Start chat interface (default behavior)
cru

# Show available commands
cru --help
```

## 🔁 Architecture Evolution (In Progress)

**Current State (2025-11)**
- EPR (Entity-Property-Relation) schema provides unified document storage
- Hash-based change detection using hybrid Merkle trees
- File processing integrated into startup with incremental updates
- CLI provides chat interface for ACP-based agent interactions

**Target Architecture**
1. **Unified Core**: All interfaces (CLI chat, desktop, web) route through `crucible-core` façade
2. **Trait-Based APIs**: Storage, embedding, and agent services exposed via DI-friendly traits
3. **ACP Integration**: Agent Context Protocol provides standardized agent interaction layer
4. **Multi-Interface**: Chat CLI today, desktop/web UIs sharing the same core tomorrow

**Migration Progress**
- ✅ EPR schema and hash lookup migrated to entity-based storage
- ✅ Change detection using EPR metadata instead of legacy `notes` table
- 🔄 Hybrid Merkle integration for section-level change tracking
- 🔄 Database capability traits for ACP service layer
- 📋 Legacy cleanup and CLI chat interface implementation

## 🖥️ Current Interface: Chat CLI

The Crucible CLI (`cru`) currently provides a chat-based interface for interacting with your knowledge base. This is a transitional interface while the ACP integration is being developed.

### Getting Started
```bash
# Start chat interface (processes files on startup)
cru

# Skip file processing for quick commands
cru --no-process

# Set custom processing timeout
cru --process-timeout 60
```

### File Processing
Crucible automatically processes files on startup to ensure data is up-to-date:

**What happens automatically:**
- ✅ Scans for new and modified files using hash-based change detection
- ✅ Updates embeddings for semantic search
- ✅ Processes only changed files (incremental)
- ✅ Shows progress and handles errors gracefully
- ✅ Continues even if processing fails (graceful degradation)

### Planned Features (ACP Integration)
The CLI will transition to a chat-oriented interface that:
- Provides natural language interaction with your knowledge base
- Uses Agent Context Protocol for standardized agent communication
- Offers simple status and diff commands
- Minimizes direct database access in favor of ACP service layer

### Legacy Commands (Being Phased Out)
The following commands exist but will be replaced by ACP-based interactions:
```bash
cru search "query"           # Text search
cru fuzzy "concept"          # Fuzzy matching
cru semantic "ml"            # Semantic search
cru note create path.md      # Note management
```

**Note:** The SurrealQL REPL and direct database commands are being phased out in favor of the ACP chat interface.

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

### Current Phase: ACP + Chat CLI (2025-11)
**Goal**: Build Agent Context Protocol integration with a chat-oriented CLI interface, establishing the foundation for agent-driven knowledge exploration.

**Completed**:
- ✅ EPR schema migration (Entity-Property-Relation model)
- ✅ Hash-based change detection using hybrid Merkle trees
- ✅ Incremental file processing on startup
- ✅ Wikilink parsing and graph structure
- ✅ Tag indexing and block-level embeddings

**In Progress** (per STATUS.md):
1. **Hybrid Merkle Integration**: Persist section/block hashes for fine-grained change detection
2. **Legacy Cleanup**: Remove `notes:` table dependencies and normalize to EPR entities
3. **Database Capability Traits**: Introduce trait layers (`EntityStore`, `RelationStore`) for ACP services
4. **Chunk Hash Coverage**: Integration tests for incremental embedding updates
5. **ACP + Chat CLI**: Port Zed's ACP implementation and replace CLI with chat interface

**Guiding Principles** (from STATUS.md):
- **Start-from-scratch mindset**: Use EPR schema everywhere, defer CRDT until ACP + chat MVP is solid
- **SOLID + DI-first**: Traits over concrete implementations for testability and flexibility
- **Concise MVP**: Build only surfaces needed for ACP + chat; delete scope creep
- **Surreal-first ACP**: ACP precedes the CLI chat shell

### Future Enhancements (Post-ACP)
- **Desktop & Web UIs**: Additional interfaces sharing the same `crucible-core` façade
- **CRDT Sync**: Multi-device collaboration through Yjs integration
- **Plugin System**: Rune-based extensibility for custom workflows
- **Reinforcement Learning**: Context selection optimization based on agent accuracy metrics
- **Custom Agent Definitions**: Markdown-based agent behavior specifications

## License

Copyright (c) 2024 Crucible. All Rights Reserved.

This software is proprietary and may not be used, reproduced, or distributed without permission from Crucible.

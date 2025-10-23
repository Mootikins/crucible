# 🔥 Crucible

> Where ideas transform through linked thinking

A high-performance knowledge management system that combines hierarchical organization, real-time collaboration, and AI agent integration. Built on a simplified ScriptEngine service architecture, Crucible promotes **linked thinking** - the seamless connection and evolution of ideas across time and context.

## ✨ Key Features

- 🔍 **Advanced Search**: Fuzzy search, semantic search with embeddings, and SurrealQL queries
- 🖥️ **Interactive REPL**: Full-featured terminal interface with syntax highlighting and auto-completion
- 🤖 **AI Chat Integration**: Multiple AI agents for research, writing, and analysis
- 🚀 **ScriptEngine Service**: Production-ready Rune script execution with security and performance monitoring
- 🔧 **Service Management**: Comprehensive CLI commands for service orchestration and monitoring
- 🔄 **Migration System**: Automated tool migration with validation, rollback, and integrity checking
- 📊 **Real-time Metrics**: Service health monitoring, performance tracking, and resource management
- ⚡ **High Performance**: Simplified architecture with 83% complexity reduction and 51% fewer dependencies
- 🛡️ **Security First**: Multiple security levels, sandboxed execution, and comprehensive validation

## 🚀 Quick Start

```bash
# Clone the repository
git clone https://github.com/matthewkrohn/crucible.git
cd crucible

# Build and install CLI
cargo build -p crucible-cli

# Start interactive REPL (default behavior)
cargo run -p crucible-cli

# Show available commands
cargo run -p crucible-cli -- --help
```

## 🖥️ CLI Overview

The Crucible CLI provides comprehensive command-line tools for knowledge management, service orchestration, and AI integration:

### Core Commands
```bash
# Interactive REPL with SurrealQL support
crucible-cli

# Search operations
crucible-cli search "your query" --limit 20 --format table
crucible-cli fuzzy "concept" --content --tags --paths
crucible-cli semantic "machine learning concepts" --show-scores

# Note management
crucible-cli note create projects/research.md --edit
crucible-cli note get projects/research.md --format json
crucible-cli note list --format table
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
:stats          # Show vault statistics
:config         # Show configuration
:help           # Show help
```

## 🏗️ Architecture Highlights

### ScriptEngine Service Architecture
- **Simplified Design**: 83% reduction in complexity, removed 5,000+ lines of over-engineered code
- **Production Ready**: VM-per-execution pattern with security isolation and resource monitoring
- **Event-Driven**: Comprehensive event system for service coordination and monitoring
- **High Performance**: 51% reduction in dependencies, improved compilation and runtime performance

### Service Integration
- **Service Discovery**: Automatic service detection and registration
- **Health Monitoring**: Real-time health checks and performance metrics
- **Configuration Management**: Hot-reloadable configuration with validation
- **Migration System**: Automated tool migration with rollback capabilities

## 🔧 Tech Stack

- **Core**: Rust + Tauri + ScriptEngine Services
- **Frontend**: Svelte 5 + TypeScript
- **Database**: SurrealDB + DuckDB with vector extensions
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
- **[ScriptEngine API](./docs/SCRIPTENGINE_API.md)** - Service architecture and API
- **[Service Integration](./crates/crucible-cli/CLI_SERVICE_INTEGRATION.md)** - CLI and service integration guide
- **[Developer Guide](./docs/DEVELOPER_GUIDE.md)** - Development environment and workflow
- **[AI Agent Guide](./AGENTS.md)** - Instructions for AI agents working on the codebase

### Contributing
- **[Contributing Guidelines](./CONTRIBUTING.md)** - How to contribute to Crucible
- **[Documentation Audit Report](./PHASE8_5_DOCUMENTATION_AUDIT_REPORT.md)** - Latest documentation quality assessment

## 🔥 New in This Version

### ScriptEngine Service Architecture
- **Simplified Services**: Clean, focused service traits with 83% complexity reduction
- **Production Ready**: Security levels, resource limits, and comprehensive monitoring
- **Event System**: Real-time service coordination and health monitoring

### Enhanced CLI Capabilities
- **20+ New Commands**: Service management, migration operations, and advanced monitoring
- **Interactive REPL**: Enhanced with syntax highlighting, auto-completion, and tool integration
- **Service Integration**: Automatic service discovery and management

### Migration System
- **Automated Migration**: Tool migration with validation and rollback
- **Multiple Security Levels**: Safe, Development, and Production modes
- **Integrity Checking**: Comprehensive validation and auto-fix capabilities

## License

Copyright (c) 2024 Crucible. All Rights Reserved.

This software is proprietary and may not be used, reproduced, or distributed without permission from Crucible.


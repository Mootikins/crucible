# 🔥 Crucible

> A plaintext-first knowledge management system for metadata-rich knowledge graphs

Crucible is a high-performance knowledge management system built around a simple principle: **wikilinks define the knowledge graph, and applications explore it through a unified core API.** By combining portable markdown files with block-level embeddings, graph traversal, and semantic search, Crucible provides powerful context discovery for AI agents and personal knowledge management.

**Key Design Principles:**
- **Plaintext-First**: Markdown files are source of truth—works with any text editor
- **Local-First**: Everything stays on your machine, database is optional
- **Agent-Ready**: Built for AI agent integration via the Agent Context Protocol (ACP)
- **Block-Level Granularity**: Semantic search operates at paragraph/heading level for precise context

## 📖 User Philosophy

This project is guided by user-focused principles that ensure technology serves human knowledge management. See [docs/PHILOSOPHY.md](docs/PHILOSOPHY.md) for the complete user story philosophy that drives all development decisions.

## ✨ Features

### Knowledge Management
- 🧠 **Wikilink-Based Graph**: `[[Note Name]]` links define entities and relationships—no extraction needed
- 🎯 **Block-Level Embeddings**: Semantic search operates at paragraph/heading level for precise context
- 🔍 **Hybrid Search**: Combine semantic similarity, graph traversal, tags, and fuzzy matching
- 🏷️ **Rich Metadata**: Frontmatter support with bidirectional sync between files and database

### Architecture & Performance
- 📄 **Plaintext-First**: Markdown files are source of truth—works with any text editor
- ⚡ **Incremental Processing**: Hash-based change detection for fast updates
- 🗃️ **Optional Database**: SurrealDB (embedded) provides rich queries when needed
- 🔒 **Memory Safety**: Large file protection, UTF-8 safety, and input validation
- 🔧 **Clean Architecture**: Trait-based design with dependency injection for extensibility

### AI Agent Integration
- 🤖 **Agent Context Protocol**: Standardized protocol for AI agent communication
- 📊 **Context Enrichment**: Automatically gather relevant notes and graph structure for agents
- 🛡️ **Sandboxed Execution**: Rune-based scripting with security controls

## 🚀 Quick Start

```bash
# Clone the repository
git clone https://github.com/mootikins/crucible.git
cd crucible

# Build the system
cargo build --release

# Start chat interface (default behavior)
cru

# Show available commands
cru --help
```

## 🖥️ Using Crucible

The Crucible CLI (`cru`) provides the primary interface for interacting with your knowledge base.

### Basic Usage
```bash
# Start the CLI (processes files on startup)
cru

# Skip file processing for quick commands
cru --no-process
```

### File Processing
Crucible automatically processes files on startup:
- Scans for new and modified files using hash-based change detection
- Updates embeddings for semantic search
- Processes only changed files (incremental)
- Shows progress and handles errors gracefully

### Available Commands
```bash
cru search "query"           # Text search
cru fuzzy "concept"          # Fuzzy matching
cru semantic "ml"            # Semantic search
cru note create path.md      # Note management
```

## 🏗️ Architecture

Crucible uses a clean, layered architecture:

- **Core Layer** (`crucible-core`): Domain logic, parsing, storage traits, agent orchestration
- **Infrastructure Layer**: SurrealDB storage, embedding providers (Fastembed, OpenAI, Ollama), file watching
- **Interface Layer**: CLI (current), with future desktop/web interfaces planned
- **Trait-Based Design**: All major components exposed via traits for testability and extensibility

### Tech Stack

- **Language**: Rust with Tokio async runtime
- **Database**: SurrealDB (embedded) with vector extensions
- **Embeddings**: Fastembed (local), OpenAI, or Ollama
- **Scripting**: Rune with security sandboxing
- **CLI**: Clap-based command line interface

## 📚 Documentation

- **[Philosophy](./docs/PHILOSOPHY.md)** - Core principles and design philosophy
- **[Architecture](./docs/ARCHITECTURE.md)** - Comprehensive system architecture and technical details
- **[AI Agent Guide](./AGENTS.md)** - Instructions for AI agents working on the codebase

## 🔒 Safety & Performance

- **Memory Protection**: Large file handling with size limits and streaming reads
- **UTF-8 Safety**: Graceful handling of encoding errors with character replacement
- **Input Validation**: Query limits, whitespace normalization, and null character protection
- **Incremental Processing**: Hash-based change detection for efficient updates
- **Comprehensive Testing**: Full test coverage across core, CLI, and integration layers


## License

Copyright (c) 2024 Crucible. All Rights Reserved.

This software is proprietary and may not be used, reproduced, or distributed without permission from Crucible.

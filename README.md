# Crucible

> A plaintext-first knowledge management system for metadata-rich knowledge graphs

Crucible is a high-performance knowledge management system built around a simple principle: **wikilinks define the knowledge graph, and applications explore it through a unified core API.** By combining portable markdown files with block-level embeddings, graph traversal, and semantic search, Crucible provides powerful context discovery for AI agents and personal knowledge management.

**Key Design Principles:**
- **Plaintext-First**: Markdown files are source of truth—works with any text editor
- **Local-First**: Everything stays on your machine, database is optional
- **Agent-Ready**: Built for AI agent integration via MCP (Model Context Protocol)
- **Block-Level Granularity**: Semantic search operates at paragraph/heading level for precise context

## Features

### Knowledge Management
- **Wikilink-Based Graph**: `[[Note Name]]` links define entities and relationships—no extraction needed
- **Block-Level Embeddings**: Semantic search operates at paragraph/heading level for precise context
- **Hybrid Search**: Combine semantic similarity, graph traversal, tags, and fuzzy matching
- **Rich Metadata**: Frontmatter support with bidirectional sync between files and database

### Architecture & Performance
- **Plaintext-First**: Markdown files are source of truth—works with any text editor
- **Incremental Processing**: Hash-based change detection for fast updates
- **Optional Database**: SurrealDB (embedded) provides rich queries when needed
- **Memory Safety**: Large file protection, UTF-8 safety, and input validation
- **Clean Architecture**: Trait-based design with dependency injection for extensibility

### AI Agent Integration
- **MCP Server**: Model Context Protocol server exposing tools for knowledge management
- **Unified LLM Providers**: Pluggable embedding and chat providers (Ollama, OpenAI, FastEmbed, LlamaCpp)
- **Context Enrichment**: Automatically gather relevant notes and graph structure for agents
- **Tool Discovery**: Agents automatically discover and use Crucible's tools via MCP
- **Sandboxed Execution**: Rune-based scripting with security controls

## Quick Start

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

**Windows Users:** See [Windows Configuration Guide](docs/WINDOWS-CONFIGURATION.md) for Windows-specific setup and troubleshooting, including C runtime library configuration.

## Using Crucible

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
cru chat                     # Interactive chat with AI agent
cru mcp                      # Start MCP server for tool exposure
```

### AI Agent Integration

Crucible includes a built-in MCP (Model Context Protocol) server that exposes knowledge management tools to AI agents:

```bash
# Start the MCP server (typically invoked by agents automatically)
cru mcp
```

**Note Tools:**
- `create_note` - Create new notes with YAML frontmatter
- `read_note` - Read note content with optional line ranges
- `read_metadata` - Get note metadata without loading full content
- `update_note` - Update note content and/or frontmatter
- `delete_note` - Remove notes from the kiln
- `list_notes` - List notes in a directory (recursive or non-recursive)

**Search Tools:**
- `semantic_search` - Find semantically similar notes using embeddings
- `text_search` - Fast full-text search across all notes
- `property_search` - Search by frontmatter properties and tags

**Kiln Tools:**
- `get_kiln_info` - Get kiln path and statistics
- `get_kiln_roots` - Get kiln root directory information
- `get_kiln_stats` - Get detailed kiln statistics

## Architecture

Crucible uses a clean, layered architecture with orthogonal systems:

### Crate Organization

| Crate | Purpose |
|-------|---------|
| `crucible-core` | Domain logic, traits, parser types, storage abstractions |
| `crucible-parser` | Markdown parsing implementation |
| `crucible-config` | Configuration types and loading |
| `crucible-llm` | LLM providers (embeddings, chat, text generation) |
| `crucible-surrealdb` | SurrealDB storage with EAV graph schema |
| `crucible-cli` | Command-line interface |
| `crucible-web` | Browser-based chat UI (Svelte 5 + Axum) |
| `crucible-tools` | MCP server and tool implementations |
| `crucible-rune` | Rune scripting integration |
| `crucible-watch` | File system watching |
| `tq` | TOON Query - jq-like query language |

### LLM Provider Architecture

Crucible uses a unified provider system with capability-based traits:

```
Provider (base trait)
   ├── CanEmbed (embedding capability)
   ├── CanChat (chat/completion capability)
   └── CanConstrainGeneration (grammar/schema constraints)
```

**Supported Backends:**
- **Ollama** - Local LLM server (embeddings + chat)
- **OpenAI** - Cloud API (embeddings + chat)
- **FastEmbed** - Local ONNX embeddings (CPU-optimized)
- **LlamaCpp** - Local GGUF models with GPU acceleration
- **Burn** - Rust ML framework embeddings

### Tech Stack

- **Language**: Rust with Tokio async runtime
- **Database**: SurrealDB (embedded) with vector extensions
- **Embeddings**: FastEmbed (local), OpenAI, Ollama, LlamaCpp
- **Scripting**: Rune with security sandboxing
- **CLI**: Clap-based command line interface
- **Web**: Svelte 5 frontend with Axum backend and SSE
- **Agent Integration**: Model Context Protocol (MCP)
- **Query Language**: TOON Query (tq) - jq-like structured data manipulation

## Documentation

- **[AI Agent Guide](./AGENTS.md)** - Instructions for AI agents working on the codebase
- **[OpenSpec Workflow](./openspec/AGENTS.md)** - Change proposal and specification system
- **[System Boundaries](./openspec/SYSTEMS.md)** - Orthogonal system organization
- **[Example Kiln](./examples/test-kiln/)** - Test vault with comprehensive search scenarios

## Example Kiln

The `examples/test-kiln/` directory contains a comprehensive test vault with:
- 12 realistic markdown files covering diverse content types
- 150+ search test scenarios
- 8 different link formats (wikilinks, embeds, aliases)
- 45 unique frontmatter property types
- ~25,000 words of content across business, technical, academic, and personal domains

See `examples/test-kiln/README - Test Kiln Structure.md` for full details.

## Safety & Performance

- **Memory Protection**: Large file handling with size limits and streaming reads
- **UTF-8 Safety**: Graceful handling of encoding errors with character replacement
- **Input Validation**: Query limits, whitespace normalization, and null character protection
- **Incremental Processing**: Hash-based change detection for efficient updates
- **Comprehensive Testing**: Full test coverage across core, CLI, and integration layers

## License

Copyright (c) 2024 Crucible. All Rights Reserved.

This software is proprietary and may not be used, reproduced, or distributed without permission from Crucible.

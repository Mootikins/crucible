# Crucible Architecture

> Technical architecture overview for the Crucible knowledge management system

This document provides a detailed look at Crucible's architecture, crate organization, trait system, and data flow.

## Design Principles

1. **Plaintext-First**: Markdown files are the source of truth. No database lock-in.
2. **Local-First**: Everything runs on your machine. Database is optional enrichment.
3. **Trait-Based Extensibility**: Core behaviors defined as traits, implementations swappable.
4. **Block-Level Granularity**: Semantic search operates at paragraph/heading level.
5. **Agent-Ready**: Built for AI integration via MCP (Model Context Protocol).

## System Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                         User Interfaces                             │
├──────────────┬──────────────┬──────────────┬───────────────────────┤
│  CLI (cru)   │   Web UI     │  MCP Server  │    Rune Scripts       │
│ crucible-cli │ crucible-web │crucible-tools│   crucible-rune       │
└──────┬───────┴──────┬───────┴──────┬───────┴───────────┬───────────┘
       │              │              │                   │
       └──────────────┴──────────────┴───────────────────┘
                              │
                    ┌─────────▼─────────┐
                    │   Core Business   │
                    │      Logic        │
                    │  crucible-core    │
                    └─────────┬─────────┘
                              │
       ┌──────────────────────┼──────────────────────┐
       │                      │                      │
┌──────▼──────┐      ┌────────▼────────┐     ┌──────▼──────┐
│   Parser    │      │  LLM Providers  │     │   Storage   │
│crucible-    │      │  crucible-llm   │     │crucible-    │
│  parser     │      │                 │     │ surrealdb   │
└─────────────┘      └─────────────────┘     └─────────────┘
```

## Crate Organization

### Core Crates

| Crate | Purpose | Dependencies |
|-------|---------|--------------|
| `crucible-core` | Domain logic, traits, parser types | - |
| `crucible-config` | Configuration types and loading | crucible-core |
| `crucible-parser` | Markdown parsing implementation | crucible-core |

### Infrastructure Crates

| Crate | Purpose | Dependencies |
|-------|---------|--------------|
| `crucible-llm` | LLM providers (embeddings, chat, generation) | crucible-core, crucible-config |
| `crucible-surrealdb` | SurrealDB storage with EAV graph | crucible-core |
| `crucible-watch` | File system watching | crucible-core |

### Interface Crates

| Crate | Purpose | Dependencies |
|-------|---------|--------------|
| `crucible-cli` | Command-line interface | All infrastructure |
| `crucible-web` | Browser chat UI (Svelte 5 + Axum) | crucible-llm, crucible-surrealdb |
| `crucible-tools` | MCP server and tool implementations | crucible-core |
| `crucible-rune` | Rune scripting integration | crucible-core |

### Utility Crates

| Crate | Purpose | Dependencies |
|-------|---------|--------------|
| `tq` | TOON Query - jq-like query language | - |
| `crucible-acp` | Agent Context Protocol types | - |
| `crucible-agents` | Agent orchestration | crucible-acp, crucible-llm |

## Type Ownership

Types are defined in exactly one location to avoid duplication:

### Parser Types (`crucible-core/src/parser/types/`)

```rust
// Canonical location for all parser types
use crucible_core::parser::{
    ParsedNote,   // Full parsed note structure
    Block,        // Individual content block
    Wikilink,     // [[Link]] references
    Tag,          // #tag references
    BlockHash,    // Content-addressable hash
    Frontmatter,  // YAML metadata
};
```

### Hash Types (`crucible-core/src/types/hashing.rs`)

```rust
use crucible_core::types::hashing::{
    FileHash,        // File content hash
    HashAlgorithm,   // Hashing algorithm selection
};
```

### LLM Types (`crucible-core/src/traits/`)

```rust
use crucible_core::traits::provider::{
    Provider,              // Base provider trait
    CanEmbed,              // Embedding capability
    CanChat,               // Chat capability
    CanConstrainGeneration, // Grammar/schema constraints
    EmbeddingResponse,     // Unified embedding response
    ExtendedCapabilities,  // Provider capability flags
};

use crucible_core::traits::llm::{
    LlmResult,       // Result type for LLM operations
    LlmError,        // Error type for LLM operations
    ProviderCapabilities, // Basic capability flags
};
```

## LLM Provider Architecture

Crucible uses a capability-based extension trait pattern for LLM providers:

```
┌─────────────────────────────────────────────────────────────┐
│                    Provider (base trait)                    │
│  - name() -> &str                                           │
│  - backend_type() -> BackendType                            │
│  - endpoint() -> Option<&str>                               │
│  - capabilities() -> ExtendedCapabilities                   │
│  - health_check() -> Result<bool>                           │
└─────────────────────────────────────────────────────────────┘
                              ▲
          ┌───────────────────┼───────────────────┐
          │                   │                   │
┌─────────┴─────────┐ ┌───────┴───────┐ ┌────────┴────────┐
│     CanEmbed      │ │    CanChat    │ │CanConstrain     │
│                   │ │               │ │Generation       │
│ - embed()         │ │ - chat()      │ │                 │
│ - embed_batch()   │ │ - stream()    │ │ - grammar()     │
│ - dimensions()    │ │ - model()     │ │ - json_schema() │
│ - model()         │ │               │ │                 │
└───────────────────┘ └───────────────┘ └─────────────────┘
```

### Supported Backends

| Backend | Embeddings | Chat | Constrained | Feature Flag |
|---------|------------|------|-------------|--------------|
| Ollama | Yes | Yes | No | default |
| OpenAI | Yes | Yes | JSON Schema | default |
| FastEmbed | Yes | No | No | `fastembed` |
| LlamaCpp | Yes | Yes | GBNF Grammar | `llama-cpp` |
| Burn | Yes | No | No | `burn` |

### Creating Providers

```rust
use crucible_llm::unified::{create_provider, create_embedding_provider};
use crucible_config::ProviderConfig;

// Create a generic provider (trait object)
let provider = create_provider(&config).await?;

// Create specifically for embeddings
let embedder = create_embedding_provider(&config).await?;
let response = embedder.embed("text to embed").await?;
```

### Legacy Compatibility

The unified traits wrap legacy providers. Legacy code using `EmbeddingProvider` continues to work:

```rust
// Legacy API (still supported)
use crucible_llm::embeddings::{create_provider, EmbeddingProvider};
let provider = create_provider(config).await?;
let response = provider.embed("text").await?;

// New unified API
use crucible_core::traits::provider::{Provider, CanEmbed};
let unified: Box<dyn CanEmbed> = /* ... */;
let response = unified.embed("text").await?;
```

## Storage Architecture

### SurrealDB Schema

Crucible uses SurrealDB with an Entity-Attribute-Value (EAV) graph schema:

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│    Note     │────▶│    Block    │────▶│  Embedding  │
│  (entity)   │     │  (entity)   │     │  (vector)   │
└─────────────┘     └─────────────┘     └─────────────┘
       │                   │
       │                   │
       ▼                   ▼
┌─────────────┐     ┌─────────────┐
│  Wikilink   │     │    Tag      │
│  (relation) │     │ (relation)  │
└─────────────┘     └─────────────┘
```

### Key Tables

- **notes**: Parsed note metadata and content
- **blocks**: Individual content blocks with hashes
- **embeddings**: Vector embeddings for semantic search
- **wikilinks**: Directed graph edges between notes
- **tags**: Note-to-tag relationships
- **frontmatter**: Key-value metadata storage

### Change Detection

Crucible uses hash-based change detection for incremental processing:

1. File content hashed on scan
2. Hash compared against stored hash
3. Only changed files re-parsed and re-embedded
4. Block-level hashes enable granular embedding updates

## Data Flow

### File Processing Pipeline

```
Markdown File
     │
     ▼
┌─────────────┐
│   Parser    │──▶ ParsedNote { blocks, wikilinks, tags, frontmatter }
└─────────────┘
     │
     ▼
┌─────────────┐
│  Embedder   │──▶ Vec<EmbeddingResponse> (one per block)
└─────────────┘
     │
     ▼
┌─────────────┐
│   Storage   │──▶ SurrealDB tables
└─────────────┘
```

### Search Flow

```
User Query
     │
     ▼
┌─────────────┐
│  Embedder   │──▶ Query Embedding Vector
└─────────────┘
     │
     ▼
┌─────────────┐
│  Vector DB  │──▶ Similar Block IDs (cosine similarity)
└─────────────┘
     │
     ▼
┌─────────────┐
│  Storage    │──▶ Full Block Content + Note Context
└─────────────┘
     │
     ▼
Search Results
```

## MCP Integration

Crucible exposes tools via Model Context Protocol for AI agent integration:

### Available Tools

**Note Tools:**
- `create_note` - Create notes with YAML frontmatter
- `read_note` - Read note content with line ranges
- `update_note` - Update content and/or frontmatter
- `delete_note` - Remove notes

**Search Tools:**
- `semantic_search` - Vector similarity search
- `text_search` - Full-text search
- `property_search` - Frontmatter property queries

**Kiln Tools:**
- `get_kiln_info` - Kiln path and statistics
- `get_kiln_stats` - Detailed statistics

### MCP Server

```bash
# Start MCP server
cru mcp

# Or configure in Claude Desktop:
{
  "mcpServers": {
    "crucible": {
      "command": "cru",
      "args": ["mcp"]
    }
  }
}
```

## Configuration

### Config File Location

```
~/.config/crucible/config.toml
```

### Provider Configuration

```toml
[providers]
default_embedding = "local-ollama"
default_chat = "local-ollama"

[providers.instances.local-ollama]
backend = "ollama"
endpoint = "http://localhost:11434"
models.embedding = "nomic-embed-text"
models.chat = "llama3.2"

[providers.instances.openai-prod]
backend = "openai"
api_key = { env = "OPENAI_API_KEY" }
models.embedding = "text-embedding-3-small"
models.chat = "gpt-4o"
```

### Legacy Config (Auto-migrated)

```toml
# Old format (still supported, migrated automatically)
[embedding]
provider = "ollama"
endpoint = "http://localhost:11434"
model = "nomic-embed-text"
```

## Feature Flags

The workspace uses feature flags for optional backends:

```toml
# crucible-llm features
[features]
default = ["fastembed"]
fastembed = ["dep:fastembed"]      # Local ONNX embeddings
llama-cpp = ["dep:llama-cpp-2"]    # GGUF model support
burn = ["dep:burn"]                # Burn ML framework
test-utils = []                    # Mock providers for testing
```

### Building with Features

```bash
# Default (includes fastembed)
cargo build --release

# Minimal (no local embeddings)
cargo build --release --no-default-features

# All features
cargo build --release --all-features
```

## Testing

### Running Tests

```bash
# All tests
cargo test --workspace

# With feature-gated tests
cargo test --workspace --features test-utils

# Specific crate
cargo test -p crucible-llm
```

### Test Organization

- **Unit tests**: In-file `#[cfg(test)]` modules
- **Integration tests**: `crates/*/tests/` directories
- **Example kiln**: `examples/test-kiln/` for search testing

## Development

### Using Just

```bash
just              # List all recipes
just build        # Build all crates
just test         # Run all tests
just check        # Cargo check workspace
just web          # Build and run web UI
just mcp          # Start MCP server
```

### Adding a New Provider

1. Implement `Provider` base trait
2. Implement capability traits (`CanEmbed`, `CanChat`, etc.)
3. Add factory function in `crucible-llm/src/unified/factory.rs`
4. Add `BackendType` variant in `crucible-config`
5. Write tests following TDD pattern

---

*This architecture document reflects the current state of Crucible. See `openspec/` for proposed changes and specifications.*

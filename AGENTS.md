<!-- OPENSPEC:START -->
# OpenSpec Instructions

These instructions are for AI assistants working in this project.

Always open `@/openspec/AGENTS.md` when the request:
- Mentions planning or proposals (words like proposal, spec, change, plan)
- Introduces new capabilities, breaking changes, architecture shifts, or big performance/security work
- Sounds ambiguous and you need the authoritative spec before coding

Use `@/openspec/AGENTS.md` to learn:
- How to create and apply change proposals
- Spec format and conventions
- Project structure and guidelines

Keep this managed block so 'openspec update' can refresh the instructions.

<!-- OPENSPEC:END -->

# AI Agent Guide for Crucible

> Instructions for AI agents (Claude, Codex, etc.) working on the Crucible codebase

This file provides essential information for AI agents to understand and contribute to the Crucible knowledge management system effectively.

## Project Overview

**Crucible** is a plaintext-first knowledge management system that combines wikilink-based knowledge graphs with AI agent integration. It promotes **linked thinking** through semantic search, block-level embeddings, and the Model Context Protocol (MCP).

**Core Principles:**
- Markdown files are source of truth (works with any editor)
- Wikilinks `[[Note Name]]` define the knowledge graph
- Block-level granularity for precise semantic search
- Unified LLM provider system with capability-based traits

## Architecture

### Crate Organization

| Crate | Purpose | Key Traits/Types |
|-------|---------|------------------|
| `crucible-core` | Domain logic, traits, parser types | `Provider`, `CanEmbed`, `CanChat`, `ParsedNote` |
| `crucible-parser` | Markdown parsing implementation | `MarkdownParser` |
| `crucible-config` | Configuration types and loading | `AppConfig`, `EmbeddingConfig` |
| `crucible-llm` | LLM providers (embeddings, chat) | `EmbeddingProvider`, `TextGenerationProvider` |
| `crucible-surrealdb` | SurrealDB storage with EAV schema | `SurrealStorage`, `EavGraph` |
| `crucible-cli` | Command-line interface | CLI commands and REPL |
| `crucible-web` | Browser chat UI (Svelte 5 + Axum) | HTTP/SSE endpoints |
| `crucible-tools` | MCP server and tools | Tool implementations |
| `crucible-rune` | Rune scripting integration | Script execution |
| `crucible-watch` | File system watching | Change detection |
| `crucible-agents` | Agent orchestration | Agent runtime |
| `crucible-acp` | Agent Context Protocol | Protocol types |
| `tq` | TOON Query language | Query parsing/execution |

### Type Ownership

**Parser Types** are canonically defined in `crucible-core/src/parser/types/` (split into submodules).
Core re-exports these types via `crucible_core::parser::*` for convenience.

**Hash Types**: `BlockHash` is defined in `crucible-core/src/parser/types/block_hash.rs`.
Other hash infrastructure is in `crucible-core/src/types/hashing.rs`.

**DO NOT duplicate types between crates.** Each type should be defined in exactly one location. Use re-exports for convenience.

**Import patterns:**
```rust
// Parser types - prefer canonical location
use crucible_core::parser::{ParsedNote, Wikilink, Tag, BlockHash};

// Hash infrastructure - from core
use crucible_core::types::hashing::{FileHash, HashAlgorithm};

// LLM traits - unified provider system
use crucible_core::traits::provider::{Provider, CanEmbed, CanChat};
```

### LLM Provider System

Crucible uses a unified provider architecture with capability-based extension traits:

```
Provider (base trait)
   ├── CanEmbed (embedding generation)
   ├── CanChat (chat completions)
   └── CanConstrainGeneration (grammar/schema constraints)
```

**Supported Backends:**
| Backend | Embeddings | Chat | Constrained | Feature Flag |
|---------|------------|------|-------------|--------------|
| Ollama | Yes | Yes | No | default |
| OpenAI | Yes | Yes | JSON Schema | default |
| FastEmbed | Yes | No | No | `fastembed` |
| LlamaCpp | Yes | Yes | GBNF | `llama-cpp` |
| Burn | Yes | No | No | `burn` |

**Creating Providers:**
```rust
use crucible_llm::embeddings::{create_provider, EmbeddingConfig};

// Factory function returns trait object
let provider = create_provider(config).await?;

// Use unified traits
let response = provider.embed("text").await?;
```

### Systems

Crucible is organized into orthogonal systems. See **[openspec/SYSTEMS.md](./openspec/SYSTEMS.md)** for full details.

| System | Scope |
|--------|-------|
| **parser** | Markdown → structured data |
| **storage** | SurrealDB, EAV graph, Merkle trees |
| **agents** | Agent cards, LLM providers, tools |
| **workflows** | Definitions + sessions |
| **plugins** | Extension points, scripting |
| **apis** | HTTP, WebSocket, events |
| **cli** | Commands, REPL, configuration |
| **desktop** | Tauri GUI (future) |

## Project Structure

```
crucible/
├── crates/                      # Rust workspace crates
│   ├── crucible-core/           # Core business logic and traits
│   ├── crucible-cli/            # CLI application
│   ├── crucible-llm/            # LLM providers (embeddings, chat)
│   ├── crucible-web/            # Browser-based chat UI
│   ├── crucible-surrealdb/      # Database layer
│   ├── crucible-parser/         # Markdown parsing
│   ├── crucible-tools/          # MCP server and tools
│   ├── crucible-config/         # Configuration types
│   ├── crucible-rune/           # Rune scripting
│   ├── crucible-watch/          # File watching
│   ├── tq/                      # TOON Query library
│   └── ...                      # Other crates
├── examples/
│   └── test-kiln/               # Test vault with search scenarios
├── openspec/                    # Change proposals & specs
│   ├── SYSTEMS.md               # System boundaries
│   ├── AGENTS.md                # OpenSpec workflow
│   ├── changes/                 # Active proposals
│   └── specs/                   # Current specs
├── justfile                     # Development recipes
├── AGENTS.md                    # This file (CLAUDE.md symlinks here)
└── README.md                    # Project overview
```

### Where to Put Things

**Keep the repo root clean!** Only essential files belong here.

**Allowed in root:**
- `README.md`, `AGENTS.md` - documentation
- `Cargo.toml`, `package.json` - build configuration
- `LICENSE`, `.gitignore` - project metadata

**Do NOT create in root:**
- Documentation files (use `docs/` or `openspec/`)
- Temporary files (clean up after use)
- Agent conversation logs (don't commit)

**Where things belong:**
- **Change proposals**: `openspec/changes/` - see `openspec/AGENTS.md`
- **Specifications**: `openspec/specs/` - current system capabilities
- **Examples**: `examples/`
- **Scripts**: `scripts/`
- **Tests**: `tests/` or `crates/*/tests/`

## Development Guidelines

### Development Workflow

**Use `just`**: The project uses Just for common development recipes:
- `just build` - Build all crates
- `just test` - Run all tests
- `just check` - Cargo check workspace
- `just web` - Build and run web UI
- `just mcp` - Start MCP server

Run `just` to see all available commands.

### Code Style

- **Rust**: Use `snake_case` for functions/variables, `PascalCase` for types
- **Error Handling**: Use `Result<T, E>` with proper error context
- **Documentation**: Add doc comments for public items
- **Testing**: Write tests for new functionality, use TDD

### Feature Flags

The `crucible-llm` crate uses feature flags for optional backends:

```toml
[features]
default = ["fastembed"]
fastembed = ["dep:fastembed"]      # Local ONNX embeddings
llama-cpp = ["dep:llama-cpp-2"]    # GGUF model support
burn = ["dep:burn"]                # Burn ML framework
test-utils = []                    # Mock providers for testing
```

### Testing

- Write unit tests for core functionality
- Include integration tests for component interactions
- Use `#[cfg(feature = "test-utils")]` for mock providers
- Test error conditions and edge cases
- Use descriptive test names that explain the scenario
- use `just test` to save on context

### Quality Checklist

Before submitting changes:
- [ ] Code follows project style guidelines
- [ ] Tests pass (`cargo test --workspace`)
- [ ] Error handling is comprehensive
- [ ] OpenSpec updated if needed (architectural changes)
- [ ] No debug code left in
- [ ] Conventional commit messages

## Using OpenSpec

For architectural changes, new features, or breaking changes, use the OpenSpec workflow:

**See `openspec/AGENTS.md` for complete details.** Quick reference:
- Create proposal in `openspec/changes/[change-id]/`
- Write `proposal.md`, `tasks.md`, and spec deltas
- Validate with `openspec validate [change-id] --strict`
- Get approval before implementing

## Key Resources

- **[README.md](./README.md)** - Project overview and quick start
- **[OpenSpec AGENTS.md](./openspec/AGENTS.md)** - Change proposal workflow
- **[SYSTEMS.md](./openspec/SYSTEMS.md)** - System boundaries and organization
- **[Example Kiln](./examples/test-kiln/)** - Test vault with search scenarios
- **[justfile](./justfile)** - Development recipes

---

*This guide helps AI agents work effectively with the Crucible codebase. Follow these guidelines to maintain code quality, consistency, and project integrity.*

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
| `crucible-llm` | LLM providers (embeddings, chat) | `EmbeddingBackend`, `CompletionBackend` |
| `crucible-surrealdb` | SurrealDB storage with EAV schema | `SurrealStorage`, `EavGraph` |
| `crucible-cli` | Command-line interface | CLI commands and REPL |
| `crucible-web` | Browser chat UI (SolidJS + Axum) | HTTP/SSE endpoints |
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

**LLM Types** (unified contracts):
- `ContextMessage` - canonical message type for all conversation contexts
- `BackendError` / `BackendResult` - canonical error type for LLM operations
- `CompletionBackend` - canonical trait for chat/completion providers

**Event Types**:
- `SessionEvent` includes pre-events (`PreToolCall`, `PreParse`, `PreLlmCall`) for handler interception

**DO NOT duplicate types between crates.** Each type should be defined in exactly one location. Use re-exports for convenience.

**Import patterns:**
```rust
// Parser types - prefer canonical location
use crucible_core::parser::{ParsedNote, Wikilink, Tag, BlockHash};

// Hash infrastructure - from core
use crucible_core::types::hashing::{FileHash, HashAlgorithm};

// LLM traits - unified provider system
use crucible_core::traits::provider::{Provider, CanEmbed, CanChat};
use crucible_core::traits::{CompletionBackend, BackendError, ContextMessage};
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

Crucible is organized into orthogonal systems. See **[docs/Meta/Systems.md](./docs/Meta/Systems.md)** for full details.

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
├── docs/                        # Documentation kiln (user guides + test fixture)
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
- Documentation files (use `docs/`)
- Temporary files (clean up after use)
- Agent conversation logs (don't commit)

**Where things belong:**
- **Feature docs**: `docs/Help/` - user-facing reference
- **Architecture docs**: `docs/Meta/` - contributor docs
- **Examples**: `examples/`
- **Scripts**: `scripts/`
- **Tests**: `tests/` or `crates/*/tests/`

### Documentation Kiln

The `docs/` directory is a **reference kiln** — a valid Crucible vault that serves as both documentation and test fixture:

1. **User Documentation**: Guides, Help references, and examples using wikilinks
2. **Test Fixture**: Integration tests validate the kiln parses and indexes correctly

**Use the docs kiln to document:**
- Roadmap items and features (in `Meta/Roadmap.md`)
- Technical decisions (in `Meta/Analysis/`)
- Usage guides (in `Guides/` and `Help/`)

**Conventions:**
- Use wikilinks to connect related concepts: `[[Help/Wikilinks]]`
- Add frontmatter with tags for discoverability
- Keep notes focused and well-linked rather than monolithic

## Development Guidelines

### Development Workflow

**Use `just`**: The project uses Just for common development recipes:
- `just build` - Build all crates
- `just test` - Run all tests
- `just check` - Cargo check workspace
- `just web` - Build and run web UI
- `just mcp` - Start MCP server

Run `just` to see all available commands.

**Web frontend uses `bun`**: For crucible-web frontend development, use bun (not npm/yarn):
- `bun install` - Install dependencies
- `bun run dev` - Start dev server
- `bun run build` - Production build

See `crates/crucible-web/web/AGENTS.md` for frontend-specific guidelines.

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

The test suite uses **cargo-nextest** for parallel execution. Tests are organized into tiers:

| Tier | Purpose | Command |
|------|---------|---------|
| **Unit** | Fast, isolated, mocked I/O | `cargo nextest run --profile unit` |
| **Integration** | Real DB, real files | `cargo nextest run --profile integration` |
| **Contract** | API/trait verification | `cargo nextest run --profile contract` |
| **CI** | All non-slow tests | `cargo nextest run --profile ci` |

**Guidelines:**
- Write unit tests for core functionality (mock external dependencies)
- Use `#[cfg(feature = "test-utils")]` for mock providers
- Mark slow/manual tests with `#[ignore = "reason"]`
- Use `test-case` crate for parameterized tests
- Test error conditions and edge cases
- Use descriptive test names that explain the scenario

**Running tests:**
```bash
just test              # Run all tests with nextest
cargo nextest run      # Same as above
cargo test --workspace # Fallback to cargo test
```

### Infrastructure Tests

Some tests require external infrastructure (Ollama, embedding endpoints, developer vaults). These use `#[ignore]` and can be run explicitly:

```bash
# Run ignored tests (requires infrastructure to be available)
cargo nextest run -- --ignored

# Run specific ignored test
cargo test -p crucible-surrealdb clustering -- --ignored
```

Tests handle missing infrastructure gracefully with runtime checks.

**Cross-platform test paths:**
- Use `tempfile::TempDir` for tests that need real filesystem access
- Use `crucible_core::test_support::nonexistent_path()` for paths that don't need to exist
- Never use hardcoded `/tmp` paths (not portable to Windows)

### TUI Testing Workflow

When fixing TUI bugs or implementing UX changes, follow this pattern:

**1. Write failing test first:**
```rust
use crate::tui::testing::{Harness, fixtures::sessions};
use crossterm::event::KeyCode;

#[test]
fn popup_should_close_on_escape() {
    let mut h = Harness::new(80, 24);
    h.key(KeyCode::Char('/'));
    assert!(h.has_popup());

    h.key(KeyCode::Esc);
    assert!(!h.has_popup());
}
```

**2. Run test, confirm it fails:**
```bash
cargo test -p crucible-cli --features test-utils popup_should_close
```

**3. Fix implementation** - Make minimal changes to pass the test.

**4. Add snapshot if visual:**
```rust
insta::assert_snapshot!(h.render(), @"popup_closed");
```

**5. Run full test suite:**
```bash
cargo test -p crucible-cli --features test-utils tui::testing
```

**Fixture reuse:** Before creating new fixtures, check `tui/testing/fixtures/`:
- `sessions.rs` - Conversation histories
- `registries.rs` - Commands, agents
- `events.rs` - Event sequences

Extend existing fixtures rather than duplicating.

**Cross-component tests:** When an event should affect multiple components, test them together:
```rust
#[test]
fn streaming_affects_status_and_history() {
    let mut h = Harness::new(80, 24);
    h.events(fixtures::events::streaming_chunks("Hello world"));

    // Verify ALL expected effects
    assert!(!h.has_error());
    insta::assert_snapshot!(h.render());
}
```

### Quality Checklist

Before submitting changes:
- [ ] Code follows project style guidelines
- [ ] Tests pass (`cargo nextest run --profile ci`)
- [ ] Error handling is comprehensive
- [ ] Docs updated if needed (architectural changes go in `docs/Meta/`)
- [ ] No debug code left in
- [ ] Conventional commit messages

## Key Resources

- **[README.md](./README.md)** - Project overview and quick start
- **[docs/Meta/Systems.md](./docs/Meta/Systems.md)** - System boundaries and organization
- **[docs/Meta/Roadmap.md](./docs/Meta/Roadmap.md)** - Development roadmap
- **[Documentation](./docs/)** - Reference kiln (user guides + test fixture)
- **[justfile](./justfile)** - Development recipes

---

*This guide helps AI agents work effectively with the Crucible codebase. Follow these guidelines to maintain code quality, consistency, and project integrity.*

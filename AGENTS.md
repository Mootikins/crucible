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

# 🤖 AI Agent Guide for Crucible

> Instructions for AI agents (Claude, Codex, etc.) working on the Crucible codebase

This file provides essential information for AI agents to understand and contribute to the Crucible knowledge management system effectively.

## 🎯 Project Overview

**Crucible** is a knowledge management system that combines hierarchical organization, real-time collaboration, and AI agent integration. It promotes **linked thinking** - the seamless connection and evolution of ideas across time and context.

## 🏗️ Architecture

### Type Ownership

**Parser Types** are canonically defined in `crucible-core/src/parser/types/` (split into 10 submodules).
Core re-exports these types via `crucible_core::parser::*` for convenience.

**Hash Types**: `BlockHash` is defined in `crucible-core/src/parser/types/block_hash.rs` to avoid circular
dependencies. Other hash infrastructure is in `crucible-core/src/types/hashing.rs`.

**DO NOT duplicate types between crates.** Each type should be defined in exactly
one location. Use re-exports for convenience.

**Import patterns:**
```rust
// Parser types - prefer canonical location
use crucible_core::parser::{ParsedNote, Wikilink, Tag, BlockHash};

// Or use re-export for convenience (same location)
use crucible_core::parser::{ParsedNote, Wikilink, Tag, BlockHash};

// Hash infrastructure - from core
use crucible_core::types::hashing::{FileHash, HashAlgorithm};
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

### Core Components
- **Rust Core** (`crates/crucible-core/`): Business logic, parsing, storage traits
- **CLI** (`crates/crucible-cli/`): Command-line interface (current primary interface)
- **Web UI** (`crates/crucible-web/`): Browser-based chat interface using Svelte 5
- **SurrealDB Layer** (`crates/crucible-surrealdb/`): Database integration with EPR schema
- **Parser Implementation** (`crates/crucible-parser/`): Markdown parsing implementation
- **MCP Server** (`crates/crucible-tools/`): Model Context Protocol server for AI agent integration
- **Justfile MCP** (`crates/crucible-just/`): Exposes justfile recipes as MCP tools
- **TOON Query** (`crates/tq/`): jq-like query language for structured data manipulation

### Key Technologies
- **Rust**: Core performance-critical components
- **SurrealDB**: Embedded database with RocksDB backend
- **Svelte 5**: Frontend framework for web UI
- **Axum**: Web server framework for HTTP API and SSE
- **MCP**: Model Context Protocol for AI agent integration
- **Justfile**: Task runner with automatic MCP tool exposure
- **Tauri**: Desktop application framework (future)
- **Rune**: Plugin scripting language with MCP tool support

## 📁 Project Structure & File Organization

### Directory Layout
```
crucible/
├── crates/                      # Rust workspace crates
│   ├── crucible-core/           # Core business logic
│   ├── crucible-cli/            # CLI application
│   ├── crucible-web/            # Browser-based chat UI
│   ├── crucible-surrealdb/      # Database layer
│   ├── crucible-parser/         # Markdown parsing implementation
│   ├── crucible-tools/          # MCP server and tools
│   ├── crucible-rune/           # Rune scripting language
│   ├── crucible-just/           # Justfile parser and MCP tool generator
│   ├── tq/                      # TOON Query library
│   └── ...                      # Other crates
├── openspec/                    # Change proposals & specs
│   ├── SYSTEMS.md               # System boundaries and organization
│   ├── AGENTS.md                # OpenSpec workflow guide
│   ├── changes/                 # Active proposals
│   ├── specs/                   # Current specs (organized by system)
│   └── archive/                 # Completed changes (organized by system)
├── docs/                        # EMPTY - reserved for future user docs
├── examples/                    # Example code and demos
├── packages/                    # Other packages (web UI for desktop, MCP, etc.)
├── scripts/                     # Build and utility scripts
├── tests/                       # Integration tests
├── justfile                     # Development recipes (run `just` to see all)
├── AGENTS.md                    # This file - AI agent guide
├── README.md                    # Project overview
└── Cargo.toml                   # Rust workspace definition
```

### 📋 Where to Put Things

**Keep the repo root clean!** Only essential files belong here.

**✅ Allowed in root:**
- `README.md` - project information
- `AGENTS.md` - this file (CLAUDE.md symlinks to it)
- `Cargo.toml`, `package.json` - build configuration
- `LICENSE`, `.gitignore` - project metadata

**❌ Do NOT create in root:**
- Documentation (use `docs/` when needed, currently empty)
- Exploration notes (delete when done)
- Temporary markdown files (clean up after use)
- Agent conversation logs (don't commit)

**Where things belong:**
- **Change proposals**: `openspec/changes/` - see `openspec/AGENTS.md` for full workflow
- **Specifications**: `openspec/specs/` - current system capabilities
- **Future user docs**: `docs/` (reserved, currently empty)
- **Examples**: `examples/`
- **Scripts**: `scripts/`
- **Tests**: `tests/` or `crates/*/tests/`

### 🔄 Using OpenSpec

For architectural changes, new features, or breaking changes, use the OpenSpec workflow:

**See `openspec/AGENTS.md` for complete details.** Quick reference:
- Create proposal in `openspec/changes/[change-id]/`
- Write `proposal.md`, `tasks.md`, and spec deltas
- Validate with `openspec validate [change-id] --strict`
- Get approval before implementing

### 🗂️ Docs Folder

The `docs/` folder is **empty and reserved for future use**. Don't create documentation there without discussion. Use OpenSpec for technical specs and change proposals.

## 🔧 Development Guidelines

### Development Workflow
- **Use `just`**: The project uses Just for common development recipes
  - `just build` - Build all crates
  - `just test` - Run all tests
  - `just web` - Build and run web UI
  - `just mcp` - Start MCP server
  - All justfile recipes are automatically exposed as MCP tools (prefixed with `just_`)
  - Run `just` to see all available commands

### Code Style
- **Rust**: Use `snake_case` for functions/variables, `PascalCase` for types
- **Error Handling**: Use `Result<T, E>` with proper error context
- **Documentation**: Add comments for complex logic, clear commit messages

### Testing
- Write unit tests for core functionality
- Include integration tests for component interactions
- Test error conditions and edge cases
- Use descriptive test names that explain the scenario

### Quality Checklist
Before submitting changes:
- [ ] Code follows project style guidelines
- [ ] Tests pass and provide good coverage
- [ ] Error handling is comprehensive
- [ ] OpenSpec updated if needed (see `openspec/AGENTS.md`)
- [ ] Performance and security implications considered
- [ ] No debug code left in
- [ ] Conventional commit messages

## 🔗 Key Resources

- **[STATUS.md](./STATUS.md)**: Current refactor status and next steps
- **[README.md](./README.md)**: Project overview
- **[OpenSpec AGENTS.md](./openspec/AGENTS.md)**: Change proposal workflow
- **[crucible-web/AGENTS.md](./crates/crucible-web/AGENTS.md)**: Web UI development guide
- **[justfile](./justfile)**: Development recipes (run `just` to see all)
- **[Rust Documentation](https://doc.rust-lang.org/)**: Rust language reference

---

*This guide helps AI agents work effectively with the Crucible codebase. Follow these guidelines to maintain code quality, consistency, and project integrity.*

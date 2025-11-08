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

### Core Components
- **Rust Core** (`crates/crucible-core/`): Business logic, parsing, storage traits
- **CLI** (`crates/crucible-cli/`): Command-line interface (current primary interface)
- **SurrealDB Layer** (`crates/crucible-surrealdb/`): Database integration with EPR schema
- **Desktop App** (`crates/crucible-tauri/`): Tauri-based desktop application (future)

### Key Technologies
- **Rust**: Core performance-critical components
- **SurrealDB**: Embedded database with RocksDB backend
- **Tauri**: Desktop application framework (future)
- **Rune**: Plugin scripting language (future)

## 📁 Project Structure & File Organization

### Directory Layout
```
crucible/
├── crates/                      # Rust workspace crates
│   ├── crucible-core/           # Core business logic
│   ├── crucible-cli/            # CLI application
│   ├── crucible-surrealdb/      # Database layer
│   ├── crucible-tauri/          # Desktop app (future)
│   └── ...                      # Other crates
├── openspec/                    # Change proposals & specs (see AGENTS.md there)
│   ├── AGENTS.md                # OpenSpec workflow guide
│   ├── changes/                 # Proposed changes
│   └── specs/                   # Current specifications
├── docs/                        # EMPTY - reserved for future user docs
├── examples/                    # Example code and demos
├── packages/                    # Other packages (web UI for desktop, MCP, etc.)
├── scripts/                     # Build and utility scripts
├── tests/                       # Integration tests
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
- **[Rust Documentation](https://doc.rust-lang.org/)**: Rust language reference

---

*This guide helps AI agents work effectively with the Crucible codebase. Follow these guidelines to maintain code quality, consistency, and project integrity.*

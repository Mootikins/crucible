# AI Agent Guide for Crucible

> Instructions for AI agents (Claude, Codex, etc.) working on the Crucible codebase

## Project Overview

**Crucible** is a local-first AI assistant where every conversation becomes a searchable note. It combines:
- **Agent chat** with session persistence as markdown
- **Knowledge graph** from wikilinks with semantic search
- **Lua plugins** with Fennel support
- **MCP server** for external agent integration

**Core Principles:**
- Plaintext first — markdown files are source of truth
- Sessions as notes — conversations saved to your kiln
- Lua extensibility — write plugins in Lua or Fennel
- Capability-based LLM providers — swap backends freely

## Architecture

### Crate Organization

| Crate | Purpose | Key Types |
|-------|---------|-----------|
| `crucible-core` | Domain logic, traits, parser types | `Provider`, `CanEmbed`, `CanChat`, `ParsedNote` |
| `crucible-cli` | Terminal UI, REPL, commands | `InkChatApp`, `ChatAppMsg` |
| `crucible-oil` | Terminal rendering primitives | `Node`, `render_to_string` |
| `crucible-web` | Browser chat UI (SolidJS + Axum) | HTTP/SSE endpoints |
| `crucible-tools` | MCP server and tools | Tool implementations |
| `crucible-sqlite` | SQLite storage (default); fast, lightweight | `SqliteStorage` |
| `crucible-surrealdb` | SurrealDB storage (advanced); EAV schema | `SurrealStorage`, `EavGraph` |
| `crucible-lua` | Lua/Luau with Fennel support | `LuaExecutor`, `FennelCompiler` |
| `crucible-llm` | Embedding backends | `EmbeddingBackend` (FastEmbed, Burn, LlamaCpp) |
| `crucible-rig` | LLM chat via Rig | Ollama, OpenAI, Anthropic adapters |
| `crucible-parser` | Markdown parsing | `MarkdownParser` |
| `crucible-config` | Configuration types and loading | `AppConfig`, provider configs |
| `crucible-watch` | File system watching | Change detection |
| `crucible-acp` | Agent Context Protocol | Protocol types |
| `crucible-daemon` | Daemon server (cru-server) | `Server`, `SessionManager`, `AgentManager` |
| `crucible-rpc` | Daemon RPC client library | `DaemonClient`, `DaemonStorageClient` |

See `crates/` for additional crates (lance, query, skills, plugins, pipeline, etc.)

### Daemon Architecture

Crucible uses a **separate daemon binary** (`cru-server`) for multi-session support:

```
┌─────────────────────────────────────────────────────────────┐
│  CLI (cru)                    Daemon (cru-server)           │
│  ┌─────────────┐              ┌──────────────────────────┐  │
│  │ cru chat    │◄────────────►│ Unix Socket Server       │  │
│  │ cru search  │   JSON-RPC   │ ($XDG_RUNTIME_DIR/       │  │
│  │ cru process │              │  crucible.sock)          │  │
│  └─────────────┘              │                          │  │
│                               │ Managers:                │  │
│  storage.mode = "embedded"    │ • KilnManager            │  │
│  → Direct DB access           │ • SessionManager         │  │
│                               │ • AgentManager           │  │
│  storage.mode = "daemon"      │ • SubscriptionManager    │  │
│  → RPC to cru-server          └──────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

- **Socket path**: `$CRUCIBLE_SOCKET` env var, or `$XDG_RUNTIME_DIR/crucible.sock`, or `/tmp/crucible.sock`
- **Storage modes**: `embedded` (default, direct DB), `daemon` (RPC to cru-server), `sqlite`, `lightweight`
- **Auto-spawn**: `DaemonClient::connect_or_start()` spawns `cru-server` if not running
- **Protocol**: JSON-RPC 2.0 over Unix socket with async event streaming

**RPC methods:**
- Kiln: `kiln.open`, `kiln.close`, `kiln.list`, `search_vectors`, `list_notes`, `get_note_by_name`
- Sessions: `session.create`, `session.list`, `session.get`, `session.load`, `session.pause`, `session.resume`, `session.end`
- Agents: `session.configure_agent`, `session.send_message`, `session.cancel`, `session.switch_model`, `session.list_models`
- Config: `session.set_thinking_budget`, `session.get_thinking_budget`
- Events: `session.subscribe`, `session.unsubscribe`

### Cross-Layer Feature Checklist

When implementing features that affect agent/session behavior (not just UI display):

**Scope Classification:**
| Scope | Examples | Where State Lives |
|-------|----------|-------------------|
| Session-scoped | model, thinking_budget, temperature | Daemon `SessionAgent`, synced via RPC |
| TUI-local | theme, show_thinking, verbose | `InkChatApp` fields, no RPC needed |

**Before Implementing:**
- [ ] Check if daemon already has RPC for this (`crucible-rpc/src/client.rs`)
- [ ] Check if `SessionAgent` has a field for this (`crucible-core/src/session/types.rs`)
- [ ] Determine scope: Does this need multi-client consistency? If yes → session-scoped

**Implementation (session-scoped features):**
- [ ] Add method to `AgentHandle` trait (`crucible-core/src/traits/chat.rs`)
- [ ] Implement in `DaemonAgentHandle` (`crucible-rpc/src/agent.rs`)
- [ ] Add `ChatAppMsg` variant (`crucible-cli/src/tui/oil/chat_app.rs`)
- [ ] Handle in `chat_runner` (`crucible-cli/src/tui/oil/chat_runner.rs`)
- [ ] Wire TUI command (`:set`, etc.) to emit the `ChatAppMsg`

**Validation:**
- [ ] RPC field names match between client and server (common bug: `"budget"` vs `"thinking_budget"`)
- [ ] Test with daemon running (`cru-server`)
- [ ] `session.get_*` returns what `session.set_*` stored
- [ ] State persists across TUI restart (resume session)

**Common Mistakes:**
- Implementing in TUI only without daemon RPC → breaks multi-client
- Different JSON field names in client vs server → silent failures
- Soft-prompt injection in TUI instead of daemon-side → inconsistent behavior

### Type Ownership

**Parser Types** are canonically defined in `crucible-core/src/parser/types/` (split into submodules).
Core re-exports via `crucible_core::parser::*`.

**Hash Types**: `BlockHash` in `crucible-core/src/parser/types/block_hash.rs`.
Other hash infrastructure in `crucible-core/src/types/hashing.rs`.

**LLM Types** (unified contracts):
- `ContextMessage` — canonical message type for all conversation contexts
- `BackendError` / `BackendResult` — canonical error type for LLM operations
- `CompletionBackend` — canonical trait for chat/completion providers

**Event Types**: `SessionEvent` includes pre-events (`PreToolCall`, `PreParse`, `PreLlmCall`) for handler interception.

**DO NOT duplicate types between crates.** Each type has exactly one canonical location. Use re-exports.

**Result Type Aliases** follow `<Domain>Result<T>`: `StorageResult`, `ChatResult`, `BackendResult`, `ToolResult`, `ParserResult`, `AcpResult`. The crate-level `crucible_core::Result<T>` is for general operations.

**Import patterns:**
```rust
use crucible_core::parser::{ParsedNote, Wikilink, Tag, BlockHash};
use crucible_core::types::hashing::{FileHash, HashAlgorithm};
use crucible_core::traits::provider::{Provider, CanEmbed, CanChat};
use crucible_core::traits::{CompletionBackend, BackendError, ContextMessage};
use crucible_core::traits::{StorageResult, ChatResult, BackendResult, ToolResult};
```

### LLM Provider System

```
Provider (base trait)
   ├── CanEmbed (embedding generation)
   ├── CanChat (chat completions)
   └── CanConstrainGeneration (grammar/schema constraints)
```

| Backend | Embeddings | Chat | Constrained | Feature Flag |
|---------|------------|------|-------------|--------------|
| Ollama | Yes | Yes | No | default |
| OpenAI | Yes | Yes | JSON Schema | default |
| FastEmbed | Yes | No | No | `fastembed` |
| LlamaCpp | Yes | Yes | GBNF | `llama-cpp` |
| Burn | Yes | No | No | `burn` |

### Systems

See **[docs/Meta/Systems.md](./docs/Meta/Systems.md)** for full details.

| System | Scope |
|--------|-------|
| **chat** | TUI/Web interfaces, session persistence |
| **agents** | Agent cards, LLM providers, tools |
| **parser** | Markdown → structured data |
| **storage** | SQLite (default), SurrealDB (advanced), Merkle trees |
| **scripting** | Lua/Fennel runtimes |
| **workflows** | Definitions + sessions |
| **apis** | HTTP, WebSocket, MCP, events |
| **cli** | Commands, REPL, configuration |

## Project Structure

```
crucible/
├── crates/                      # Rust workspace crates
│   ├── crucible-core/           # Core business logic and traits
│   ├── crucible-cli/            # Terminal UI, REPL, commands
│   ├── crucible-oil/            # Terminal rendering primitives
│   ├── crucible-web/            # Browser-based chat UI
│   ├── crucible-tools/          # MCP server and tools
│   ├── crucible-daemon/         # Daemon server (cru-server)
│   ├── crucible-rpc/            # Daemon RPC client library
│   ├── crucible-surrealdb/      # Database layer
│   ├── crucible-lua/            # Lua/Luau with Fennel
│   ├── crucible-llm/            # Embedding backends
│   ├── crucible-rig/            # LLM chat via Rig
│   ├── crucible-parser/         # Markdown parsing
│   ├── crucible-config/         # Configuration types
│   ├── crucible-watch/          # File watching
│   └── ...                      # lance, query, sqlite, skills, plugins, etc.
├── vendor/                      # Patched upstream dependencies
├── docs/                        # Documentation kiln (user guides + test fixture)
├── justfile                     # Development recipes
├── AGENTS.md                    # This file (CLAUDE.md symlinks here)
└── README.md                    # Project overview
```

### Where to Put Things

**Keep the repo root clean.** Only build config, metadata, and top-level docs belong here.

| Location | Content |
|----------|---------|
| `docs/Help/` | User-facing reference |
| `docs/Meta/` | Architecture docs, analysis |
| `docs/Guides/` | Usage guides |
| `vendor/` | Patched upstream crates |
| `examples/` | Examples |
| `scripts/` | Scripts |
| `tests/` or `crates/*/tests/` | Tests |

Do NOT create documentation files, temp files, or conversation logs in the root.

### Documentation Kiln

The `docs/` directory is a **reference kiln** — a valid Crucible vault serving as both documentation and test fixture. Integration tests validate it parses and indexes correctly.

Conventions: use wikilinks (`[[Help/Wikilinks]]`), add frontmatter with tags, keep notes focused and well-linked.

## Development Guidelines

### Workflow

**Use `just`** for common recipes:
- `just ci` — **Run before committing**: format check, clippy, quick tests
- `just build` / `just test` / `just check` — build, test, check
- `just web` / `just mcp` — web UI, MCP server

**Don't build release unless installing.** Release builds use LTO and take 5-10 minutes. Use debug builds for iteration.

**Web frontend uses `bun`** (not npm/yarn). See `crates/crucible-web/web/AGENTS.md`.

### Code Style

- `snake_case` for functions/variables, `PascalCase` for types
- `Result<T, E>` with proper error context
- Doc comments for public items
- TDD — write tests for new functionality
- Fix clippy warnings properly — no module-level `#![allow(...)]`

### Feature Flags

```toml
[features]
default = ["fastembed"]
fastembed = ["dep:fastembed"]      # Local ONNX embeddings
llama-cpp = ["dep:llama-cpp-2"]   # GGUF model support
burn = ["dep:burn"]               # Burn ML framework
test-utils = []                   # Mock providers for testing
```

### Vendored Dependencies

Patched upstream crates live in `vendor/`. `Cargo.toml` has `[patch.crates-io]` entries.

| Crate | Reason | Patches |
|-------|--------|---------|
| `markdown-it` | Semi-abandoned, panic bugs | Underflow fixes in `emph_pair.rs` |

When patching: add `NOTE(crucible):` comments, update `vendor/README.md`, add regression tests.

### Testing

Tests use **cargo-nextest** with tier profiles:

| Tier | Purpose | Command |
|------|---------|---------|
| **Unit** | Fast, isolated, mocked I/O | `cargo nextest run --profile unit` |
| **Integration** | Real DB, real files | `cargo nextest run --profile integration` |
| **Contract** | API/trait verification | `cargo nextest run --profile contract` |
| **CI** | All non-slow tests | `cargo nextest run --profile ci` |

Guidelines:
- Mock external dependencies in unit tests
- Use `#[cfg(feature = "test-utils")]` for mock providers
- Mark slow tests with `#[ignore = "reason"]`
- Use `test-case` for parameterized tests
- Use `tempfile::TempDir` for filesystem tests (never hardcode `/tmp`)
- Descriptive test names that explain the scenario

### Snapshot and Golden File Policy

**⚠️ NEVER update snapshots or golden files until the output is verified correct.**

This applies to `insta` snapshots (`.snap` files), golden test outputs, and any file that captures "expected" program output.

**Rules:**
1. **Do not run `cargo insta accept`** or manually update `.snap` files to make tests pass. A passing snapshot test only means output is stable, not correct.
2. **When snapshots change**, read the actual snapshot content and verify it matches the intended visual/textual output before accepting.
3. **When snapshots fail after your changes**, the default assumption is your code is wrong, not the snapshot. Investigate the implementation first.
4. **New snapshots** require reading the generated `.snap.new` file and confirming correctness before accepting.
5. **Bulk snapshot updates** (`cargo insta accept --all`) are forbidden without per-file review.

**Verification steps when snapshots change:**
```bash
# See which snapshots changed
git diff --name-only | grep '\.snap$'

# Read each changed snapshot
cat crates/crucible-cli/src/tui/oil/tests/snapshots/<test_name>.snap

# If a reference script exists, compare
python3 scripts/notif_styling_demo.py > /tmp/reference.txt
diff /tmp/reference.txt <snapshot_content>
```

**What to check:**
- Visual correctness (layout, alignment, spacing)
- Unicode glyphs are the right characters (not just visually similar)
- ANSI escape codes produce correct colors
- No content duplication, missing sections, or ordering issues

**If snapshot doesn't match expectations:** fix the implementation, not the snapshot.

### TUI Testing Workflow

**Test type selection:**

| Scenario | Test Type | Why |
|----------|-----------|-----|
| State changes (popup open/close, mode switch) | Unit test with `InkChatApp` | Fast, isolated |
| Visual output (layout, colors, content) | Snapshot test with `insta` | Catches regressions |
| Keyboard interactions | Unit test with `Event::Key` | Deterministic |
| Multi-turn flows (chat, streaming) | Integration test | Component interaction |
| Real terminal behavior (escape sequences) | PTY test with `expectrl` | E2E verification |

Start with unit tests. Escalate to PTY tests only when unit tests can't verify the behavior.

**Fixture reuse:** Check `tui/testing/fixtures/` before creating new fixtures:
- `sessions.rs` — Conversation histories
- `registries.rs` — Commands, agents, files, sessions, models
- `events.rs` — Event sequences

**New TUI features require full-flow snapshot tests** covering: initial state → navigation → selection → final state. See `popup_snapshot_tests.rs` for examples.

**PTY E2E tests** live in `crates/crucible-cli/tests/tui_e2e_harness.rs`. They're slow and flaky — reserve for behaviors that can't be verified any other way:

```bash
cargo test -p crucible-cli streaming_completion -- --ignored --nocapture
```

### Bugfix Workflow (Test-First)

```
1. Write failing test that reproduces the bug
2. Confirm it fails
3. Minimal code change to pass
4. Confirm it passes
5. Commit fix + test together
```

Test naming: describe the correct behavior, not the bug.
- Good: `ctrl_c_closes_popup_instead_of_inserting_c`
- Bad: `test_ctrl_c_bug`

| Validation | Confidence |
|------------|------------|
| Code review only | Low (50%) |
| Existing tests pass | Medium (70%) |
| New regression tests pass | High (90%) |
| Manual verification + tests | Very High (95%) |

### Quality Checklist

Before submitting changes:
- [ ] Code follows project style
- [ ] Tests pass (`cargo nextest run --profile ci`)
- [ ] Error handling is comprehensive
- [ ] Docs updated if needed (architecture → `docs/Meta/`)
- [ ] No debug code left in
- [ ] Conventional commit messages
- [ ] Bugfixes include regression tests
- [ ] **Snapshot changes verified correct** (see Snapshot Policy above)

## Key Resources

- **[README.md](./README.md)** — Project overview and quick start
- **[docs/Meta/Systems.md](./docs/Meta/Systems.md)** — System boundaries
- **[docs/Meta/Roadmap.md](./docs/Meta/Roadmap.md)** — Development roadmap
- **[docs/](./docs/)** — Reference kiln (user guides + test fixture)
- **[justfile](./justfile)** — Development recipes
- **[vendor/README.md](./vendor/README.md)** — Patched upstream dependencies

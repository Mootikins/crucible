# AI Agent Guide for Crucible

> Instructions for AI agents (Claude, Codex, etc.) working on the Crucible codebase

This file provides essential information for AI agents to understand and contribute to the Crucible project effectively.

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

| Crate | Purpose | Key Traits/Types |
|-------|---------|------------------|
| `crucible-core` | Domain logic, traits, parser types | `Provider`, `CanEmbed`, `CanChat`, `ParsedNote` |
| `crucible-cli` | Terminal UI, REPL, commands | `InkChatApp`, `ChatAppMsg` |
| `crucible-web` | Browser chat UI (SolidJS + Axum) | HTTP/SSE endpoints |
| `crucible-tools` | MCP server and tools | Tool implementations |
| `crucible-surrealdb` | SurrealDB storage with EAV schema | `SurrealStorage`, `EavGraph` |
| `crucible-lua` | Lua/Luau with Fennel support | `LuaExecutor`, `FennelCompiler` |
| `crucible-rune` | Rune scripting (legacy, use crucible-lua) | — |
| `crucible-llm` | Embedding backends | `EmbeddingBackend` (FastEmbed, Burn, LlamaCpp) |
| `crucible-rig` | LLM chat via Rig | Ollama, OpenAI, Anthropic adapters |
| `crucible-parser` | Markdown parsing implementation | `MarkdownParser` |
| `crucible-config` | Configuration types and loading | `AppConfig`, provider configs |
| `crucible-watch` | File system watching | Change detection |
| `crucible-acp` | Agent Context Protocol | Protocol types |
| `crucible-daemon` | Daemon server (cru-server) | `Server`, `SessionManager`, `AgentManager` |
| `crucible-daemon-client` | Daemon client library | `DaemonClient`, `DaemonStorageClient` |
| `crucible-oil` | Terminal rendering primitives | `Node`, `render_to_string` |

*See `crates/` for additional crates (lance, query, sqlite, skills, etc.)*

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

**Key concepts:**
- **Socket path**: `$CRUCIBLE_SOCKET` env var, or `$XDG_RUNTIME_DIR/crucible.sock`, or `/tmp/crucible.sock`
- **Storage modes**: `embedded` (default, direct DB), `daemon` (RPC to cru-server), `sqlite`, `lightweight`
- **Auto-spawn**: `DaemonClient::connect_or_start()` spawns `cru-server` if not running
- **Protocol**: JSON-RPC 2.0 over Unix socket with async event streaming

**Daemon RPC methods:**
- Kiln: `kiln.open`, `kiln.close`, `kiln.list`, `search_vectors`, `list_notes`, `get_note_by_name`
- Sessions: `session.create`, `session.list`, `session.get`, `session.pause`, `session.resume`, `session.end`
- Agents: `session.configure_agent`, `session.send_message`, `session.cancel`
- Events: `session.subscribe`, `session.unsubscribe`

### Cross-Layer Feature Checklist

When implementing features that affect agent/session behavior (not just UI display), use this checklist to ensure proper CLI↔daemon integration.

**Scope Classification:**
| Scope | Examples | Where State Lives |
|-------|----------|-------------------|
| Session-scoped | model, thinking_budget, temperature | Daemon `SessionAgent`, synced via RPC |
| TUI-local | theme, show_thinking, verbose | `InkChatApp` fields, no RPC needed |

**Before Implementing:**
- [ ] Check if daemon already has RPC for this (`crucible-daemon-client/src/client.rs`)
- [ ] Check if `SessionAgent` has a field for this (`crucible-core/src/session/types.rs`)
- [ ] Determine scope: Does this need multi-client consistency? If yes → session-scoped

**Implementation (for session-scoped features):**
- [ ] Add method to `AgentHandle` trait (`crucible-core/src/traits/chat.rs`)
- [ ] Implement in `DaemonAgentHandle` to call RPC (`crucible-daemon-client/src/agent.rs`)
- [ ] Add `ChatAppMsg` variant for the action (`crucible-cli/src/tui/oil/chat_app.rs`)
- [ ] Handle message in `chat_runner` by calling `agent.method()` (`crucible-cli/src/tui/oil/chat_runner.rs`)
- [ ] Wire TUI command (`:set`, etc.) to emit the `ChatAppMsg`

**Validation:**
- [ ] Verify RPC field names match between client and server (common bug: `"budget"` vs `"thinking_budget"`)
- [ ] Test with daemon running (`cru-server`)
- [ ] Verify `session.get_*` returns what `session.set_*` stored
- [ ] Check state persists across TUI restart (resume session)

**Common Mistakes:**
- Implementing in TUI only (RuntimeConfig) without daemon RPC → breaks multi-client
- Different JSON field names in client vs server → silent failures
- Soft-prompt injection in TUI instead of daemon-side handling → inconsistent behavior

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

**Result Type Aliases** follow the pattern `<Domain>Result<T>`:
- `StorageResult<T>` - storage operations
- `ChatResult<T>` - chat/conversation operations
- `BackendResult<T>` - LLM backend operations
- `ToolResult<T>` - tool execution
- `ParserResult<T>` - parsing operations
- `AcpResult<T>` - agent protocol operations

The crate-level `Result<T>` in `crucible_core::Result` is for general operations.

**Import patterns:**
```rust
// Parser types - prefer canonical location
use crucible_core::parser::{ParsedNote, Wikilink, Tag, BlockHash};

// Hash infrastructure - from core
use crucible_core::types::hashing::{FileHash, HashAlgorithm};

// LLM traits - unified provider system
use crucible_core::traits::provider::{Provider, CanEmbed, CanChat};
use crucible_core::traits::{CompletionBackend, BackendError, ContextMessage};

// Domain-specific results
use crucible_core::traits::{StorageResult, ChatResult, BackendResult, ToolResult};
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
| **chat** | TUI/Web interfaces, session persistence |
| **agents** | Agent cards, LLM providers, tools |
| **parser** | Markdown → structured data |
| **storage** | SurrealDB, EAV graph, Merkle trees |
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
│   ├── crucible-web/            # Browser-based chat UI
│   ├── crucible-tools/          # MCP server and tools
│   ├── crucible-surrealdb/      # Database layer
│   ├── crucible-lua/            # Lua/Luau with Fennel
│   ├── crucible-rune/           # Rune scripting (legacy)
│   ├── crucible-llm/            # Embedding backends
│   ├── crucible-rig/            # LLM chat via Rig
│   ├── crucible-parser/         # Markdown parsing
│   ├── crucible-config/         # Configuration types
│   ├── crucible-watch/          # File watching
│   └── ...                      # Other crates
├── vendor/                      # Patched upstream dependencies
│   ├── markdown-it/             # Patched markdown-it (panic fixes)
│   └── README.md                # Documents patches and update process
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
- `vendor/` - patched upstream crates

**Do NOT create in root:**
- Documentation files (use `docs/`)
- Temporary files (clean up after use)
- Agent conversation logs (don't commit)

**Where things belong:**
- **Feature docs**: `docs/Help/` - user-facing reference
- **Architecture docs**: `docs/Meta/` - contributor docs
- **Vendored deps**: `vendor/` - patched upstream crates
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
- `just ci` - **Run before committing**: format check, clippy, and quick tests
- `just build` - Build all crates
- `just test` - Run all tests
- `just check` - Cargo check workspace
- `just web` - Build and run web UI
- `just mcp` - Start MCP server

Run `just` to see all available commands.

**Don't build release unless installing**: Release builds use LTO and take 5-10 minutes. For development iteration, always use debug builds (`cargo build` or `cargo run`). Only build release when the user explicitly asks to install or needs a release binary.

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
- **Clippy**: Fix all warnings properly — no module-level `#![allow(...)]` suppression

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

### Vendored Dependencies

Some upstream crates have bugs or are abandoned. We maintain local patches in `vendor/`.

**Currently vendored:**

| Crate | Reason | Patches |
|-------|--------|---------|
| `markdown-it` | Semi-abandoned, panic bugs | Underflow fixes in `emph_pair.rs` |

**How it works:**
- `Cargo.toml` has `[patch.crates-io]` pointing to local path
- `vendor/markdown-it` is excluded from workspace via `workspace.exclude`
- See `vendor/README.md` for patch details and update instructions

**Adding patches:**
1. Edit files in `vendor/<crate>/src/`
2. Add `NOTE(crucible):` comments explaining the fix
3. Update `vendor/README.md` with the patch description
4. Add regression tests when possible

**When to vendor vs. fork:**
- **Vendor** (in-repo): Small fixes, abandoned upstreams, want git history together
- **Fork** (separate repo): Major changes, want to publish to crates.io, multiple consumers

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

**Test Type Selection:**

| Scenario | Test Type | Why |
|----------|-----------|-----|
| State changes (popup open/close, mode switch) | Unit test with `InkChatApp` | Fast, isolated, no I/O |
| Visual output (layout, colors, content) | Snapshot test with `insta` | Catches regressions, reviewable |
| Keyboard interactions (shortcuts, navigation) | Unit test with `Event::Key` | Deterministic, fast |
| Multi-turn flows (chat, streaming) | Integration test | Tests component interaction |
| Real terminal behavior (escape sequences, timing) | PTY test with `expectrl` | E2E verification |
| Cross-platform rendering | PTY test | Catches platform-specific bugs |

**Start with unit tests. Escalate to PTY tests only when unit tests can't verify the behavior.**

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
- `registries.rs` - Commands, agents, files, sessions, models
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

**New TUI features require full-flow snapshot tests.** When adding popups, dialogs, or interactive elements:

1. Add fixture helpers to `tui/testing/fixtures/registries.rs`:
   ```rust
   pub fn test_models() -> Vec<PopupItem> { ... }
   ```

2. Add snapshot tests covering the full interaction flow:
   - Initial state (popup opens)
   - Navigation (cursor moves with arrow keys)
   - Selection (item selected, popup closes)
   - Final state (status bar/view updated)

3. Example test structure (see `popup_snapshot_tests.rs`):
   ```rust
   mod model_popup_tests {
       #[test]
       fn popup_model_list() {
           let h = Harness::new(80, 24)
               .with_popup_items(PopupKind::Model, registries::test_models());
           assert_snapshot!("popup_model_list", h.render());
       }

       #[test]
       fn popup_model_navigation() {
           let mut h = Harness::new(80, 24)
               .with_popup_items(PopupKind::Model, registries::test_models());
           h.key(KeyCode::Down);
           assert_snapshot!("popup_model_second", h.render());
       }
   }
   ```

4. Review snapshots with `cargo insta review` before accepting.

### Bugfix Workflow (Test-First)

When fixing bugs, **write failing tests before fixing code**. This ensures:
1. The bug is reproducible
2. The fix actually works
3. Regression protection for the future

**Workflow:**

```
1. Report bug → Write failing test that reproduces it
2. Run test → Confirm it fails (proves bug exists)
3. Fix code → Minimal change to pass the test
4. Run test → Confirm it passes (proves fix works)
5. Commit → Include both fix AND test together
```

**Example test for a bug:**
```rust
#[test]
fn ctrl_c_closes_popup_instead_of_inserting_c() {
    let mut app = InkChatApp::default();
    app.set_workspace_files(vec!["test.rs".to_string()]);

    app.update(Event::Key(key(KeyCode::Char('@'))));
    assert!(app.is_popup_visible(), "Popup should open on @");

    app.update(Event::Key(ctrl('c')));

    assert!(!app.is_popup_visible(), "Ctrl+C should close popup");
    assert!(
        !app.input_content().contains('c'),
        "Ctrl+C should not insert 'c' character"
    );
}
```

**Test naming convention for bugfixes:**
- Name describes the correct behavior, not the bug
- Good: `ctrl_c_closes_popup_instead_of_inserting_c`
- Bad: `test_ctrl_c_bug` or `fix_popup_issue`

**Commit message format:**
```
fix(component): brief description of what was fixed

- Bullet points explaining the fix
- Include edge cases handled

Added regression tests:
- test_name_one
- test_name_two
```

**Confidence levels after bugfix:**
| Validation | Confidence |
|------------|------------|
| Code review only | Low (50%) |
| Existing tests pass | Medium (70%) |
| New regression tests pass | High (90%) |
| Manual verification + tests | Very High (95%) |

### PTY-Based E2E Testing

For testing real TUI behavior with actual terminal emulation, use the `expectrl`-based harness in `tests/tui_e2e_harness.rs`:

```rust
use tui_e2e_harness::{TuiTestBuilder, Key};

#[test]
#[ignore = "requires built binary and Ollama"]
fn test_chat_interaction() {
    let mut session = TuiTestBuilder::new()
        .command("chat")
        .env("RUST_LOG", "crucible_rig=info")
        .timeout(30)
        .spawn()
        .expect("Failed to spawn");

    // Send message
    session.send_line("hello").expect("send failed");

    // Wait for response
    session.expect_regex(r"Hello").expect("no response");

    // Send Ctrl+C to exit
    session.send_control('c').expect("Ctrl+C failed");
}
```

**When to use PTY tests:**
- Testing real streaming behavior end-to-end
- Debugging timing issues (spinner, status updates)
- Verifying terminal escape sequence handling
- Multi-turn conversation flows with real LLM

**When NOT to use PTY tests:**
- Simple state changes (use unit tests)
- Layout verification (use snapshot tests)
- Keyboard shortcut handling (use unit tests with `Event::Key`)

PTY tests are slow and flaky. Reserve them for behaviors that can't be verified any other way.

**Run PTY tests:**
```bash
cargo test -p crucible-cli streaming_completion -- --ignored --nocapture
```

### Quality Checklist

Before submitting changes:
- [ ] Code follows project style guidelines
- [ ] Tests pass (`cargo nextest run --profile ci`)
- [ ] Error handling is comprehensive
- [ ] Docs updated if needed (architectural changes go in `docs/Meta/`)
- [ ] No debug code left in
- [ ] Conventional commit messages
- [ ] **Bugfixes include regression tests** (see Bugfix Workflow above)

## Key Resources

- **[README.md](./README.md)** - Project overview and quick start
- **[docs/Meta/Systems.md](./docs/Meta/Systems.md)** - System boundaries and organization
- **[docs/Meta/Roadmap.md](./docs/Meta/Roadmap.md)** - Development roadmap
- **[Documentation](./docs/)** - Reference kiln (user guides + test fixture)
- **[justfile](./justfile)** - Development recipes
- **[vendor/README.md](./vendor/README.md)** - Patched upstream dependencies

---

*This guide helps AI agents work effectively with the Crucible codebase. Follow these guidelines to maintain code quality, consistency, and project integrity.*

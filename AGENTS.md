# AI Agent Guide for Crucible

> Instructions for AI agents working on Crucible. `CLAUDE.md` symlinks here.

**Crucible** is a knowledge-grounded agent runtime: notes, sessions, and wikilinks form a
knowledge graph that agents draw from and contribute to. Plaintext-first (markdown is the
source of truth), Neovim-like architecture (headless daemon + RPC, Lua/Fennel extensibility,
TUI-first, plugin-driven).

## Architecture

| Crate | Purpose |
|-------|---------|
| `crucible-core` | Domain types, traits, parser, config (`Provider`, `CanChat`, `ParsedNote`, `AppConfig`) |
| `crucible-cli` | TUI (`OilChatApp`), REPL, commands; `cru web` behind default-on `web` feature |
| `crucible-daemon` | RPC server, sessions, enrichment, ACP host (`acp/`), embeddings (`llm/`), SQLite (`storage/sqlite/`) + LanceDB (`storage/lance/`), skills, tools |
| `crucible-web` | Axum server + SolidJS frontend (`web/`, embedded via rust-embed) |
| `crucible-oil` | Terminal rendering primitives |
| `crucible-lua` | Lua/Luau scripting with Fennel support |

Single `cru` binary. The daemon (`cru daemon serve`) is auto-spawned by
`DaemonClient::connect_or_start()`; JSON-RPC 2.0 over Unix socket
(`$CRUCIBLE_SOCKET`, else `$XDG_RUNTIME_DIR/crucible.sock`, else
`<tmpdir>/crucible-<uid>/crucible.sock` — per-uid and 0700, because a shared
`/tmp/crucible.sock` let any local user reach an unauthenticated RPC surface).
**All storage is daemon-side** — the CLI has zero direct storage access.
**Daemon owns business logic** (enrichment, providers, agent lifecycle); CLI/TUI/Web are
thin rendering/input layers. If a web frontend would need to duplicate it, it's in the wrong place.

### Terminology — do not use interchangeably

- **Project** — where work output goes. A registered directory (git root or invocation dir). `.crucible/project.toml`.
- **Kiln** — where accrued knowledge goes: notes, sessions, linked content. `.crucible/kiln.toml`.
- **Workspace** — a specific instance of a project directory (often the root, could be a worktree). Runtime concept, **no config file**. Do NOT rename existing correct uses: `session.workspace`, `WorkspaceTools`, `workspace: &Path`, Lua `paths.workspace()`. Old `workspace.toml` is still read as a backward-compat fallback.

### Type ownership

- Parser types live canonically in `crucible-core/src/parser/types/` (`BlockHash` included); other hashing in `crucible-core/src/types/hashing.rs`. `ContextMessage` is the canonical conversation message type.
- **Never duplicate types between crates.** One canonical location per type; use re-exports.
- Result aliases follow `<Domain>Result<T>` (`StorageResult`, `ChatResult`, `ToolResult`, `ParserResult`, `AcpResult`); `crucible_core::Result<T>` for general operations.

### Session-scoped vs TUI-local features

State needing multi-client consistency (model, thinking budget, temperature) lives in the
daemon's `SessionAgent` and syncs via RPC. Pure display state (theme, show_thinking) stays in
`OilChatApp`. For session-scoped features, wire the full chain:
`AgentHandle` trait (`crucible-core/src/traits/chat.rs`) → `DaemonAgentHandle`
(`crucible-daemon/src/rpc_client/agent.rs`) → `ChatAppMsg` variant → `chat_runner` handler →
TUI command. Pitfalls: implementing TUI-only (breaks multi-client), mismatched JSON field
names between client and server (silent failure — verify `session.get_*` returns what
`session.set_*` stored, and that state survives session resume).

### Hooks and ACP delegation

- Plugins can fully handle tool calls: `crucible.on("pre_tool_call", opts, handler)` returns `{ handled = true, result = ... }` to replace execution, `{ cancel = true, reason = ... }` to block, or `nil` to observe. Supports `pattern` and `priority` opts; handlers may call async APIs. Reference implementation: `runtime/plugins/oci/`.
- Crucible delegates to external agents (Claude Code, OpenCode, Gemini CLI) via ACP: `cru chat -a claude`, `cru session create --agent claude`, or the `delegate_session` tool. Trust/depth limits in `~/.config/crucible/config.toml` (`[acp.agents.*]` profiles can `extends` built-ins). Code: `crucible-daemon/src/acp/`, `agent_manager/`, `tools/mcp_server.rs`.

## Workflow

**Use `just` recipes before invoking cargo/bunx/vitest/playwright directly** — they encode
this box's constraints (thread caps, build prereqs). Scoped runs pass through args. If a flow
has no recipe and you need it twice, add a recipe.

- `just ci` — **run before committing**
- `just build` / `just test` / `just check`; `just test-crate <crate>`; `just test ignored` / `just test full` for `#[ignore]`d tests
- `just web-test-unit [paths…]` / `just web-test [specs…]` / `just web-typecheck`
- `just web` (build + serve on 3000) / `just web-debug [port]` (side port, safe next to installed instance); `just mcp`

**Don't build release unless installing** — LTO takes 5–10 minutes; iterate on debug builds.
Web frontend uses **bun** (not npm/yarn); see `crates/crucible-web/web/AGENTS.md`.

**Keep the repo root clean.** Docs go in `docs/Help|Meta|Guides/`, scripts in `scripts/`,
examples in `examples/`. Never create documentation, temp files, or conversation logs in the
root. `docs/` is a reference kiln — a valid Crucible vault that integration tests parse and
index; use wikilinks and frontmatter tags. Patched upstream crates live in `vendor/` (via
`[patch.crates-io]`): add `NOTE(crucible):` comments, update `vendor/README.md`, add
regression tests.

## Code Principles

- **Crate boundaries are for compilation, not organization.** If related types need `pub` wrappers to see each other, question the boundary. Prefer fewer, larger crates. Co-locate related state.
- **anyhow by default, thiserror at boundaries.** `anyhow::Result` + `.context()` internally; structured error enums only at API/RPC boundaries where callers match on variants.
- **YAGNI.** No speculative variants, fields, or abstractions. Every `Option<T>` needs a `None` path; every error variant needs a distinct handler.
- **Enums over traits** unless 2+ implementations exist in different crates; `dyn` only for genuine runtime polymorphism.
- **Compress via the type system**: `From`/`Into` over `.map_err()` chains, `?`, iterator combinators, `#[derive]`. A pattern repeated 5+ times is a missing helper. One pattern, used uniformly.
- **Comments explain why, not what** — tradeoffs, workarounds, non-obvious decisions.
- **Lua sees the same domain model.** Rust types are source of truth; Lua bindings project them, never duplicate them.
- `snake_case` functions, `PascalCase` types, `snake_case.rs` modules. Fix clippy warnings properly — no module-level `#![allow(...)]`.
- **Name for what the code does, not where it lives.** `new()` for simple constructors (few params, no external resources); `new_with_*()` when exactly one thing varies (`AgentManager::new_with_delegation`); `create_*()` for factories that build external resources or return trait objects, typically at the composition root (`create_knowledge_repository`) — start with `new()` and rename only once it is clearly a factory. Suffixes: `*Handler` reacts to events or messages (`IndexingHandler`), `*Executor` executes actions or commands (`ToolExecutor`), `*Config` is configuration loaded or assembled at startup (`ChatConfig`).

## Testing

Tests use **cargo-nextest**. Profiles (`unit`, `integration`, `contract`, `ci`) select
retry/timeout behavior only — none filter the test set; scope with `-p <crate>` or
`-E 'test(...)'`. Tests needing external prerequisites (daemon, Ollama, agent binaries) are
`#[ignore]`d with the prerequisite in the reason string — that, not cargo features, is how
slow/external tests are gated.

- TDD: bugfixes start with a failing test that reproduces the bug; commit fix + test together. Name tests for the correct behavior (`ctrl_c_closes_popup_instead_of_inserting_c`), not the bug.
- Mock external deps in unit tests (`#[cfg(feature = "test-utils")]` mock providers); `tempfile::TempDir` for filesystem tests — never hardcode `/tmp`.
- **Never dismiss test failures as "pre-existing" or "unrelated."** If tests fail after your changes, assume your change broke them until proven otherwise.

### Hermeticity (env / data root)

Never mutate process env with raw `std::env::set_var` — it races under parallel runs.

- **In-process** tests (constructing `Server`/managers directly): inject an isolated data root as a value — `Server::bind_with_data_home(&sock, tempdir.path().to_path_buf())` (or pass the tempdir as the `data_home` argument to session-list/sweep handlers). Skipping this loads the developer's real `~/.crucible` registry: passes on CI, fails locally. Fixtures that list providers must also call the rustls `install_default()` helper.
- **Out-of-process** tests (spawning `cru daemon serve`): child-scoped env only — `Command::env("CRUCIBLE_HOME", tempdir)` (see `TestDaemon` in `tests/common/mod.rs`).
- `EnvVarGuard` (`crucible_core::test_support`) is only for tests that genuinely exercise env-reading behavior — not a hermeticity band-aid.

### Snapshots / golden files

**Never accept a snapshot until you've verified the output is correct.** A passing snapshot
test proves stability, not correctness. No `cargo insta accept --all` without per-file
review; read every changed `.snap`/`.snap.new` and check layout, exact Unicode glyphs, ANSI
colors, and no duplicated/missing content. When a snapshot fails after your changes, the
default assumption is the implementation is wrong — fix the code, not the snapshot.

### TUI tests

Start with unit tests on `OilChatApp` (state, keyboard via `Event::Key`); snapshot tests
(`insta`) for visual output; PTY tests (`expectrl`, `tests/tui_e2e_harness.rs`) only for
behavior nothing else can verify — they're slow and flaky. Drive the app via
`Vt100TestRuntime` (rendered frames) or `AppHarness` (string frames). JSONL session fixtures
live in `assets/fixtures/*.jsonl`; mock agents use the `impl_noop_agent!` / `CountingAgent`
pattern. **New TUI features require a story in `docs/Meta/TUI User Stories.md` plus T1 + T2
coverage** — full-flow tests in `src/tui/oil/tests/user_story_tests/` via `StoryRuntime`.

## Before Submitting

Style followed · tests pass (`just ci`) · docs updated (architecture → `docs/Meta/`) · no
debug code · conventional commits · bugfixes include regression tests · snapshot changes
verified correct.

## Key Resources

- [README.md](./README.md) — overview and quick start
- [docs/Meta/Analysis/Systems.md](./docs/Meta/Analysis/Systems.md) — system boundaries
- [justfile](./justfile) — development recipes
- [vendor/README.md](./vendor/README.md) — patched dependencies
- `docs/Help/Concepts/` — ACP, MCP, and Agent Skills specification references

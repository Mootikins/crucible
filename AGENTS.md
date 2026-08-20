# AI Agent Guide for Crucible

> Instructions for AI agents working on Crucible. `CLAUDE.md` symlinks here.

**Crucible** is a knowledge-grounded agent runtime: notes, sessions and wikilinks form a
knowledge graph agents draw from and contribute to. Plaintext-first, Neovim-like (headless
daemon + RPC, Lua/Fennel extensibility, TUI-first, plugin-driven).

## Architecture

| Crate | Purpose |
|-------|---------|
| `crucible-core` | Domain types, traits, parser, config |
| `crucible-cli` | TUI (`OilChatApp`), REPL, commands; `cru web` behind default-on `web` feature |
| `crucible-daemon` | RPC server, sessions, ACP host, embeddings, SQLite, skills, tools |
| `crucible-web` | Axum server + SolidJS frontend (`web/`, embedded via rust-embed) |
| `crucible-oil` | Terminal rendering primitives |
| `crucible-lua` | Lua/Luau scripting with Fennel support |

Single `cru` binary. The daemon is auto-spawned by `DaemonClient::connect_or_start()`;
JSON-RPC 2.0 over a per-uid 0700 Unix socket (`$CRUCIBLE_SOCKET`, else `$XDG_RUNTIME_DIR`,
else `<tmpdir>/crucible-<uid>/`) — a shared `/tmp/crucible.sock` let any local user reach an
unauthenticated RPC surface. **All storage is daemon-side**; the CLI has none. **Daemon owns
business logic**; CLI/TUI/Web are thin render/input layers. If a web frontend would need to
duplicate it, it is in the wrong place.

## Key Abstractions

Crates are compilation units. These are the seams a change lands in — know which you are in.

| Seam | Owns | Lives in |
|------|------|----------|
| **Scope / containment** | Given a session: what may this turn read, write, search, load, execute | `agent_manager/scope.rs`, `tools/{containment,surface}.rs`, `execution_roots.rs` |
| **Session / turn lifecycle** | Turn loop, tool admission, context assembly | `agent_manager/messaging/`; `Session` reaches ~127 production files |
| **Knowledge** | Four subsystems, not one | see below |
| **Events & requests** | Fan-out with no reply; correlated one-reply-with-timeout | `event_emitter.rs`, `protocol/session_events/`, pending-reply registries |
| **Wire bindings** | Four, not one | daemon JSON-RPC; web HTTP/SSE/WS; ACP + MCP (both vendored) |
| **Lua** | Projection *and* interception | `crucible-lua/`, `runtime/` |
| **Render** | TUI and web presentation | `crucible-cli/src/tui/`, `crucible-oil/`, `crucible-web/web/` |

**Knowledge is four things.** The parser (`crucible-core/src/parser/`) is an island: raw text
plus byte spans, no resolution. Wikilink resolution, backlinks and rename splicing live in
`storage/sqlite/link_index.rs`, over types the parser never sees. `KilnName`/`KilnRegistry`
own identity. Embeddings serve retrieval as much as indexing. `NotePipeline` is the adapter
between them — a seam, not a subsystem. A change in one does not reach the others.

**Wire bindings share one type and nothing else** — `SessionEventMessage`. No shared codec,
framing, correlation or error classification. ACP re-exports `agent_client_protocol` and MCP
re-exports `rmcp`: two of four surfaces are vendored.

**Lua is not only a shim.** Projection modules (theme, statusline, geometry, oil, json, fs,
notify, paths) are safe in isolation. Interception is not: `runtime/defaults/init.lua` is
compiled in as `BUILTIN_INIT_LUA` and is the *only* definition of the three permission modes,
the plan-mode deny hook, the default system prompt and the precognition formatter.
`ModeRegistry` has no Rust default and no fallback.

**Adding a name to a closed set → one enumerated table with a real gate.** `tools/surface.rs`
is the exemplar: exhaustive match, two module-level clippy denies (both needed — with one, a
variant with `_ => Daemon` passed review), no `Default` on the return type, and a test
deriving its expectation from the running system rather than from source text. Prefer this to
a hand-maintained list checked by a source-text grep.

### Terminology — never interchangeable

- **Project** — where work output goes. Registered directory (git root or invocation dir). `.crucible/project.toml`.
- **Kiln** — where knowledge goes. `.crucible/kiln.toml`. A session *attaches* kilns (flat set, no primary); it is not *stored* in one — transcripts live under the daemon data root regardless.
- **Workspace** — an instance of a project directory (root, or a worktree). Runtime concept, no config file. Do NOT rename correct existing uses (`session.workspace`, `WorkspaceTools`, Lua `paths.workspace()`).

### Type ownership

Parser types are canonical in `crucible-core/src/parser/types/`; other hashing in
`types/hashing.rs`; `ContextMessage` is the conversation message type. **Never duplicate types
between crates** — one canonical location, then re-export. Result aliases follow
`<Domain>Result<T>`.

### Session-scoped vs TUI-local

Multi-client state (model, thinking budget, temperature) lives in the daemon's `SessionAgent`
and syncs via RPC; pure display state (theme, show_thinking) stays in `OilChatApp`.
Session-scoped needs the full chain: `AgentHandle` → `DaemonAgentHandle` → `ChatAppMsg` →
`chat_runner` handler → TUI command. TUI-only breaks multi-client, and mismatched JSON field
names fail silently — verify `session.get_*` returns what `session.set_*` stored and survives
resume. (`AgentHandle` is 44 methods, 37 defaulted: a new knob compiles everywhere without
being implemented anywhere.)

### Hooks and ACP

- `crucible.on("pre_tool_call", opts, handler)` → `{ cancel = true }` blocks, `{ handled = true, result = … }` replaces execution, `nil` observes. **`cancel` is safe; `handled` and transform are capability-grade** — `handled` returns *before* the permission gate, and only gate ordering in `messaging/tool_call.rs` prevents escalation. Its one legitimate use is `runtime/plugins/oci/`, where taking the call over *is* the sandbox. Preserve that ordering.
- ACP delegation: `cru chat --acp claude`, `cru session create --acp claude`, or `delegate_session`. (`--agent` names an agent *card*, not an ACP profile.) Limits in `[acp.agents.*]`. Code: `acp/`, `agent_manager/`, `tools/mcp_server.rs`.

## Workflow

**Use `just` recipes over raw cargo/bunx/vitest/playwright** — they encode this box's
constraints. Recipes take a sub-target (`just test ci`, `just lint clippy`); an unknown one
prints the valid set. No recipe and you need it twice → add one.

**`just ci` before committing**; `just test quick` (~17s) to iterate. **Don't build release
unless installing** (LTO is 5–10 min). Web frontend uses **bun** — see
`crates/crucible-web/web/AGENTS.md`.

**Keep the root clean.** Docs in `docs/Help|Meta|Guides/`, scripts in `scripts/`, examples in
`examples/`. Never put docs, temp files or logs in the root. `docs/` is a reference kiln
integration tests parse — use wikilinks and frontmatter tags. Patched crates in `vendor/`:
`NOTE(crucible):` comments, update `vendor/README.md`, add regression tests.

## Code Principles

- **Crate boundaries are for compilation, not organization.** Prefer fewer, larger crates; co-locate related state.
- **anyhow by default, thiserror at boundaries** — structured enums only where callers match on variants.
- **YAGNI.** Every `Option<T>` needs a `None` path; every error variant a distinct handler.
- **Enums over traits** unless 2+ implementations in different crates; `dyn` only for genuine runtime polymorphism. Two fair exemptions: a test double, and a crate-dependency firewall.
- **Required methods beat defaulted ones.** A trait requiring nothing cannot fail to compile when the contract grows. Shared behaviour goes in the caller, a blanket impl, or free functions — not defaults.
- **Compress via the type system**: `From`/`Into` over `.map_err()` chains, `?`, combinators, `#[derive]`. A pattern repeated 5+ times is a missing helper.
- **Comments explain why, not what.**
- **Lua sees the same domain model** — Rust types are source of truth; bindings project them.
- `snake_case` fns, `PascalCase` types, `snake_case.rs` modules. No module-level `#![allow(...)]`.
- **Name for what the code does.** `new()` simple; `new_with_*()` when one thing varies; `create_*()` for factories building external resources. `*Handler` reacts, `*Executor` executes, `*Config` loads at startup.

## Testing

**cargo-nextest** (process-per-test; `cargo test` on `crucible-daemon` is flaky from shared
in-process state). Profiles set retry/timeout only — scope with `-p` or `-E 'test(...)'`.
External-prerequisite tests are `#[ignore]`d with the reason naming the prerequisite.

- TDD: bugfixes start with a failing test; commit fix + test together. Name tests for the correct behaviour, not the bug.
- **Red-proof every gate.** A test written alongside its fix has never failed. Break the fix, watch it fail, restore. Gates that grep their own source text are the usual failure.
- Mock external deps (`#[cfg(feature = "test-utils")]`); `tempfile::TempDir`, never a hardcoded `/tmp`.
- **Never dismiss failures as "pre-existing" or "unrelated."** Assume your change broke them.
- A feature crossing a process or language boundary needs one test that crosses it.

**Hermeticity.** Never use raw `std::env::set_var` — it races. In-process: inject the data
root as a value (`Server::bind_with_data_home(...)`), or you load the developer's real
`~/.crucible` — passes CI, fails locally; provider fixtures also need the rustls
`install_default()` helper. Out-of-process: child-scoped env only
(`Command::env("CRUCIBLE_HOME", tempdir)`, see `TestDaemon`). `EnvVarGuard` is only for tests
that genuinely exercise env-reading.

**Snapshots.** A passing snapshot proves stability, not correctness. Never
`cargo insta accept --all`; read every changed `.snap` and check layout, exact Unicode glyphs,
ANSI colors, no duplicated or missing content. On failure, assume the implementation is wrong.

**TUI.** Unit tests on `OilChatApp` first; `insta` for visuals; PTY (`expectrl`) only for what
nothing else can verify — slow and flaky. Drive via `Vt100TestRuntime` or `AppHarness`;
fixtures in `assets/fixtures/*.jsonl`; mock agents via `impl_noop_agent!`/`CountingAgent`. **New
TUI features need a story in `docs/Meta/TUI User Stories.md` plus T1 + T2 coverage** in
`src/tui/oil/tests/user_story_tests/`.

## Before Submitting

Style followed · `just ci` passes · docs updated (architecture → `docs/Meta/`) · no debug code ·
conventional commits · bugfixes include regression tests · snapshots verified correct.

## Key Resources

- [README.md](./README.md) — overview and quick start
- [docs/Meta/Analysis/Systems.md](./docs/Meta/Analysis/Systems.md) — system boundaries by crate
- [justfile](./justfile) — development recipes
- [vendor/README.md](./vendor/README.md) — patched dependencies
- `docs/Help/Concepts/` — ACP, MCP and Agent Skills specification references

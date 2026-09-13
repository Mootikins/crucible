# AI Agent Guide for Crucible

> Instructions for AI agents working on Crucible. `CLAUDE.md` symlinks here.

**Crucible** is a knowledge-grounded agent runtime: notes, sessions and wikilinks form a
knowledge graph agents draw from and contribute to. Plaintext-first, Neovim-like (headless
daemon + RPC, Luau extensibility, TUI-first, plugin-driven).

## Architecture

| Crate | Purpose |
|-------|---------|
| `crucible-core` | Domain types, traits, parser, config |
| `crucible-cli` | TUI (`OilChatApp`) and its `:` command table, CLI commands; `cru web` behind default-on `web` feature |
| `crucible-daemon` | RPC server, sessions, ACP host, embeddings, SQLite, skills, tools |
| `crucible-web` | Axum server + SolidJS frontend (`web/`, embedded via rust-embed) |
| `crucible-oil` | Terminal rendering primitives |
| `crucible-lua` | Luau scripting: the VM, the module resolver, the `cru.*` projections |

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
| **Events & requests** | Fan-out with no reply; correlated one-reply-with-timeout | `crucible-daemon/src/event_emitter.rs`, `crucible-core/src/protocol/session_events/`, pending-reply registries |
| **Wire bindings** | Four, not one | daemon JSON-RPC; web HTTP/SSE/WS; ACP + MCP (both from external crates) |
| **Lua** | Projection *and* interception | `crucible-lua/`, `runtime/` |
| **Activation** | One body that runs a plugin's module once, binds what it exports and runs its spec entry's `config`; the spec-driven boot pass, a `require` from `init.lua`, an install and a reload all end there | `crucible-daemon/src/daemon_plugins/activate.rs`, `resolve.rs`; the spec store in `crucible-lua/src/plugin_spec_store.rs`; the fragment reader in `crucible-lua/src/lifecycle/fragment.rs` |
| **Render** | TUI and web presentation | `crucible-cli/src/tui/`, `crucible-oil/`, `crucible-web/web/` |

**Knowledge is four things.** The parser (`crucible-core/src/parser/`) is an island: raw text
plus byte spans, no resolution. Wikilink resolution, backlinks and rename splicing live in
`storage/sqlite/link_index.rs`, over types the parser never sees. `KilnName`/`KilnRegistry`
own identity. Embeddings serve retrieval as much as indexing. `NotePipeline` is the adapter
between them — a seam, not a subsystem. A change in one does not reach the others.

**Wire bindings share one type and nothing else** — `SessionEventMessage`. No shared codec,
framing, correlation or error classification. Two of the four do not own their wire types at
all: ACP's come from `agent_client_protocol`, MCP's from `rmcp`. (Neither is *vendored* —
`vendor/` holds one crate, `markdown-it`.)

**Lua is not only a shim.** Projection modules (theme, statusline, geometry, oil, json, fs,
notify, paths) are safe in isolation. Interception is not: `runtime/defaults/init.luau` is
compiled in as `BUILTIN_INIT_LUA` and is the *only* definition of the three permission modes,
the plan-mode deny hook and the precognition formatter. The default system prompt is NOT
there: it ships as `chat.system_prompt` from `ChatConfig::default()`, so it lands on the
config store's `Default` layer and `settings.json` can outrank it.
`ModeRegistry` has no Rust default and no fallback.

**The runtime is Luau, and `require` is the host's.** Luau ships no
`package.path`, no `package.searchers`, no `io` and no file half of `os`.
`crucible-lua/src/modules.rs` owns module lookup — public roots cache by name
in a `package.loaded` compatibility table, a plugin's own `lua/` directory is
private to it and cached by path — and `luau_compat.rs` provides `io`,
`os.getenv`, `os.tmpname`, `os.remove` and `os.rename`. Never reintroduce
`package.path`: lookup is import authority, so it belongs to the host.
`io.popen` and `os.execute` are there too. The host once held them back to
make `cru.shell` the one gated door to a process, and that reading was wrong:
`PluginShellPolicy::default()` blocks four command names with no allow-list,
and the check reads the command name and never the arguments, so
`cru.shell.exec("sh", { "-c", … })` runs anything. A plugin is code the
operator installed. `loadlib` and `os.exit` stay out, for reasons the module
doc of `luau_compat.rs` records.

**A declared type is checked, not decorated.** `signature.rs` reads a tool's
declared parameter types and renders them as JSON Schema, as Luau
declarations, and as messages. An unreadable declaration refuses the load
(`LifecycleError::InvalidDeclaration`); `host_api.rs` holds the host's own
`cru.*` signatures, and every one of them must name a function the running VM
has.

**Adding a name to a closed set → one enumerated table with a real gate.** `tools/surface.rs`
is the exemplar: exhaustive match, two module-level clippy denies (both needed — with one, a
variant with `_ => Daemon` passed review), no `Default` on the return type, and a test
deriving its expectation from the running system rather than from source text. Prefer this to
a hand-maintained list checked by a source-text grep — five such greps have now been replaced,
each of which was satisfiable without adding the entry it was meant to require. The fifth cost
two red CI runs first: `every_rpc_session_knob_is_reachable_from_the_*` read
`"session\.set_([a-z0-9_]+)"` over the whole text of `dispatch.rs`, which names each method in
the `rpc_methods!` table, in the setter router and in `#[cfg(test)] mod tests`, so a stale
literal in a test body advertised a deleted knob. Both gates now read `SessionKnob::ALL`
through `rpc::rpc_set_method` (`rpc/knob_method.rs`).

The live tables: `BuiltinTool` (`crucible-daemon/src/tools/surface.rs`) and `ToolSurface`
(`crucible-core/src/traits/tools.rs`) · `EventName` + `StageId`
(`crucible-lua/src/handlers/hook_name.rs`) · `RpcMethod` + `METHODS`, both generated from one
`rpc_methods!` table (`rpc/dispatch.rs`), which `rpc/knob_method.rs` maps `SessionKnob` onto ·
`ScriptingEvent`
(`crucible-core/src/events/session_event/`), the ten names the scripting and transport
vocabularies share. Completeness of each `ALL` array is proved by walking `strum::EnumIter`,
which is what the compiler knows.

### Terminology — never interchangeable

The full glossary is [docs/Meta/CONTEXT.md](./docs/Meta/CONTEXT.md) (`CONTEXT.md` symlinks there). Add a term there when a design names a new concept.

- **Project** — where work output goes. Registered directory (git root or invocation dir). `.crucible/project.toml`.
- **Kiln** — where knowledge goes. `.crucible/kiln.toml`. A session *attaches* kilns (flat set, no primary); it is not *stored* in one — transcripts live under the daemon data root regardless.
- **Workspace** — an instance of a project directory (root, or a worktree). Runtime concept, no config file. Do NOT rename correct existing uses (`session.workspace`, `WorkspaceTools`, Lua `paths.workspace()`).
- **Review vs proposal** — a *review* disposes the agent's file edits (the composed diff in `review/`); a *proposal* disposes a suggested knowledge note (`KILN/.crucible/proposals/`, `cru proposals`). Never use one for the other.
- **Spec, spec entry, fragment, discovery, activation, source, intercept grant** — the plugin words. The *spec* is the operator's list of plugins (`cru.plugin.setup` in `init.lua`); a *fragment* is a partial entry a plugin ships as `spec.luau` or that the shipped defaults provide; *discovery* reads fragments and runs no plugin code; *activation* runs the module once and calls the entry's `config`. CONTEXT.md defines each and names the words to avoid (manifest, load, plugin config).

### Type ownership

Parser types are canonical in `crucible-core/src/parser/types/`; `BlockHash` is the one
content hash; `ContextMessage` is the conversation message type. **Never duplicate types
between crates** — one canonical location, then re-export. Result aliases follow
`<Domain>Result<T>`.

### Session-scoped vs TUI-local

Multi-client state (model, context budget, mode) lives in the daemon's `SessionAgent`
and syncs via RPC; pure display state (theme, show_thinking) stays in `OilChatApp`.
Session-scoped needs the full chain: `AgentHandle` → `DaemonAgentHandle` → `ChatAppMsg` →
`chat_runner` handler → TUI command. TUI-only breaks multi-client, and mismatched JSON field
names fail silently — verify `session.get_*` returns what `session.set_*` stored and survives
resume. (The knobs live in `SessionKnobs`, a supertrait of `AgentHandle`, and every
knob is required: a handle that omits one does not compile. A test double that uses no
knob writes `crucible_core::impl_unsupported_session_knobs!(Ty)`.)

### Hooks and ACP

- `crucible.on(name, opts, handler)` takes a **`StageId`** (17 synchronous stages) or an **`EventName`** (10 daemon broadcast events); the two are different contracts and now different types. At a stage the return value decides what happens next; at an event nothing downstream reads it, and only `cancel` (stop the remaining handlers) means anything. `cru.on` registers 13 of the 17 stages: it REFUSES `permission:request`, `session:start`, `session:end` and `provider:auth`, because each carries a payload that is not `(ctx, event)` and each has its own registration API (`HookName::own_api` names it).
- **Every `cru.*` callback lands in one store**, `LuaScriptHandlerRegistry`, tagged with an **`Owner`** — `Plugin(name)`, `UserLua`, `Builtin` or `Eval`. The enum is total, so no registration sits outside every group; `None` used to mean "the operator" at three separate readers. `clear_owner(lua, registry, &owner)` drops exactly one owner's registrations and its schedules, which is what makes `make_plugin_inert`'s doc comment true.
- `crucible.on("pre_tool_call", opts, handler)` → `{ cancel = true }` blocks, `{ handled = true, result = … }` replaces execution, `nil` observes. **`cancel` is safe; `handled` and transform are capability-grade** — `handled` returns *before* the permission gate. To use either, a plugin sets `intercepts_tools = true` in its fragment (`spec.luau`, read by `crucible-lua/src/lifecycle/fragment.rs` in a read-only environment that runs no plugin code); serde renames the manifest field to `intercept_tools`. The same flag in the module table `init.luau` returns grants nothing. There is no manifest file and no capability enum: the host builds `PluginManifest` from the plugin directory and its fragment. The host refuses a plugin without the declaration, logs the refusal, then dispatches the call normally (`messaging/tool_call.rs`). Gate ordering in that file is the second defence, not the first. Only `runtime/plugins/oci/` declares it, because taking the call over *is* the sandbox. Preserve both the declaration check and the ordering.
- **`LuaSource` does NOT decide interception; `may_take_a_tool_call_over` in `messaging/tool_call.rs` does.** The type answers who defined a registration and nothing more — it carried a `may_intercept` method, which made a provenance tag grant a capability. One function at that seam decides, with four explicit arms and no wildcard, and **the partition is by trust root, not identity**:
  - **A plugin needs the declaration.** Third-party code, so `intercepts_tools` is its boundary. The loader records it by name (`record_plugin_intercept`, VM app data Lua cannot reach); an unrecorded name answers `false`, so a plugin the loader never admitted gains nothing by being unknown.
  - **The operator's own two sources are EXEMPT, and this is deliberate.** `UserLua` (the user's `init.lua`) and `Builtin` (`runtime/defaults/init.luau`) may intercept with no declaration, and there is no fragment for either to declare in. `handled` returns before the permission gate, but that gate protects the *user from the agent* — it was never a boundary between the user and their own configuration. Withholding the power buys no containment: `init.lua` already runs arbitrary Lua and reaches `cru.shell`, whose default policy blocks four command names and never reads arguments, so `cru.shell.exec("sh", { "-c", … })` runs anything. And `Builtin` is the only definition of the permission modes and the plan-mode deny hook, so refusing interception to the code that *defines* the gate is incoherent. This is the same mistake as holding back `io.popen` to make `cru.shell` the one gated door — recorded above, and already reversed once. Do not re-derive it.
  - **An eval is NOT the operator.** `Eval` answers `false`. A human types `cru lua`, which makes this the arm most likely to be widened on that ground, and the reading is wrong: an eval is a socket call, so treating it as the operator lets any local caller that can open the daemon socket intercept a session it merely *names*. `cancel` stays open to every source, because refusing a call can only narrow.

  Gating `init.lua` interception later is a deliberate feature with a config knob, not a refactor side effect.
- ACP delegation: `cru chat --acp claude`, `cru session create --acp claude`, or `delegate_session`. For `cru chat`, `--agent` is an alias of `--acp`. For `cru session create`, `--agent` names an agent *card*. Limits in `[acp.agents.*]`. Code: `acp/`, `agent_manager/`, `tools/mcp_server.rs`.

## Workflow

**Use `just` recipes over raw cargo/bunx/vitest/playwright** — they encode this box's
constraints. Recipes take a sub-target (`just test ci`, `just lint clippy`); an unknown one
prints the valid set. No recipe and you need it twice → add one.

**`just ci` before committing** (~250s, and it is the only tier that lints);
`just test quick` (40-80s) to iterate. **Don't build release
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

**A feature is done when a user can reach it and read about it.** Four questions, each of
which has shipped broken at least once because nothing asked it:

- **Where does a user meet it?** TUI *and* web, unless it truly belongs to neither — then say
  which and why in the commit. They are separate render layers and shipping one is not
  shipping the other. A daemon capability nothing surfaces is invisible.
- **Which doc already describes the old behaviour?** Grep for it and edit *that*, rather than
  adding a note beside it. `docs/Help/` is what a user does, `docs/Meta/` is architecture.
- **What did it change out from under the front ends?** A new id set or a widened type reaches
  every renderer. Test it where it is *drawn*, not only where it is produced — a front end fed
  unfamiliar data is where this breaks, and the producing side's tests all still pass.
- **Which defaults still name the old thing?** Fallback lists, initial signals, seed constants.
  These rarely fail a test, because a fallback only runs before the real answer arrives.

## Key Resources

- [README.md](./README.md) — overview and quick start
- [docs/Meta/Architecture/Index.md](./docs/Meta/Architecture/Index.md) — expected vs actual architecture, gaps, consolidation plan
- [docs/Meta/Analysis/Systems.md](./docs/Meta/Analysis/Systems.md) — system boundaries by crate
- [justfile](./justfile) — development recipes
- [vendor/README.md](./vendor/README.md) — patched dependencies
- `docs/Help/Concepts/` — ACP, MCP and Agent Skills specification references

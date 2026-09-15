# Working on Crucible

Crucible is a plaintext-first, knowledge-grounded agent runtime: headless daemon,
RPC clients, Luau extensions, TUI-first. Prefer fewer moving parts and explicit
ownership. `CLAUDE.md` symlinks here.

Read [Product](docs/Meta/Product.md) for behavior and proof,
[Architecture](docs/Meta/Architecture/Index.md) for ownership, and
[CONTEXT](docs/Meta/CONTEXT.md) for terminology. Update existing docs when behavior changes.

## Ownership

| Owner | Responsibility |
|---|---|
| `crucible-core` | Canonical domain types, config, parser |
| `crucible-daemon` | Sessions, admission, tools, storage, retrieval, review, plugin lifecycle |
| `crucible-cli` / `crucible-web` | Input, presentation, client-local state; web is Axum + SolidJS |
| `crucible-oil` | Terminal rendering primitives |
| `crucible-lua` / `runtime/` | Luau host, bindings, plugin behavior and defaults |

One `cru` binary; `DaemonClient::connect_or_start()` starts the daemon.
Use the per-user 0700 socket directory, never a shared unauthenticated socket.

- Business logic and authoritative storage belong in the daemon. Clients send
  intent; they must not construct a second agent configuration or write pipeline.
- A **project** names where work goes; a **workspace** is its runtime directory
  instance. A **kiln** holds knowledge. Sessions attach a flat set of kilns;
  transcripts live under the daemon data root, not in a kiln.
- Multi-client state (model, mode, context budget) belongs to the session.
  Display state (theme, show-thinking) stays in the client. Wire session knobs
  through `SessionKnobs`, the daemon handle and both frontends; test set/get/resume.
- Knowledge has separate owners: parser = text and byte spans; SQLite link index =
  resolution/backlinks/rename; `KilnName`/`KilnRegistry` = identity; embeddings =
  retrieval/indexing. `NotePipeline` connects them; it does not merge them.
- JSON-RPC, web transports, ACP and MCP share `SessionEventMessage`, not codecs,
  correlation or error policy. ACP/MCP wire types belong to their external crates.
- Session-scoped runtime state belongs in `SessionSlot`. Events broadcast;
  requests need a correlated reply, timeout and cleanup. Publish job state before
  announcing it; wake collectors on completion rather than polling.

## Boundaries to preserve

- Scope/admission: `agent_manager/scope.rs`, `tools/{containment,surface}.rs`,
  `execution_roots.rs`. Creation, resume, delegation and fork must honor current
  kiln trust and isolation. Copying configuration is not admission; an absent
  isolation claim is not proof that a session never required one.
- Turn lifecycle: `agent_manager/messaging/`. Injected context is not a user turn;
  preserve its role and provenance through live input, replay, undo and fork.
- Note writes and review are daemon-owned. Agent edits and plugin note writes use
  the same disposition. Rejection must distinguish an absent file from an empty
  one; shared write locks coordinate participating daemon writers, not outside editors.
- Keep parser types canonical in `crucible-core/src/parser/types/`, `BlockHash`
  as the content hash and `ContextMessage` as the conversation message. Re-export
  types instead of duplicating them across crates or in Lua.

## Luau and plugins

- The daemon owns one shared plugin VM; session-local state lives in scopes
  and `SessionSlot`, not per-session VMs.
- Host-owned `require` lives in `modules.rs`; never add `package.path`.
  Public roots cache by name, private plugin modules by path. `luau_compat.rs`
  supplies compatibility APIs; installed plugins are operator code, not a sandbox.
- **Discovery** reads fragments without running plugin code. **Activation** runs
  the module once and its spec entry's config. Boot, require, install and reload
  converge on `daemon_plugins/activate.rs`; do not add another activation path.
- Every callback has an `Owner` in `LuaScriptHandlerRegistry`; owner cleanup
  removes callbacks and schedules. Keep synchronous `StageId` hooks separate
  from broadcast `EventName` observers. Lifecycle, permission and auth hooks
  use their dedicated registration APIs.
- Tool takeover is decided by `may_take_a_tool_call_over`, not `LuaSource`:
  plugins need `intercepts_tools = true` in their fragment; `UserLua` and
  `Builtin` are deliberately exempt; `Eval` cannot take calls over. Cancel is open
  to all sources. Preserve both the declaration check and permission-gate ordering.
- `runtime/defaults/init.luau` defines permission modes, plan denial and
  precognition formatting; permission behavior has no Rust fallback. The default system
  prompt is `ChatConfig::default().system_prompt` on the config store's Default layer.
- Declared tool types must parse or activation fails. `signature.rs` projects
  them; `host_api.rs` must name functions the running VM actually provides.
- `cru chat --agent` aliases `--acp`; `cru session create --agent` selects a
  card. Do not conflate cards and ACP profiles.

## Design

- Crates are compilation boundaries, not folders. Prefer fewer, larger crates.
- Use enums unless traits have real cross-crate implementations, a test double
  or a dependency-firewall purpose. Required methods beat silent trait defaults.
- Use `anyhow` internally, `thiserror` where callers match variants.
  Every option needs a meaningful absent path; every error variant a distinct handler.
- Closed sets need one exhaustive table and a compiler/runtime completeness gate
  (`EnumIter`, required methods, exhaustive matches), not source-text greps.
- Prefer derives, conversions, `?` and small shared helpers over repeated plumbing.
  Comments explain why. Name code for its actual role; no module-level lint allows.

## Workflow and tests

- Use `just` recipes: `just test quick` to iterate, `just ci` before committing.
  Only full CI includes linting. Scope nextest with `-p` / `-E`; use `--lib`
  for focused unit runs. Do not build release unless installing. Web uses **bun**.
- Read narrower `AGENTS.md` files. Keep docs in `docs/Help|Meta|Guides/`,
  scripts in `scripts/`, examples in `examples/`; no root-level scratch files.
  The docs kiln is test input: retain frontmatter tags and valid wikilinks.
- Bugfixes start red. **Break every new gate, observe failure, restore it.**
  When behavior crosses a process/language boundary, test that actual crossing.
  Do not dismiss failures as unrelated or pre-existing.
- Use nextest's process isolation, injected data roots, `TempDir` and mocked
  providers. Never raw `std::env::set_var`; child processes get scoped env.
  `EnvVarGuard` is only for env-reading tests. Provider fixtures install rustls.
  Ignore external-prerequisite tests explicitly, naming the prerequisite.
- TUI: `OilChatApp` units and `AppHarness`/`Vt100TestRuntime` first; PTY only
  where necessary. New behavior needs a user story plus T1/T2 coverage.
  Inspect every changed snapshot's layout, Unicode and colors; never bulk-accept.
- A feature must be reachable in TUI **and** web, or explain why it belongs to
  neither. Update the doc describing the old behavior, check frontend defaults,
  and test widened data where it is rendered.
- Conventional commits; fix and regression together. Vendor changes need
  `NOTE(crucible):`, regression coverage and `vendor/README.md` updates.

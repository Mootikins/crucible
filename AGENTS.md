# Working on Crucible

Crucible is a plaintext-first, knowledge-grounded agent runtime. It has a
headless daemon, RPC clients, Luau extensions and a TUI-first interface.
A local, gitignored `CLAUDE.md` symlinks here.

[Product](docs/Meta/Product.md) gives the behavior and its proof.
[Architecture](docs/Meta/Architecture/Index.md) gives one page for each
subsystem: its owners, flows, boundaries and extension seams.
[CONTEXT](docs/Meta/CONTEXT.md) defines the terms (project, workspace, kiln,
discovery, activation).

## How a change works

Many agents change this code in parallel. The same concept then gets a second
implementation, and two clients drift apart. Each step below prevents one
cause of that. Do them for every change, also a small one.

1. **Find the owner.** Before you write a type, a function, a list or a
   handler, search for the concept. Search the Rust crates, the web frontend
   (`crates/crucible-web/web/src`) and `runtime/`. Search for the likely
   names, the RPC method, the event name and the config key. If
   `graphify-out/` exists, `graphify explain "<Symbol>"` shows callers.
2. **Read the page of the owner.** The architecture index names the page.
   The page tells you the flow you are about to change.
3. **Extend the owner.** Do not add a parallel path, a wrapper or a copy
   for your caller. If the owner does not fit, change the owner.
4. **Remove what you replace.** Delete the old path in the same change.
   Keep a shim only when stored data or an external wire needs it. Name
   that need in a comment.
5. **Remove a duplicate that you find.** Merge it in the same change, or
   write the reason for the delay in the commit message.
6. **Keep a behavior in the daemon.** A client sends intent and renders the
   result. If the TUI and the web client both need a decision, the daemon
   makes it once.
7. **Update the docs.** Change the architecture page and the Help note that
   describe the old behavior.

A change that adds a second way to do one thing is not finished. This is also
true when the change is small.

## Where a change goes

Start at the owner. The architecture page lists the rest of the path.

| To add | Start at | Page |
|---|---|---|
| A builtin tool | `crates/crucible-daemon/src/tools/surface.rs`, then its executor | [Tools and Admission](<docs/Meta/Architecture/Tools and Admission.md>) |
| An RPC method | `RpcMethod` in `crates/crucible-core/src/protocol/rpc/method.rs`, then its arm in `crates/crucible-daemon/src/rpc/dispatch.rs` | [Daemon Server](<docs/Meta/Architecture/Daemon Server.md>) |
| A request or reply type | `crates/crucible-core/src/protocol/requests/` | [RPC Client](<docs/Meta/Architecture/RPC Client.md>) |
| A session event | `SessionEventPayload` in `crates/crucible-core/src/protocol/session_events/` | [Core Domain Types](<docs/Meta/Architecture/Core Domain Types.md>) |
| A session setting | `SessionKnob` in `crates/crucible-core/src/types/knob.rs`, then the cross-layer checklist below | [Session Services](<docs/Meta/Architecture/Session Services.md>) |
| A config key | `crates/crucible-core/src/config/components/` | [Core Config](<docs/Meta/Architecture/Core Config.md>) |
| A provider | `crates/crucible-core/src/config/components/backend.rs`, then the daemon factory | [Providers and LLM](<docs/Meta/Architecture/Providers and LLM.md>) |
| A Lua hook | `crates/crucible-lua/src/handlers/hook_name.rs`, then the call site | [Luau Host](<docs/Meta/Architecture/Luau Host.md>) |
| A `cru.*` function | its binding in `crates/crucible-lua/src/`, and its declaration | [Luau APIs](<docs/Meta/Architecture/Luau APIs.md>) |
| Storage | the core storage traits, then `crates/crucible-daemon/src/storage/` | [Knowledge Storage and Retrieval](<docs/Meta/Architecture/Knowledge Storage and Retrieval.md>) |
| A web route | `crates/crucible-web/src/routes/` | [Web Server](<docs/Meta/Architecture/Web Server.md>) |
| A TUI component | `crates/crucible-cli/src/tui/oil/components/` | [TUI Components](<docs/Meta/Architecture/TUI Components.md>) |

A feature must be reachable in the TUI **and** the web client. If it belongs
in neither, write the reason in the change.

## Ownership

| Owner | Responsibility |
|---|---|
| `crucible-core` | Canonical domain types, wire types, config, parser |
| `crucible-daemon` | Sessions, admission, tools, storage, retrieval, review, plugin lifecycle |
| `crucible-cli` / `crucible-web` | Input, presentation, client-local state |
| `crucible-oil` | Terminal rendering primitives |
| `crucible-lua` / `runtime/` | Luau host, bindings, plugin behavior and defaults |

- The daemon owns business logic and authoritative storage. A client must not
  make a second agent configuration or a second write pipeline.
- The session owns state that all clients share: model, mode and context
  budget. The client owns display state: theme and show-thinking. Wire a
  session setting through `SessionKnobs`, the daemon handle and both clients.
  Test set, get and resume.
- See the cross-layer checklist below for the files that one session
  setting touches.
- One `cru` binary exists. `DaemonClient::connect_or_start()` starts the
  daemon. Use the per-user 0700 socket directory. Never use a shared socket
  without authentication.
- Knowledge has separate owners. The parser owns text and byte spans. The
  SQLite link index owns resolution, backlinks and rename. `KilnName` and
  `KilnRegistry` own identity. Embeddings own retrieval and indexing.
  `NotePipeline` connects them. It does not merge them.
- JSON-RPC, web transports, ACP and MCP share `SessionEventMessage`. They do
  not share codecs, correlation or error policy. ACP and MCP wire types
  belong to their external crates.
- Session-scoped runtime state belongs in `SessionSlot` and in Lua session
  scopes, not in a VM for each session. An event is a broadcast. A request
  needs a correlated reply, a timeout and cleanup. Publish job state before
  you announce it. Wake a collector on completion. Do not poll.

## Cross-layer checklist

Use this checklist for a setting that changes agent or session behavior. A
setting that only changes the display (theme, show-thinking, verbose) stays
in the client. It needs no RPC. Before you start, decide the scope: if two
clients on one session must agree, the setting belongs to the session.

**Before you add a setting**
- [ ] Look for an existing `SessionKnob` variant or RPC method with the same job.
- [ ] Read [Session Services](<docs/Meta/Architecture/Session Services.md>).

**Core**
- [ ] Add the variant to `SessionKnob` in `crates/crucible-core/src/types/knob.rs`.
- [ ] Add the field to the session record in `crates/crucible-core/src/session/types/session.rs`.
- [ ] Add the field to the settings event in `crates/crucible-core/src/protocol/session_events/settings.rs`.

**Daemon**
- [ ] Add the getter and setter to `SessionKnobs` in `crates/crucible-daemon/src/agent_manager/handle.rs`.
- [ ] Implement them in `GenaiAgentHandle` and `AcpAgentHandle`.
- [ ] Forward them in the `Box<dyn AgentHandle>` implementation in the same file.
- [ ] Add the setter to `crates/crucible-daemon/src/agent_manager/models.rs`.
- [ ] Add the RPC arm to `crates/crucible-daemon/src/rpc/dispatch.rs`.
- [ ] Add the params to `crates/crucible-daemon/src/server/session/params.rs`.
- [ ] Add the client method to `crates/crucible-daemon/src/rpc_client/client/agent.rs`.

**TUI**
- [ ] Add the `:set` key to `crates/crucible-cli/src/tui/oil/commands/set.rs`.
- [ ] Handle the message in `crates/crucible-cli/src/tui/oil/chat_app/command_handling.rs`.
- [ ] Call the `DaemonClient` method in `crates/crucible-cli/src/tui/oil/chat_runner/actions.rs`.

**Web**
- [ ] Add the route to `crates/crucible-web/src/routes/session_config/`.
- [ ] Forward it in `crates/crucible-web/src/services/daemon.rs`.
- [ ] Run `just web-contract` to regenerate the API schema.
- [ ] Use it in `crates/crucible-web/web/src/lib/query/routes/session.ts`.

**Proof**
- [ ] The RPC field names are the same in the client and the server.
- [ ] A test proves set, get and resume.
- [ ] The knob RPC matrix in `crates/crucible-cli/src/tui/oil/chat_runner/tests/knob_rpc.rs` covers the new knob.
- [ ] The architecture tests in `crates/crucible-cli/tests/architecture_tests.rs` find the knob in the TUI and the web client.

The TUI calls the daemon directly. It holds no agent handle and no copy
of the session state.

## Rules that no compiler checks

These rules guard behavior that a type cannot express. Keep each one when you
change the code near it.

- **Admission.** Creation, resume, delegation and fork must honor the current
  kiln trust and isolation. A copied configuration is not admission. An
  absent isolation claim does not prove that a session never needed one. The
  owners are `agent_manager/scope.rs`, `tools/containment.rs`,
  `tools/surface.rs` and `execution_roots.rs`.
- **Turns.** `agent_manager/messaging/` owns the turn lifecycle. Injected
  context is not a user turn. Keep its role and provenance through live
  input, replay, undo and fork.
- **Note writes.** Agent edits and plugin note writes use the same
  review disposition, and the daemon owns it. A rejection must tell an absent
  file from an empty file. Shared write locks coordinate the daemon writers
  only, not outside editors.
- **Canonical types.** Parser types live in `crucible-core/src/parser/types/`.
  `BlockHash` is the content hash. `ContextMessage` is the conversation
  message. Re-export a type. Do not copy it into another crate or into Lua.
- **Plugins.** The daemon owns one shared plugin VM. Installed plugins are
  operator code, not a sandbox. Host-owned `require` lives in `modules.rs`.
  Never add `package.path`.
- **Activation.** Discovery runs no plugin code. Boot, require, install and
  reload all use `daemon_plugins/activate.rs`. Do not add another activation
  path.
- **Callbacks.** Each callback in `LuaScriptHandlerRegistry` records its
  `LuaSource`. `handlers::clear_source` removes the callbacks, schedules and
  tasks of that source. Keep synchronous `StageId` hooks apart from broadcast
  `EventName` observers. Permission, session start and end, and provider-auth
  hooks have their own registration APIs.
- **Tool takeover.** `may_take_a_tool_call_over` decides it, not a method on
  `LuaSource`. A plugin needs `intercepts_tools = true` in its fragment.
  `UserLua` and `Builtin` are exempt by design. `Eval` cannot take a call
  over. All sources can cancel. Keep the declaration check before the
  permission gate.
- **Permission defaults.** `runtime/defaults/init.luau` defines the
  permission modes, the plan denial and the precognition format. Permission
  behavior has no Rust fallback. The default system prompt is
  `ChatConfig::default().system_prompt` on the Default layer of the config
  store.
- **Declarations.** A declared tool type must parse, or activation fails.
  `signature.rs` projects the types. `host_api.rs` must name only functions
  that the running VM provides.
- **Cards and profiles.** `cru chat --agent` is an alias of `--acp` and
  selects an ACP profile. `cru session create --agent` selects an agent card.
  Do not mix the two.

## Design

- A crate is a compilation boundary, not a folder. Prefer fewer, larger
  crates.
- Use an enum. Use a trait only for real implementations in two crates, a
  test double, or a dependency firewall. A required method is better than a
  silent default.
- Use `anyhow` inside a crate. Use `thiserror` where a caller matches the
  variants. Each option needs a meaningful absent path. Each error variant
  needs its own handler.
- A closed set needs one exhaustive table. Let the compiler or a runtime
  check prove it complete (`EnumIter`, required methods, exhaustive
  matches). Do not use a source-text grep as the check.
- Prefer derives, conversions, `?` and small shared helpers to repeated
  plumbing. A comment explains why. Name code for its actual role. Do not
  add a module-level lint allow.

## Tests and commits

- To iterate, run `just test quick`. Before a commit, run `just ci`. Only
  `just ci` runs the linters. Scope nextest with `-p` or `-E`. Use `--lib`
  for a focused unit run. Build release only to install.
- A bugfix starts with a failing test. **Break each new gate, see it fail,
  then restore it.** When behavior crosses a process or language boundary,
  test that crossing.
- Do not call a failure unrelated or pre-existing. Find its cause.
- Use nextest process isolation, injected data roots, `TempDir` and mocked
  providers. Never call `std::env::set_var` directly. Give a child process
  a scoped environment. Use `EnvVarGuard` only in a test that reads the
  environment. Provider fixtures install rustls. Mark a test that needs an
  external prerequisite as ignored, and name the prerequisite.
- Before you add a test server, a harness or a mock, look for the shared
  one. Shared test code lives in `tests/common/` and `src/test_support` of
  each crate. Extend it. Do not copy it.
- TUI: test `OilChatApp` units and `AppHarness` or `Vt100TestRuntime` first.
  Use a PTY only where you must. New behavior needs a user story and T1 and
  T2 coverage ([TUI User Stories](<docs/Meta/TUI User Stories.md>)).
  Examine the layout, Unicode and colors of each changed snapshot. Never
  accept snapshots in bulk.
- When a change widens data, test it where the data is rendered. Check the
  defaults of both clients.
- Read the narrower `AGENTS.md` files in the crates you change. Put new docs
  in `docs/Help/`, `docs/Meta/` or `docs/Guides/`. Put scripts in `scripts/`
  and examples in `examples/`. Do not put scratch files at the root. The
  docs kiln is test input: keep the frontmatter tags and valid wikilinks.
- Use conventional commits. Put a fix and its regression test in one
  commit. A vendor change needs a `NOTE(crucible):` marker, regression
  coverage and an update to `vendor/README.md`.

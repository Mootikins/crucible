# Working on Crucible

Crucible is a plaintext-first, knowledge-grounded agent runtime. It has a
headless daemon, two clients (the TUI and the web), and Luau plugins.
A local, gitignored `CLAUDE.md` symlinks here.

[Product](docs/Meta/Product.md) gives the behavior and its proof.
[Architecture](docs/Meta/Architecture/Index.md) gives one page for each
subsystem. [CONTEXT](docs/Meta/CONTEXT.md) defines the terms (project,
workspace, kiln, discovery, activation).

## The rule

Many agents change this code in parallel. The usual failure is a second
implementation of one concept, and then two clients that disagree. So:

1. **Find the owner first.** Search the Rust crates, the web frontend
   (`crates/crucible-web/web/src`) and `runtime/` for the concept, its RPC
   method, its event and its config key. Read the architecture page of the
   owner.
2. **Extend the owner.** Do not add a wrapper, a copy or a parallel path. If
   the owner does not fit, change the owner.
3. **Delete what you replace, in the same change.** Keep a shim only for
   stored data or an external wire, and name that need in a comment.
4. **Put behavior in the daemon.** A client sends intent and renders the
   result. If the TUI and the web both need a decision, the daemon makes it.
   A feature must work in both clients, or the change says why not.

## The core abstractions

Each abstraction below has one definition. Everything else is generated from
it or reads it.

**Core types are the one source of truth.** Wire and domain types live in
`crucible-core`. The TS types and the Luau declarations are generated from
them. Never copy a core type into another crate, into TS or into Luau.

**One table of RPC methods.** `rpc_methods!` in
`crates/crucible-core/src/protocol/rpc/method.rs` has one row per method:

```rust
SessionGet = read "session.get": Scoped<()> => SessionDetail,
```

The row gives the variant, `read` (safe to repeat) or `write`, the wire name,
the params type and the reply type. From the rows come `RpcMethod`, the typed
client method `DaemonClient::rpc_session_get` (which retries a `read` row),
the schema document and the TS map `RpcMethods`. To add a method: add the row,
add its arm in `crates/crucible-daemon/src/rpc/dispatch.rs`, and run
`just web-contract`. A hand-written `DaemonClient` method is allowed only
when it adds behavior, and its comment names that behavior.

**One web route for RPC.** The browser calls
`rpc('session.get', params)` (`web/src/lib/api-client.ts`), which posts to
`POST /api/rpc/{method}`. `browser_may_call` in
`crates/crucible-web/src/routes/rpc.rs` is the allow list: one exhaustive
match. `plugin_may_call` narrows a plugin caller. Add a separate web route
only for behavior that the web alone has (auth, raw bytes, streams).

**One event vocabulary.** `SessionEventPayload` in
`crates/crucible-core/src/protocol/session_events/` names every session
event. The web gets all topics on one SSE stream, `GET /api/events`. An event
is a broadcast. A request needs a correlated reply, a timeout and cleanup.

**One transcript fold.** `crucible-core/src/transcript` turns events into a
transcript. The daemon folds. The clients render the result.

**One knob per session setting.** `SessionKnob` and `KnobValue` in
`crates/crucible-core/src/types/knob.rs`, with `session.knob.set` and
`session.knob.get`. A new setting is a variant, a daemon apply arm and,
if wanted, a client control. It needs no new method or route. Display-only
state (theme, show-thinking) stays in the client.

**One plugin VM.** The daemon owns one shared Luau VM. Installed plugins are
operator code, not a sandbox. `daemon_plugins/activate.rs` is the only
activation path. Discovery runs no plugin code. A plugin reads its settings
through `cru.settings`.

## Rules that no compiler checks

- **Admission.** Creation, resume, delegation and fork honor the current
  kiln trust and isolation. A copied configuration is not admission. Owners:
  `agent_manager/scope.rs`, `tools/containment.rs`, `tools/surface.rs`,
  `execution_roots.rs`.
- **Turns.** `agent_manager/messaging/` owns the turn lifecycle. Injected
  context is not a user turn. Keep its role and provenance through replay,
  undo and fork.
- **Note writes.** Agent edits and plugin writes use one review disposition,
  which the daemon owns. A rejection tells an absent file from an empty one.
- **Knowledge.** The parser owns text and spans. The SQLite link index owns
  resolution, backlinks and rename. `KilnName` owns identity. Embeddings own
  retrieval. `NotePipeline` connects them and does not merge them.
- **Callbacks.** Each Lua callback records its `LuaSource`, and
  `handlers::clear_source` removes them. `may_take_a_tool_call_over` decides
  tool takeover; a plugin needs `intercepts_tools = true`.
- **Permissions.** `runtime/defaults/init.luau` defines the permission
  modes. Permission behavior has no Rust fallback.
- **Sockets.** One `cru` binary. Use the per-user 0700 socket directory.
- **Cards and profiles.** `cru chat --agent` selects an ACP profile.
  `cru session create --agent` selects an agent card. Do not mix them.

## Design

- Prefer fewer, larger crates. A crate is a compilation boundary.
- Use an enum. Use a trait only for implementations in two crates, a test
  double or a dependency firewall.
- A closed set has one exhaustive table that the compiler checks. A source
  grep is not a check.
- Use `anyhow` inside a crate and `thiserror` where a caller matches
  variants. Give each option a meaningful absent path.
- A comment explains why. Do not write a comment about deleted code; git
  keeps the history.

## Tests and commits

- Iterate with `just test quick`. Run `just ci` before a commit. Under
  parallel agents, run `flock /tmp/crucible-ci.lock just ci`.
- A bugfix starts with a failing test. Break each new gate, see it fail,
  then restore it. Test each process or language boundary that a change
  crosses.
- Do not call a failure flaky, unrelated or pre-existing. Find its cause.
- Use `TempDir`, injected data roots and mocked providers. Never call
  `std::env::set_var`. Use the shared helpers in `tests/common/` and
  `src/test_support`; extend them, do not copy them.
- TUI: test `OilChatApp` and `AppHarness` first. Read each changed snapshot.
  Never accept snapshots in bulk.
- The docs kiln is test input. After a docs change, run
  `git add docs && cargo nextest run -p crucible-core --test dev_kiln --run-ignored all`.
- Update the architecture page and the Help note that describe a behavior
  you change. Read the narrower `AGENTS.md` of each crate you change.
- Use conventional commits. Put a fix and its test in one commit. A vendor
  change needs a `NOTE(crucible):` marker and an entry in `vendor/README.md`.

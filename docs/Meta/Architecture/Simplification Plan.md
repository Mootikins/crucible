---
title: Simplification Plan
description: Ordered steps that delete duplicate layers and copies, so that each concept has one obvious place
tags: [meta, architecture, maintenance]
status: proposal
as_of: 582c5e6c1
---

# Simplification Plan

This is a proposal, not a record of shipped work. It describes the code at
`582c5e6c1`. Each step deletes a layer or a copy. No step adds a gate, a
ratchet or a list that a person must keep current.

## Why simplify, and why not gate

Duplicates grow where a concept has more than one plausible home. An agent
that changes the code copies the nearest pattern. When a concept passes
through five layers, the agent adds a sixth copy in one of them. A gate for a
concept needs complete and current docs, and nothing keeps them current.

This plan removes homes instead. When a concept has one home, the search in
step 1 of the repository agent guide finds it, and the agent extends it. The
compiler enforces a few results for free: when a crate stops using a module,
the module can become private, and a second copy then cannot compile.

The evidence for each step comes from the as-built pages, mainly their
Findings sections. The counts below were measured at `582c5e6c1`.

## Order of the work

The order puts first the steps that delete the most layers on the paths that
change most often. Do one step at a time. Do not run two agents on one step
at the same time. Each step leaves the tree working.

| Step | Deletes | Size | Depends on |
|---|---|---|---|
| 1. Remove the client-side agent proxy (done) | one client API layer | L | none |
| 2. One event path to the clients (sub-steps 1 and 2 done, sub-step 3 started) | three event projections, one event type | L | step 1 helps |
| 3. One command registry (done) | two command interpreters, one hand list | M | none |
| 4. The CLI is an RPC client (done) | a swapped pair of type names | S | none |
| 5. Shell commands run in the session workspace (done) | one wrong working directory, one dead route | S | none |
| 6. Wire types live in core (done) | a second home for wire types | M | steps 1 and 4 |
| 7. One test server | 18 test-server copies, a hand mock | M | step 6 helps |
| 8. Local duplicates | about ten small copies | S each | none |
| 9. Dead code (done) | unused modules and features | S | none |
| 10. Group the daemon modules | 87 flat entries | M | steps 1 to 6 |

Size: S is days, M is one to two weeks, L is three to six weeks.

## Step 1. Remove the client-side agent proxy

**Status: done.** The TUI and `cru chat` call `DaemonClient` directly.

**Before.** The TUI drove the daemon through an agent-shaped object, a
`DaemonAgentHandle` in the daemon's `rpc_client`. It implemented
`AgentHandle` and `SessionKnobs` over RPC, and it kept its own copies of the
model, the context strategy, precognition and the plugin approvals. A
`NoopAgentHandle` was a second implementation for replay. The TUI also
called `DaemonClient` directly, so two client APIs existed for one RPC
surface. The proxy also had a client-side clear that ended the session and
configured a new one: a second agent configuration in the client.

**Change.**
1. `LiveSession` in `crates/crucible-cli/src/session.rs` holds only the
   client and the session id. `open_session` creates or resumes the
   session. The TUI runner holds an optional `LiveSession`, and a replay
   holds none.
2. Each action in `crates/crucible-cli/src/tui/oil/chat_runner/actions.rs`
   calls the daemon. A value that the TUI shows after a change comes back
   from the daemon, not from a client copy.
3. The live event consumer opens interaction prompts from the session's
   event stream. The separate interaction channel is gone.
4. `AgentHandle` and `SessionKnobs` moved to
   `crates/crucible-daemon/src/agent_manager/handle.rs`. Five trait
   methods that only the proxy used are gone.
5. `crates/crucible-cli/src/test_daemon.rs` is a fake daemon on a Unix
   socket. The runner tests assert on the requests that reach it, in place
   of fake agent handles.

**Left for later.** The traits are still `pub` in the daemon crate,
because integration tests outside the crate implement them. `apply_mode`
and `set_mode_str` are now near-duplicates; the mode regression tests
still use the split.

## Step 2. One event path to the clients

**Now.** One wire event reaches the screen through four projections.
- The TUI decodes `SessionEventPayload`, and then matches three event names as
  strings in `crates/crucible-cli/src/tui/oil/chat_runner/commands.rs`.
- `cru acp` matches event names as strings in `classify_event` in
  `crates/crucible-cli/src/commands/acp/translate.rs`.
- The web backend re-encodes each event as `ChatEvent` in
  `crates/crucible-web/src/events.rs`.
- The Lua bridge shapes its own payloads in
  `crates/crucible-daemon/src/session_bridge.rs`.

Three transcript folds then decide what a turn and a tool card are: the
daemon in `crates/crucible-daemon/src/observe/markdown.rs`, the TUI in
`crates/crucible-cli/src/tui/oil/chat_app/`, and the web frontend in
`crates/crucible-web/web/src/contexts/chatEventReducer.ts`. The persisted
history uses a separate type, `LogEvent` in
`crates/crucible-daemon/src/observe/events.rs`.

**Status: sub-step 1 done.** `cru acp`, `cru session`, the TUI stream and
the web file events decode `SessionEventPayload` and match its typed
variants. The TUI and the web client no longer handle the three
`subagent_*` names, which no producer sent: delegation reaches both
clients as `delegation_*`. Two name checks stay on purpose: `cru acp`
still ends a turn on a `turn_finished` that does not decode, and the web
backend still accepts a pre-flattened `interaction_requested` from old
stored history. `LogEvent` still has `subagent_*` variants, because old
session files can hold them; sub-step 2 replaces that type.

**Change.**
1. Make every client decode `SessionEventPayload` only. Delete the string
   matches on event names.
2. **(done)** Store only wire-shaped lines. Most of `session.jsonl` is already wire
   events that `persist_event` writes, and `wire_to_log_event` turns them
   into `LogEvent` when a reader loads them. So `LogEvent` is the read
   model, not the stored majority. Two writers still store `LogEvent`
   lines directly: the clear marker and the accepted context injection in
   `crates/crucible-daemon/src/agent_manager/messaging/send.rs`. They write
   directly, and in order, because the broadcast writer can store an event
   later than the turn that it belongs to. The wire vocabulary cannot
   carry their data yet: `context_cleared` is not stored, and
   `ContextInjected` has no tags, kind, source or anchor. So this sub-step
   first widens those wire events, then writes them in wire shape on the
   same direct path, and keeps the `LogEvent` reader for old lines. The
   fork also writes wire lines: it copies each wire line of the parent, and
   converts the old view lines. An old plain system line has no wire form,
   so a fork copies it as it is.
3. Move the transcript fold into the daemon. Serve the folded transcript
   with the session history. Let the TUI and the web client render it, not
   fold it.

Do sub-steps 1 and 2 first. Sub-step 3 is the largest part, and it removes
the class of bug where the TUI and the web client show one turn differently.

**Sub-step 3, the plan.** The fold lives in core, and the daemon is the only
process that runs it for a session. The daemon folds each event as it
broadcasts it, and sends the ops of the fold with the event. The session
history carries the folded snapshot. A client applies the snapshot, then the
ops. The commits, in order:
1. **(done)** `crates/crucible-core/src/transcript/`: the types, the fold and
   the op replay, with golden files for five recordings.
2. **(done)** `session.history` returns the snapshot as `transcript`. `SessionManager::load_transcript` folds the whole stored log, after the migration of old lines. A resident session answers the live fold of the event bus instead, which also holds the streamed text.
3. **(done)** The daemon event bus folds each event and sends its ops in the `transcript` field of the live copy. The journal copy has no ops.
4. **(done)** The web backend forwards the ops: `to_sse` sends a `transcript` SSE frame after each live event that has ops.
5. **(done)** The web client renders the snapshot and the ops. Its own fold goes: `transcriptStore` applies the ops with a port of `Transcript::apply`, and `itemToMessage` maps each item to the view model. See [[Web Server]].
6. **(done)** The TUI renders the snapshot and the ops. Its own fold
   goes: `turn_msgs` makes no transcript messages, and `SessionEventStream`
   keeps no turn state. The old stream messages of `ChatAppMsg`
   (`TextDelta`, `ToolCall` and the rest) are gone. Their tests send wire
   events through `EventFeed`, which runs the core fold, as the daemon does.
7. **(done)** `cru acp` replays the snapshot on `session/load`, and maps the ops (`HostProjection` in `crates/crucible-cli/src/commands/acp/project.rs`).
8. A parity test renders one fixture in all three clients.
9. The markdown export and the Lua history read the snapshot.

**Proof.** One fixture transcript renders the same turns, segments and tool
cards in the TUI, the web client and `cru acp`. See [[Data Flows]] and
[[Core Domain Types]].

## Step 3. One command registry

**Status: done.** The daemon builds one command catalog per session
(`AgentManager::session_commands`, `crates/crucible-daemon/src/agent_manager/commands.rs`):
the built-in commands (`BuiltinCommand` in `crates/crucible-core/src/types/command.rs`),
then declared modes, then plugin commands, then discovered skills, then the
commands an ACP agent advertises — an earlier source keeps a name two
sources share. `session.commands` serves the list. `session.send_message`
routes a leading `/name` from the same catalog (`slash_route`) and answers
a `SendOutcome` (`Turn { message_id }` or `Command { command, result }`),
so a mode switch, a plugin command or a skill invocation no longer needs
its own RPC. The TUI matches `BuiltinCommand` with no wildcard and sends
every other `/name` to the daemon as a chat message;
`known_slash_commands`, `plugin_command_names` and
`ChatAppMsg::RunPluginCommand` are gone. The web server's static table in
`crates/crucible-web/src/routes/session_commands.rs` is gone too: `GET
/api/session/{id}/commands` answers the daemon's catalog, and `POST
/api/session/{id}/command` runs only the built-in commands, over the same
exhaustive `BuiltinCommand` match the TUI uses. `cru acp` advertises the
catalog to its host as `available_commands_update`, minus the built-in
commands, which have no meaning for a host.

**Known gaps.**
- A plugin reload does not emit `commands_changed`. A client must refetch
  the catalog itself after a reload it asked for.
- The web draft composer — before a session exists — has no catalog to
  list: `useAutocomplete`'s `/` trigger lists nothing until a session id
  is available.

**Kept, with the reason.** `lua.register_commands` stays even though no
client sends it any more: `crates/crucible-cli/tests/tui_e2e_tests/session_store.rs`
uses it as a Lua-session probe.

**Proof.** `/model x` from the TUI, the web client and Lua reaches
`session.switch_model` once. See [[TUI Chat App]] and [[Web Server]].

## Step 4. The CLI is an RPC client

**Status: done.** A closer read showed less to change than this step first
said.

**What the code does.** `cru plugin add` and `cru plugin remove` go through
`plugin.install` and `plugin.remove`. Four places run daemon code in the
CLI process, and each one calls the daemon's own function, not a copy:
- `cru plugin add` falls back to `plugin_ops::install` only when no daemon
  can start.
- `cru plugin stubs --offline` builds the daemon's plugin VM for a CI job
  that has no daemon.
- `cru plugin check` checks a plugin against the working tree for its
  author.
- `cru doctor` and the bootstrap commands evaluate the config through
  `evaluate_boot_config`, because they must work when the daemon cannot
  start.

These are offline and diagnostic uses of one implementation, so they stay.

**Change.** The CLI re-exported the two config types under each other's
names: its `CliConfig` was core's `CliAppConfig`, and the reverse. The CLI
now uses the core names.

## Step 5. Shell commands run in the session workspace

**Status: done.** A user's shell command runs where the session acts,
whichever client starts it, as in other harnesses.

**Before.** The TUI ran a `!` command in its own process directory, so a
session resumed from another directory ran commands in the wrong place. The
web client's PTY terminal already started in the session's workspace. The
web server also had a `POST /api/shell/exec` route that ran `sh -c` in the
web process, and no client called it.

**Change.**
1. `open_session` reads the session's workspace from the daemon. The TUI
   runs a `!` command there. A replay has no session, so it uses the
   process directory.
2. Delete the unused `/api/shell/exec` route.

The PTY terminal stays in the web process: it is the browser's terminal
transport, and it streams raw terminal bytes that the daemon's event bus
does not carry. The TUI modal also stays in the TUI, because it draws in
the user's terminal.

## Step 6. Wire types live in core

**Status: done.** The request and reply types moved to
`crates/crucible-core/src/protocol/requests/`. `RpcMethod`, `METHODS` and
`rpc_set_method` live in `crates/crucible-core/src/protocol/rpc/method.rs`.
Every `DaemonClient` call takes an `RpcMethod`, so a misspelled method does
not compile. The two DTO-only files that used to sit beside the client
submodules are deleted; each client submodule now holds only `DaemonClient`
methods and helpers. The server, the CLI, the web backend and the tests
import each request and reply type from core, not from the daemon's client
module. A test that sends a raw request to the dispatcher
still names the method as a string, because it tests the wire.

Each dispatch handler deserializes the request type that the client
serializes, with `typed_params::<T>` or `parse_params::<T>`. The
`require_param!` and `optional_param!` macros are deleted. Where a handler
accepted an absent field, the core type has a serde default, so an old caller
still works. The gate `wire_request_types_are_deserialized_not_hand_plucked`
lists each request type and the server file that deserializes it.

**Change.**
1. **(done)** Move the request and reply types to `crates/crucible-core/src/protocol/requests/`.
2. **(done)** Move `RpcMethod` next to them.
3. **(done)** Make each client call a method through `RpcMethod`, not a string.
4. **(done)** Give each dispatch handler a typed request.

**Proof.** `just ci` passes. A misspelled method no longer compiles. See
[[Daemon Server]] and [[RPC Client]].

## Step 7. One test server

**Status: done for the in-process harness (change items 1 and 2), item 3 open.**
`InProcessDaemonBuilder`/`InProcessDaemon` (`crates/crucible-daemon/src/test_support.rs`,
behind the crate's `test-utils` feature) is now the one definition. It
replaces the eighteen `struct TestServer` copies under
`crates/crucible-daemon/tests/` (fifteen top-level files, plus
`rpc_integration/server.rs`, which keeps its own name as a thin wrapper so
its nine sibling files needed no edit) and `crates/crucible-cli/tests/`
(`storage_factory_integration.rs`, `process_command_tests.rs`, which reach
the harness directly through `crucible_daemon::test_support`, since a crate
cannot use another crate's `tests/common`). `crates/crucible-daemon/tests/common/in_process.rs`
re-exports it for the daemon's own test files. The nineteenth copy,
`crates/crucible-daemon/src/server/tests/mod.rs`'s `pub(super) struct
TestServer`, stays: it sits inside the crate's unit tests, which cannot
reach `tests/common`. It is out of scope for this step.

Remaining, and NOT part of this step: `crates/crucible-web/src/test_support.rs`,
the web crate's hand-written mock daemon. It is a different kind of
duplicate — a mock that answers about 99 RPC methods by hand, not a copy of
an in-process real-daemon bind — and change item 3 below still describes
work to do there.

**Now.** The shared harness
in `crates/crucible-daemon/tests/common/` existed before most of the copies.
`crates/crucible-web/src/test_support.rs` (1722 lines) answers about 99 RPC
methods by hand. Each RPC change must also update it.

**Change.**
1. Keep one test server in `crates/crucible-daemon/tests/common/`.
2. Delete the copies.
3. Run the web route tests against a real in-process daemon where
   possible. Where a mock stays, derive its method set from `RpcMethod`, so
   an unhandled method fails to compile.

**Proof.** `just test ci` passes. See [[Test Architecture]].

## Step 8. Local duplicates

Each item is small and independent. Merge each one into the owner named here.

| Duplicate | Keep | Page |
|---|---|---|
| Two markdown renderers for the terminal: `crates/crucible-cli/src/formatting/markdown_renderer.rs` and `crates/crucible-cli/src/tui/oil/markdown/` | the Oil renderer; render it to a string for plain output | [[TUI Components]] |
| **Done in part.** Three ANSI parsers: `crates/crucible-oil/src/ansi.rs`, `crates/crucible-oil/src/cell_grid.rs`, `crates/crucible-oil/src/overlay.rs` | The overlay reads through `CellGrid` and truncates through `crates/crucible-oil/src/utils.rs`, so a joined grapheme keeps its cells. `ansi.rs` and `cell_grid.rs` keep their documented, tracked divergence | [[Oil Renderer]] |
| **Done.** Two color readers: `parse_color_string` in `crates/crucible-lua/src/theme.rs` and one for `cru.oil` nodes and HTML templates. The second knew fewer forms, so a theme color could fail in a node | `Color::parse` in `crates/crucible-oil/src/style.rs` | [[Oil Renderer]] |
| **Done.** Five frontmatter scans: three in `crates/crucible-core/src/parser/`, one in `crates/crucible-core/tests/dev_kiln.rs`, and the writer's `split_fences` | `split_frontmatter` in `crates/crucible-core/src/parser/frontmatter.rs`, which uses `split_fences` for YAML | [[Parser]] |
| The selection flow in three TUI modals in `crates/crucible-cli/src/tui/oil/components/interaction_modal/` | one shared helper | [[TUI Components]] |
| `ToolCall` and `ChatToolCall` in `crates/crucible-core/src/traits/` | one model tool-call record | [[Core Domain Types]] |
| Two `SessionError` types, and the legacy `CrucibleError` | one error for each domain | [[Session Services]] |
| **Done.** `from_toml` copied in four plugins under `runtime/plugins/`. Each copy read the absent `crucible` global, so the `plugins.<name>` section never answered a key before `setup()`. `cru.service` had a fifth copy | `cru.settings.new` in `crates/crucible-lua/src/prelude/stdlib.rs` | [[Luau APIs]] |
| Lua twins of core types in `crates/crucible-lua/src/` (`PermissionRequest`, `LuaTool`, `BaseOperation`) | the core type, with a conversion | [[Luau APIs]] |
| `session_api.rs` next to `sessions/` in `crates/crucible-lua/src/` | `sessions/` | [[Luau APIs]] |
| **Done.** `perm.autoconfirm_session`, a session-named flag that one client holds. It repeated the session mode `auto`, which the daemon owns, and `cru chat` ignored it in one-shot mode | removed; the `auto` mode (`Shift+Tab`) approves each call | [[TUI Components]] |

## Step 9. Dead code

**Status: done.** Deleted:
- `ModelDiscovery`, a local GGUF catalog that only an example used.
- `NodeSpec` and `spec_to_node`, a second markup front end that nothing
  called. Its color parser moved to `crates/crucible-oil/src/template/html.rs`
  and serves both the HTML subset and `cru.oil`.
- The `linkify` and `syntect` features of `vendor/markdown-it`: the
  workspace builds the crate with `default-features = false`.
- `ToolResult` in `crates/crucible-lua/src/types.rs`, `PluginSpec::source`,
  and three `LifecycleError` variants that nothing built.
- Comments that named merged crates.

Kept, with the reason:
- The `storage.*` RPCs that answer `not_implemented`: storage maintenance
  is a P0 item in [[Meta/Product]], so the product decides whether they
  get built or go.
- The `FullscreenShell` prototype: the full-screen user story names its
  pane code as tested prototype work.
- `lua.register_commands`: no client sends it, but see step 3 for why it
  stays.

## Step 10. Group the daemon modules

**Now.** `crates/crucible-daemon/src/` has 87 flat entries. The MCP, kiln and
session families are spread across the root and `tools/`. Some names
collide, for example `rpc/workflow_handlers.rs` and `workflow_handlers/`.

**Change.** Move the modules into about ten domain folders, for example
transport, session, agent, tools, knowledge, review, plugins and client.
Change no behavior. Do this step last, because steps 1 to 6 delete many of
these modules first. A move during heavy change causes merge conflicts.

**Why it helps.** Step 1 of the repository agent guide is to find the owner.
A folder for each domain makes the owner easy to find.

**Proof.** `cargo check --workspace --all-targets` passes, and the diff
shows only renames.

## What not to merge

These look like duplicates but are separate contracts. Keep them apart.
- The parser byte spans, the SQLite link index, kiln identity and the
  embeddings. `NotePipeline` connects them.
- Byte, character and terminal-width truncation.
- `RawToolCall` (provenance) and `CanonicalToolCall` (classification).
- `FixtureEmbeddingProvider` (a runtime backend) and `MockEmbeddingProvider`
  (a test double).
- Same-name modules in core and the daemon, where core holds the types and
  the daemon holds the behavior.
- `ShellPolicy` and `PatternStore`, which have different bypass models.
- The TUI and web renderers of one `InteractionRequest`.
- The web REST layer as a whole. It is the browser boundary. Step 6 makes
  each route cheaper; it does not remove the layer.
- A missing embedding provider and a failed one. They have different
  failure policies.

See also [[Consolidation Plan]] for the decisions that earlier cleanups
recorded.

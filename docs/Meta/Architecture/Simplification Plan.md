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
| 2. One event path to the clients (sub-step 1 done) | three event projections, one event type | L | step 1 helps |
| 3. One command registry | two command interpreters, one hand list | M | none |
| 4. The CLI is an RPC client | an in-process daemon in the CLI | M | none |
| 5. Shell commands run in the daemon | two process spawners | M | none |
| 6. Wire types live in core | a second home for wire types | M | steps 1 and 4 |
| 7. One test server | 18 test-server copies, a hand mock | M | step 6 helps |
| 8. Local duplicates | about ten small copies | S each | none |
| 9. Dead code | unused modules and features | S | none |
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
2. Replace `LogEvent` with the persisted subset of `SessionEventPayload`.
   Keep one writer of event history.
3. Move the transcript fold into the daemon. Serve the folded transcript
   with the session history. Let the TUI and the web client render it, not
   fold it.

Do sub-steps 1 and 2 first. Sub-step 3 is the largest part, and it removes
the class of bug where the TUI and the web client show one turn differently.

**Proof.** One fixture transcript renders the same turns, segments and tool
cards in the TUI, the web client and `cru acp`. See [[Data Flows]] and
[[Core Domain Types]].

## Step 3. One command registry

**Now.** Three interpreters parse slash commands. The TUI has `ReplCommand`
in `crates/crucible-cli/src/tui/oil/chat_app/repl_command.rs`. The web server
has a static table in `crates/crucible-web/src/routes/session_commands.rs`.
The daemon lists plugin commands with `plugin.commands`. The web palette in
`crates/crucible-web/web/src/App.tsx` has a fourth list.

**Change.**
1. Put one registry in the daemon. It holds the builtin commands and the
   plugin commands. It serves a list and an execute RPC.
2. Delete `crates/crucible-web/src/routes/session_commands.rs`.
3. Make the web palette and the TUI read the list from the daemon.
4. Keep in the TUI only the commands that change display state, for example
   quit and the palette.

**Proof.** `/model x` from the TUI, the web client and Lua reaches
`session.switch_model` once. See [[TUI Chat App]] and [[Web Server]].

## Step 4. The CLI is an RPC client

**Now.** The CLI runs daemon code in its own process.
- `cru plugin add` calls `plugin_ops::install` directly in
  `crates/crucible-cli/src/commands/plugin/add.rs`. The daemon also serves
  `plugin.install`.
- `cru plugin check` and `cru plugin stubs` build a `DaemonPluginLoader` in
  `crates/crucible-cli/src/commands/plugin/`.
- `crates/crucible-cli/src/config.rs`, `crates/crucible-cli/src/main.rs`,
  `crates/crucible-cli/src/commands/daemon.rs` and
  `crates/crucible-cli/src/commands/doctor.rs` call `evaluate_boot_config`.
- The legacy `CliAppConfig` is still the config type of 11 CLI files.

**Change.**
1. Send plugin install, check and stubs to the daemon as RPC calls.
2. Read the effective config from the daemon. Keep a local evaluation only
   where the daemon cannot start, for example in `cru doctor`, and name that
   reason in the code.
3. Keep `CliAppConfig` only inside `cru config migrate`.
4. In `crates/crucible-daemon/src/lib.rs`, make each module private when no
   other crate uses it any more.

**Result.** One activation path exists, as the repository agent guide
requires. The compiler rejects a new in-process copy.

**Proof.** The plugin install and doctor flows run against a real daemon.
See [[CLI Commands]] and [[Luau Host]].

## Step 5. Shell commands run in the daemon

**Now.** The daemon owns the `bash` tool and background jobs. The web server
spawns `sh` in `crates/crucible-web/src/routes/shell.rs`, and its own comment
calls this a stopgap. The TUI spawns a shell in
`crates/crucible-cli/src/tui/oil/components/shell_modal.rs`.

**Change.** Run a user shell command as a daemon job. Stream its output to
the client. Delete both client-side spawners. Keep the editor launch in the
TUI, because it needs the terminal of the user.

**Proof.** The same command from the TUI and the web client appears as one
daemon job, and cancel stops it. See [[Tools and Admission]].

## Step 6. Wire types live in core

**Now.** 95 request types live in `crates/crucible-daemon/src/rpc_client/`,
not in `crates/crucible-core/src/protocol/`. The server imports them from its
own client module. The clients spell method names as strings in many places,
next to `RpcMethod` in `crates/crucible-daemon/src/rpc/dispatch.rs`.

**Change.**
1. Move the request and reply types to `crates/crucible-core/src/protocol/`.
2. Move `RpcMethod` next to them.
3. Make each client call a method through `RpcMethod`, not a string.
4. Give each dispatch handler a typed request. Today `dispatch.rs` still
   reads raw JSON fields in several handlers.

**Proof.** `just ci` passes. A misspelled method no longer compiles. See
[[Daemon Server]] and [[RPC Client]].

## Step 7. One test server

**Now.** The crates define `struct TestServer` 19 times. The shared harness
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
| Three ANSI parsers: `crates/crucible-oil/src/ansi.rs`, `crates/crucible-oil/src/cell_grid.rs`, `crates/crucible-oil/src/overlay.rs` | one grapheme-aware parser; `overlay.rs` still uses one `char` per cell | [[Oil Renderer]] |
| Three frontmatter scanners in `crates/crucible-core/src/parser/` | `frontmatter_extractor.rs` | [[Parser]] |
| The selection flow in three TUI modals in `crates/crucible-cli/src/tui/oil/components/interaction_modal/` | one shared helper | [[TUI Components]] |
| `ToolCall` and `ChatToolCall` in `crates/crucible-core/src/traits/` | one model tool-call record | [[Core Domain Types]] |
| Two `SessionError` types, and the legacy `CrucibleError` | one error for each domain | [[Session Services]] |
| `from_toml` copied in four plugins under `runtime/plugins/` | one host-owned helper module in `crates/crucible-lua/src/modules.rs` | [[Luau APIs]] |
| Lua twins of core types in `crates/crucible-lua/src/` (`PermissionRequest`, `LuaTool`, `BaseOperation`) | the core type, with a conversion | [[Luau APIs]] |
| `session_api.rs` next to `sessions/` in `crates/crucible-lua/src/` | `sessions/` | [[Luau APIs]] |
| `perm.autoconfirm_session`, a session-named flag that one client holds | a session knob, or remove it | [[TUI Components]] |

## Step 9. Dead code

Delete each item, or finish it if a product need exists.
- The `storage.*` RPCs that answer `not_implemented`:
  `crates/crucible-daemon/src/server/storage.rs`.
- `ModelDiscovery`, used only by an example:
  `crates/crucible-daemon/src/llm/model_discovery.rs`.
- `NodeSpec` and `spec_to_node`: `crates/crucible-oil/src/template/node_spec.rs`.
- The `FullscreenShell` prototype:
  `crates/crucible-cli/src/tui/oil/fullscreen/shell.rs`.
- Unused Lua and RPC types and variants in `crates/crucible-lua/src/types.rs`
  and `crates/crucible-lua/src/lifecycle/`.
- The `linkify` and `syntect` features of `vendor/markdown-it`, which no
  Crucible code uses.
- Comments that still name merged crates, and the stale draft
  `crates/crucible-cli/AGENTS.md`.

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

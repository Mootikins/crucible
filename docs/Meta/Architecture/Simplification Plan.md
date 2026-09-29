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
| 2. One event path to the clients (done) | three event projections, one event type | L | step 1 helps |
| 3. One command registry (done) | two command interpreters, one hand list | M | none |
| 4. The CLI is an RPC client (done) | a swapped pair of type names | S | none |
| 5. Shell commands run in the session workspace (done) | one wrong working directory, one dead route | S | none |
| 6. Wire types live in core (done) | a second home for wire types | M | steps 1 and 4 |
| 7. One test server (done) | 18 test-server copies, a hand mock | M | step 6 helps |
| 8. Local duplicates (done) | about ten small copies | S each | none |
| 9. Dead code (done) | unused modules and features | S | none |
| 10. Typed core replies (done, except the knob replies of step 13) | 36 web row types; 6 were net deletions, the rest became core types | M | step 6 |
| 11. One event vocabulary to the browser | `ChatEvent` and 2 more web types, 29 TS copies: about 32 | M | step 10 |
| 12. One request body per shape (in part: 33 types went; 4 web bodies and the typed `call` remain) | about 35: 19 web request copies, 6 core shape copies, 10 local `Params` | M | step 10 |
| 13. One generic knob | about 11 types, 10 RPC methods, about 90 per-knob functions | M | step 12 |
| 14. Simpler core APIs and the audit list | about 40 | S each | step 12 |
| 15. Luau types from the schema | about 8 | M | step 12 |
| 16. The last TS copies | about 16 | S | step 11 |
| 17. One daemon test fixture (done) | 3 test types | S | none |
| 18. Enums on the wire | about 0, string fields become enums | S | step 12 |
| 19. One RPC route for the web | about 90 types, about 70 TS functions, about 100 routes | L | steps 12 and 13 |
| 20. Plugin data schemas | 0 core types; plugin data gets checked shapes | M | step 19 |
| 21. Owner decisions | about 9, see the step | S each | none |

Size: S is days, M is one to two weeks, L is three to six weeks.

## The type budget

Steps 10 to 18 are measured by the count of types that they delete. A step
that only moves or renames code does not count.

At `514c4cc3b`, the tree has 2595 types: 1867 Rust types in `crates/*/src`,
701 hand-written TypeScript types and 27 Luau types. The target is 15 percent
fewer: 2206 or less. At `2f1736802` the count is 2584.

The audit of step 18 of the old plan read every crate. Most types that look
alike are separate contracts: a raw read and a resolved record, a create
input and a query filter, a render node and a stateful component. Merging
them would make required fields optional. Steps 11 to 18 therefore give
about 140 types, about 6 percent. Step 19 adds about 90, and step 21
about 9. Together that is about 9 percent. More needs research after step
19, when the count shows what remains.

The count:

```sh
rg -c -t rust '^\s*(pub(\([a-z:]+\))? )?(struct|enum) [A-Z]' crates/*/src
rg -c '^\s*(export )?(interface|type) [A-Z]' crates/crucible-web/web/src \
  -g '*.ts' -g '*.tsx' -g '!api-schema.d.ts' -g '!**/__tests__/**'
rg -c '^\s*(export )?type [A-Z]' runtime -g '*.luau'
```

Rust core types are the one source of each boundary type. The derive is
`utoipa::ToSchema`, which already writes `openapi.json` and, through
`openapi-typescript`, `api-schema.d.ts`. Steps 11 and 14 extend it to every
wire type and to Luau. No second derive and no separate IDL language.

## How a step is accepted

A step is done when the code base is better in a way a person can see, not
when a check passes. Each "Done when" below names outcomes of three kinds:

1. **Gone.** Named types, functions or routes no longer exist, and the type
   count falls by at least the stated amount. A move or a rename does not
   count.
2. **Cheaper to change.** The change cost below falls: the number of places
   a person must edit to add a method, a setting, an event or a plugin data
   shape. It is measured by doing the change once on a scratch branch after
   the step, and counting the files and declarations touched.
3. **Same behavior on every path.** The behavior is tested through the real
   path of each client that has it (the TUI, the web client, Lua, `cru acp`),
   not through a helper that only one client calls.

These do not count as acceptance, because they pass without an improvement
or can be made to pass: a test that only checks that a type or a file
exists; a grep or source-text gate; a golden file written by the new code;
a count ratchet. A wire fixture proves compatibility only when it was
captured from the code before the change and is not regenerated in the same
change. A rule that the compiler enforces (an exhaustive match, a missing
type, a required trait method) counts, because it stops the old path from
coming back.

### Change cost, measured at `226da3efb`

| Change | Places to edit today |
|---|---|
| A daemon method that the browser calls | about 9: the core request and reply, the `rpc_methods!` row, the dispatch arm, the handler, the client method, the web route and its types, the forwarding function in `services/daemon.rs`, the TS function and its TS types |
| A session setting (knob) | about 15: see the cross-layer checklist in `AGENTS.md` |
| A session event that the browser shows | about 5: the payload variant, the `ChatEvent` variant and `from_daemon_event`, the TS type, the reducer |
| A plugin data shape that the web reads | a hand TS type in the plugin's block, with no check against what the plugin sends |

After steps 11 to 21, the targets are: a browser method in 3 places (the
body and reply type, the `rpc_methods!` row, the handler; the allow list
entry if the browser may call it); a knob in 3 (the `SessionKnob` variant and
its value type, the daemon's apply arm, a client control if one is wanted);
an event in 2 (the payload variant, the reducer arm that `tsc` demands); a
plugin data shape in 1 (the Luau declaration in the plugin's spec).

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

**Status: done for the transcript.** Sub-step 3 finished with the parity
test: the daemon folds each session once, and the TUI, the web client and
`cru acp` draw its transcript. The web backend still re-encodes each event
as `ChatEvent` for the browser; step 11 removes that. The notes below describe sub-step 1 as it ended.

**Sub-step 1.** `cru acp`, `cru session`, the TUI stream and
the web file events decode `SessionEventPayload` and match its typed
variants. The TUI and the web client no longer handle the three
`subagent_*` names, which no producer sent: delegation reaches both
clients as `delegation_*`. Two name checks stay on purpose: `cru acp`
still ends a turn on a `turn_finished` that does not decode, and the web
backend still accepts a pre-flattened `interaction_requested` from old
stored history. `LogEvent` still has `subagent_*` variants, because old
session files can hold them; sub-step 2 replaces that type.

**Change.**
1. **(done)** Make every client decode `SessionEventPayload` only. Delete the string
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
3. **(done)** Move the transcript fold into the daemon. Serve the folded transcript
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
8. **(done)** A parity test renders each golden transcript in all three
   clients. `each_golden_transcript_has_its_client_rows` in
   `crates/crucible-core/src/transcript/tests.rs` writes the rows of each
   file to `assets/fixtures/golden/transcript/rows/`: the prompts, the
   segments, the tool cards and the notices. The web test
   (`lib/__tests__/transcript.test.tsx`), the TUI test
   (`tui/oil/tests/transcript_parity_tests.rs`) and the ACP test
   (`a_load_draws_the_rows_of_each_golden_transcript` in
   `commands/acp/project.rs`) compare their drawn rows with the same file.
   ACP has no update for a notice, so its test leaves the notices out.
9. **(done)** The markdown export and the Lua history read the snapshot.
   `render_to_markdown` (`observe/markdown.rs`), `message_rows`
   (`session_bridge.rs`), `session.list_persisted`, `session.cleanup` and
   `cru session show`, `list` and `export` read a `Transcript`. The CLI
   gets it from `session.history`; when no daemon starts, it runs the
   daemon's fold on the file (`crucible_daemon::load_transcript`). The CLI
   has no renderer of its own for markdown. `parse_session_log`,
   `load_events` and the `session.load_events` RPC are gone.
   `stored_events` (`observe/events.rs`) turns each old view line and
   `context_injection` line into its wire event before the fold. `LogEvent`
   stays only as the form of a model-context message: the conversation
   tree (`rebuild.rs`), a fork and accepted context read it through
   `replay_session_log`. Golden files in `assets/fixtures/golden/export/`,
   `lua_rows/` and `cli_session/` pin each reader.

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

**Status: done.**
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

Change item 3 is done. `start_real_daemon_with_kilns` in
`crates/crucible-web/src/test_support.rs` starts a real daemon through
`InProcessDaemonBuilder`, registers the given kilns and indexes their notes.
The file, canvas, note-write and backlinks route tests, the `/health` and
`/ready` tests, and the private-endpoint session test use it. The mock no
longer holds kilns or serves real files. `bases_daemon_e2e.rs`,
`file_root_daemon_e2e.rs` and `proposal_daemon_e2e.rs` also use the builder
now, not a bind of their own. The mock stays for a test that needs a daemon
failure, a fixed reply shape, or a record of the RPC params. Its reply
`match` is exhaustive over `RpcMethod`, with no wildcard arm, and its
scripted errors and recorded calls use `RpcMethod` keys. A dummy
`RpcMethod` variant fails to compile at that `match`.

**Before.** The shared harness
in `crates/crucible-daemon/tests/common/` existed before most of the copies.
`crates/crucible-web/src/test_support.rs` (1722 lines) answered about 99 RPC
methods by hand, through a `match` on strings with a `null` wildcard. Each
RPC change had to update it, and no check found a method that it missed.

**Change.**
1. **Done.** Keep one test server in `crates/crucible-daemon/tests/common/`.
2. **Done.** Delete the copies.
3. **Done.** Run the web route tests against a real in-process daemon where
   possible. Where a mock stays, derive its method set from `RpcMethod`, so
   an unhandled method fails to compile.

**Proof.** `just test ci` passes. See [[Test Architecture]].

## Step 8. Local duplicates

Each item is small and independent. Merge each one into the owner named here.

| Duplicate | Keep | Page |
|---|---|---|
| **Done.** Two markdown renderers for the terminal: `markdown_renderer.rs` in `crates/crucible-cli/src/formatting/`, and `crates/crucible-cli/src/tui/oil/markdown/` | the Oil renderer. `markdown_to_string` renders it to a string, with styles or as plain text. `cru chat -q` uses it | [[TUI Components]] |
| **Done in part.** Three ANSI parsers: `crates/crucible-oil/src/ansi.rs`, `crates/crucible-oil/src/cell_grid.rs`, `crates/crucible-oil/src/overlay.rs` | The overlay reads through `CellGrid` and truncates through `crates/crucible-oil/src/utils.rs`, so a joined grapheme keeps its cells. `ansi.rs` and `cell_grid.rs` keep their documented, tracked divergence | [[Oil Renderer]] |
| **Done.** Two color readers: `parse_color_string` in `crates/crucible-lua/src/theme.rs` and one for `cru.oil` nodes and HTML templates. The second knew fewer forms, so a theme color could fail in a node | `Color::parse` in `crates/crucible-oil/src/style.rs` | [[Oil Renderer]] |
| **Done.** Five frontmatter scans: three in `crates/crucible-core/src/parser/`, one in `crates/crucible-core/tests/dev_kiln.rs`, and the writer's `split_fences` | `split_frontmatter` in `crates/crucible-core/src/parser/frontmatter.rs`, which uses `split_fences` for YAML | [[Parser]] |
| **Done.** The selection flow in three TUI modals in `crates/crucible-cli/src/tui/oil/components/interaction_modal/`. The panel had its own cursor wrap and toggle, so the cursor never reached its "Other" row | `ChoiceList` in `crates/crucible-cli/src/tui/oil/components/interaction_modal/choice.rs`, which the Ask, AskBatch, Popup and Panel modals use | [[TUI Components]] |
| **Done.** `ToolCall` and `ChatToolCall` in `crates/crucible-core/src/traits/`. `ToolCall` kept an OpenAI shape with a JSON string for arguments. Only `MessageMetadata::tool_calls` used it, and no code filled that field | `ChatToolCall` in `crates/crucible-core/src/traits/chat.rs` | [[Core Domain Types]] |
| **Done.** Two `SessionError` types, and the legacy `CrucibleError`. No caller matched a variant of the `observe` copy or of `CrucibleError` | `SessionError` in `crates/crucible-daemon/src/session_manager.rs`. `KnowledgeRepository` returns `anyhow::Result` | [[Session Services]] |
| **Done.** `from_toml` copied in four plugins under `runtime/plugins/`. Each copy read the absent `crucible` global, so the `plugins.<name>` section never answered a key before `setup()`. `cru.service` had a fifth copy | `cru.settings.new` in `crates/crucible-lua/src/prelude/stdlib.rs` | [[Luau APIs]] |
| **Done.** Lua twins of core types in `crates/crucible-lua/src/` (`PermissionRequest`, `LuaTool`, `BaseOperation`). `BaseOperation` lived in the Lua crate, and the daemon imported it from there. `LuaTool` and `ToolParam` copied `DiscoveredTool`, and no code read them. `PermissionRequest` copied a `file_path` that the daemon read from the arguments | `BaseOperation` in `crates/crucible-core/src/bases/operation.rs`. `LuaTool` and `ToolParam` are deleted. `PermissionRequest` stays as a Lua-side view over `CanonicalToolCall`: it holds only the arguments, the read-only class and the mode, which no core type holds. Its `IntoLua` conversion makes the hook table and reads `file_path` | [[Luau APIs]] |
| **Done.** `session_api.rs` next to `sessions/` in `crates/crucible-lua/src/`. Both registered on `cru.session`, and each file imported the other | `sessions/`: `handle.rs` holds the `Session` userdata, `current.rs` holds `CurrentSession`. One helper answers `current` and `get_session` | [[Luau APIs]] |
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

## Step 10. Typed core replies

**Status: done, except the provider, mode, knob and agent-option replies,
which step 13 changes.** The session, skill, surface, comment, search, kiln,
fs and plugin replies are core types with `ToSchema`. The daemon builds each
one, and the web route returns it unchanged.

**What it gave.** Where a web row copied an existing core type, the web row
went (surfaces, comments, skills: 13 types). Where the daemon built the
reply with `json!`, no type existed, so one web row became one core type
(search, plugins: 0 or +1 each). The typed replies found a real drift: a
test double answered `plugin.commands` with a shape that the daemon never
sends.

## Step 11. One event vocabulary to the browser

**Now.** `ChatEvent` in `crates/crucible-web/src/events.rs` re-encodes each
`SessionEventPayload` into 21 variants of its own. `normalize_interaction`
flattens a permission request. `lib/types.ts` copies 29 wire types by hand.

**Change.** Give `ToSchema` to the event payloads, the interaction types and
the tool-call types. The SSE route sends `SessionEventMessage`. Delete
`ChatEvent`, `PrecognitionNote`, `SessionHistoryEvent`,
`normalize_interaction` and the TS copies; alias the generated types.

**Done when.**
- `ChatEvent`, `from_daemon_event`, `normalize_interaction`,
  `PrecognitionNote`, `SessionHistoryEvent` and the 29 hand TS copies are
  gone: about 32 types fewer.
- A new `SessionEventPayload` variant reaches the browser's TS union with no
  hand edit, and the reducer's exhaustive switch fails `tsc` until it
  handles the variant. The change cost of an event falls from about 5 places
  to 2.
- The transcript parity test passes on the live SSE path of the web client,
  and on the TUI and `cru acp` paths.


## Step 12. One request body per shape

**Status: in part.** Items 1 to 4 below are done. Item 5 is not done. The
count of Rust types in `crates/crucible-{core,daemon,web,cli}/src` went
from 1616 to 1583: core 654 to 652, daemon 557 to 545, web 156 to 137, CLI
249 to 249. The golden fixtures in `assets/fixtures/golden/requests/` hold
the JSON of the code before the change, and
`crates/crucible-core/src/protocol/requests/golden_tests.rs` proves that no
request JSON changed.

What went:
- Core, merged by shape: `NoteRef`, `KilnRef`, `DiffsetRef` and
  `DiffCommentKey` replace eight method types. `Scoped<()>` replaces
  `SessionIdRequest` and `LuaShutdownSessionRequest`. `Scoped<Page>`
  replaces `SessionHistoryRequest` and `SessionResumeFromStorageRequest`.
- Core, client copies: `SessionCreateParams`, `SessionAgentSpec` and
  `EmptyParams`. A caller builds `SessionCreateRequest`.
- Each other session-scoped request, except the five knob requests of step
  13, is now a body inside `Scoped<T>`. This deletes no type, but the web
  can read the body.
- Web: `FsListQuery`, `FsMoveBody`, `FsPathBody`, `CommentBody`,
  `ResolveCommentBody`, `DeleteCommentBody`, `InstallRequest`,
  `PublicationsQuery`, `CommandRequest`, `CloneRequest`, the proposals
  `ListQuery`, `CanvasPathQuery`, `FilePathQuery`, `KilnPathQuery`,
  `HistoryQuery`, `KilnRequest`, `SetTitleRequest` and
  `SetWorkspaceRequest`. `FsRootKind` moved to core.
- Daemon: the nine local `Params` in `rpc/dispatch.rs`, with
  `ConfigValuesParams`, and the three in `rpc/workflow_handlers.rs`.
  `config.get` and `config.origin` share `ConfigLookupRequest`.

What remains:
- The web keeps `AcceptProposalBody`, `RejectProposalBody` and
  `ResolveProposalBody`. Their RPC requests hold the proposal id, not a
  session id, and the plan allows no envelope other than `Scoped<T>`. A
  shared `ProposalFiles` body would add one core type for each web type
  that goes.
- The web keeps `InteractionResponseRequest`. Its route takes `session_id`
  in the body, not the path, and `Scoped<T>` has no schema. Step 11 owns
  the interaction route.
- Item 5 is open. After the merge, one body type serves more than one
  method (`NoteRef`, `Scoped<()>`), so a body cannot name one method and one
  reply. A typed `call` needs a row for each method in `rpc_methods!` that
  names its request and its reply, and many replies are still `Value`.

A new session-scoped method now needs: a body type in
`crates/crucible-core/src/protocol/requests/`, or an existing one with the
same shape; an `rpc_methods!` row; a dispatch arm; a handler that reads
`Scoped<Body>`; a client method; a `WIRE_REQUEST_TYPES` row; and a golden
fixture. A web route reads the same body, so it needs no web type.

**Now.** Core has one request type per RPC method: 96 `*Request` types. 31
of them carry `session_id`. Methods with one shape still have separate
types (`GetNoteByNameRequest` and `GetBacklinksRequest`; `KilnGraphRequest`
and `NoteListRequest`; `DiffGetRequest` and `DiffCommentsRequest`;
`DiffResolveCommentRequest` and `DiffDeleteCommentRequest`;
`SessionHistoryRequest` and `SessionResumeFromStorageRequest`;
`LuaShutdownSessionRequest` and `SessionIdRequest`). The web takes
`session_id` or another id from the URL path, so it declares its own body
types: 19 of them equal a core request, or equal it minus the id. The
daemon declares 12 local `Params` in `rpc/dispatch.rs` and
`rpc/workflow_handlers.rs`; 3 of them copy a core request.

**Change.**
1. Name each core body for its shape, not for its method, and let methods
   with one shape share it.
2. A session-scoped method takes `Scoped<T> { session_id, #[serde(flatten)]
   body: T }`. The JSON on the wire does not change: no core request uses
   `deny_unknown_fields`, and the web builds its bodies with `session_id` at
   the top level. `Scoped<T>` is RPC-only, so it needs no `ToSchema`; the web
   declares `T`.
3. The web route takes `Path(id)` and `Json<T>` or `Query<T>` of the core
   body. Delete the 19 web copies.
4. Replace the daemon's local `Params` with core bodies. Delete
   `SessionCreateParams`, `SessionAgentSpec`, `EmptyParams`.
5. Each body names its method and reply in the `rpc_methods!` row, and
   `DaemonClient::call<Req>` replaces the methods that return `Value`.

A client and a daemon of different builds never talk: the client restarts
a daemon whose `build_sha` differs. So no method needs an old alias.

**Done when.**
- The same-shape core requests, the 19 web copies, the daemon's local
  `Params`, `SessionCreateParams`, `SessionAgentSpec` and `EmptyParams` are
  gone: about 35 types fewer.
- For each changed method, the JSON that the new types send equals a golden
  fixture captured from the code before the change. The fixtures are
  committed first, in their own commit, and the change does not rewrite
  them.
- Each `rpc_methods!` row names its body and reply type, and a row without
  them does not compile. A new session-scoped method needs one body type,
  not three.


## Step 13. One generic knob

**Now.** Each of the five `SessionKnob` values (model, mode, context
strategy, precognition, plugin turn limit) has its own RPC method pair, core
request type, handler, client method, web route pair, web request and
response types, TUI message and TS hook: about 90 functions and 11 types in
all. Mode sits on `AgentHandle`, not on `SessionKnobs`. The AGENTS.md
cross-layer checklist exists because of this.

**Change.** One RPC pair, `session.knob.set { session_id, knob, value }` and
`session.knob.get { session_id, knob }`, with `value: KnobValue`, an enum
with one variant per knob. The daemon validates the value once and applies
it through the existing `on_acp` table, so the ACP rules stay in one place.
The web gets one route pair. The TUI's `:set` and `/mode` send the knob and
the value. Delete the per-knob methods, types, routes, client methods,
messages and hooks. Replace the cross-layer checklist with the few steps
that a new knob still needs.

**Done when.**
- The ten per-knob RPC methods, their request types, client methods, web
  routes, web types, TUI messages and TS hooks are gone: about 11 types and
  about 90 functions fewer.
- The change cost of a knob falls from about 15 places to 3, measured by
  adding a scratch knob on a scratch branch.
- Set, get and resume work for every knob through the TUI's `:set`, the web
  client and Lua, tested on each real path. An ACP session refuses the
  knobs that `on_acp` marks absent, on every path.
- The AGENTS.md cross-layer checklist is replaced by the three steps a new
  knob still needs.


## Step 14. Simpler core APIs and the audit list

**Change.** Delete what the audit found, each with its evidence in the
audit record:
- core: the two `TokenUsage` types and the TS copy; `LlmToolDefinition` and
  `FunctionDefinition`; `ChatToolResult`'s strings; the two `SearchResult`;
  `DocumentId`; `LlmProviderConfigBuilder`; the 32 named constructors of
  `SessionEventMessage`; `ConfigValidationError` into `ConfigError`; the dead
  `EventRing`, `EventError`, `ModelCapability`, `UnifiedModelInfo`; the four
  `Value` fields whose shape is known; the unread fields of `ToolDefinition`
  and `ModeDescriptor`.
- across crates: `ModeStance` (lua) into core `PermissionMode`; `Unprompted`
  (daemon) into `PermissionDecision`; the TS `PermissionScope`.
- error enums whose variants no caller matches become `anyhow`:
  `watch::Error`, `SkillError`, `BackgroundError`, `ModelListingError`,
  `ReplayError`, and the others that a sweep finds. **(Done.** The sweep
  also removed `GatewayError`, `HtmlError`, `GateError`, `GraphError`,
  `TaskError`, `RegistryError`, `WebhookAuthError`, `ProjectError`,
  `CredentialError`, `ManifestError`, `ScopeError`, `SourcesError`,
  `HttpError` and `IncludeError`. `LuaError`, `StorageError`, core
  `turn::AgentError`, `PatternError` and `ParserError` stay for now: each
  change touches a trait contract or an area that another change owns.**)**
- small copies: `RenderedOverlay` (oil), the identical one-field tool
  parameter structs, `ModelInfoBuilder`, `ExecuteMultiKilnSearchParams`,
  the duplicate test `Daemon` in `server/diff.rs`, the web path and kiln
  query copies, the TS `PaneDropPosition` and `Rect` copies. **(Done,**
  also `EventUtils`, the TS `FileBodyProps` and the Lua notify test
  doubles. `GetJobResultParams` and `CancelJobParams` stay: their field
  descriptions differ, and each description is the text of a tool
  schema.**)**

**Done when.**
- Each listed item is gone, or the step records why it stays, with the code
  reason. The type count falls by about 40.
- Each error enum that stays has a named caller that matches its variants.
- The golden wire tests, captured before the change, still pass.


**Part A progress (core and cross-crate items).** Tracked item by item, each
with its evidence in the commit that did it:
1. `TokenUsage` merge: skipped. `traits::llm::TokenUsage` has three required
   `u32` fields plus two optional cache fields; `transcript::TokenUsage` has
   four fields that are all `Option<u32>`, because
   `TurnPayload::MessageComplete` can carry a partial usage report. A merge
   would put `Option` on the required fields of the first type, which the
   plan's design rules forbid.
4. `SearchResult` merge: skipped. `types/database.rs::SearchResult` carries
   `document_id`/`score`/`highlights`/`snippet`/`kiln`/`block`.
   `storage/note_store.rs::SearchResult` carries a whole `NoteRecord` and a
   score. Fitting one inside the other needs an optional `NoteRecord` or an
   optional `document_id`, which the plan's design rules forbid.
5. `DocumentId` done: replaced with `String` in
   `crates/crucible-core/src/types/database.rs`. `rg -n "DocumentId" -t rust`
   found 9 files; all call sites now use the inner `String` directly.
6. `LlmProviderConfigBuilder` done: deleted from
   `crates/crucible-core/src/config/components/llm.rs`.
   `LlmProviderConfig` now derives `Default`, and every one of the roughly
   100 call sites (`rg -n "LlmProviderConfig::builder" -t rust` found 23
   files) builds the struct directly with `..Default::default()`.
   `with_api_key_env_var_name` had no caller outside its own doc example, so
   its replacement (`default_api_key_env_var`) went too once confirmed
   unread.
10. `ModeDescriptor.icon`/`.color` done in part: `.color` deleted — no writer
    ever set it to `Some` (`rg -n '"\.color\s*=|color:"'` found only the
    struct field and two deliberate `None`/`null` cases) and no reader
    exists in the daemon, the TUI or the web client. `.icon` stays: the web
    mode control (`ChatModeControl.tsx`) reads `mode.icon` even though no
    Rust path yet sets it to `Some`, so it is not unread. Regenerated
    `openapi.json` and `api-schema.d.ts` with `just web-contract`.
9. `EventRing` done: deleted `crates/crucible-core/src/events/ring.rs`; no
   caller outside its own tests named it. `ModelCapability`/`UnifiedModelInfo`
   done: deleted `crates/crucible-core/src/traits/provider.rs` whole, since
   every item in it was read only by its own tests. `EventError` done: no
   real `EventEmitter` implementation (`DaemonEventBridge`, `Kept` in
   `indexing.rs`'s tests) ever returned `Err`, and the one caller that
   matched `Err` (`indexing.rs`) matched it only to log — fail-open by
   design. `EventEmitter::emit` and `emit_recursive` now return
   `EmitOutcome<E>` directly. `emit_recursive` done: deleted from the trait
   and every impl; `rg -n "\.emit_recursive\("` found only its own two
   tests as callers.

## Step 15. Luau types from the schema

**Change.** Add `LuaType::from_json_schema` in
`crates/crucible-lua/src/signature.rs` and a `Json<T>` binding wrapper, so
a binding's declaration comes from the core schema. `cru.d.luau` gets one
`export type` for each core type that Lua reaches. Delete the hand Luau
contract types (`PERMISSION_REQUEST`, the `ToolResult` copies,
`Interception`, `OilStyle` and the six style fields that `OilProps` repeats)
and the Lua `PermissionRequest` view.

**Done when.**
- `cru.d.luau` holds a generated `export type` for each core type that Lua
  reaches, and the listed hand Luau types and the Lua `PermissionRequest`
  view are gone.
- A change to a field of a core type changes the Luau declaration with no
  hand edit, and `luau-lsp` reports a plugin in `runtime/` that reads a
  removed field. This is shown once by a deliberate break.


## Step 16. The last TS copies

**Change.** Replace the remaining hand TS wire types (`GrepHit`,
`SemanticHit`, the Bases and file request types, four `api.ts` types) with
aliases of the generated schema. Keep one `rel_path` mapper, or none.

**Done when.** The listed TS types are gone, and each is an alias of the
generated schema. A field that the daemon renames makes `tsc` fail at each
reader.


## Step 17. One daemon test fixture

**Status: done.** The two daemon test files that bound their own daemon use
the shared `InProcessDaemon`, and the two plugin-bridge rigs are one `Rig`.
The other local fixtures build different things, so they stay.

## Step 18. Enums on the wire

**Change.** Use the existing enums, not strings, for `session_type`,
`agent_type`, `recording_mode`, `state`, `FsPathRequest.kind` and
`FsMoveRequest.kind`. The generated TS and Luau types then hold literal
unions.

**Done when.** The generated TS and Luau types hold literal unions for these
fields, so a misspelled value fails `tsc` or `luau-lsp`. Old stored values
still load, shown by a fixture captured before the change.


## Step 19. One RPC route for the web

**Now.** The web server has 119 routes and 156 Rust types. About 75 routes
only forward one daemon RPC, and about 87 functions in `services/daemon.rs`
do the forwarding. The frontend has 104 functions in `lib/api.ts` and 128
types in `lib/api.ts` and `lib/query/`. Each new daemon method needs a
route, route types, a forwarding function, a TS function and TS types.

**Prior art.** Systems with a process or language boundary use one RPC
channel with many typed methods, not a route per operation: T3 Code (one
authenticated channel, one `RpcGroup` of typed methods from a shared
contract), LSP (one JSON-RPC channel, one meta-model generated into many
languages), Zed (one protobuf RPC). Joplin wraps its REST API in four
generic calls for its plugins. The type defines the operation, not the
route.

**What the web itself needs.**

| Need | Decision | Reason |
|---|---|---|
| Login and logout (the cookie session), static files, health and ready | keep | Browser transport; the daemon has no part in it |
| The terminal WebSocket | keep | Raw PTY bytes, with its own stricter access rule |
| Raw file serving | keep | The MIME, CSP and `Content-Disposition` rules stop a served file from running script on the app origin |
| Four SSE streams (chat, system, fs, surfaces) | one stream | The client subscribes to topics; the daemon already has topic subscription |
| The catalog cache (`services/catalog.rs`, `SwrCache`) | move to the daemon | The daemon knows when a catalog changes |
| Layout and recents on the web server's disk | one generic client-state get/set in the daemon | They need no routes of their own |
| The untrusted-root rule of `project.register`, the reference containment of a canvas write, the name and size checks of a note write, the resume fallback | move to the daemon | A TUI or Lua caller skips them today; AGENTS.md puts a decision in the daemon |

**Change.**
1. One authenticated route, `POST /api/rpc/{method}`, behind the existing
   auth middleware and the origin checks. It forwards the body to the daemon
   method and returns the reply.
2. An allow list of the methods a browser may call. Local-admin methods stay
   off it: `shutdown`, `lua.eval`, the Lua lifecycle and test methods,
   `plugin.install`/`remove`, the `config.*` writes, `storage.*`, `mcp.*`,
   `kiln.register`/`forget`, `project.register`/`unregister`,
   `llm.register_provider`, `webhook.receive`. The allow list is the
   security boundary; the opaque route is not.
3. A second allow list per caller. A plugin block is same-origin script
   today, and its `x-crucible-plugin` identity is asserted, not proved
   (`routes/plugin_caller.rs`). So a block can call what the app can call,
   as it can reach every route today. The proxy does not make this worse.
   When blocks run in a sandboxed origin behind a bridge that stamps their
   identity (`docs/Meta/Analysis/Plugin API Plan.md`), the per-caller list
   lets a block call only its own plugin's methods.
4. A generated typed client: `rpc<M>(method, params)`, from the typed
   `rpc_methods!` rows of step 12, with a generated method map in TS. The
   per-route TS functions and types go.
5. Do the decisions in the table above: one event stream, the catalog cache
   and client state in the daemon, the four rules in the daemon.
6. Map daemon error codes to one error shape in the browser, in one place.

**Done when.**
- The web server has the routes in the "keep" rows, one event stream and the
  RPC route, and no route that only forwards one RPC. About 90 types and
  about 70 TS functions are gone.
- The change cost of a browser method falls from about 9 places to 3 (plus
  one allow-list line), measured by adding a scratch method.
- Each local-admin method answers 403 through the real HTTP route.
- The four moved rules refuse a bad call from the TUI and from Lua too,
  tested through the daemon RPC, not only through the web.
- The web end-to-end tests pass through the one route and the one stream.


## Step 20. Plugin data schemas

**Now.** Plugin data reaches the clients in two ways. Presentation uses
fixed core vocabularies that both clients draw: surfaces (the core `Shape`
enum), interactions (ask, popup, panel) and status items. Plugin data is
untyped: command arguments and results, publications and options are
`Value`. Only tool parameters are declared, as Luau types that `LuaType`
(`crates/crucible-lua/src/signature.rs`) already turns into JSON Schema.

**Prior art.** VS Code declares a JSON Schema for each contributed setting;
Joplin registers a typed `SettingItem` for each key.

**Change.**
1. Presentation: a plugin composes the core shapes. It never adds a wire
   type that a client must know, so the core type count does not grow with
   the plugins.
2. Data: a plugin declares the shape of its commands, publications and
   options as Luau types in its spec. The daemon turns each into JSON Schema
   with `LuaType`, checks each value at the boundary, and publishes the
   schemas through one method, `plugin.schemas`.
3. The generic clients validate against the schema or render from it. A
   plugin's own web block can generate its TS types from the schema in the
   plugin's build, not in core.
4. Plugin data flows through the generic methods (`plugin.run_command
   { plugin, name, args }`, publications, options). A plugin adds no RPC
   method.

**Done when.**
- A plugin in `runtime/` declares a command's argument and result shapes in
  Luau, and the daemon refuses a wrong argument with an error that names the
  field, through the RPC path that the TUI and the web both use.
- The web client reads a plugin's schema from `plugin.schemas` and uses it,
  and no core type was added for that plugin.
- The change cost of a plugin data shape is one Luau declaration.

## Step 21. Owner decisions

Each item changes behavior or a plugin API. None starts without the owner's
decision.

| Item | Deletes | Cost |
|---|---|---|
| The fullscreen multi-pane prototype (`FullscreenShell`, `ChatPane`, `PluginBuffer`) | 3, with their tests, bench and example | No production path reaches it |
| Popup requests become single-select panels | `PopupRequest`, `PopupResponse` | Changes `cru.ui.popup` and both client renderers |
| An ask becomes a batch of one | `AskRequest`, `AskResponse` | Changes `cru.ui.ask` and both client renderers |
| The workflow engine emits `WorkflowPayload` | `WorkflowEvent` | One translator reads it |
| `daemon.capabilities` drops its constant flags | `CapabilityFlags` | An outside client may read them |

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
- The web server's own transports: authentication, the SSE streams, the
  terminal WebSocket and static files. Step 19 removes the routes that only
  forward one RPC; it keeps the browser boundary, which is now one
  authenticated RPC route with an allow list.
- A missing embedding provider and a failed one. They have different
  failure policies.

See also [[Consolidation Plan]] for the decisions that earlier cleanups
recorded.

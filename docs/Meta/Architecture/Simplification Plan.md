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
| 11. One event vocabulary to the browser (done: Rust −1, TS −10; the hand copies became generated aliases) | `ChatEvent` and 2 more web types, 29 TS copies: about 32 | M | step 10 |
| 12. One request body per shape (in part: 33 types went; 4 web bodies and the typed `call` remain) | about 35: 19 web request copies, 6 core shape copies, 10 local `Params` | M | step 10 |
| 13. One generic knob | about 11 types, 10 RPC methods, about 90 per-knob functions | M | step 12 |
| 14. Simpler core APIs and the audit list | about 40 | S each | step 12 |
| 15. Luau types from the schema | about 8 | M | step 12 |
| 16. The last TS copies (done) | about 16 | S | step 11 |
| 17. One daemon test fixture (done) | 3 test types | S | none |
| 18. Enums on the wire (in part: `FsRootKind` and `ContextStrategy` done; `session_type`/`agent_type`/`recording_mode`/`state` remain) | about 0, string fields become enums | S | step 12 |
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
- The web backend re-encodes each event as `ChatEvent` (deleted in step 11;
  the SSE route now forwards the daemon's own `{event, data}` pair — see
  `crates/crucible-web/src/routes/chat.rs`).
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

**Status: done.** The deleted `ChatEvent` enum used to re-encode each
`SessionEventPayload` into 21 variants of its own; the deleted
`normalize_interaction` function flattened a permission request;
`lib/types.ts` copied 29 wire types by hand.

`ToSchema` (behind the `openapi` feature) now reaches the event payload
groups (`protocol/session_events/*.rs`), the interaction types
(`interaction/*.rs`) and the tool-call types they reference
(`CanonicalToolCall`, `ToolRender`, `RawToolCall`, `FileDiff`). The
`event_payload!` macro gained a per-variant `#[schema(rename = ...)]`, needed
because `utoipa`'s adjacently-tagged schema does not see a per-variant
`#[serde(rename = ...)]` whose literal came from a macro substitution.
`crates/crucible-web/src/routes/chat.rs`'s `to_sse` sends the daemon's own
`{event, data}` pair — no re-encoding — and the route's `#[utoipa::path]`
names `ChatSseFrame` (`SessionEventPayload | TranscriptFrame`), so
`openapi.json` and the generated `web/src/lib/api-schema.d.ts` describe the
real wire union. `ChatEvent`, `ChatEvent::from_daemon_event`,
`normalize_interaction`, `PrecognitionNote` and the web-local
`SessionHistoryEvent` struct are deleted; `crucible_core::protocol::rpc::SessionEventMessage`
(now `ToSchema`) replaces the last one. `web/src/lib/types.ts` aliases the
generated `SessionEvent`/`TranscriptFrame`/`InteractionRequest`/
`InteractionResponse`/`CanonicalToolCall`/`ToolRender`/`RawToolCall`/`FileDiff`
unions instead of copying them by hand. `PermRequest.pattern` (the "always
allow" suggestion) and `MessageComplete.stop_notice` (the stop-reason
wording) are now fields the daemon fills in once —
`SessionEventMessage::interaction_requested` and
`SessionEventMessage::message_complete` — rather than values the web layer
re-derived from a flattened request or a copied wording table.

`web/src/contexts/chatEventReducer.ts` now switches on
`SessionEventPayload['event']` directly (no `SessionEvent` passthrough
wrapper) and ends in an exhaustive `never` check, so a variant added to any
of the eight payload groups fails `bun run typecheck` — in the reducer and
in `web/src/lib/api.ts`'s `SSE_EVENT_TYPES` completeness check — until it is
named. Proved once with a scratch `"scratch_demo_event"` variant added to
`SystemPayload`, regenerated, observed to fail both checks, then reverted.

**Proof.** `routes::chat::tests::a_live_event_sends_its_transcript_ops_as_a_second_frame`
pins the live SSE wire shape byte for byte. The reducer's exhaustive switch
and `SSE_EVENT_TYPES`'s completeness check are the generated-union proof;
`web/src/contexts/chatEventReducer.test.ts` and
`web/src/lib/__tests__/api-schema.test.ts` exercise them. `just web-contract`
and `bun run typecheck` are clean; the full frontend suite (305 files, 3453
tests) and the full Rust workspace test suite pass.

**Done when.**
- `ChatEvent`, `from_daemon_event`, `normalize_interaction`,
  `PrecognitionNote`, `SessionHistoryEvent` and the hand TS field lists are
  gone. Measured: the count fell by 11 (Rust −1, TS −10), not 32, because an
  alias of a generated type still counts as one declaration.
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

**Status: done.**

**Was.** Each of the five `SessionKnob` values (model, mode, context
strategy, precognition, plugin turn limit) had its own RPC method pair, core
request type, handler, client method, web route pair, web request and
response types, TUI message and TS hook. Mode sat on `AgentHandle`, not on
`SessionKnobs`. The AGENTS.md cross-layer checklist existed because of this.

**Change.** One RPC pair, `session.knob.set` and `session.knob.get`, in
`crates/crucible-core/src/protocol/rpc/method.rs`. The set body IS
`crucible_core::types::KnobValue` — an adjacently tagged enum (`{"knob":
"model", "value": "…"}`) with one variant per knob, so the tag names the
knob and the daemon cannot receive a value shaped for the wrong one. The get
body is `KnobRef { knob: SessionKnob }`, and the reply is a `KnobValue`. The
daemon's `handle_session_knob_set`/`handle_session_knob_get`
(`crates/crucible-daemon/src/server/session/params.rs`) refuse a knob
`on_acp` marks `Absent` for an ACP session once, in one place, before
dispatching to each knob's own apply logic (kept exactly as it was: the ACP
live-handle path for model, the alias resolution and deferred-apply for
mode, the string parse for context strategy). The web gets one route pair,
`PUT /api/session/{id}/knob` and `GET /api/session/{id}/knob/{knob}`
(`routes/session/mod.rs`'s `set_knob`/`get_knob`), using the core types
directly — no web-side request/response types. The TUI's `:set` and
`ModeChanged` (`:mode`, Shift+Tab) build a `KnobValue` and send it through
one `ChatAppMsg::SetKnob`. Deleted: the ten per-knob RPC methods
(`SessionSwitchModel`, `SessionSetMode`/`GetMode`,
`SessionSetContextStrategy`/`GetContextStrategy`,
`SessionSetPrecognition`/`GetPrecognition`,
`SessionSetPluginTurnLimit`/`GetPluginTurnLimit`, and `rpc_set_method`'s
whole reason to exist), their five core request types, the nine daemon
handlers plus the two `dispatch_session_setter!`/`dispatch_session_getter!`
macros, the nine `DaemonClient` methods plus `get_session_option`, the nine
web route handlers plus their nine request/response types
(`session_config/prompt.rs` deleted outright), and four of the five
per-knob `ChatAppMsg`/`SetRpcAction` variants (mode keeps `ModeChanged`,
since Shift-Tab and `:mode` are not `classify_set_value` call sites).

**Done.**
- **Gone** (measured with the counts in "How a step is accepted"): Rust
  struct/enum declarations in `crates/{core,daemon,web,cli}/src` went from
  1550 to 1538 — core 635→632 (net of 5 removed request types and the 2
  added, `KnobValue`/`KnobRef`), daemon 531→531 (no type changed, only
  functions and macro invocations), web 136→127 (9 request/response types
  deleted, none added), CLI 248→248 (variants collapsed inside existing
  enums, not a type count). The ten RPC methods, the five core request
  types, the nine `DaemonClient` client methods, the nine web route
  handlers and their nine types, and six TS hooks
  (`useGetPrecognition`/`useSetPrecognition`/`useGetContextStrategy`/
  `useSetContextStrategy`/`usePluginTurnLimit`/`useSetPluginTurnLimit`) no
  longer exist. `crucible-web`'s `services/daemon_session_config.rs` and
  `routes/session_config/prompt.rs` are deleted files.
- **Cheaper to change.** Verified on a scratch branch (deleted after
  measuring): a sixth knob, `plugin_priority` (a `u8`), needed the three
  places the rewritten cross-layer checklist names — the `SessionKnob`/
  `KnobValue` variant and its `on_acp` arm in
  `crates/crucible-core/src/types/knob.rs`; the apply arm in
  `handle_session_knob_set`/`get` plus a getter/setter on `AgentManager`
  (two files, one bullet: "the daemon's apply arm"); and a `:set` key in
  `tui/oil/commands/set.rs` plus its local-mirror decision in
  `command_handling.rs` (again two files, one bullet: "a client control").
  No RPC method, no web route, no client method and no new message were
  touched. The scratch knob needed two more production files —
  `agent_manager/mod.rs` and `residue.rs` — but only because it invented
  new per-session daemon state to hold its value; the five real knobs all
  reuse a field that predates the knob system (`SessionAgent::model`,
  `::mode`, `::precognition_enabled`, `::context_strategy`,
  `Session::plugin_turn_limit`) and need no such field, so a knob that
  reuses existing storage is 3 files, and one that needs new per-session
  daemon state is 5. Three test files also needed the new variant
  (`acp_session_knobs_e2e.rs`, `test_support.rs`'s mock daemon match, and
  the TUI reachability gate's `TUI_SET_KEYS` row) — expected of the "Proof"
  step, not counted against the places a person edits to add the setting
  itself. Before step 13: about 15 (the cross-layer checklist this
  replaced).
- **Same behavior on every path.** `rpc_config_agent_e2e.rs`'s
  `all_config_knobs_round_trip_over_the_wire` and
  `rpc_integration/models.rs`'s switch-model/set-mode round trips prove
  set/get/resume through the real socket. The knob RPC matrix
  (`crates/crucible-cli/src/tui/oil/chat_runner/tests/knob_rpc.rs`) proves
  every knob's `:set` key (and `ModeChanged`) reaches `session.knob.set`
  carrying the matching `knob` tag, not just the matching method name — the
  exact miswiring class the matrix exists to catch, which a method-name-only
  check can no longer catch now that every knob shares one method.
  `crucible-web`'s `session_config/tests.rs` and `openapi_contract.rs` prove
  the one web route pair for all five. `session_bridge.rs`'s Lua
  `set_mode` calls `AgentManager::set_mode` directly, the same method
  `session.knob.set` calls; no other `cru.session` function writes a knob.
- **The AGENTS.md cross-layer checklist** is replaced by the three steps in
  its "Cross-layer checklist" section: the `SessionKnob`/`KnobValue` variant,
  the daemon's apply arm, and a client control if one is wanted.


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
   plan's design rules forbid. The TS `TokenUsage`
   (`crucible-web/web/src/lib/types.ts`) stays hand-written too, but for a
   different reason: its own doc comment already says it is client-local —
   `itemToMessage` builds it from a transcript segment's `usage` and nothing
   sends it back over the wire, so there is no generated schema for it to
   alias.
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
9. `EventRing` done: deleted (formerly `events/ring.rs` in `crucible-core`);
   no caller outside its own tests named it. `ModelCapability`/`UnifiedModelInfo`
   done: deleted (formerly `traits/provider.rs` in `crucible-core`) whole,
   since every item in it was read only by its own tests. `EventError` done: no
   real `EventEmitter` implementation (`DaemonEventBridge`, `Kept` in
   `indexing.rs`'s tests) ever returned `Err`, and the one caller that
   matched `Err` (`indexing.rs`) matched it only to log — fail-open by
   design. `EventEmitter::emit` and `emit_recursive` now return
   `EmitOutcome<E>` directly. `emit_recursive` done: deleted from the trait
   and every impl; `rg -n "\.emit_recursive\("` found only its own two
   tests as callers.
2. `LlmToolDefinition`/`FunctionDefinition` done: deleted from
   `traits/llm.rs`. No caller read `r#type` (always `"function"`) or
   serialized the wrapper to the wire; every caller converted it straight to
   `genai::chat::Tool`. `crucible-daemon/src/provider/tool_bridge.rs`'s
   `llm_tool_to_genai` now takes a `traits::tools::ToolDefinition` directly,
   and `agent_factory.rs`/`provider/genai_handle.rs` build/hold
   `ToolDefinition` throughout. Also deleted `ToolDefinition`'s unread
   `category`, `returns`, `examples`, `required_permissions` fields, the
   `ToolExample` type, and `with_category`/`with_permission`: `rg` found no
   reader for any of them outside their own tests.
3. `ChatToolResult`'s `result`/`error` strings: skipped.
   `crucible-daemon/src/provider/genai_handle.rs::tool_response_payload`
   keeps both when a tool exits non-zero with output already produced
   ("Content AND an error: a non-zero exit with output on the way is still
   output. Keep both rather than choosing."). `ToolResultBody` is untagged
   `Ok { result } | Err { error }`, decided by which key is present — it
   cannot carry both without inventing a shape neither variant is, which
   the plan's design rules forbid.
13. The TS `PermissionScope` alias: skipped this session. The Rust
    `PermissionScope` (`crucible-core/src/interaction/permission.rs`) lives
    beside `PermRequest`/`PermResponse`, the interaction types step 11 (web
    events, interaction types, reducer) owns and another agent was changing
    at the same time. Touching it here risked a collision in exactly the
    file that step is working through; it is left for that step or a
    follow-up pass.
8. `ConfigValidationError` into `ConfigError` done. Its `MissingField`
   became `ConfigError::MissingValue` (same shape); its
   `InvalidValue { reason }` became `ConfigError::InvalidValue { value }`
   (same two-string shape, `reason` text moved to `value`). The `Multiple`
   variant was never built (`rg -n "ConfigValidationError::Multiple"` found
   only the match arm that handled it, no constructor), so it has no
   `ConfigError` counterpart. `EnrichmentBackendConfig::validate` and the
   daemon's `EmbeddingError` conversion now use `ConfigError` directly.
7. The 32 named `SessionEventMessage` constructors: skipped this session.
   `Self::typed(session_id, payload)` already exists
   (`protocol/rpc/mod.rs`) and the named constructors are thin wrappers
   over it; deleting them touches roughly 104 call sites across the RPC,
   agent-manager and daemon-server layers that several other agents
   (`audit-rpc`, `audit-rpc-agentmgr`, `wire-compat-mapper`) were actively
   changing at the same time this session ran. Left for a dedicated pass
   once that work lands.
11. `ModeStance` (`crucible-lua/src/modes.rs`) into core `PermissionMode`
    done. Same three variants, same strings (`rg` confirms
    `crucible_core::config::PermissionMode`'s `FromStr`/`Display` use
    `"allow"`/`"deny"`/`"ask"`, matching `ModeStance::parse`/`as_str`
    exactly), same default (`Ask`). `ModeStance` is gone, with no
    alias: `crucible-lua` and the daemon name `PermissionMode`.
12. The private `Unprompted` enum (`gate_decision.rs`) into
    `PermissionDecision`: skipped. `Unprompted::Ask(layer: String)` carries
    which layer is asking, read as `PermRequest.layer` and shown to the
    user; `PermissionDecision::Ask { rule_matched: bool }` carries whether
    an explicit `ask` rule matched, read only inside the permission
    engine's own read-only auto-approve exemption. Every `Unprompted::Ask`
    site outside the permission engine (a card `ask`, a plugin `ask`, a
    mode-stance `ask`) has no rule to have matched, so `rule_matched` would
    carry a fabricated value there, not a widened optional field. `Allow`
    and `Deny` fold cleanly (`Deny { reason }` already matches
    `Unprompted::Deny(String)`; `Allow` would gain one optional
    `provenance: Option<String>` field), but folding `Ask` too would give
    two thirds of its call sites a meaningless bit rather than an absent
    optional value, which is not the same thing the plan's design rules
    allow.

## Step 15. Luau types from the schema

**Status: done, with two of the four listed hand types kept and their
reason recorded.**

**Change.** `LuaType::from_json_schema` (`crates/crucible-lua/src/signature.rs`)
reads a `utoipa` JSON Schema (a `$ref` to `Named`, `oneOf`/`anyOf` — with the
two-branch "one side is `null`" case read as `Optional`, not a union — to
`Union`, an `object` with `properties` to `Record` with `required` deciding
`Optional`, `additionalProperties` to `Map`, a string `enum` to a
[`LuaType::Literal`] union, an array to `Array`). `LuaType::of_schema::<T>()`
calls it over `T::schema()`. `crucible-lua` depends on `utoipa` directly and
turns on `openapi` on its `crucible-core` and (new) `crucible-oil`
dependencies — the smallest wiring that reaches both crates' schemas, ahead
of building a schema registry nothing else needs yet.

`Json<T>` (`crates/crucible-lua/src/json_binding.rs`) is the binding
wrapper: `FromLua`/`IntoLua` go through `Lua::to_value`/`from_value`, and its
`LuauValue` impl declares `T::ty()` — `LuaType::of_schema::<T>()` — so a
binding written `Json<crucible_oil::Style>` needs no declaration text.

Of the plan's four hand types:
- **`PERMISSION_REQUEST`** is gone. Its declaration is now
  `LuaType::of_schema::<handlers::permission::PermissionRequestPayload>()`,
  a small schema-only struct (`handlers/permission.rs`) whose field list is
  the one the hook table (`PermissionRequest::into_lua`) also builds;
  `payload_contract::the_permission_payload_matches_its_declaration` still
  compares the two, but there is one field list to edit, not two. The Lua
  `PermissionRequest` view itself (the struct with `call`, `args`, `is_safe`,
  `mode`) is **kept**: no core type carries `is_safe`/`mode`/raw `args`, and
  the closest, core `PermRequest`, is a different object built later, for a
  different reader (see the struct's own doc comment) — folding them would
  give `PermRequest` two fields no other builder of it has a value for.
- **`OilStyle`** is gone. It is now `LuaType::of_schema::<crucible_oil::Style>()`
  (`crucible-oil` gained an `openapi` feature; `Color` gets a hand
  `PartialSchema`/`ToSchema` reading as a plain string, since
  `Color::parse` is the one reader every caller uses and a derived tagged-enum
  schema would describe a shape no caller may write). This closed a real
  drift: the hand declaration and `oil::parse::style_from_table` both named
  six of `Style`'s seven fields, missing `reverse` — a plugin that wrote
  `{ reverse = true }` got no type error and no effect. Both now read
  `reverse`. `OilProps` is `OilStyle & { gap, padding, margin, border,
  justify, align }`, an intersection, not a second copy of the style fields.
- **The `ToolResult` copies** (four `runtime/` plugins) are gone, replaced
  by one `export type ToolResult = { [string]: any }` in `cru.d.luau`
  (`host_api::OIL_TYPES_STATIC`). Not schema-generated: no Rust type
  describes it — it is what a Lua tool handler answers with, `{ error =
  "..." }` or its own result shape, and the daemon reads only `error`
  before treating the rest as opaque JSON. Merging four copies into the one
  declaration every plugin already sees ambiently removes the duplication
  without inventing a schema for "arbitrary JSON, except this one string
  key."
- **`Interception`** (`runtime/plugins/oci/init.luau`) is **kept**. The
  closest core-adjacent type, `ScriptHandlerResult::Handled { result:
  JsonValue, terminate: bool }` (a `cru.on` event handler's answer, a
  different hook), carries a decoded `result` and a `terminate` flag this
  pre-tool-call interception's `{ handled: boolean, result: string }` has no
  counterpart for; folding them would give `terminate` a fabricated value at
  every `oci` call site. The type's doc comment now records this.

**Gone** (measured with the plan's own count commands): the Luau side goes
from 27 `type`/`export type` declarations under `runtime -g '*.luau'` to 23
— the four `ToolResult` copies, and no new plugin-local type. `PERMISSION_REQUEST`,
`OilStyle`'s and `OilProps`'s hand style fields were hand Luau **strings**
inside `crates/crucible-lua/src/host_api.rs`, not under `runtime/`, so the
plan's `runtime`-scoped count does not see their removal; they are gone from
`host_api.rs` itself (`rg -n 'fg: string?' crates/crucible-lua/src/host_api.rs`
finds nothing). The Rust struct/enum count under `crates/*/src` goes from
1770 to 1772: `PermissionRequestPayload` and `Json<T>` are the two additions,
needed to let two hand strings and a hand table's 12-field duplicate become
one field list each; nothing else changed shape.

**Cheaper to change.** Renamed `PermissionRequestPayload::is_safe` to `safe`
on a scratch change (`crucible-lua/src/handlers/permission.rs`), leaving the
table `PermissionRequest::into_lua` builds unchanged: `cargo test -p
crucible-lua --lib` failed immediately —
`payload_contract::the_permission_payload_matches_its_declaration` reported
the built table still names `is_safe` while the schema now names `safe`,
with no generated file involved. `just plugin-check` then reported the
real drift a plugin author would see:
`runtime/defaults/init.luau(189,37): TypeError: Key 'is_safe' not found in
table 'PermissionRequest'` — the one shipped file that reads
`request.is_safe` (`cru.permissions.on_request`'s built-in `plan`-mode
policy), against the regenerated declaration, with no hand edit to
`host_api.rs` on either side of the rename. `crucible_oil::Style` has no
shipped `runtime/` plugin to break the same way today (`cru.oil` is
UI-only, so no daemon-run plugin builds an `OilStyle` table), which is why
the measured rename used the permission payload instead. Both edits
reverted after measuring.

**Done when.**
- `cru.d.luau` holds a generated `export type` for `PermissionRequest` and
  `OilStyle`, read from their Rust types' own schemas. `PERMISSION_REQUEST`,
  the four `ToolResult` copies, and the hand `OilStyle`/`OilProps` style
  fields are gone. `Interception` and the Lua `PermissionRequest` view stay,
  each with the reason recorded above.
- A change to a field of `crucible_oil::Style` (or `PermissionRequestPayload`)
  changes the Luau declaration with no hand edit, and `luau-lsp` reports
  every plugin in `runtime/` that reads a removed field — shown once, above,
  by a deliberate rename.


## Step 16. The last TS copies (done)

**Change.** Replace the remaining hand TS wire types (`GrepHit`,
`SemanticHit`, the Bases and file request types, four `api.ts` types) with
aliases of the generated schema. Keep one `rel_path` mapper, or none.

**Done when.** The listed TS types are gone, and each is an alias of the
generated schema. A field that the daemon renames makes `tsc` fail at each
reader.

**Outcome.** `GrepHit`, `SemanticHit` and `GrepResponse` are schema aliases;
their camelCase mappers are gone, and `SearchPanel.tsx` reads `rel_path`/
`match_start`/`match_end` off the wire directly. `SystemEvent` ties its
fields to `PublicationChangedEvent`/`ProposalChangedEvent`; it keeps the
`event` tag by hand because the daemon's type is `#[serde(untagged)]` and the
document cannot describe an SSE frame name. `AppConfigControls` and
`PluginOptions` stay hand-written — `ConfigResponse.controls` is
`serde_json::Value`, so no generated shape exists to alias; this is the
plugin-vocabulary gap Step 20 owns. In `lib/query/bases.ts`, eight of the
nine listed types were already aliases; `BaseRequest` stays a client-local
discriminated union (the design rules forbid merging its `path`/`yaml` OR
into the wire's two-optional-fields shape) with its field types now drawn
from the generated operation parameters. In `lib/query/fs.ts`,
`SaveFileParams` is now a schema alias; `DirRequest`/`FsMoveParams`/
`FsPathParams` stay client-local hook-parameter shapes, each documented and
each keeping the one mapper the rule allows. See the Web Server page's
Findings for the measured counts and the drift proof.


## Step 17. One daemon test fixture

**Status: done.** The two daemon test files that bound their own daemon use
the shared `InProcessDaemon`, and the two plugin-bridge rigs are one `Rig`.
The other local fixtures build different things, so they stay.

## Step 18. Enums on the wire (in part)

**Change.** Use the existing enums, not strings, for `session_type`,
`agent_type`, `recording_mode`, `state`, `FsPathRequest.kind` and
`FsMoveRequest.kind`. The generated TS and Luau types then hold literal
unions.

**Done when.** The generated TS and Luau types hold literal unions for these
fields, so a misspelled value fails `tsc` or `luau-lsp`. Old stored values
still load, shown by a fixture captured before the change.

**Outcome (in part).** `FsPathRequest.kind`, `FsMoveRequest.kind` (now
`FsRootKind`) and `KnobValue::ContextStrategy` (now `ContextStrategy`) are
done, on the wire and in the generated TS. `ContextStrategy` gained
`#[serde(rename_all = "snake_case")]` with a `#[serde(alias)]` on each
variant, because its derive's old output (`"Truncate"`/`"Summarize"`) is
what every persisted `SessionAgent.context_strategy` actually holds; a
fixture captured before the change proves those records still load. An
unknown `FsPathRequest.kind`/`FsMoveRequest.kind` now fails 422 at the
`Json<T>` extractor, before `resolve_root`'s own message — an accepted,
tested behavior change (see the Core Domain Types and Web Server pages'
Findings).

`session_type`, `agent_type` and `recording_mode` on `SessionCreateRequest`,
and the filter fields `session_type`/`state` on `SessionListRequest` and
`SessionListPersistedRequest`, are **not done**. The two list/persisted
filters are documented to tolerate an unknown value ("the daemon ignores a
type or a state it does not know") so a caller newer than the daemon is not
refused; an enum field cannot do that without a lossy catch-all variant,
which is a design decision this step should not make silently. `agent_type`
has no core enum: `session::types::agent::SessionAgent.agent_type` is a
`String` compared against `"acp"`/`"internal"` in `types/mode.rs` and in the
ACP branch of `server/session/create.rs`, and moving the request field alone
would leave the two sides of one concept typed differently. This is a
larger, separate change — introduce `AgentType`, retype `SessionAgent`, and
migrate its call sites — left for a follow-up step.


## Step 19. One RPC route for the web

**Status: part A done (typed `rpc_methods!` rows); gap 1 of part A closed;
the route itself (parts 1-6 of the change below) is not started.**

**Status: part A done (typed `rpc_methods!` rows); gap 2 done for the
`session.*`, `lua.*`, `plugin.*`, `surface.*`, `config.*`, `ui.*`,
`notification.*`, `workflow.*`, `subagent.*`, `daemon.*`, `ping` and
`shutdown` rows (below); "What the web itself needs"'s four moved rules and
two moved stores are done (below); the route itself (parts 1-6 of the
change below) is not started.**

**Status: the "one event stream" decision of part 5 is done.** `GET
/api/events` (`crates/crucible-web/src/routes/events.rs`) replaces the four
former routes `GET /api/chat/events/{session_id}` (`routes/chat.rs`), `GET
/api/events/system` (`routes/events.rs`'s own former route), `GET
/api/fs/events` (`routes/fs.rs`) and `GET /api/surfaces/events`
(`routes/surface.rs`). A client names as many topics as it wants in
`?topics=a,b,...`: a session id for the chat stream, or `system` for
publications, proposals, surface changes and filesystem changes — the same
grouping the daemon's own event bus already used, since all four rode the
daemon's `"system"` session before this step. Every frame's JSON body gains
a `topic` field; the payload types are unchanged
(`SessionEventPayload`/`TranscriptFrame`, `FsEvent`, `SurfaceChangedEvent`,
`PublicationChangedEvent`, `ProposalChangedEvent`), matching the design
rule that only the envelope, not the vocabulary, may grow.

**Gone** (measured): the four routes and their handlers
(`chat::event_stream`, `fs::fs_event_stream`, `surface::surface_event_stream`,
`events::system_event_stream`), the `chat.rs` types `EventStreamQuery` and
`ChatSseFrame`, the `events.rs` types `SystemStream` and the `system_stream`
helper function, and the frontend's four independent `subscribeTo*`
`EventSource` constructions (`lib/api.ts`) and the e2e helper
`mockSSERoute`. Rust struct/enum count is unchanged, 1839 in `crates/*/src`
before and after
(`rg -c -t rust '^\s*(pub(\([a-z:]+\))? )?(struct|enum) [A-Z]' crates/*/src`):
the route reuses every existing payload type rather than adding a
one-struct-per-topic table (`EventsQuery`, `EventsFrame` and the small
per-frame builders are new, offset by the four deleted route-local types).
TS interface/type count is unchanged, 697 before and after
(`rg -c '^\s*(export )?(interface|type) [A-Z]' crates/crucible-web/web/src
-g '*.ts' -g '*.tsx' -g '!api-schema.d.ts' -g '!rpc-methods.d.ts' -g
'!**/__tests__/**'`): the deleted `SideChannelListeners` type is offset by
the new `EventsTopicHandlers` interface the shared connection needs, and
`ChatEvent`/`SequencedChatEvent`/`FsEvent`/`SurfaceChangedEvent`/
`SystemEvent` are untouched.

**Same behavior on every path, tested through the one stream.** Each
behavior the four streams had, and where it lives now:
- **Per-session resume cursor.** `?after=` becomes `topic:seq` pairs,
  comma-separated (`sess-1:5,sess-2:9`); the `Last-Event-ID` header (a
  browser's own retry, which cannot set a query string) carries one
  `topic:seq` pair the same way. `events_stream`'s per-topic `session_events_after`
  replay and its `floor` gap filter are unchanged in kind, just keyed per
  topic instead of once per route. Tested by
  `route_contract_tests/chat.rs::chat_events_replays_past_the_cursor_and_stamps_seq_ids`
  and `::chat_events_accepts_the_last_event_id_header_as_the_cursor`.
- **The stream-version handshake.** One `stream_version` frame opens the
  merged stream (not one per topic), and the `X-Crucible-Stream-Version`
  header is unchanged. Tested by
  `route_contract_tests/stream_version.rs`.
- **Subscribe-before-forward and the replay-gap floor reset.** Every named
  topic subscribes to the daemon's `EventBroker` before any topic's replay
  read runs, and a `stream_gap` on one topic's own channel resets only that
  topic's floor. Tested by
  `route_contract_tests/chat.rs::chat_events_skips_live_events_the_replay_already_covered`
  and the daemon-reconnect tests in `services/daemon_retry_tests.rs`.
- **The `system` topic's four projections and its own gap handling.**
  `FsEvent`, `SurfaceChangedEvent`, `PublicationChangedEvent` and
  `ProposalChangedEvent` each still decode from the raw daemon event the
  same way; `stream_gap` on the `system` topic is forwarded raw, not
  wrapped in `{event, data}`, exactly as `system_stream` did. Tested by
  `route_contract_tests/system_events.rs`.
- **Topic isolation.** A client subscribed to one session's topic gets no
  frame of another session, proved through the one route in
  `route_contract_tests/chat.rs::a_topic_carries_no_frame_of_another_session`,
  and through the shared connection's own dispatch in
  `lib/query/__tests__/sse.test.ts` ("the shared connection" describe
  block).
- **The frontend's one `EventSource`.** `lib/api.ts`'s `joinEventsTopic`
  owns one shared connection; a new topic joining or the last reader of a
  topic leaving rebuilds it with the updated `topics=` set, while a second
  reader of an already-carried topic (two chat panes, or the filesystem and
  surfaces panels both reading `system`) costs no new connection.
  `subscribeToEvents`, `subscribeToFsEvents`, `subscribeToSurfaceEvents` and
  `subscribeToSystemEvents` keep their old names and signatures, so
  `lib/query/sse.ts`'s four stream roots did not change their own public
  shape. Tested by `lib/query/__tests__/sse.test.ts` and the mocked
  Playwright tier (`crates/crucible-web/web/e2e`, the `ui` and `stories`
  projects), all passing through the one connection.

**Not verified in this pass:** the `e2e/live` Playwright tier (a real
`cru` binary and daemon) was updated to the new topic-occupancy assertions
but not executed end to end in this session; `just web-test live` should
confirm it before this status line is trusted as fully proven.

**Gap 2, done for one row set.** Of the 97 rows in that set, 64 named
`serde_json::Value` as their reply (plus 2 more — `ui.config` and
`session.set_agent_option` — whose *params* were `serde_json::Value`
because the handler read a raw `&Request` by hand). After this change, 11
reply-`Value` rows remain, each with a row comment naming why it stays
open:

- `config.get`, `config.origin`, `config.reset`, `config.pop`,
  `config.unset`, `config.effective`, `config.controls` — the reply
  embeds an arbitrary config value or a Lua-declared control tree.
- `ui.config` — the reply is the Lua-declared theme/highlight/geometry/
  layout snapshot (`ui.config`'s *params* are now typed).
- `lua.eval` — the reply is whatever the evaluated Lua returned.
- `subagent.collect` — each job answers with its own tool call's shape.
- `session.list_persisted` — already documented as deliberately open
  (a page of mixed session-summary shapes); unchanged by this pass.

Every other row in the set now names a core reply type in
`crucible_core::protocol::requests` (`session.rs`, `lua.rs`, `config.rs`,
`workflow.rs`, a new `ui.rs`). Two daemon-local types moved to core because
they already named only core types (`ConfigSaveReply`, and
`workflow_registry::WorkflowStatusSnapshot` → `WorkflowStatusReply`); one
function-local params struct moved to core
(`SessionSetAgentOptionRequest`). `session.reindex`, a retired stub that
never builds a reply, is typed `()`. `shutdown`'s reply was always a bare
JSON string, so it is typed `String`, matching `ping`. Wire compatibility
is proved in `crucible-core/src/protocol/requests/step19_gap2_wire.rs`:
each new type, filled with sample data, must serialize to the exact JSON
the pre-change `json!` call built (transcribed from the daemon source
before the type existed) and that JSON must still deserialize into the new
type. The other row groups — kiln, note, search, fs, base, diff, proposal,
storage, mcp, skills, agents, models, providers, embeddings, project, scm,
webhook, llm, embed, and the bare `session.*` storage methods — are a
separate pass.

**Open after part A.** One gap stays, and the route needs it closed:
1. 91 of 169 rows still reply `serde_json::Value`, so the generated TS map
   says `unknown` for them. Each needs a core reply type, as step 10 gave
   the other domains; a daemon-local reply type moves to core. The route is
   only as typed as these replies. **Update:** the `session.*`/`lua.*`/
   `plugin.*`/`surface.*`/`config.*`/`ui.*`/`notification.*`/`workflow.*`/
   `subagent.*`/`daemon.*`/`ping`/`shutdown` slice of these rows is typed
   (see "Gap 2, done for one row set" above); the generated TS map still
   says `unknown` for nearly all of them regardless, because
   `gen_rpc_methods_ts` only names a type that `api-schema.d.ts` already
   has a schema for, and `utoipa` only emits one for a type a live web
   route returns — which is parts 1-6 below, still not started. Typing a
   row's Rust reply and making the browser see it are two different
   gates; this pass closed the first for that row slice, not the second.

**Gap 1, closed.** A Rust call site was not bound to its row:
`DaemonClient::call<Req, Resp>` let the caller pick the types, so a caller
that disagreed with the row still compiled.
`crates/crucible-daemon/src/rpc_client/client/generated.rs` now generates
one typed client method per row, `rpc_<variant in snake_case>`, whose
signature is the row's own `Req`/`Resp` pair. `rpc_methods!` also emits
`#[macro_export] macro_rules! for_each_rpc_method`, an X-macro callback:
`crucible-core` cannot name `DaemonClient` (the daemon depends on core, not
the reverse), so it hands every row's data to a callback macro a crate that
*can* name the client supplies — `gen_rpc_methods` in `generated.rs`, which
expands the rows into one `impl DaemonClient` block. Functions, not types:
the struct/enum count did not grow (`rg -c -t rust
'^\s*(pub(\([a-z:]+\))? )?(struct|enum) [A-Z]' crates/*/src` stays at 1772
in this worktree's baseline, before and after) — one macro invocation
generates 169 methods, and the methods are named `rpc_<variant>` rather
than curated onto the bare name, so no facade type and no per-row
name-collision table were needed either. Two thin hand-written forwarders
that only passed a row's struct straight through
(`workflow_start`/`workflow_approve_gate`) were deleted and their two call
sites moved to the generated method; roughly 160 more hand-written methods
across `rpc_client/client/*.rs` were kept, because each does real work the
generated method does not (reshapes an ergonomic argument list into the
wire body, decodes/derives part of the reply, adds a retry or timeout
policy, or is called by name from `crucible-web`'s `forward_rpc!` macro) —
see [[RPC Client#Findings]] for the categorized accounting. **Compiler
proof:** `client.rpc_session_get(...)` (row: `Scoped<()> =>
SessionDetail`) read as a `String` fails `crucible-daemon`'s build with a
type mismatch, proved once and reverted. **Change cost, measured on a
scratch method** (`ScratchPing2`, added then reverted): 3 places — the row,
the dispatch arm, and the call site (`client.rpc_scratch_ping2(())`) — down
from part A's own 3-places baseline, because no wrapper method is needed
at all now. **A latent wire bug found and fixed along the way:** a
`()`-params row serializes to JSON `null`, but the daemon has always
required `{}` for a no-params method (`NO_PARAMS`'s own doc comment);
`DaemonClient::send_raw` now canonicalizes `null` to `{}` for every caller,
generated or hand-written, proved by a new live-server test
(`generated_method_of_a_no_params_row_reaches_the_daemon`). **Not
closed:** nothing stops a row's declared types from being edited to
something a dispatch handler no longer matches — that remains a
dispatch-side property, not a caller-side one; closing it fully still
needs either 169 marker types (disfavored) or rewiring dispatch through a
shared macro-generated helper, left to a follow-up, as part A already
recorded.

   **Part B, storage-side rows, done.** Of the 91, the storage-owned share —
   every `kiln.*`, `note.*`, `fs.*`, `base.*`, `diff.*`, `proposal.*`,
   `storage.*`, `mcp.*`, `skills.*`, `agents.*`, `models.*`, `providers.*`,
   `embeddings.*`, `project.*`, `scm.*`, `webhook.*`, `llm.*`, `embed.*`
   row, plus the bare `search_vectors`/`search_text`/`search_grep`/
   `list_notes`/`get_note_by_name`/`get_backlinks`/`process_file`/
   `process_batch`/`suggest_links` methods — is typed. 45 of these rows
   named `serde_json::Value` before this pass; 38 now name a real core
   type (`KilnOpenReply`, `StatusReply`, `KilnRegisterReply`,
   `KilnForgetReply`, `LlmRegisterProviderReply`, `Vec<FtsResult>`,
   `GrepSearchResponse`, `EmbedQueryReply`, `NoteUpsertReply`,
   `ProcessFileReply`, `ProcessBatchReply`, `ProjectOpenKilnsReply`,
   `ScmCloneResponse`, `FsListing`, `FsMoveReply`, `FsMkdirReply`,
   `FsTrashReply`, `NoteRenameReply`, `NotImplementedReply` (the four
   `storage.*` methods), `McpStartReply`, `McpStopReply`, `McpStatus`,
   `AgentProfilesReply`, `AgentCardsListReply`,
   `Option<AgentProfileResolved>`, `ModelsListReply`,
   `ProvidersListReply`, `WebhookReceiveReply`, `SuggestLinksReply`). 7 stay
   `Value`, each with the reason inline in `rpc_methods!`:
   `kiln.registry_list` and `project.registry_list` (a row is a real record
   or a hand-built stand-in for one, plus injected keys — no single struct
   names both starting shapes without an Option-per-field merge); `fs.read`
   and `fs.write` (one of five-plus mutually exclusive shapes chosen at
   runtime by a multi-way retry/merge/restore); and the six `base.*` rows
   (one handler answers six operations from a raw `&Request`, merging
   `req.params` with a resolved `kiln` key before a per-operation dispatch
   this pass does not restructure). `McpStatus`, `ScmCloneResponse`,
   `GrepSearchResponse`, `FtsResult`, `FsListing`/`FsMoveReply`/
   `FsTrashReply` (with `SkipReason`/`SkippedRef`), `SuggestLinksReply`,
   `WebhookReceiveReply` and `AgentProfilesReply`/`AgentProfileEntry` moved
   from `crucible-daemon` to core, with no daemon-side copy left (the
   daemon crate re-exports them). `crates/crucible-core/src/protocol/requests/golden_reply_tests.rs`
   and `assets/fixtures/golden/replies/*.json` pin each changed reply's
   wire JSON, captured from the code before this change. The session-side
   share of the 91 (session, lua, plugin, surface, config, ui,
   notification, workflow, subagent, daemon, ping, shutdown) is a separate
   pass.

**Part A, done.** Every row in `rpc_methods!`
(`crates/crucible-core/src/protocol/rpc/method.rs`) now names its params and
reply type: `Variant = "wire.name": Req => Resp`. The macro's grammar makes
a row with no types fail to compile, and a hidden
`ASSERT_ROW_TYPES_RESOLVE` const forces every named type to actually
resolve (proved once with a row renamed to a nonexistent type, observed to
fail `cargo check -p crucible-core`, then reverted). `RpcMethod::params_type`/
`reply_type` read a row's text back at runtime (`stringify!` of the macro
argument), which the new `crates/crucible-core/examples/gen_rpc_methods_ts.rs`
turns into `crates/crucible-web/web/src/lib/rpc-methods.d.ts` — one method
map entry per row, referencing `api-schema.d.ts`'s schema names where they
exist and `unknown` otherwise (`cargo run -p crucible-core --example
gen_rpc_methods_ts -- crates/crucible-web/web/src/lib/api-schema.d.ts`).

Of the 169 rows, 78 name a real core type on both sides. The other 91 name
`serde_json::Value` on one or both sides, in three groups (counted in
[[RPC Client#Findings]]): 49 methods whose `DaemonClient` reply was already
`Value` before this change; about 15 whose true reply type lives in the
`crucible-daemon` crate, which `crucible-core` cannot name without a
dependency cycle (`McpStatus`, `ScmCloneResponse`, `GrepSearchResponse`,
`FtsResult`, the `fs.*`/`base.*`/`suggest_links`/`webhook.receive`/
`agents.list_profiles` replies); and about 27 behind a `rpc/dispatch.rs`
handler that still reads a raw `&Request` and answers hand-built `json!`,
never having called `typed_params`. None of these were given an invented
type to fill the cell — moving a daemon-local type to core, and giving a raw
handler a typed params struct, are both unfinished step 6/10 work, not part
of this step.

`DaemonClient::call`/`call_with_timeout`/`call_with_retry`
(`crates/crucible-daemon/src/rpc_client/client/mod.rs`) are now generic over
the request and reply (`Req: Serialize`, `Resp: DeserializeOwned`); `Req =
Resp = serde_json::Value` behaves exactly like the old untyped `call`, so
every existing Value-in-Value-out call site needed only a type annotation
where inference could not otherwise pick `Resp` (about 30 call sites across
`crucible-daemon`, `crucible-cli` and their tests — mostly `let x =
client.call(...)` becoming `let x: serde_json::Value = ...`, or
`.call::<_, serde_json::Value>(...)` at a call site with no local binding to
annotate). The former `typed_call`/`typed_call_with_timeout`/
`typed_call_with_retry`/`typed_unit_call` were thin wrappers that
serialized/deserialized around a Value-only `call`; once `call` itself
became generic they were redundant and are deleted, with their ~140 call
sites mechanically renamed (`typed_call` → `call`, etc.) across every
submodule of `crates/crucible-daemon/src/rpc_client/client/`,
`crates/crucible-cli/src/commands/base.rs`,
`crates/crucible-cli/src/tui/oil/chat_runner/actions.rs` and
`crates/crucible-web/src/services/daemon.rs`'s `forward_rpc!` expansions.
`typed_unit_call`/`session_id_call` stay as small `pub(super)` convenience
wrappers built on `call`. The named, argument-transforming client methods
(`kiln_forget`, `session_pause`, and the like) were not deleted: each turns
an ergonomic Rust argument list into a wire body, which is not the
redundancy `call` removes.

**Gone** (measured): 0 new Rust `struct`/`enum` declarations
(`rg -c -t rust '^\s*(pub(\([a-z:]+\))? )?(struct|enum) [A-Z]' crates/*/src`
stays at 1770 in `crates/{core,daemon,web,cli}` and the other crates
combined — the design rule this step's own instructions set: no
one-struct-per-method table). `Result<serde_json::Value>` signatures in
`crates/crucible-daemon/src/rpc_client` went from 49 to 48 (the two
generic-infra methods that used to spell it literally — `call` and
`call_with_timeout` — no longer do, offset by one call site gaining an
explicit `Result<serde_json::Value>` annotation it did not need before).

**Compiler proof.** A row whose type does not exist fails
`crucible-core`'s build (`ASSERT_ROW_TYPES_RESOLVE`). A client call site
whose annotated `Resp` does not match what it does with the reply (e.g.
`notification_dismiss` re-typed to read `.dismissed` off a `McpStatus`)
fails `crucible-daemon`'s build, at the point where `call`'s generic
`Resp` is inferred from the annotation and the field access fails. Neither
proof ties a row's declared pair to a specific call site's chosen types —
Rust cannot bind one concrete type pair to one enum *value* without a
marker type per variant, and a marker type per method is the
one-struct-per-method growth this step's own design rule forbids. The
trade-off: the row is a documented, compiler-verified-to-exist contract and
a generator input, not a type-level guarantee that a given `call` site
honors it. Closing that gap fully would need either 169 marker types (the
explicitly disfavored fallback) or rewiring every dispatch arm to return
its row's exact `Resp` through a shared macro-generated dispatch helper,
which is web-route-sized work of its own and is left to a follow-up.

**Change cost, measured on a scratch method** (`ScratchPing2`, added then
reverted): 3 places — the `rpc_methods!` row (with its types), the dispatch
arm, and the call site itself (`client.call(RpcMethod::ScratchPing2,
()).await`, no wrapper method needed). This is the RPC-method half of the
"about 9 places" figure in "How a step is accepted"; the web-route places
(a route, its types, a forwarding function, a TS function and TS types)
are still 6 more, because part A does not touch the web layer — that is
parts 1-6 of the change below, not started.

**Not started:** the `POST /api/rpc/{method}` route, the browser allow
list, the per-caller allow list, replacing the per-route TS functions with
the generated `rpc-methods.d.ts` map, the one-event-stream/catalog/
client-state moves, and the daemon error-code mapping. `rpc-methods.d.ts`
exists and typechecks (`bun run typecheck` in
`crates/crucible-web/web`) but nothing imports it yet.

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

**The last two rows are done.** The four SSE streams, the RPC route and
the browser/per-caller allow lists (rows 4, and the "Change" list below)
remain not started.

- **`project.register`'s untrusted-root rule.** `project.register` takes an
  `untrusted` flag; `crucible_daemon::project_manager::register_untrusted`
  applies `untrusted_root_refusal` (a credential store, the user's
  config/state tree) on top of the daemon floor every caller gets. A local
  caller (the CLI, the TUI, Lua) omits the flag. The web route sets it and
  keeps only `[web] registration_roots`, an operator setting of the web
  process, not a daemon concept. Before: a raw `project.register` call with
  a `.ssh`-holding path succeeded (no `untrusted` field existed).
  `crates/crucible-daemon/tests/project_register.rs` proves the refusal
  through the live RPC method, and `crates/crucible-web/src/routes/project.rs`'s
  `register_refuses_a_directory_that_holds_a_credential_store` proves the
  web still refuses the same case, through a real daemon rather than the
  web's own pre-check.
- **A canvas write's reference containment.** `fs.write`'s
  `write_for_roots` refuses a `.canvas` write whose parsed content names a
  reference outside the resolved root
  (`crucible_daemon::file_write::canvas_containment_refusal`), checked on
  the bytes actually written — after the web's own read-redaction
  restoration, closing a gap where a historical bad reference rode back to
  disk unchecked. Before: only `crates/crucible-web/src/routes/canvas.rs`'s
  `put_canvas` ran this check, on the client's submission alone, before
  restoration. `crates/crucible-daemon/tests/file_write.rs`'s
  `fs_write_size_and_containment` module proves the refusal and the allow
  case through the live RPC method; `put_canvas_refuses_a_reference_outside_the_root`
  (already a real-daemon test) proves the web still refuses.
- **A note write's name and size checks.** These were already `fs.write`'s
  own gates (`MAX_CONTENT_SIZE`, `enclosing_root`'s containment), shared by
  every caller, before this step — the web's copies in `put_note` were
  redundant, not a gap. Deleted, with a test proving the daemon enforces
  both directly
  (`fs_write_size_and_containment::fs_write_refuses_content_over_the_size_limit`/
  `fs_write_refuses_a_path_that_escapes_the_kiln`) and the web still
  refuses through a real daemon
  (`put_note_refuses_content_over_the_size_limit`/
  `put_note_refuses_a_name_that_escapes_the_kiln`).
- **The resume fallback.** `session.resume` already fell back to storage
  for a session not held in memory (`NotFound`). It now falls back for one
  held but not `Paused` too (most commonly `Ended`), and reports which
  path it took (`SessionTransitionReply::resumed_from_storage`). Before: a
  raw `session.resume` call against a session `Ended` in this daemon's own
  memory refused outright; only the web route's own two-call retry
  revived it. `crates/crucible-daemon/tests/session_resume_from_ended.rs`
  proves both cases through the live RPC method. The web route now makes
  one call and reads the flag, instead of guessing from which of two RPC
  calls succeeded.
- **The catalog cache.** `crucible-web`'s `SwrCache` (`services/catalog.rs`)
  is gone. `AgentManager` caches `agents.list_profiles` and
  `providers.list` itself (`agent_profiles_cache`, `providers_cache`,
  `CATALOG_CACHE_TTL`), the same shape as its existing `model_cache`, and
  warms both at daemon startup. Before: a raw RPC caller re-probed every
  agent binary and provider endpoint on every call; only the browser's
  cache avoided it.
  `crates/crucible-daemon/src/server/platform.rs`/`.../server/session/models.rs`
  each gain a test proving a fresh cache entry is served as-is and an
  expired one triggers a re-probe, through the handler every RPC caller
  reaches.
- **Layout and recents.** `client_state.get`/`client_state.set`
  (`crates/crucible-daemon/src/server/client_state.rs`) is the one generic,
  opaque blob store, keyed by `(client, key)` and written with
  `crucible_core::fs::write_private` under the daemon's data root.
  `crucible-web`'s `default_layout_path`/`standalone_layout_path` and its
  own JSON files are gone; `AppState::client_state_id` (`"web"` /
  `"web-standalone"`) gives the same cross-instance isolation.
  `crates/crucible-daemon/tests/client_state.rs` proves the store,
  including the isolation, through the live RPC method.

**Type count.** `rg -c -t rust '^\s*(pub(\([a-z:]+\))? )?(struct|enum)
[A-Z]' crates/*/src` went from 1839 to 1840 across this pass: new core
request/reply types (`ProjectRegisterRequest`, `ClientStateKey`,
`ClientStateSetRequest`, `ClientStateGetReply`) against deleted duplicates
(`ProviderRow` — a field-for-field copy of
`crucible_core::types::ProviderInfo` — and `SwrCache`/`Entry`). This pass
did not target the type-count reduction steps 10-18 measure; it moved
behavior, and the six items above are "Cheaper to change"/"Same behavior
on every path" outcomes in the sense "How a step is accepted" defines them,
not "Gone" ones.

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

7. **Done.** Put every `rpc_methods!` row's params and reply type in the
   schema document, not only the types that a web route names. `utoipa`
   emitted a schema only for a type a route referenced, so the generated TS
   method map said `unknown` for most typed rows. A generated, checked-in
   struct in core, `crucible_core::protocol::RpcMethodSchemas`
   (`crates/crucible-core/src/protocol/rpc/schema_types.rs`, behind the
   `openapi` feature), lists every row's named params and reply type under
   `#[openapi(components(schemas(...)))]`; `crucible-web`'s `api_spec()`
   merges it into the router's own document. The list is generated from the
   `rpc_methods!` rows themselves
   (`crates/crucible-core/examples/gen_rpc_schema_types.rs`, sharing a row-text
   parser with the TS generator in
   `crates/crucible-core/src/protocol/rpc/type_text.rs`), so a row's type
   reaches the list without a hand edit, and a row whose type lacks
   `ToSchema` fails `crucible-core`'s `--features openapi` build (proved with
   a scratch row, then reverted). **Measured:** the `unknown` count in
   `rpc-methods.d.ts` (`rg -c ': unknown'`) went from 149 to 21; the 21 that
   remain are exactly the rows `rpc_methods!` still names
   `serde_json::Value`, each with its own row comment. 107 core types gained
   the `ToSchema` derive they lacked (84 row types, 23 nested fields); two
   fields took a `schema(value_type = ...)` override (`Uuid`, `PathBuf`) and
   one took it for a type-alias/`ComposeSchema` reason
   (`WorkflowStatusReply.scope`). The Rust struct/enum count went from 1839
   to 1840 — the one new `RpcMethodSchemas` struct; every other change is a
   derive on an existing type. `just lint types` now also fails when
   `rpc-methods.d.ts` is stale. See [[RPC Client#Findings]] for the full
   account.
8. **Done.** Capture the reply fixtures of gap 2A again, from the code
   before that change. Its 31 wire tests used to compare the new types with
   JSON written by hand from the old `json!` calls in the same change, which
   the acceptance rules do not accept as proof. `step19_gap2_wire.rs` now
   reads `assets/fixtures/golden/replies/*.json`, captured by running the
   daemon at `0dfc583f1` (the commit before gap 2A) over its real socket, and
   — for `session.pending_interactions`' populated case and
   `workflow.cancel`'s "cancelled" case, which have no RPC-only way to reach
   that state — through the real `RpcDispatcher` in a unit test that seeds
   the state with the production `AgentManager::request_interaction` and a
   hung agent turn. `session.set_workspace` shares `SessionScopeReply` but
   the old handler always refused it, so its fixture comes from
   `connect_kiln`/`disconnect_kiln` instead.
9. Delete the hand-written `DaemonClient` methods that only forward one
   row. Gap 1 added one generated `rpc_<method>` function per row, so each
   such method is now a second way to make the same call. About 160 stay
   because the web's `forward_rpc!` calls them by name; the one route of
   this step removes those callers. Keep a method only where it adds
   behavior (a retry or timeout policy, a derived value, an argument
   transform), and name the behavior.

**Done when.**
- The web server has the routes in the "keep" rows, one event stream and the
  RPC route, and no route that only forwards one RPC. About 90 types and
  about 70 TS functions are gone.
- The change cost of a browser method falls from about 9 places to 3 (plus
  one allow-list line), measured by adding a scratch method.
- No hand-written `DaemonClient` method only forwards one row.
- The generated TS method map has no `unknown` entry except for the rows
  whose reply is open by design (for example `lua.eval`, `config.get`), and
  each such row names its reason.
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

---
title: Actual Architecture
description: As-built architecture at 7053bcfe7: seams, types, traits, duplicates and dead code with file:line cites.
tags: [meta, architecture]
status: as-built
as_of: 7053bcfe7
---

# Actual Architecture

This document describes the code as it is at commit `7053bcfe7`. It does not
describe the product. Where this document and [[Expected]] differ, the
difference is a finding. The seam names below are the names in the
"Key Abstractions" section of `AGENTS.md`.

Path convention: every path is relative to `crates/`. A citation such as
`crucible-daemon/src/tools/surface.rs:60` names a file and a line at `7053bcfe7`.

## 1. Method

Thirty-one readers worked in parallel. Each reader took one module of the
workspace, read every non-test source file in it, and wrote one record with
the same headings: purpose, public types, traits, cross-crate imports, seams,
suspected duplicates, suspected dead code, concerns. Each reader ran a
workspace-wide grep for every name it called dead. A skeptic then re-read each
duplicate claim against both definitions and marked it `exact-duplicate`,
`near-duplicate` or `distinct`; this document keeps only the first two kinds.
A separate audit checked the older analysis documents and kept the facts that
are still true; this document links those documents instead of repeating them.
One writer merged the records into this document. The writer opened no source
file except to count lines for section 2.

## 2. Crate map

| Crate | Lines | Top modules by size |
|---|---|---|
| `crucible-core` | 67.8k | `config/` 15.6k, `parser/` 13.3k, `events/` 7.9k, `session/` 4.3k, `protocol/` 3.7k, `types/` 3.6k, `traits/` 2.5k, `interaction/` 1.9k |
| `crucible-cli` | 66.5k | `tui/oil/` 44.2k, `commands/` 14.9k, `cli/` 1.7k, `formatting/` 1.4k, `factories/` 0.9k |
| `crucible-daemon` | 153.8k | `agent_manager/` 28.0k (of which `messaging/` 6.1k), `server/` 20.7k, top-level files 20.2k, `tools/` 15.6k, `acp/` 9.0k, `rpc_client/` 7.4k, `watch/` 6.2k, `storage/` 5.9k, `review/` 5.4k |
| `crucible-lua` | 35.1k | top-level files 22.0k, `sessions/` 3.7k, `handlers/` 3.5k, `lifecycle/` 3.0k, `lua_stdlib/` 1.9k, `vault/` 1.0k |
| `crucible-oil` | 12.2k | top-level files 8.0k, `layout/` 2.0k, `template/` 1.4k, `components/` 0.8k |
| `crucible-web` | 18.2k | `routes/` 9.8k, `middleware/` 3.1k, `services/` 2.3k |

Line counts include tests. The largest single file is
`crucible-daemon/src/provider/genai_handle.rs` at about 3,200 lines; it sits on
the grandfathered size ledger in `crucible-daemon/tests/architecture_tests.rs:718`.

The dependency direction is `core <- oil <- lua <- daemon <- web <- cli`.
`crucible-oil` depends on no workspace crate (`oil.md` record). `crucible-core`
depends on no workspace crate (`core-rest` record). No crate imports
`crucible_cli`; the only mention is a stale doc comment at
`crucible-core/src/types/acp.rs:117` that names a type that no longer exists.

## 3. Seams

### 3.1 Scope / containment

**Owns.** Given a session: what a turn may read, write, search, load and
execute.

**Modules.** `crucible-daemon/src/tools/{containment,fs_scope,path_resolution,protected,surface}.rs`,
`crucible-daemon/src/agent_manager/{scope,session_permissions}.rs`,
`crucible-daemon/src/agent_manager/messaging/{gate_decision,isolation_gate,permission}.rs`,
`crucible-daemon/src/execution_roots.rs`, `crucible-daemon/src/kiln_registry.rs`,
`crucible-daemon/src/permission_bridge.rs`,
`crucible-core/src/config/components/permissions/`, `crucible-lua/src/isolation.rs`.

**Types.**

| Type | Location | Purpose |
|---|---|---|
| `Containment` | `crucible-daemon/src/tools/containment.rs:73` | Verdict of one path judgement; symlink escape is its own outcome |
| `Access` | `crucible-daemon/src/tools/containment.rs:103` | Read or Write intent |
| `RootSet` | `crucible-daemon/src/tools/containment.rs:136` | Ambient or Rooted session roots |
| `Roots` | `crucible-daemon/src/tools/containment.rs:163` | allowed, denied, carved, protected roots |
| `FsScope` | `crucible-daemon/src/tools/fs_scope.rs:162` | The one door: anchor plus naming rule plus `RootSet` |
| `ContainedPath` | `crucible-daemon/src/tools/fs_scope.rs:92` | Proof that a path is readable |
| `WritablePath` | `crucible-daemon/src/tools/fs_scope.rs:130` | Proof that a path is writable |
| `ResolvedPath` | `crucible-daemon/src/tools/path_resolution.rs:174` | Lexical and canonical forms of one path |
| `Protection` | `crucible-daemon/src/tools/protected.rs:137` | Why a write into a daemon-executed tree is refused |
| `BuiltinTool` | `crucible-daemon/src/tools/surface.rs:60` | 23 built-in tools; the exhaustive surface table |
| `ToolSurface` | `crucible-core/src/traits/tools.rs:56` | Host, kiln, daemon or unknown; Unknown is refused under isolation |
| `KilnRegistry` | `crucible-daemon/src/kiln_registry.rs:291` | The only door where a path becomes a kiln |
| `KilnRegistryContext` | `crucible-daemon/src/kiln_registry.rs:148` | Anchors and denials a registry is built against |
| `RegistrationRefused` | `crucible-daemon/src/kiln_registry.rs:251` | The floor refused a root |
| `KilnScope` | `crucible-daemon/src/session_manager.rs:74` | Caller kiln set; overlap predicate for list, search, cleanup |
| `PermissionEngine` | `crucible-core/src/config/components/permissions/engine.rs:10` | Evaluates a tool call against compiled glob rules |
| `PermissionDecision` | `crucible-core/src/config/components/permissions/types.rs:98` | Allow, Deny, Ask |
| `PermissionConfig` | `crucible-core/src/config/components/permissions/types.rs:63` | default, allow, deny, ask rule lists |
| `PatternStore` | `crucible-core/src/config/patterns.rs:84` | Per-project allowlists under `whitelists.d/` |
| `ShellPolicy` | `crucible-core/src/config/security.rs:54` | Prefix whitelist and blacklist, fail-closed |
| `DaemonPermissionGate` | `crucible-daemon/src/permission_bridge.rs:19` | `PermissionGate` over `PermissionEngine` for ACP prompts |
| `PermissionSerializer` | `crucible-daemon/src/agent_manager/messaging/permission.rs:76` | Serialises ACP permission prompts per session |
| `IsolationRegistry` | `crucible-lua/src/isolation.rs:122` | session id to `IsolationClaim`; gates host tool execution |
| `IsolationClaim` | `crucible-lua/src/isolation.rs:31` | plugin, exempt tool set, sandbox exec spec |
| `SandboxExec` | `crucible-lua/src/isolation.rs:63` | prefix, env, suffix argv wrapper for a sandboxed child |
| `SandboxEnv` | `crucible-lua/src/isolation.rs:94` | Unsupported, Flag, Inline |

Free functions: `session_containment` (`agent_manager/scope.rs:74`),
`session_tool_root` (`scope.rs:108`), `refuse_forbidden_scope`
(`kiln_registry.rs:74`), `forbidden_root_reason` and `resolve_registration_root`
(`project_manager.rs:45,68`), `execution_roots::{record,baseline,all}`,
`requires_permission_gate` (`messaging/gate_decision.rs:341`).

**Traits.** `PermissionGate` (`crucible-core/src/traits/permission_gate.rs:13`):
1 required, 0 defaulted, 1 impl, no test double, used as
`Arc<dyn PermissionGate>` at `messaging/permission.rs:210`.

**Enters and leaves.** A tool path enters `FsScope` (sync). A tool name enters
`BuiltinTool::surface` (sync, pure). A bash command enters `PermissionEngine`
then `PatternStore` in series (`messaging/permission.rs:732`). An ACP
`RequestPermissionRequest` enters the closure built at
`messaging/permission.rs:217` (async). The gate order in
`messaging/tool_call.rs` is: plan-mode bar, active-tool set, card policy,
review gate, `pre_tool_call` Lua hooks, isolation gate, permission gate,
dispatch (`daemon-agent_manager-b` record). `handled` from a Lua hook returns
before the permission gate; only statement order protects it.

**Confirmed problems.**

- `Arc<dyn PermissionGate>` has one impl and no test double
  (`messaging/permission.rs:210`, `permission_bridge.rs:56`). Neither `dyn`
  exemption applies.
- `requires_permission_gate` holds `unreachable!("denied above")` for
  `ToolPolicy::Deny` (`messaging/gate_decision.rs:341`); the invariant lives in
  `messaging/tool_call.rs:308`. A new caller panics the daemon.
- `execute_permission_hooks_with_timeout` has no timeout; it runs the hooks and
  discards the result when elapsed time exceeds 1 s
  (`messaging/permission.rs:1108`).
- `PatternStore::load_sync` and `save_sync` do blocking file I/O inside the
  async gate (`messaging/permission.rs:732,1094`).
- Hand-maintained name lists beside `BuiltinTool`: `is_core_tool_name`
  (`tool_dispatch.rs:181`), `KILN_BACKED_TOOLS` (`tools/mcp_server.rs:126`),
  `DISCOVERY_TOOL_NAMES` (`tool_dispatch.rs:30`), `PLAN_TOOL_NAMES`
  (`tools/tool_modes.rs:17`), `is_write_tool_name`
  (`provider/genai_handle.rs:103`), and the CLI `BUILTIN_TOOLS`
  (`crucible-cli/src/commands/tools.rs:34`), which already disagrees with the
  table.
- The file-tool name list is written twice in `messaging/permission.rs:1073,1098`
  and a third, different list is `is_file_tool` in
  `crucible-core/src/config/components/permissions/engine.rs:193`.
- The permission-engine input snippet (`if tool == "bash" { command } else { args }`)
  is written three times: `messaging/permission.rs:585,628,690`.
- Three bash allowlists exist in one module: `PermissionConfig.allow`,
  `PatternStore.bash_commands.allowed_prefixes` (`patterns.rs:57`),
  `ShellPolicy.whitelist` (`security.rs:56`); two hardcoded deny lists overlap
  (`hardcoded.rs:21`, `security.rs:163`).
- The `Component::Normal` whitelist loop is written six times:
  `server/fs/mod.rs:160,387,460`, `server/session/review/mod.rs:605`,
  `server/note_refactor.rs:232`, `crucible-core/src/canvas/containment.rs:215`.
- The CLI builds its own `KilnRegistry` over `crucible_home()`
  (`crucible-cli/src/kiln_attach.rs:112`) while the doc calls the daemon
  registry the authority. `KilnRegistryContext::for_daemon` reads
  `current_dir` and `home_dir` (`kiln_registry.rs:174`).
- `execution_roots::baseline` reads env vars and `config.toml` from disk
  (`execution_roots.rs:73-99`); `kiln_registry.rs:323` names it as the
  precedent for the wrong choice.
- The web layer holds its own policy: credential-directory deny list
  (`crucible-web/src/routes/project.rs:28-76`), SSRF address classification
  (`routes/session/mod.rs:239-322`), enclosing-root resolution twice
  (`routes/canvas.rs:195`, `routes/kiln.rs:363-437`).
- `DelegationService::enforce_child_isolation` skips silently when
  `session_lifecycle` is unbound (`delegation.rs:181`).
- Two `PermissionScope` enums share one name
  (`crucible-core/src/config/components/permissions/types.rs:5`,
  `crucible-core/src/interaction/permission.rs:20`); the TUI maps one to the
  other by hand (`crucible-cli/src/tui/oil/chat_app/shell.rs:113-121`).

Still true from the older notes: no cap-std `Dir` handle and no Landlock
backstop exist; `PROTECTED_DIRS` and the `skills/discovery.rs` harness lists
are independent; `ensure_md_suffix` keeps a foreign extension.

### 3.2 Session / turn lifecycle

**Owns.** The turn loop, tool admission, context assembly, session registry
and persistence.

**Modules.** `crucible-daemon/src/agent_manager/` (21 files plus
`messaging/` and `precognition/`), `session_manager.rs`, `session_lifecycle.rs`,
`session_storage.rs`, `session_bridge.rs`, `delegation.rs`, `agent_factory.rs`,
`agent_cards.rs`, `tool_dispatch.rs`, `review/`, `workspace_snapshot.rs`,
`server/session/`, `provider/genai_handle.rs`, `acp_handle.rs`,
`rpc_client/agent/`, `crucible-core/src/{session,turn}/`.

**Types.**

| Type | Location | Purpose |
|---|---|---|
| `AgentManager` | `crucible-daemon/src/agent_manager/mod.rs:363` | One `SessionSlot` per live session; providers, knobs, scope mutation, teardown |
| `AgentManagerParams` | `crucible-daemon/src/agent_manager/mod.rs:480` | Constructor parameters |
| `SessionSlot` | `crucible-daemon/src/agent_manager/slot.rs:29` | Agent handle, dispatcher, Lua VM, tree, pending prompts, cache stats |
| `SessionEventState` | `crucible-daemon/src/agent_manager/mod.rs:244` | Per-session Lua VM, handler registry, permission hooks |
| `StreamContext` | `crucible-daemon/src/agent_manager/mod.rs:294` | Everything the stream loop reads for one turn |
| `AgentStreamConfig` | `crucible-daemon/src/agent_manager/stream_config.rs:10` | Frozen per-turn config |
| `TurnEnvironment` | `crucible-daemon/src/agent_manager/stream_config.rs:110` | Daemon-side inputs snapshotted per turn |
| `RequestState` | `crucible-daemon/src/agent_manager/mod.rs:133` | In-flight turn: cancel sender, task handle |
| `TurnStatus`, `TurnOutcome` | `crucible-daemon/src/agent_manager/mod.rs:142,154` | Terminal status and completion payload |
| `PendingPermission`, `PendingInteraction` | `mod.rs:288`, `interaction.rs:25` | Parked prompt plus oneshot sender |
| `CachedAgent`, `BuildCache` | `crucible-daemon/src/agent_manager/slot.rs:104,87` | Cached handle plus generation |
| `GateSubject` | `crucible-daemon/src/agent_manager/messaging/review_gate.rs:53` | What the review gate holds against |
| `ToolCallTracker` | `crucible-daemon/src/agent_manager/tool_tracking.rs:3` | Counts repeated identical tool calls |
| `SessionManager` | `crucible-daemon/src/session_manager.rs:147` | In-memory session map plus persistence |
| `SessionError` | `crucible-daemon/src/session_manager.rs:925` | NotFound, AlreadyEnded, InvalidState, IoError |
| `SessionLifecycle` | `crucible-daemon/src/session_lifecycle.rs:37` | Plugin start and end hooks; refuses an unenforceable isolation claim |
| `DelegationService` | `crucible-daemon/src/delegation.rs:93` | Spawn, await, cancel, list child sessions |
| `DelegationRequest`, `DelegationSpawned` | `crucible-daemon/src/delegation.rs:41,55` | Spawn input and result |
| `AgentFactoryError` | `crucible-daemon/src/agent_factory.rs:370` | ClientCreation, AgentBuild, UnsupportedAgentType |
| `EnrichedPrompt` | `crucible-daemon/src/agent_factory.rs:404` | System prompt split at the cache boundary |
| `FileSessionStorage` | `crucible-daemon/src/session_storage.rs:92` | `{data_home}/sessions/{id}/` with `meta.json`, `session.jsonl`, `session.md` |
| `DaemonToolDispatcher` | `crucible-daemon/src/tool_dispatch.rs:117` | Provider chain plus hydrated catalog |
| `ReviewLedgers` | `crucible-daemon/src/review/mod.rs:134` | Per-session ledgers, states, comments, brackets, gate |
| `CaptureHandle`, `GateHold` | `crucible-daemon/src/review/mod.rs:64,112` | RAII bracket and gate block |
| `SnapshotMap`, `WorkspaceSnapshot` | `crucible-daemon/src/workspace_snapshot.rs:48,123` | Turn-level workspace undo |
| `GenaiAgentHandle` | `crucible-daemon/src/provider/genai_handle.rs:376` | Streaming chat agent over `genai` |
| `AcpAgentHandle` | `crucible-daemon/src/acp_handle.rs:69` | `Agent` over an external ACP process |
| `DaemonAgentHandle` | `crucible-daemon/src/rpc_client/agent/mod.rs:29` | Client-side `Agent` over RPC with 16 cached knobs |
| `Session` | `crucible-core/src/session/types/session.rs:31` | Session record: flat kiln set, workspace, agent, state |
| `SessionAgent` | `crucible-core/src/session/types/agent.rs:19` | Inlined agent config for resume |
| `SessionId` | `crucible-core/src/session/types/id.rs:45` | Path-safe validated id |
| `SessionSummary`, `SessionState` | `summary.rs:15`, `enums.rs:82` | Listing row; active, paused, compacting, ended |
| `TurnEvent` | `crucible-core/src/turn/mod.rs:39` | Agent-to-runtime event stream, 16 variants |
| `TurnContext` | `crucible-core/src/turn/mod.rs:293` | Inputs to one turn |
| `ConversationTree` | `crucible-core/src/turn/tree.rs:124` | Append-only tree with cursor |
| `Ledger`, `Interval`, `ComposedHunk`, `GateBlock` | `crucible-core/src/session/types/review.rs:332,268,639,710` | Review domain types |

**Traits.** `Agent` (`crucible-core/src/turn/mod.rs:346`, 4 required, 6
production impls). `AgentHandle` (`crucible-core/src/traits/chat.rs:143`, 3
required, 41 defaulted). `ToolDispatcher` (`tool_dispatch.rs:99`, 4 required,
1 production impl plus one test double). `SessionStorage`
(`session_storage.rs:34`, 8 required, 1 production impl plus 6 doubles).
`DelegationSpawner` (`delegation.rs:65`, 5 required, 1 production impl plus 5
mocks). `Undoable` (`crucible-core/src/traits/undoable.rs:15`, 3 required,
1 impl).

**Enters and leaves.** `send_message` and `send_message_notified`
(`messaging/send.rs:14,37`, async) from RPC `session.send`, delegation and the
Lua bridge. The `TurnEvent` stream from `Agent::turn` (`messaging/stream.rs:291`,
async). Permission answers over `oneshot::Sender<PermResponse>`
(`messaging/permission.rs:894,144`). Out: `SessionEventMessage` on the
broadcast sender (sync `emit_event`); `TurnEvent::{ToolResult, DepthCapHit,
ContextAttach}` back to the adapter over `mpsc` (`stream.rs:548,751,771`);
tool dispatch through `Arc<dyn ToolDispatcher>` with a 30 s timeout, or
delegation timeout plus 30 s (`messaging/tool_call.rs:647-660`); spill files
under `<session_dir>/tools/` (`tool_call.rs:887`). Lock order: the
session-state `tokio::Mutex` is held for session-VM handler passes and released
before plugin-VM passes (`messaging/permission.rs:1130-1138`). The auto-archive
sweep runs every 30 min with a 72 h default (`server/mod.rs:664-667`).

**Confirmed problems.**

- `AgentHandle` has 41 defaulted methods of 44. `MockSubagentHandle` implements
  3 (`test_support.rs:178`). `GenaiAgentHandle::send_message_fire_and_forget`
  is a no-op kept to satisfy the trait (`genai_handle.rs:1500`). `AcpAgentHandle`
  returns `NotSupported` for two methods and has two no-op `cancel` bodies
  (`acp_handle.rs:423,821`). The `Box<dyn AgentHandle>` forwarder re-lists all
  44 methods (`crucible-core/src/traits/chat.rs:486`); a new defaulted knob that
  misses the forwarder routes to the default silently.
- `DaemonAgentHandle::clear_history` re-creates the session, re-applies 8
  cached knobs and re-subscribes on the client side
  (`rpc_client/agent/agent_handle.rs:73-151`); `Drop` spawns a `session.end`
  RPC (`rpc_client/agent/mod.rs:297-313`). The `Undoable` impl returns
  constants while the matching RPC wrappers are unused (`agent_handle.rs:441-464`).
- `FileSessionStorage::new(sm.sessions_root()).with_registry(...)` is rebuilt at
  four sites that bypass the manager's `Arc<dyn SessionStorage>`:
  `server/session/models.rs:207`, `server/session/messaging.rs:126`,
  `session_bridge.rs:416`, `server/mod.rs:486`. `archive_session` and
  `unarchive_session` read `meta.json` directly with 15 identical lines
  (`session_manager.rs:699-714,726-741`).
- `build_default_internal_agent` (`server/session/create.rs:454-542`) and
  the CLI literal at `crucible-cli/src/commands/session/acp.rs:527-558`
  both repeat `SessionAgent::internal_from_config`
  (`crucible-core/src/session/types/agent.rs:335-398`).
- The "session VM under lock, then plugin VM" two-pass loop is hand-written
  eleven times: `tool_call.rs:358-393`; `permission.rs:331-365,502-533`;
  `stream.rs:1181-1199,1250-1279`; `tool_hooks.rs:26-70,77-121,192-218,226-274`;
  `precognition/mod.rs:159-175,253-274`.
- The `ChatToolResult` error literal appears seven times
  (`tool_call.rs:44,499,517,815,875`; `stream.rs:619,697`); core has no
  error constructor (`crucible-core/src/traits/chat.rs:90`).
- `deny_tool_call` exists (`tool_call.rs:23`) and the same sequence is inlined
  at `tool_call.rs:485-506` and six `permission.rs` sites (`660,705,788,841,871,1028`).
- `StreamContext` derives `Clone` yet `stream.rs:1098-1124` rebuilds it field by field.
- `execute_agent_stream` takes nine arguments under
  `#[allow(clippy::too_many_arguments)]` (`stream.rs:230`); `RpcContext::new`
  takes 16 (`rpc/context.rs:112`); `update_agent_config_and_emit` is called 13
  times with near-identical closures (`agent_manager/models.rs:402`).
- Test-only production methods behind `#[allow(dead_code)]`:
  `await_permission`, `get_pending_permission`, `list_pending_permissions`
  (`agent_manager/permissions.rs:23,110,120`); `remove_session`, `active_count`,
  `total_count` (`session_manager.rs:784,803,812`); `RequestState.started_at`
  is write-only (`mod.rs:137`).
- `AgentStreamConfig.review` and `.active_tools` are `Option` only so tests can
  omit them (`stream_config.rs:153-174`); `Option<Arc<ReviewLedgers>>` at
  `stream_config.rs:93` while `AgentManager` always builds one (`mod.rs:537`).
- `AgentError::PermissionNotFound` is reused for a missing interaction id
  (`agent_manager/interaction.rs:125`).
- `SessionManager::child_session_ids` turns a storage error into an empty list
  (`session_manager.rs:369`), so the archive cascade can miss children.
- `DaemonSessionBridge::fork_session` builds a second storage and does not copy
  agent config (`session_bridge.rs:395`); three hand-built JSON projections of
  `Session` differ on `title` (`session_bridge.rs:89,100,118`).
- `spawn_setup_task` indexes only the first kiln of the flat set
  (`server/session/mod.rs:126-131`). `session.set_thinking_budget` stores
  `unwrap_or(0)` and echoes the raw `Option` (`server/session/params.rs:311-325`).
- `inject_context_impl` returns `Result<(), String>` and the handler classifies
  by `starts_with` (`server/session/messaging.rs:161-162`).
- `StreamingChunk` (`acp/streaming.rs:23`, 7 variants) is a translation layer
  over `TurnEvent`; the streaming loop exists twice and the non-callback path
  has no production caller (`acp/client/streaming.rs:164,687,788` vs `266,386,461`).
- `DEPTH_CAP_PROMPT` (`genai_handle.rs:1285`) is a second copy of
  `TOOL_DEPTH_LIMIT_FINAL_PROMPT` kept in sync by comment.
- `post_llm_call` is emitted with two payloads under one name
  (`stream.rs:1152` wire, `stream.rs:1173` Lua).
- Git plumbing is duplicated between `review/git.rs:300,210-240` and
  `workspace_snapshot.rs:349,143,334-341`. `journal.rs:101` maps a serde
  failure to `ReviewError::Git`.
- `WorkspaceSnapshot` encodes three shapes in four fields
  (`workspace_snapshot.rs:123`); an enum removes the invalid combinations.
- `SessionSummary.event_count` is always 0 (`crucible-core/src/session/types/summary.rs:62`).
  `Session.notifications` persists an `Instant`-aged queue in `meta.json`
  (`session.rs`, `types/notification.rs:20`). `SessionState::Compacting` is
  never assigned (`enums.rs:90`). `StopReason::MaxToolDepth` is never built
  (`turn/mod.rs:169`).
- `TurnError` and `AgentError` share four identical variants
  (`crucible-core/src/turn/mod.rs:200,221`).

Still true from the older notes: `ContextStrategy` has three live arms and
`Summarize` makes a real LLM call; `TOOL_SCHEMA_BUDGET_SHARE = 0.15` at
`genai_handle.rs:30`; the model chain for delegation is card, then
`[llm.models]` specialty, then parent.

### 3.3 Knowledge — four subsystems

**Owns.** Notes, links, kiln identity and embeddings. The parser is an island.
Link resolution lives in SQLite. `KilnName` and `KilnRegistry` own identity.
Embeddings serve retrieval and indexing. `NotePipeline` is the adapter between
them.

**Modules.** `crucible-core/src/parser/`, `crucible-core/src/storage/`,
`crucible-core/src/enrichment/`, `crucible-core/src/canvas/`,
`crucible-daemon/src/storage/sqlite/`, `crucible-daemon/src/pipeline/`,
`crucible-daemon/src/enrichment/`, `crucible-daemon/src/kiln_manager.rs`,
`crucible-daemon/src/kiln_registry.rs`, `crucible-daemon/src/llm/embeddings/`,
`crucible-daemon/src/embedding.rs`, `crucible-daemon/src/multi_kiln_search.rs`,
`crucible-daemon/src/agent_manager/precognition/`, `crucible-daemon/src/watch/`,
`crucible-daemon/src/tools/{notes,search,kiln,autolink}.rs`.

**Parser types.**

| Type | Location | Purpose |
|---|---|---|
| `CrucibleParser` | `crucible-core/src/parser/implementation.rs:65` | The only `MarkdownParser` impl; splits frontmatter, runs extensions in priority order |
| `ParsedNote` | `crucible-core/src/parser/types/parsed_note.rs:31` | Parse output; raw text plus byte offsets, no resolution |
| `NoteContent` | `crucible-core/src/parser/types/content.rs:11` | All extracted structure |
| `Wikilink` | `crucible-core/src/parser/types/links.rs:15` | target, alias, offset, target_span, refs |
| `Tag`, `InlineLink`, `FootnoteMap` | `links.rs:154,202,250` | |
| `Frontmatter`, `FrontmatterFormat` | `crucible-core/src/parser/types/frontmatter.rs:13,121` | Raw text plus lazy map; Yaml, Toml, None |
| `BlockHash` | `crucible-core/src/parser/types/block_hash.rs:12` | 32-byte newtype; `NoteRecord.content_hash` |
| `ASTBlock`, `ASTBlockType` | `crucible-core/src/parser/types/ast.rs:83,12` | Semantic blocks when `BlockProcessingConfig.enabled` (default false) |
| `BlockExtractor`, `SimpleBlockHasher` | `block_extractor.rs:130`, `block_hasher.rs:16` | Blocks, BLAKE3 hashes, Merkle root |
| `ExtensionRegistry` | `crucible-core/src/parser/extensions.rs:94` | Ordered `Arc<dyn SyntaxExtension>` |
| `TaskFile`, `TaskGraph` | `crucible-core/src/parser/types/task.rs:116,267` | TASKS.md view |
| `WorkflowDoc`, `WorkflowStep`, `Gate` | `crucible-core/src/parser/types/workflow.rs:25,61,128` | `type: workflow` view |
| `KilnFileKind` | `crucible-core/src/kiln.rs:22` | Note, Canvas, PlainText, Asset; the single file-kind predicate |

**Link, identity and storage types.**

| Type | Location | Purpose |
|---|---|---|
| `LinkResolution` | `crucible-daemon/src/storage/sqlite/link_index.rs:108` | Result of one wikilink resolution at index time |
| `KilnName` | `crucible-core/src/config/kiln_name.rs:51` | Validated `[a-z0-9._-]` key, custom `Deserialize` |
| `KilnEntry` | `crucible-core/src/config/registry.rs:12` | `[kilns]` value: path or table with `lazy`, `auto` |
| `RegisteredKiln`, `KilnResolution` | `crucible-daemon/src/kiln_registry.rs:196,235` | Name to path; Ready, Lazy, Unknown |
| `KilnManager` | `crucible-daemon/src/kiln_manager.rs:383` | Opens, indexes, closes kilns by path; owns storage, pipeline, watcher |
| `StorageHandle`, `KilnConnection` | `crucible-daemon/src/kiln_manager.rs:67,374` | Per-kiln SQLite handle plus FTS; one open kiln |
| `SqlitePool` | `crucible-daemon/src/storage/sqlite/connection.rs:20` | One mutex-guarded connection plus migration outcome |
| `SqliteNoteStore` | `crucible-daemon/src/storage/sqlite/note_store.rs:354` | `NoteStore` and `PropertyStore` over the pool |
| `FtsIndex`, `FtsResult` | `crucible-daemon/src/storage/sqlite/fts.rs:157,41` | FTS5 wrapper |
| `SqliteKnowledgeRepository` | `crucible-daemon/src/storage/sqlite/repository.rs:32` | `KnowledgeRepository` over the note store |
| `SqliteClientHandle` | `crucible-daemon/src/storage/sqlite/adapters.rs:21` | Hands out `dyn NoteStore`, `dyn PropertyStore`, `dyn KnowledgeRepository` |
| `NoteRecord`, `LinkOccurrence`, `GraphLink`, `InboundLink` | `crucible-core/src/storage/note_store.rs:68,135,151,162` | Index model |
| `Scope` | `crucible-core/src/storage/scope.rs:73` | One-variant workspace scope |
| `NotePipeline` | `crucible-daemon/src/pipeline/note_pipeline.rs:57` | Parse, enrich, store for one note, canvas or text file |
| `CanvasLinks` | `crucible-daemon/src/pipeline/canvas_index.rs:31` | Links and tags from a canvas |
| `Canvas` | `crucible-core/src/canvas/mod.rs:38` | JSON Canvas 1.0 with `extra` maps |
| `Enricher` | `crucible-daemon/src/enrichment/service.rs:21` | Block embeddings and metadata |
| `OllamaProvider`, `OpenAIProvider`, `FastEmbedProvider`, `MockEmbeddingProvider` | `llm/embeddings/{ollama.rs:57,openai.rs:63,fastembed.rs:70,mock.rs:13}` | `EmbeddingProvider` impls |
| `KilnSearchSource` | `crucible-daemon/src/multi_kiln_search.rs:21` | One kiln for the vector fan-out |
| `WatchManager` | `crucible-daemon/src/watch/manager.rs:83` | Backends, handlers, queue, debouncer; one per open kiln |
| `NotifyWatcher` | `crucible-daemon/src/watch/backends/notify_backend.rs:22` | The only backend that runs |
| `IndexingHandler` | `crucible-daemon/src/watch/handlers/indexing.rs:21` | `FileEvent` to `SessionEvent` |
| `ExternalChangeTracker`, `ExternalChangeWatch` | `crucible-daemon/src/watch/external_changes.rs:141,397` | Review backstop over session roots |
| `LinkSuggestion` | `crucible-daemon/src/tools/autolink.rs:12` | One unlinked mention (`suggest_links`) |
| `GrepHit`, `GrepSearchResponse` | `crucible-daemon/src/tools/grep_engine.rs:55,74` | ripgrep-crate grep |

The schema ladder and the DDL ownership rules are in [[Storage Schema]] and
are still true. The canvas model, containment and index rules are in [[Canvas]]
and are still true.

**Traits.** `MarkdownParser` (`parser/traits.rs:17`, 4 required, 1 impl, `dyn`
at `note_pipeline.rs:59`). `SyntaxExtension` (`parser/extensions.rs:18`, 5
required, 4 defaulted, 8 impls all in one crate). `NoteStore`
(`storage/note_store.rs:443`, 7 required, 5 defaulted, 2 production impls).
`PropertyStore` (`storage/property_store.rs:15`, 5 required, 2 production
impls). `KnowledgeRepository` (`traits/knowledge.rs:78`, 2 required, 1
defaulted, 3 production impls). `EmbeddingProvider`
(`enrichment/embedding.rs:36`, 6 required, 2 defaulted, 5 production impls).
`ContentHasher` (`storage/traits.rs:17`, 3 required, 1 defaulted, 2 impls with
zero callers). `HashingAlgorithm` (`hashing/algorithm.rs:48`, 3 required, 4
defaulted, zero callers). `FileWatcher`, `WatcherFactory`, `EventHandler`
(`watch/traits.rs:10,316`, `watch/backends/mod.rs:20`).

**Enters and leaves.** `MarkdownParser::parse_content` takes a string and a
source path (async via `async_trait`, no I/O). `NoteStore::upsert` returns
`Vec<SessionEvent>` and `delete` returns `SessionEvent`; callers must announce
them (`crucible-core/src/storage/note_store.rs:455,487`). `SqlitePool` is
synchronous; every async entry point clones the pool into `spawn_blocking`.
`KilnManager` broadcasts `SessionEventMessage` (async). The watch pipeline is
backend, unbounded mpsc, processor task (`manager.rs:442`), `Debouncer`,
`EventQueue`, `HandlerRegistry`, one `tokio::spawn` per handler per event.
Two debounce layers sit in series: `notify_debouncer_full` at a fixed 100 ms
(`notify_backend.rs:60`) and `Debouncer` at `WatchManagerConfig.debounce_delay`,
plus a 50 ms flush tick (`manager.rs:445`). `precognition/` computes the
kiln-search system message tagged `PRECOGNITION_TAG`
(`precognition/mod.rs:556`) and sends it to the provider, not to a client.

**Confirmed problems.**

- `hashing/` (5 files, about 1,000 lines) has zero callers; live hashing calls
  `blake3` directly (`ast.rs:180`, `block_hasher.rs:60`), so
  `normalize_block_text` never runs. Two hashes carry the name "block hash":
  `ASTBlock.block_hash` is `blake3(content)`, `SimpleBlockHasher::hash_block`
  hashes JSON of type, content, metadata and offsets.
- `ParsedNote` carries two copies of six lists (`parsed_note.rs:39-57`,
  `content.rs:31-49`); `implementation.rs:454-459` clones them;
  `block_extractor.rs:330-344` re-merges with `contains`; the daemon DTO fills
  only the top-level copies (`rpc_client/storage.rs:86`) while
  `repository.rs:111` writes `note.content.wikilinks`.
- Five frontmatter splitters in core (`implementation.rs:241`,
  `frontmatter_extractor.rs:111`, `workflow.rs:276`, `task.rs:213`) plus
  `extract_yaml_frontmatter` in `crucible-cli/src/commands/workflow.rs:365` and
  `crucible-daemon/src/rpc/workflow_handlers.rs:583`, plus
  `tools/utils.rs:31` and `tools/notes/helpers.rs:91`.
- `Arc<dyn MarkdownParser>` has one impl (`note_pipeline.rs:59`). All eight
  `SyntaxExtension` impls live in one crate; only `BasicMarkdownItExtension`
  can be disabled, through the dead `disabled()` (`basic_markdown_it.rs:40`).
  `process_content` blocks on a Tokio handle inside a trait default with zero
  callers (`extensions.rs:59`).
- `NoteStore` defaults five link methods to empty results
  (`note_store.rs:498-526`); a backend that omits them has no backlinks.
  `KnowledgeRepository::search_vectors` defaults to `Ok(vec![])` (`knowledge.rs:87`).
- `SqlitePropertyStore` (`property_store.rs:223`) duplicates
  `impl PropertyStore for SqliteNoteStore` (`property_store.rs:172`) and has no
  production caller. `FtsIndex::is_empty` repeats `count` (`fts.rs:255,236`).
  The scope-authority match is written three times in `repository.rs:71,123,169`.
- `note_store: Option<Arc<dyn NoteStore>>` on `NoteTools`, `SearchTools`,
  `KilnTools` is always `None` in production; every index-backed branch runs
  only under tests (`tools/mcp_server.rs:251` is the only setter).
- Two grep engines: `tools/grep_engine.rs` filters inside the walk;
  `tools/workspace.rs:445` shells out to `rg` and filters output lines.
- `EMBEDDING_PROVIDER_CACHE` is a process-global `Lazy<Mutex<HashMap>>`
  (`crucible-daemon/src/embedding.rs:19`) that ignores injected data roots.
- `expected_dimensions_for_model` hardcodes dimensions and ignores
  `EmbeddingConfig::dimensions()` (`llm/embeddings/config.rs:14`).
  `ollama.rs:81-86` enables `danger_accept_invalid_certs` on a substring check.
  `llm/model_discovery.rs` (608 lines) has no production caller and alone pulls
  `gguf` and `shellexpand` into the daemon.
- Watch: three backends exist and one runs (`polling_backend.rs:100-116`,
  `editor_backend.rs:119-149` are stubs); `FileWatcher` and `WatcherFactory`
  are `dyn` with one real impl. `WatchConfig.debounce`, `handler_config`,
  `mode`, `HandlerConfig` and `WatchManagerConfig.{max_concurrent_handlers,
  enable_monitoring}` are never read; `kiln_manager.rs:1073` and
  `external_changes.rs:504` pass `DebounceConfig` values no backend sees.
  `process_queued_events` joins one task per handler, so handler priority is
  never observed (`manager.rs:562-588`). `WatchManager::shutdown` stops nothing
  (`manager.rs:207-212`). `watch::Error` has 21 variants, 10 never built.
  Module-level `#![allow]` in `watch/mod.rs:39`, `events.rs:3`,
  `handlers/indexing.rs:8`, `handlers/composite.rs:3`.
- `KilnManager::open_and_process` returns a 4-tuple of counts
  (`kiln_manager.rs`); `NoteRecord` to `NoteInfo` is written twice
  (`kiln_manager.rs:133-144,209-220`). `KilnManager::new` and `Default` are
  test-only (`kiln_manager.rs:400,1086`). `#[allow(dead_code)]` on
  `KilnManager::get` is stale (`kiln_manager.rs:731`).
- `NotePipeline::kiln_root` has a test-only `None` path; the
  `normalize_note_path` expression is written four times
  (`note_pipeline.rs:221,331,375,511`).
- Business logic outside the daemon: `crucible-cli/src/commands/tasks.rs:55`
  builds `TaskGraph` in-process; `commands/workflow.rs:360` builds
  `WorkflowDoc`; `commands/stats.rs:54`, `process.rs:329`, `workflow.rs:372`
  are three kiln walkers; `crucible-web/src/routes/search.rs:415`,
  `routes/kiln.rs:353`, `routes/canvas.rs:172` write kiln files with
  `tokio::fs`; `search.rs:128-171` walks the kiln to resolve a wikilink.
- `processing/mod.rs:44-390` and `processing/change_detection.rs` describe a
  queue architecture no code implements (about 900 lines); the daemon has its
  own private `FileState` (`watch/backends/polling_backend.rs:31`).
- `eprintln!` in library code: `implementation.rs:484`,
  `basic_markdown_it.rs:125,155`, `agent/loader.rs:43`.

Still true from the older notes: `autolink.rs` is a pure text heuristic in
Rust; `KilnFileKind::of` is the single file-kind predicate guarded by A2f in
`crucible-cli/tests/architecture_tests.rs`; session files live outside SQLite
(`session_storage.rs:9-10`).

### 3.4 Events & requests

**Owns.** Fan-out with no reply; correlated one-reply-with-timeout.

**Modules.** `crucible-core/src/events/`, `crucible-core/src/protocol/`,
`crucible-core/src/interaction/`, `crucible-daemon/src/event_emitter.rs`,
`crucible-daemon/src/event_map.rs`, `crucible-daemon/src/subscription.rs`,
`crucible-daemon/src/server/core/`, `crucible-daemon/src/file_watch_bridge.rs`,
`crucible-daemon/src/observe/`, `crucible-daemon/src/recording.rs`,
`crucible-daemon/src/replay.rs`, `crucible-daemon/src/background_manager/`,
`crucible-web/src/{events,fs_events}.rs`, `crucible-web/src/services/daemon.rs`.

**Types.**

| Type | Location | Purpose |
|---|---|---|
| `SessionEventMessage` | `crucible-core/src/protocol/rpc/mod.rs:85` | `msg_type, session_id, event, data, timestamp, seq`; the one type all four wire bindings share |
| `SessionEventPayload` | `crucible-core/src/protocol/session_events/mod.rs:104` | Typed view over `{event, data}`; eight groups, 70 wire names |
| `Group` | `crucible-core/src/protocol/session_events/mod.rs:121` | Wire name to group; hand-maintained 70-name match |
| `TurnPayload` | `crucible-core/src/protocol/session_events/turn.rs:48` | 16 turn-stream events |
| `ToolResultBody` | `crucible-core/src/protocol/session_events/turn.rs:344` | `{result}` or `{error}` plus `spill_path`, `summary` |
| `SetupPayload` | `crucible-core/src/protocol/session_events/setup.rs:424` | 7 setup-task events |
| `SettingsPayload` | `crucible-core/src/protocol/session_events/settings.rs:531` | 18 setting acknowledgements |
| `JobPayload`, `ReviewPayload`, `NotificationPayload`, `WorkflowPayload`, `SystemPayload` | `lifecycle.rs:44,116,156,178,233` | 7, 3, 2, 8, 13 variants |
| `EventDecodeError` | `crucible-core/src/protocol/session_events/mod.rs:227` | Decode boundary error |
| `Request`, `Response`, `RpcError`, `RequestId` | `crucible-core/src/protocol/rpc/mod.rs:17,27,38,11` | JSON-RPC 2.0 envelope |
| `SessionEvent` | `crucible-core/src/events/session_event/mod.rs` | Scripting vocabulary, 4 variants after plan T3-B7; Lua sees it as a flat table |
| `InternalSessionEvent` | `crucible-core/src/events/session_event/internal.rs` | 7 variants boxed in `SessionEvent::Internal` (38 before plan T3-B7) |
| `ScriptingEvent` | `crucible-core/src/events/session_event/mod.rs:75` | The ten names both vocabularies share |
| `EventEmitter`, `NoOpEmitter`, `EmitOutcome` | `crucible-core/src/events/emitter.rs:293,366,151` | Emitter trait for the watch pipeline |
| `EventRing` | `crucible-core/src/events/ring.rs:74` | Bounded ring; write-only in production |
| `InteractionRequest`, `InteractionResponse`, `InteractionEvent` | `crucible-core/src/interaction/types.rs:380,479,540` | Seven request kinds a UI answers |
| `PermRequest`, `PermResponse`, `AskBatch`, `EditRequest`, `PopupRequest`, `InteractivePanel` | `interaction/{permission.rs:65,192; ask.rs:110; edit.rs:28; types.rs:37,237}` | |
| `DaemonEventBridge` | `crucible-daemon/src/file_watch_bridge.rs:27` | `EventEmitter` over the daemon broadcast bus |
| `EventRow`, `ROWS`, `HookedEvent` | `crucible-daemon/src/event_map.rs:87,103,230` | The one table from broadcast event to Lua hook name, both directions |
| `SYSTEM_SESSION`, `WEBHOOK_SESSION` | `crucible-daemon/src/event_map.rs:55,58` | Session ids for daemon-wide events |
| `SubscriptionManager`, `ClientId`, `WILDCARD_SESSION` | `crucible-daemon/src/subscription.rs:74,20,50` | Which client listens to which session |
| `DeferredShutdown` | `crucible-daemon/src/rpc/context.rs:30` | Arms shutdown; fired after the reply is written |
| `LogEvent` | `crucible-daemon/src/observe/events.rs` | Presentation event in `session.jsonl`, 11 variants (16 before plan T3-B7) |
| `RenderOptions` | `crucible-daemon/src/observe/markdown.rs:8` | Markdown export flags |
| `RecordingWriter`, `ReplaySession` | `crucible-daemon/src/recording.rs:21`, `replay.rs:14` | `recording.jsonl` writer and replayer |
| `RecordedEvent`, `RecordingHeader`, `RecordingFooter` | `crucible-core/src/recording.rs:31,15,66` | JSONL frames |
| `BackgroundJobManager` | `crucible-daemon/src/background_manager/mod.rs:57` | Bash jobs per session; emits `bash_job_*` |
| `JobInfo`, `JobResult`, `JobKind` | `crucible-core/src/background/types.rs:96,162,27` | Job model |
| `WorkflowRegistry`, `WorkflowStatusSnapshot` | `crucible-daemon/src/workflow_registry.rs:20,74` | Session id to live `WorkflowExecution` |
| `WorkflowEvent`, `AssessmentOutcome` | `crucible-core/src/workflow/events.rs:12,46` | Domain events the daemon bridges |
| `EventBroker` | `crucible-web/src/services/daemon.rs:1215` | Per-session `broadcast::Sender<SessionEvent>`; `"*"` fans out |
| `ChatEvent` | `crucible-web/src/events.rs:7` | Browser SSE projection of `TurnPayload`, 22 variants |
| `FsEvent` | `crucible-web/src/fs_events.rs:24` | `changed/deleted/moved` SSE payload |
| `WebhookSecrets`, `Signature` | `crucible-daemon/src/webhook/mod.rs:151,127` | HMAC verification for `POST /api/webhook/{name}` |

**Traits.** `EventEmitter` (1 required plus associated type, 2 defaulted, 1
production impl, `dyn` in six daemon signatures). `BackgroundSpawner`
(`crucible-core/src/background/mod.rs:22`, 4 required, 1 production impl).
`StepHandler` (`crucible-core/src/workflow/handler.rs:53`, 1 required, 3
production impls).

**Enters and leaves.** `emit_event` at `crucible-daemon/src/event_emitter.rs:29`
is the one place `seq` and `timestamp` are set; it returns `send().is_ok()`.
`SessionEventMessage::new` (untyped) has 36 production call sites across 21
daemon and CLI files. The persist task subscribes to `event_tx`, drops lagged
events with a warning (`server/mod.rs:551-556`), decodes with `should_persist`,
then matches event names by string again (`server/core/mod.rs:465-474`).
`forward_events` in `server/core/` inserts `stream_gap` markers and writes to
the client with a 30 s deadline (`core/mod.rs:109`), at most 32 requests in
flight per connection (`core/mod.rs:241`). The web `spawn_event_router` feeds
`EventBroker::dispatch`; browsers read SSE. Correlated replies: a permission
prompt parks a `PendingPermission` with a `oneshot::Sender` and two inline 300 s
timeouts (`messaging/permission.rs:166,937`); an interaction parks a
`PendingInteraction`; a delegation parks a `DelegationRecord` with a watch
sender (`delegation.rs:84`); `send_message_notified` returns a `TurnOutcome`
over a `oneshot` (`send.rs:487`). Everything in `crucible-core` here is
synchronous except `EventEmitter::emit`.

**Confirmed problems.**

- The scripting vocabulary was mostly hollow: production constructed 12 of 52
  `SessionEvent` plus `InternalSessionEvent` variants. Plan T3-B7 removed the
  other 40 on 2026-08-22; 11 remain. The `identifier`, `priority`,
  `category`, `estimate_tokens`, `payload` families still have no caller
  since the Reactor removal noted at `crucible-core/src/events/mod.rs:14-30`.
- `crucible-core/src/events/markdown/` (1,148 lines) is unreachable; the live
  renderer is `crucible-daemon/src/observe/markdown.rs`. The daemon also
  carries a second renderer, `observe/serde_md.rs` (687 lines), which copies
  `crucible-core/src/serde_md/serializer.rs` in full; the core copy has zero
  callers and the daemon copy has no production caller.
- `EventRing` is write-only: one push at
  `crucible-cli/src/tui/oil/chat_runner/actions.rs:562`, no read. Its
  `unsafe impl Send/Sync` at `ring.rs:408-409` is redundant.
- `EventEmitter` has one production impl and two defaulted methods nobody
  calls. `EventError` (five variants) is never constructed;
  `EmitOutcome.cancelled/.errors` are never set, so
  `watch/handlers/indexing.rs:255-263` is dead.
- Parallel enums across the two vocabularies, before plan T3-B7:
  `InternalSessionEvent::PostLlmCall` equalled `TurnPayload::PostLlmCall`
  field for field; `SessionEvent::SessionEnded` equalled `TurnPayload::Ended`;
  `SessionEvent::Interaction*` equalled `TurnPayload::Interaction*`;
  `Delegation*` and `BashTask*` mirrored `JobPayload` with renamed fields;
  `LogEvent::Bash*` had no production writer. All of those are gone. What
  remains: `SessionEvent::InteractionRequested` beside
  `TurnPayload::InteractionRequested` (the CLI constructs it), and
  `LogEvent::Subagent*` beside `JobPayload` (the daemon writes them).
- `Group::of` is a hand-maintained 70-name string match that mirrors the
  `rename_all` output of eight enums; drift surfaces at runtime as
  `UnknownEvent` (`session_events/mod.rs:133`).
- `rpc_client/client/types.rs:10 SessionEvent` duplicates
  `SessionEventMessage`; the client re-parses by hand and drops `seq` and
  `timestamp` (`client/mod.rs:418-439`). The CLI copies one into the other in a
  spawned task (`crucible-cli/src/tui/oil/chat_runner/runner.rs:168-179`).
- `DaemonCapabilities` (`rpc_client/client/types.rs:18`) has no server-side
  type; the server emits `json!` at `rpc/dispatch.rs:1103-1118`.
- Six `ChatEvent` variants are unreachable (`crucible-web/src/events.rs:30,35,84,89,94,116`);
  the `sse_event_names_match_the_frontend_listener_list` test checks names,
  not reachability. The `ChatError` prefix list is copied verbatim
  (`crucible-web/src/events.rs:401`, `rpc_client/agent/convert.rs:118`).
- No typed error crosses `DaemonClient`: the RPC error object is flattened to
  `anyhow!("RPC error: {}")`; `TRANSIENT_ERROR_PATTERNS` retries on message
  text (`rpc_client/client/mod.rs`); the web sniffs `-32602` as a substring
  (`crucible-web/src/error.rs:88`, `routes/search.rs:563`);
  `services/daemon.rs:213` retries on a "broken pipe" substring.
- `Request.jsonrpc` carries `#[allow(dead_code)]` and is read nowhere
  (`protocol/rpc/mod.rs:18`).
- `ClientId::as_u64` (`subscription.rs:32`) and `DeferredShutdown::subscribe`
  (`rpc/context.rs:56`) are production-dead.
- `BackgroundJobManager::get_job_result` returns `output: None` for a running
  job; callers must inspect `info.status`. `running_count`,
  `total_running_count` are test-only behind `#[allow(dead_code)]`
  (`background_manager/mod.rs:195,202`).
- Hand-rolled calendar math in `events/markdown/format.rs:319-392` and
  `parse.rs:58-164` beside a `chrono` dependency.
- Still true from the older notes: the daemon has no request middleware;
  `to_response` is the only shared seam in the dispatcher; Lua
  `sessions.interaction_respond` hardwires `respond_to_permission`
  (`sessions/register.rs:344`); `InteractionRequest::Show` has no response
  variant; `Response::error_with_data` has no production caller.

### 3.5 Wire bindings — four

**Owns.** Daemon JSON-RPC; web HTTP, SSE and WS; ACP; MCP. The four share
`SessionEventMessage` and nothing else. ACP types come from
`agent_client_protocol`; MCP types from `rmcp`.

**Modules.** JSON-RPC: `crucible-daemon/src/rpc/`, `server/`, `rpc_client/`,
`rpc_helpers.rs`, `protocol/` in core. Web: `crucible-web/src/{routes,
middleware,services}/`, `crucible-web/web/` (SolidJS). ACP: `crucible-daemon/src/acp/`,
`acp_handle.rs`, `acp_launch.rs`, `mcp_host.rs`, `crucible-cli/src/commands/acp/`.
MCP: `crucible-daemon/src/tools/{mcp_server,extended_mcp_server,mcp_gateway,
mcp_client,gateway_executor}.rs`, `mcp_server.rs`, `mcp/`.

**JSON-RPC types.**

| Type | Location | Purpose |
|---|---|---|
| `RpcMethod`, `METHODS` | `crucible-daemon/src/rpc/dispatch.rs:54,79` | Closed set of 148 method names from one `rpc_methods!` table |
| `RpcDispatcher` | `crucible-daemon/src/rpc/dispatch.rs:297` | Routes a `Request` to a handler |
| `RpcContext` | `crucible-daemon/src/rpc/context.rs:61` | Shared managers for handlers; 18 fields |
| `ServerContext` | `crucible-daemon/src/server/mod.rs:950` | Per-connection clone of the same handles; 13 fields, 8 unread |
| `Server`, `BindWithPluginConfigParams` | `crucible-daemon/src/server/mod.rs:98`, `server/bind.rs:14` | Boot sequence, accept loop, background tasks |
| `LuaSessionState`, `NoopSessionRpc` | `crucible-daemon/src/server/mod.rs:145,142` | Per-session Lua executor for `lua.init_session` |
| `SessionCreateError` | `crucible-daemon/src/server/session/create.rs:17` | `-32602` or `-32603` |
| `FsEntry`, `DirListing` | `crucible-daemon/src/server/fs/mod.rs:58,88` | `fs.list_dir` rows; mirrored in `crucible-web/web/src/lib/types.ts:244` |
| `RenameOutcome`, `SkippedRef` | `crucible-daemon/src/server/note_refactor.rs:48,35` | `note.rename` result |
| `OptionAction` | `crucible-daemon/src/server/plugins.rs:231` | Get, Set, Execute; the one handler type `crucible-web` deserializes |
| `DaemonClient` | `crucible-daemon/src/rpc_client/client/mod.rs:105` | Socket connection, id counter, pending map, reader task |
| `SessionCreateParams`, `SessionAgentSpec`, `SessionCreateRequest` | `crucible-daemon/src/rpc_client/client/session.rs:115,145,17` | Typed create inputs and wire shape |
| `SessionIdRequest` | `crucible-daemon/src/rpc_client/client/session.rs:219` | `{session_id}` for about 20 methods |
| `DaemonStorageClient`, `DaemonNoteStore` | `crucible-daemon/src/rpc_client/storage.rs:22,212` | Core storage traits over RPC for the CLI |
| `require_param!`, `optional_param!`, `typed_params` | `crucible-daemon/src/rpc_helpers.rs:22,49,119` | Server-side param decode |
| `SocketDirRefusal`, `socket_path` | `crucible-core/src/protocol/lifecycle.rs:77` | Per-uid 0700 socket dir |

**Web types.** `WebError` (`crucible-web/src/error.rs:13`), `AppState`
(`services/daemon.rs:18`), `ReconnectingDaemon` (`services/daemon.rs:57`, about
95 forwarding methods), `SwrCache` (`services/catalog.rs:24`), `ApiKeyState`
(`middleware/auth/mod.rs:44`), `HostPolicy` (`middleware/auth/host/mod.rs:30`),
`SessionStore` (`middleware/auth/session.rs:62`, 64 tokens, 30-day TTL),
`ShellGateState` (`middleware/auth/shell.rs:99`), `EndpointPolicy`
(`routes/session/mod.rs:195`), `Assets` (`assets.rs:34`).

**ACP types.** `CrucibleAcpClient` (`acp/client/mod.rs:64`), `ClientConfig`
(`acp/client/types.rs:8`), `StreamingChunk` (`acp/streaming.rs:23`),
`AgentInfo` (`acp/discovery.rs:27`), `BUILTIN_AGENTS` (`acp/discovery.rs:52`,
five profiles), `AcpAgentHandleParams` (`acp_handle.rs:86`), `InProcessMcpHost`
(`mcp_host.rs:63`, binds `127.0.0.1:0` and serves `/mcp`), `Recorder`,
`ReplayFixture` (`acp/client/recording.rs:58`, `replay.rs:28`),
`CrucibleAcpAgent` (`crucible-cli/src/commands/acp/agent.rs:50`, the CLI side
that serves ACP on stdio and proxies to the daemon).

**MCP types.** `CrucibleMcpServer` (`tools/mcp_server.rs:58`, rmcp router over
Note, Search, Kiln, jobs, delegation), `DelegationContext`
(`tools/mcp_server.rs:69`), `ExtendedMcpServer`, `ExtendedMcpService`
(`tools/extended_mcp_server.rs:53,455`), `McpGatewayManager`, `UpstreamClient`
(`tools/mcp_gateway.rs:207,65`), `RmcpExecutor` (`tools/mcp_client.rs:44`),
`McpServerManager` (`mcp_server.rs:40`, the standalone `cru mcp` process),
`McpToolInfo`, `McpServerInfo`, `ContentBlock` (`crucible-core/src/traits/mcp.rs:116,138,33`).

**Enters and leaves.** JSON-RPC: one JSON line per request over the Unix
socket; `server/core/handle_client` checks the peer uid, then serves requests
concurrently. Every handler returns a hand-spelled `serde_json::json!` result.
Two parameter idioms coexist: `require_param!` (56 uses) and `typed_params`
(9 handler files); 57 request structs are shared with the client and validated
by `tests/architecture_tests/wire_types.rs`, while 46 are client-only. Web:
every `/api/*` route except `/health`, `/ready`, `/api/auth/*` sits behind
`bearer_auth`; `host_guard` wraps the app; one reconnect after a connection
error except `scm_clone` and the four `review.*` writes. ACP: child process
stdio, one global `REQUEST_ID` (`acp/client/mod.rs:28`); `read_response_line`
uses a 5-minute per-read floor (`acp/client/io.rs:107-112`); unhandled inbound
requests get `-32601`. MCP: `InProcessMcpHost` URL goes into
`NewSessionRequest.mcp_servers`; `build_stdio_mcp_server` resolves `cru` beside
`current_exe()` (`acp/protocol.rs:185`).

**Confirmed problems.**

- `ServerContext` duplicates `RpcContext` with 8 unread fields under a
  struct-level `#[allow(dead_code)]` (`server/mod.rs:950`, `rpc/context.rs:60`).
- `dispatch_session_setter!` and `dispatch_session_getter!` re-match the raw
  method string after the enum match with `_ => unreachable!()`
  (`rpc/dispatch.rs:273-295`); a new setter variant compiles and panics.
- `RpcMethod::SessionReindex` is retired but stays in `METHODS`, so
  `daemon.capabilities` advertises a method that always fails
  (`rpc/dispatch.rs:175,591`).
- `storage.verify/cleanup/backup/restore` return `not_implemented`
  (`server/storage.rs:3-41`) and are dispatched.
- `kiln.rs` and `observe.rs` still use `require_param!` for 13 handlers;
  `handle_note_upsert` hand-deserializes `NoteRecord` (`server/kiln.rs:556`).
  `FtsResult` is hand-serialized at `server/kiln.rs:329` and re-parsed as
  `TextSearchHit` (`rpc_client/client/storage_requests.rs:180`).
- 15 one-field `SessionSet*Request` structs and 15 get/set pairs
  (`rpc_client/client/agent.rs:32-144`); 16 `cached_*` fields on
  `DaemonAgentHandle` mirror `SessionAgent`.
- `NoteRecordDto::into_parsed_note` sets `offset = index` and
  `target_span = (0, 0)` (`rpc_client/storage.rs:100-113`); `WikilinkDto`
  (`storage.rs:60`) is a lossy copy of `Wikilink`.
- `StorageClient` has one impl whose one method always bails
  (`rpc_client/storage.rs:45-52`). `McpStartRequest.just_dir` is accepted and
  ignored (`storage_requests.rs:146`).
- Six dead client wrappers: `kiln_set_classification`, `lua_register_commands`,
  `note_move`, `session_cache_stats`, `session_can_undo`, `session_undo_depth`.
  `require_session_id!` and `session_id_field` have no callers (`rpc_helpers.rs:72,145`).
- `Server.web_config` and `web_cancel` are a stub; both CLI call sites pass
  `None` (`server/mod.rs:127,472`, `server/bind.rs:27`). `Server::bind` has no
  caller (`server/bind.rs:75`). Nine `#[allow(dead_code)]` in `server/mod.rs`
  and `server/bind.rs`.
- `plugin_boot.rs:93,139,148` and `rpc/ui.rs:152` read `dirs::config_dir()`
  while `RpcContext.config_home` exists (`context.rs:93-97`); an in-process
  test daemon evaluates the developer's real `init.lua`.
  `platform.rs:58,108,150` call `current_dir()` inside the daemon and repeat
  the same `spawn_blocking` skill-discovery block three times.
  `workflow_handlers.rs:237` reads `CRUCIBLE_WORKFLOW_DRY_RUN` per request.
- `Server::run` is about 500 lines with four inline task bodies
  (`server/mod.rs:452-945`). `handle_session_status` lives in `plugins.rs:106`.
  `lua_plugin_suite.rs:438` reads its own source with `include_str!` to
  enumerate test arms.
- `LuaSessionState` is built on two paths (`server/lua.rs:23-39`,
  `rpc/dispatch.rs:2516-2560`). `validate_grep_root` (`server/grep.rs:84`)
  re-implements `fs::resolve_root` (`server/fs/mod.rs:355`).
- Web: `ReconnectingDaemon` is about 95 six-line wrappers split over four files
  by a line budget. Seven hand-built error bodies (`error.rs:61`,
  `middleware/auth/mod.rs:398,411`, `middleware/auth/shell.rs:176,190`,
  `routes/webhook.rs:149`, `routes/auth.rs:92`). `OkResponse`
  (`routes/session/mod.rs:21`) beside eight `json!({"ok": true})` literals.
  `NoteListItem` is a positional 5-tuple (`routes/helpers.rs:22`).
  `session_get_precognition_results` hides `unwrap_or(5)` in the transport
  wrapper (`services/daemon.rs:1038`). `KeepAlive.shell` is never `Some`
  (`routes/terminal.rs:56`). `handle_webhook` returns `Result<Json, Response>`
  (`routes/webhook.rs:68`). Dead wrappers: `capabilities`, `note_upsert`,
  `lua_discover_plugins`, `lua_plugin_health`, `session_create`,
  `agents_resolve_profile`.
- ACP: the built-in agent table is written twice (`acp/discovery.rs:52`,
  `acp_launch.rs:126`) with no gate. `acp/tools.rs` describes 10 tools,
  executes 2, and its `ToolDescriptor` (`tools.rs:29`) duplicates core
  `ToolDefinition` (`crucible-core/src/traits/tools.rs:208`). `acp/mock_agent.rs:17`
  has a module-level `#![allow]`. `Recorder::from_env` reads env on every
  `with_name` (`acp/client/recording.rs:71,81`); a test calls
  `std::env::remove_var` (`recording.rs:263`). Timeout arithmetic is split
  across `acp_launch.rs:63`, `client/io.rs:110`, `client/streaming.rs:194`.
  `acp/mod.rs` exports `StreamHandler`/`StreamConfig` that no production code
  uses; the live API is `StreamingChunk` plus `channel_callback`.
  `AcpAgentHandle.session_id` is `Option` and never `None`; `ClientConfig.max_retries`
  is always `None`. `mcp_server.rs:77,113` opens the kiln twice.
- MCP: the gateway half of `ExtendedMcpServer` and `McpGatewayManager::start_reconnect_loop`
  have no callers (`extended_mcp_server.rs:126`, `mcp_gateway.rs:499`);
  `surface.rs:292` documents an ordering that cannot occur.
  `ExtendedMcpServer::new` is `async` with no await and never `Err`
  (`extended_mcp_server.rs:78`). `discovery_tools()` advertises
  `source` enum `["builtin","lua"]` while the classifier never returns `"lua"`
  (`extended_mcp_server.rs:172`, `tool_discovery.rs:76`).
  `CrucibleMcpServer::get_info` lists workspace tools the router does not
  serve (`mcp_server.rs:714`). Five copies of `CallToolResult -> Value`
  (`tool_dispatch.rs:80,423`, `workspace.rs:564`, `gateway_executor.rs:52`,
  `extended_mcp_server.rs:405,421`); three of `ToolDefinition -> rmcp::Tool`;
  three of `rmcp::Tool -> ToolDefinition`.
- The CLI reads daemon storage directly as a fallback:
  `FileSessionStorage::root_for` and `parse_session_log`
  (`crucible-cli/src/commands/session/io.rs:21,36`), `load_events` and
  `render_to_markdown` (`crucible-cli/src/tui/oil/chat_runner/actions.rs:795-804`).

### 3.6 Lua

**Owns.** Projection of Rust types into Lua and interception of the turn loop.
Projection modules are safe in isolation. Interception is not:
`runtime/defaults/init.lua` is compiled in as `BUILTIN_INIT_LUA`
(`crucible-lua/src/lib.rs:164`) and is the only definition of the three
permission modes, the priority-1000 deny hook, the default system prompt and
the precognition formatter. `ModeRegistry` has no Rust default.

**Modules.** `crucible-lua/src/` (26 top-level projection files, the bindings,
`handlers/`, `lifecycle/`, `lua_stdlib/`, `sessions/`, `vault/`),
`crucible-daemon/src/daemon_plugins/`, `plugin_tools.rs`, `plugin_ops.rs`,
`runtime_defaults.rs`, `rules_files.rs`, `skills/`, `session_bridge.rs`,
`tools_bridge.rs`, `agent_manager/session_vm.rs`, `server/{lua,plugins,
plugin_boot,plugin_install}.rs`, `rpc/ui.rs`, `runtime/`.

**Types.**

| Type | Location | Purpose |
|---|---|---|
| `LuaExecutor` | `crucible-lua/src/executor.rs:25` | Owns the VM, the Fennel compiler, the current session |
| `PluginManager` | `crucible-lua/src/lifecycle/mod.rs:31` | Discovers, loads, reloads, enables plugins |
| `PluginSpec` | `crucible-lua/src/lifecycle/spec.rs:16` | Parsed spec table an `init.lua` returns |
| `PluginManifest`, `Capability`, `PluginState`, `PluginSource` | `crucible-lua/src/manifest.rs:48,80,325,295` | `plugin.yaml` model |
| `LuaScriptHandlerRegistry`, `RuntimeHandler` | `crucible-lua/src/handlers/registry.rs:32,48` | `crucible.on` registrations |
| `StageId`, `EventName`, `HookName` | `crucible-lua/src/handlers/hook_name.rs:119,44,193` | 11 stages, 8 events, union for validation |
| `ScriptHandlerResult` | `crucible-lua/src/handlers/script_handler.rs:15` | Transform, PassThrough, Cancel, Inject, Handled |
| `EventOutcome` | `crucible-lua/src/handlers/script_handler.rs:110` | Observed or StopChain for broadcast events |
| `PermissionHook`, `PermissionRequest`, `PermissionHookResult` | `crucible-lua/src/handlers/permission.rs:48,25,14` | `crucible.permissions.on_request` |
| `ToolBeforeExecuteEvent`, `ToolDisplayStartHints`, `ToolDisplayCompleteHints` | `handlers/before_execute.rs:12`, `display_hooks.rs:18,42` | Tool hook payloads |
| `DaemonSessionApi` | `crucible-lua/src/sessions/mod.rs:103` | `cru.sessions.*` contract; the daemon implements it |
| `DaemonSessionBridge` | `crucible-daemon/src/session_bridge.rs:23` | The one production `DaemonSessionApi` |
| `DaemonToolsApi`, `DaemonToolsBridge` | `crucible-lua/src/tools_api.rs:95`, `crucible-daemon/src/tools_bridge.rs:19` | `cru.tools.*` contract and impl |
| `SessionConfigRpc`, `Session`, `CurrentSession` | `crucible-lua/src/session_api.rs:67,297,489` | Lua `session` userdata and its knob contract |
| `SessionDefaults`, `SessionDefaultValues`, `SessionDefaultsRpc` | `crucible-lua/src/session_defaults.rs:70,52,199` | `cru.defaults` |
| `ModeRegistry`, `ModeDefinition`, `ModePermissions`, `ModeStance`, `ToolSelector` | `crucible-lua/src/modes.rs:161,149,132,42,77` | `cru.modes`; permission modes live here |
| `IsolationRegistry` | `crucible-lua/src/isolation.rs:122` | `crucible.require_isolation` |
| `StatusRegistry`, `PublicationRegistry`, `ContextAttachRegistry`, `OptionsRegistry`, `StatuslineExprRegistry`, `LuaValidatorRegistry` | `plugin_status.rs:54`, `publications.rs:33`, `context_attach.rs:74`, `options.rs:65`, `statusline_exprs.rs:68`, `context.rs:187` | Daemon-read registries behind `Arc<Mutex>` |
| `ThemeConfig`, `ThemeColors` | `crucible-lua/src/theme.rs:407,23` | Theme schema; 45 `AdaptiveColor` slots |
| `Layout`, `StatusItem`, `Region`, `Element` | `crucible-lua/src/statusline_items.rs:171,111,52,160` | Closed statusline vocabulary |
| `UiGeometry`, `UiLayout` | `crucible-lua/src/ui_geometry.rs:65,56` | Closed surface set |
| `HlGroup`, `HlRegistry`, `ResolvedHl` | `crucible-lua/src/hl.rs:68,92,82` | Highlight groups |
| `ConfigState`, `CONFIG` | `crucible-lua/src/config.rs:38,55` | Process-global `OnceLock<RwLock<ConfigState>>` |
| `LuaNode` | `crucible-lua/src/oil.rs:138` | `cru.oil.*` node handle; nothing in the CLI consumes it |
| `PathsContext` | `crucible-lua/src/paths.rs:33` | kiln, session, workspace paths |
| `FennelCompiler` | `crucible-lua/src/fennel.rs:53` | Cached `compileString`; on by default |
| `DaemonPluginLoader` | `crucible-daemon/src/daemon_plugins/mod.rs:113` | One VM for all plugins; 16 fields, 1404 lines |
| `PluginServiceFn`, `BootstrapOutcome` | `daemon_plugins/mod.rs:57`, `bootstrap.rs:133` | Service spawn input; git clone result |
| `PluginRegistry`, `PluginToolExecutor` | `crucible-daemon/src/plugin_tools.rs:43,296` | Lua tools and commands by name |
| `Skill`, `SkillScope`, `ResolvedSkill`, `FolderDiscovery` | `crucible-daemon/src/skills/{types.rs:55,14,85; discovery.rs:43}` | Agent Skills from `SKILL.md` |
| `DefaultsSource` | `crucible-daemon/src/runtime_defaults.rs:30` | Where `defaults/init.lua` came from |

**Traits.** `DaemonSessionApi` (17 required, 15 defaulted to
`Err("not implemented")`, 1 production impl, firewall). `DaemonToolsApi` (4
required, 1 production impl, firewall). `SessionConfigRpc` (0 required, 22
defaulted, 3 production impls of which `NoopSessionRpc` is empty, 6 test doubles
of which 5 are empty).

**Enters and leaves.** Lua calls `cru.*` and `crucible.*` through
`create_function` (sync) and `create_async_function` (async). The daemon calls
`runtime_handlers_for` and `execute_runtime_handler` (async),
`execute_permission_hooks` (sync by design), `execute_tool_*_hooks` (async).
Plugin code runs in one VM; session VMs are per session (`session_vm.rs`).
`register_permission_hook_api` is called only from `session_vm.rs:113`; the
plugin loader never registers it. `load_plugin_spec` spawns a fresh sandboxed
`Lua` per spec (`spec.rs:140`) and the daemon executes the same file again in
the real VM (`discovery.rs:295`). `ChannelSessionRpc` uses `blocking_recv`
(`session_api.rs:156`). The plugin contract over RPC is ten opaque `plugin.*`
methods plus `ui.config` and `ui.set_theme`; the daemon stores opaque JSON.

**Confirmed problems.**

- `SessionConfigRpc` requires nothing (`session_api.rs:67`); five impls are
  `impl SessionConfigRpc for X {}`. `DaemonSessionApi` defaults 15 of 32
  (`sessions/mod.rs:288-458`).
- `SessionCommand`, `ChannelSessionRpc` and the CLI `handle_session_command`
  (`crucible-cli/src/tui/oil/chat_runner/commands.rs:15-114`) form a dead
  cross-crate path; `with_session_command_receiver` has no caller.
- `parse_capability` (`lifecycle/spec.rs:31`) hand-duplicates the serde
  `Deserialize` of `Capability` and omits `intercept_tools`, so a spec-table
  grant is dropped with a warning (`discovery.rs:267`).
- `register_permission_hook_api` names hooks from `guard.len()`
  (`handlers/permission.rs:132`), the pattern `crucible_on.rs:86-94` forbids.
- 28 `cru.sessions` names are listed twice as strings
  (`sessions/register.rs:26-74,187-867`); 6 `cru.kiln` names twice
  (`vault/mod.rs:64,133`); no check keeps them equal.
- `execute_runtime_json_handler` (`handlers/before_execute.rs:83`) repeats
  `execute_runtime_handler` (`registry.rs:192`). `runtime_handlers_for` and
  `execute_runtime_handler` `expect()` on a poisoned mutex (`registry.rs:157,204`).
- Colour codec duplicated four ways between `theme.rs`, `theme_wire.rs` and
  `hl_lua.rs` (`theme_wire.rs:40` vs `hl_lua.rs:105`; `theme.rs:740` vs
  `hl_lua.rs:40`; `theme.rs:713` vs `hl_lua.rs:18`; `theme_wire.rs:80` vs
  `hl_lua.rs:132`).
- `ThemeLayout` and `UiLayout` are twins (`theme.rs:375`, `ui_geometry.rs:56`);
  `ThemeIcons`, `ThemeSpinnerStyle`, `BorderStyle`, `StatusBarPosition` are
  parsed and serialized but no renderer reads them.
- `crucible.notify` appends to a queue only tests drain (`notify.rs:78,110,241,262`)
  while `docs/Help/Lua/Language Basics.md:74` documents it.
- `cru.defaults.mode` is stored but never exposed to Lua
  (`session_defaults.rs:92-175`).
- `CONFIG` is process-global (`config.rs:55`); every VM shares theme state.
- `daemon_plugin_paths` (`daemon_plugins/bootstrap.rs:33`) and
  `PluginManager::with_standard_paths` (`lifecycle/mod.rs:112`) both compute
  the plugin path list. `expand_tilde` (`bootstrap.rs:113`) equals
  `kiln_manager::expand_tilde_path` (`kiln_manager.rs:1199`).
- `DaemonPluginLoader` stores `plugin_config` and copies it into a Lua table
  (`daemon_plugins/mod.rs:452`). `PluginSpec.handlers` is parsed and never
  dispatched (`daemon_plugins/mod.rs:741`).
- Skills: `Skill`, `SkillSource`, `SkillScope`, `ResolvedSkill` derive serde
  but `server/platform.rs:66-180` copies fields by hand and drops five;
  `Skill.content_hash` is computed with SHA-256 on every discovery and never
  read (`skills/discovery.rs:176`); `SkillError::NotFound` is never built;
  `skills/test_utils.rs` is an empty `pub mod` in production; `SkillParser` is
  a stateless unit struct (`skills/parser.rs:8`); the `name` equals directory
  rule is documented and not checked (`skills/types.rs:95`).
- `StubGenerator::verify` writes under `std::env::temp_dir()` (`stubs.rs:82`).
  `hooks.rs:14-26` and `auth_plugin.rs:15-33` `unwrap()` inside `unwrap_or_else`.
  `fennel.rs:44` carries a stale `#[allow(unused_imports)]`.
- `LuaTool`/`DiscoveredTool` and `ToolParam`/`DiscoveredParam` are duplicate
  shapes (`types.rs:9,28`, `discovered.rs:20,31`); `executor.rs:306,418`
  (`execute_file`, `execute_tool`) and `execute_source` have no production
  caller, so `types.rs` and `schema.rs` have no production reader.
- Test-only public API on `PluginManager`: `active_plugins`, `eval_runtime`,
  `reload`, `enable`, `initialize`, `error_log`, `with_search_paths`,
  `load_plugin_spec_from_source`.
- `shell.rs:159-185,289-312` repeat Command setup; `http.rs:50-116` repeats
  five closures; `fs.rs:58,75,127,145` repeat the ensure-parent block.

Still true from the older notes: Fennel compiles and runs in the daemon VM
(`shipped.rs::a_fennel_plugin_executes_in_the_daemon_vm`); `StubGenerator`
emits Lua stubs only; the `targets` channel plus `resolve_command` is the
end-to-end publication pattern (`workspace_targets.rs`); the web shows plugin
commands only as a count and has no renderer for `crucible.set_status` slots.

### 3.7 Render

**Owns.** TUI and web presentation.

**Modules.** `crucible-cli/src/tui/` (44k lines), `crucible-cli/src/formatting/`,
`crucible-oil/src/`, `crucible-web/web/` (SolidJS), `crucible-web/src/events.rs`.

**Types.**

| Type | Location | Purpose |
|---|---|---|
| `OilChatApp` | `crucible-cli/src/tui/oil/chat_app/mod.rs:44` | Elm-style TUI state and reducer; 37 fields |
| `ChatAppMsg` | `crucible-cli/src/tui/oil/chat_app/messages.rs:58` | 75 variants; commands and events in one enum |
| `OilChatRunner` | `crucible-cli/src/tui/oil/chat_runner/mod.rs:75` | Async event loop over terminal, daemon events, interactions |
| `SessionEventStream` | `crucible-cli/src/tui/oil/chat_runner/stream.rs:34` | Stateful `SessionEvent` to `ChatAppMsg` converter |
| `App`, `ViewContext`, `Action` | `crucible-cli/src/tui/oil/app.rs:68,7,87` | Loop contract; per-frame render inputs |
| `Component`, `ComponentHarness` | `crucible-cli/src/tui/oil/component.rs:6,19` | View contract; test harness in production code |
| `InputBuffer`, `Event`, `InputAction` | `crucible-cli/src/tui/oil/event.rs:63,4,11` | Line editor with history |
| `ContainerList`, `ChatNode` | `crucible-cli/src/tui/oil/containers.rs:219,26` | Viewport blocks and graduation |
| `CachedToolCall`, `CachedSubagent`, `CachedShellExecution`, `ToolSourceDisplay` | `crucible-cli/src/tui/oil/viewport_cache.rs:47,203,167,10` | Display state |
| `RuntimeConfig`, `ConfigStack`, `ConfigValue`, `ShortcutRegistry` | `crucible-cli/src/tui/oil/config/{overlay.rs:44; stack.rs:93; value.rs:28; shortcuts.rs:182}` | `:set` overlay engine, about 2,500 lines |
| `SetCommand`, `SetEffect`, `SetRpcAction` | `crucible-cli/src/tui/oil/commands/set.rs:14,55,28` | `:set` parse and TUI-local versus RPC split |
| `InputComponent`, `InputMode` | `crucible-cli/src/tui/oil/components/{input_component.rs:11; input_area.rs:6}` | Bordered multi-line input |
| `StatusBar`, `StatusBarData`, `StatusComponent` | `components/{status_bar.rs:36; status_items.rs:19; status_component.rs:11}` | Three field-identical status snapshots |
| `InteractionModal`, `InteractionModalOutput` | `components/interaction_modal/mod.rs:62,40` | All seven request kinds |
| `ShellModal` | `components/shell_modal.rs:57` | Runs `sh -c`, streams, writes `<session_dir>/shell/*.output` |
| `NotificationArea`, `NotificationComponent`, `ThinkingComponent`, `TurnIndicator`, `CommandPanel` | `components/` | |
| `RenderStyle`, `Margins` | `crucible-cli/src/tui/oil/markdown/mod.rs:66,41` | Markdown-it AST to `Node` |
| Theme stores | `crucible-cli/src/tui/oil/theme/{global,groups,geometry,bars,exprs}.rs` | Five process-wide `RwLock` stores, leak-on-install |
| `SyntaxHighlighter` | `crucible-cli/src/formatting/syntax.rs:78` | syntect wrapper; `formatting/` imports `tui/oil` |
| `NoopAgentHandle`, `AppHarness` | `crucible-cli/src/tui/oil/noop_agent.rs:24`, `test_harness.rs:8` | Replay agent; test driver in a non-test module |
| `Node`, `BoxNode`, `TextNode`, `InputNode`, `PopupNode` | `crucible-oil/src/node.rs:14,72,59,97`, `popup_node.rs:14` | UI tree |
| `Style`, `Color`, `AdaptiveColor`, `Border`, `Padding`, `Gap` | `crucible-oil/src/style.rs:5,136,296,404,359,564` | |
| `LayoutEngine`, `LayoutTree`, `LayoutContent` | `crucible-oil/src/taffy_layout.rs:17`, `layout/types.rs:25,124` | Taffy layout; `LayoutContent` mirrors `Node` |
| `CellGrid`, `StyledCell` | `crucible-oil/src/cell_grid.rs:25,6` | 2-D styled cell buffer |
| `FramePlanner`, `FramePlan`, `Graduation`, `RenderedOverlay` | `crucible-oil/src/planning.rs:92,26,14,20` | One frame per call; scrollback hand-off |
| `Terminal<W>`, `OutputBuffer<W>` | `crucible-oil/src/terminal.rs:17`, `output.rs:20` | Raw mode, cursor, line-diff writer |
| `FrameRenderer`, `TestRuntime` | `crucible-oil/src/runtime.rs:7,23` | Render contract; headless `Terminal<Vec<u8>>` |
| `FocusContext` | `crucible-oil/src/focus.rs:25` | Focus ring |
| `Drawer`, `PopupOverlay`, `InputArea` | `crucible-oil/src/components/{drawer.rs:39; popup.rs:8; input_area.rs:29}` | Component set; `InputArea` has no caller |
| `ChatEvent` | `crucible-web/src/events.rs:7` | SSE projection for the browser |

The web frontend mirrors `ToolDisplay`, `ProviderInfo`, `ModeDescriptor`,
`Notification`, `PopupEntry`, `PanelItem`, `PermRequest`, `AskBatchRequest`,
`AskBatchResponse`, `FsEntry`, `FsListing` in `crucible-web/web/src/lib/types.ts`.

**Traits.** `App` (3 required, 2 defaulted, 1 impl). `Component` (1 required,
1 blanket plus 8 structs, no `dyn`). `FrameRenderer` (3 required, `Terminal<W>`
plus `TestRuntime`). `InputStyle` (`crucible-oil/src/components/input_area.rs:9`,
2 required, 2 defaulted, 1 production impl).

**Enters and leaves.** crossterm key and resize events (async `EventStream`);
`crucible_daemon::SessionEvent` over mpsc; `InteractionEvent` over the agent
handle receiver; `ui.config` JSON into `apply_ui_config` (`theme/remote.rs:24`).
Out: `AgentHandle` calls; fresh `DaemonClient::connect()` calls from spawned
tasks for `config.set`, `lua.eval`, `plugin_list`, `plugin_reload`,
`plugin_run_command` (`chat_runner/actions.rs`); `Node` trees to `Terminal` or
any `FrameRenderer`. Internal oil pipeline: `Node`, `LayoutEngine`,
`LayoutTree`, `render_layout_tree`, `CellGrid`, `String`, `FramePlanner`,
`Terminal::apply`, `OutputBuffer`. All of oil is synchronous. The reducer is
sync; `process_action` runs side effects, then `on_message`, then recurses;
`drain_pending_messages` calls only the reducer and drops side effects
(`chat_runner/actions.rs:110-120`, `mod.rs:273-277`).

**Confirmed problems.**

- Business logic in the CLI: the `config/` overlay engine, `:set` semantics,
  help text and `parse_config_scalar` (`chat_app/command_handling.rs`);
  `ExportSession` loads events and renders markdown in-process
  (`actions.rs:784-827`); `chat_app/shell.rs:123` writes a permission rule to
  a config file; `ShellModal::spawn` runs `sh -c` and `save_output` writes
  under the session dir (`shell_modal.rs:72,320`); RPC error JSON is parsed by
  hand (`actions.rs:619-633`); `kiln_validate.rs` (500 lines),
  `provider_detect.rs` (541 lines) and `kiln_discover.rs` decide where a kiln
  may live and which providers exist.
- Four hand-kept REPL command lists (`autocomplete.rs:201`,
  `command_handling.rs:20`, `autocomplete.rs:474`, `command_handling.rs:99`) and
  a two-entry hard-coded palette subset (`autocomplete.rs:183`).
- `ToolSourceDisplay` duplicates core `ToolSource` (`viewport_cache.rs:10`,
  `crucible-core/src/types/tool_ref.rs:41`); the daemon formats
  (`agent_manager/messaging/mod.rs:17`) and the CLI parses
  (`chat_app/message_handlers.rs:15`) a `Mcp:x` string.
- `McpServerDisplay` (`chat_app/model_state.rs:11`) repeats core `McpServerInfo`.
- `truncate_to_width` and `truncate_to_chars` exist with the same body in
  `crucible-oil/src/utils.rs:35,123` and
  `crucible-cli/src/tui/oil/utils/truncate.rs:32,128`; the CLI mixes both.
  `wrap_content`/`wrap_chars`, `clamp_input_lines` and `InputArea`/`InputComponent`
  are also duplicated across the two crates.
- In-crate oil duplicates: `StyledCell` (`cell_grid.rs:6`, `overlay.rs:17`),
  `cells_to_string` (`cell_grid.rs:239`, `overlay.rs:87`), `popup_item`
  (`popup_node.rs:50`, `components/popup.rs:149`), `PopupItemNode`/`PopupItem`
  (`popup_node.rs:37`, `layout/types.rs:204`), `InputNode`/`LayoutContent::Input`,
  `ComputedLayout`/`Rect`, two OSC parsers (`ansi.rs:62`, `cell_grid.rs:105`).
- Three status structs cloned per frame (`status_bar.rs:155-166`,
  `status_component.rs:82-96`). Four copies of the `RwLock<Option<&'static T>>`
  store. `RenderStyle::Viewport` and `::Natural` compute identical widths
  (`markdown/mod.rs:64-133`). `InputComponent::view` re-implements
  `InputStyle::display_content/display_cursor` inline (`input_component.rs:115-126`).
- `RuntimeConfig::new` is never called; production uses `empty()`, so every
  `ShortcutTarget::Path` default is `String("")` and numbers stay strings
  (`config/overlay.rs:59`, `chat_app/defaults.rs:52`).
  `resolve_dynamic` hard-codes `key == "model"` (`overlay.rs:406`).
  `ConfigValue::parse_bool` calls `expect` on user input (`value.rs:127`).
- Dead or write-only: `OilChatRunner.{recording_mode, recording_path,
  available_models, session_cmd_rx}`; `Role` (`chat_app/state.rs:5`);
  `InteractionModalOutput::{Close, Notify}` never produced but matched
  (`chat_app/shell.rs:104,157`); `OilRunner`, `run_sync`, `ComposerConfig`
  (`runner.rs:10,125`, `composer.rs:28`); `template/node_spec.rs` (1008 lines)
  dead apart from `parse_color`; `Terminal::with_alternate_screen` has no
  caller so `use_alternate_screen` is always false (`terminal.rs:168`);
  `detect_dark_terminal` (`style.rs:333`); the oil `serde` feature is enabled by
  no workspace crate.
- `AdaptiveColor::resolve` reads `NO_COLOR` (`style.rs:313`) so callers cannot
  inject it. `FramePlanner::plan_frame` takes `Option<Graduation>` by value and
  `Terminal::render_frame` clones it every frame (`terminal.rs:318`).
- `DrawerKind` has one variant (`components/drawer.rs:6`).
  `PopupOverlay::view` ignores `_focus` (`components/popup.rs:124`).
- Unicode glyphs formatted in the runner (`actions.rs:678,715`).
  `ShellModal::open_in_editor` blocks the render thread (`shell_modal.rs:281`).
  `markdown/context.rs:158` keys a cache on a 64-bit hash of the input.
- The CLI depends on `crucible-daemon` for a display helper:
  `acp::streaming::humanize_tool_title` (`components/tool_render.rs:77,408`).
- Two levenshtein implementations (`command_handling.rs:44`,
  `crucible-lua/src/handlers/crucible_on.rs:12`); two bool parsers
  (`commands/set.rs:449`, `config/value.rs:127`); two markdown renderers in
  the CLI (`formatting/markdown_renderer.rs`, `tui/oil/markdown/`).

Still true from the older notes: `statusline_items::builtin_default()` and
`ThemeConfig::default_dark()` are the embedded defaults, both in `crucible-lua`;
`TurnPayload` is matched exhaustively by both renderers; `GenaiAgentHandle`
never yields `TurnEvent::ToolResult` while `AcpAgentHandle` does
(`acp_handle.rs:629,716`); keybinding remaps remain unimplemented.

## 4. Cross-crate dependency graph

| From | To | What it imports |
|---|---|---|
| `crucible-oil` | none | Oil depends on no workspace crate |
| `crucible-lua` | `crucible-core` | `LOCATION_CONFIG_KEYS`, `crucible_home`, `estimate_tokens`, `NoteStore`, `NoteRecord`, `Scope`, `PropertyStore`, `HttpExecutor`, `glob_match`, `Notification`, `sanitize_single_line`, `is_display_hostile`, `ToolSurface`, `AuthHeaders`, `SessionEvent`, `InternalSessionEvent`, `DEFAULT_SYSTEM_PROMPT`, `InteractionRequest::KINDS`, `default_true` |
| `crucible-lua` | `crucible-oil` | `style::{AdaptiveColor, Color, Border, BorderChars, Padding}`, node builders, `template::{html_to_node, parse_color}` |
| `crucible-daemon` | `crucible-core` | 24 files import `events`, 33 import `protocol`; `session` 171 import lines, `turn` 146, `types` 59; every storage, tool, chat and config trait and type listed in section 3 |
| `crucible-daemon` | `crucible-lua` | `LuaExecutor`, `PluginManager`, `PluginSpec`, every `register_*` function, all registries, `DaemonSessionApi`, `DaemonToolsApi`, `SessionConfigRpc`, `StageId`, `EventName`, `ModeRegistry`, `ToolSelector`, `IsolationRegistry`, `SandboxExec`, `BUILTIN_INIT_LUA`, `StubGenerator`, theme and statusline modules for `ui.config` |
| `crucible-web` | `crucible-core` | `CliAppConfig`, `WebConfig`, `KilnName`, `read_project_config`, `ProjectFileAccess`, `SessionEventPayload` and groups, `InteractionResponse`, `SessionAgent`, `NoteRecord`, `SessionModes`, `Project`, `Canvas` and containment, `is_note_file`, `EXCLUDED_DIRS`, `PrecognitionNoteInfo`, `ChatError`, `TokenUsage` |
| `crucible-web` | `crucible-daemon` | `DaemonClient`, `SessionEvent`, `DaemonCapabilities`, `Lua*Request/Response`, `GrepSearchResponse`, `ScmCloneResponse`, `SessionCreateParams`, `SessionAgentSpec`, `GrepSearchRequest`, `agent_manager::providers::ProviderInfo`, `subscription::WILDCARD_SESSION`, `server::plugins::OptionAction`, `webhook::*`, `project_manager::{forbidden_root_reason, resolve_registration_root}` |
| `crucible-cli` | `crucible-core` | config loaders and writers, credentials, parser types (`TaskFile`, `TaskGraph`, `WorkflowDoc`, `extract_frontmatter`), `AgentCardRegistry`, `AgentCardLoader`, `EventRing`, `SessionEvent`, `AgentHandle`, `Agent`, `StorageClient`, `NoteStore`, `KnowledgeRepository`, interaction types, `types::*` (72 import lines), `recording::*`, `FuzzyMatcher`, `bundled_docs`, `runtime_roots` |
| `crucible-cli` | `crucible-daemon` | `DaemonClient` (25+ files), `SessionEvent` (31), `SessionCreateParams` (38), `DaemonAgentHandle`, `DaemonStorageClient`, `DaemonNoteStore`, `Server`, `BindWithPluginConfigParams`, `split_plugins_config`, `plugin_ops::{install, remove}`, `BootstrapOutcome`, `KilnRegistry`, `KilnRegistryContext`, `forbidden_root_reason`, `resolve_registration_root`, `FileSessionStorage::root_for`, `parse_session_log`, `load_events`, `render_to_markdown`, `LogEvent`, `copilot::{CopilotAuth, CopilotError}`, `webhook::{default_secrets_path, mint_secret}`, `subscription::WILDCARD_SESSION`, `acp::streaming::humanize_tool_title`, `lifecycle::*` |
| `crucible-cli` | `crucible-lua` | `theme::ThemeConfig`, `hl`, `hl_lua`, `ui_geometry`, `statusline_items`, `theme_wire`, `SessionCommand`, `statusline_items::Region` |
| `crucible-cli` | `crucible-oil` | node builders, `style`, `ansi`, `render`, `focus`, `terminal`, `planning`, `runtime`, `components`, `viewport`, `layout`, `truncate_to_chars`, `truncate_to_width`, `composite_overlays`, spinner frames |
| `crucible-cli` | `crucible-web` | behind feature `web`: `start_server`, `middleware::auth::{api_key_path, generate_and_persist_key, resolve_api_key, local_names}`, `HostPolicy` |

Surprising edges:

- `crucible-web` reaches daemon internals that are not the RPC client:
  `server::plugins::OptionAction` (`crucible-web/src/routes/plugin.rs:8`),
  `project_manager::*` (`routes/project.rs:8`), `webhook::*`
  (`routes/webhook.rs:11`), `agent_manager::providers::ProviderInfo`
  (`services/daemon.rs:4`, a re-export of the core type).
- `crucible-cli` builds a daemon `KilnRegistry` in-process
  (`crucible-cli/src/kiln_attach.rs:44`), reads the daemon session directory
  (`commands/session/io.rs:21`), and calls a daemon ACP display helper
  (`tui/oil/components/tool_render.rs:77`).
- `crucible-cli` keeps copies of daemon helpers: `ollama_endpoint`
  (`provider_detect.rs:30` vs `agent_manager/providers.rs:192`),
  `keyed_backend_display_name` (`provider_detect.rs:164` vs `providers.rs:175`),
  `collect_agent_directories` (`commands/agents.rs:59` vs `agent_cards.rs:39`),
  `extract_yaml_frontmatter` (`commands/workflow.rs:365` vs
  `rpc/workflow_handlers.rs:583`), `is_keypress_event`
  (`tui/oil/local_replay.rs:64` vs `replay.rs:199`), plugin paths
  (`commands/plugin/{list,update}.rs` vs `plugin_ops.rs:41,47`),
  `copy_dir_recursive` (`commands/setup.rs:148` vs `session_migration.rs:437`).
- `crucible-cli/src/config.rs:10-18` re-exports `CliAppConfig as CliConfig`
  and `CliConfig as CliAppConfig`, so the two names are swapped relative to core.
- `crucible-cli/src/formatting/syntax.rs:1` imports `crate::tui::oil`, so
  `formatting/` is not a leaf of `tui/`.
- `crucible-daemon/src/lib.rs:142` does `pub use watch::*`, so every watch name
  sits in the crate root.
- No crate imports `crucible-daemon/src/watch/`, `review/`, `agent_manager/messaging/`
  or `server/session/` by path; clients reach them over RPC only.

## 5. Closed sets as built

| Set | Location | Size | Completeness gate |
|---|---|---|---|
| `BuiltinTool` / `ToolSurface` | `crucible-daemon/src/tools/surface.rs:60`, `crucible-core/src/traits/tools.rs:56` | 23 tools; Host, Kiln, Daemon, Unknown | Exhaustive match, two module-level `#![deny]` clippy lints at `surface.rs:46-47`, no `Default` on the return type, a test that derives its expectation from the running system. The exemplar. Six hand lists re-spell subsets (section 3.1). |
| `StageId` | `crucible-lua/src/handlers/hook_name.rs:119` | 11 turn-loop stages | `#![deny]` at `hook_name.rs:31-32`; `strum::EnumIter` walks `ALL` |
| `EventName` | `crucible-lua/src/handlers/hook_name.rs:44` | 8 daemon broadcast events | Same file, same gate; `event_map::ROWS` (`crucible-daemon/src/event_map.rs:103`) is the one wire-to-hook table; eight `*_EVENT` consts alias `EventName::*.as_str()` |
| `RpcMethod` + `METHODS` | `crucible-daemon/src/rpc/dispatch.rs:54,79` | 148 variants | Both generated from one `rpc_methods!` table; `rpc/missing_session_contract.rs` is a contract test. Gaps: `dispatch_session_setter!/getter!` re-match on the raw string with `unreachable!` (`dispatch.rs:273-295`); `SessionReindex` stays in `METHODS` and always fails |
| `ScriptingEvent` | `crucible-core/src/events/session_event/mod.rs:75` | 10 names both vocabularies share | `ALL` and `as_str`; `TurnPayload::as_scripting_event` (`turn.rs:272`) documents the mapping but only tests call it |
| `Group` (wire event groups) | `crucible-core/src/protocol/session_events/mod.rs:121,133` | 8 groups, 70 wire names | No gate. `Group::of` is a hand-maintained string match that mirrors `rename_all` of eight enums; drift surfaces at runtime as `UnknownEvent` |
| Permission modes | `runtime/defaults/init.lua` as `BUILTIN_INIT_LUA` (`crucible-lua/src/lib.rs:164`); `ModeRegistry` (`crucible-lua/src/modes.rs:161`); `BuiltinMode` (`crucible-core/src/types/mode.rs:85`); `BUILTIN_MODE_NAMES` (`crucible-daemon/src/tools/tool_modes.rs:37`); `default_internal_modes` (`types/mode.rs:273`) | normal, plan, auto | Lua is the only definition of the rules; `ModeRegistry` has no Rust default and no fallback. `BuiltinMode::Auto` is matched but only built by `from_id`. `is_write_tool_name` (`genai_handle.rs:103`) and `PLAN_TOOL_NAMES` are hand lists that gate plan mode |
| `InteractionRequest` kinds | `crucible-core/src/interaction/types.rs:380` | 7 | `KINDS` const; `interaction-coverage.test.ts` on the web side; `Show` has no response variant |
| `Capability` (plugin) | `crucible-lua/src/manifest.rs:80` | 9 | serde derive for `plugin.yaml`; `parse_capability` (`lifecycle/spec.rs:31`) is a second decoder that omits `intercept_tools` |
| Built-in ACP agents | `crucible-daemon/src/acp/discovery.rs:52`, `acp_launch.rs:126` | 5 | None; the table is written twice |
| `cru.sessions` names | `crucible-lua/src/sessions/register.rs:26-74,187-867` | 28 | None; the stub list and the real list are both string literals |
| REPL commands | `crucible-cli/src/tui/oil/chat_app/{autocomplete.rs:201,474; command_handling.rs:20,99}` | about 20 | None; four hand-kept lists |
| `LogEvent` persisted names | `crucible-daemon/src/server/core/mod.rs:465-474` | 2 of 16 | A string match after `should_persist` already decoded the typed payload |
| `TRACKED_FIELDS` | `crucible-core/src/config/cli_app.rs:5` | 18 of about 60 leaf fields | The coverage test checks the list against itself |

## 6. Trait inventory

| Trait | Location | Impls (prod + test) | Required / defaulted | `dyn` | Verdict |
|---|---|---|---|---|---|
| `Agent` | `crucible-core/src/turn/mod.rs:346` | 6 + 12 | 4 / 0 | yes | keep |
| `AgentHandle` | `crucible-core/src/traits/chat.rs:143` | 3 (Genai, Acp, Daemon) + Box forwarder + CLI Noop + Mock | 3 / 41 | yes | keep, make knobs required |
| `ToolExecutor` | `crucible-core/src/traits/tools.rs:87` | 4 + 5 | 3 / 0 | yes | keep |
| `StepHandler` | `crucible-core/src/workflow/handler.rs:53` | 3 + 2 | 1 / 0 | yes | keep |
| `NoteStore` | `crucible-core/src/storage/note_store.rs:443` | 2 + 6 | 7 / 5 | yes | keep, remove defaults |
| `PropertyStore` | `crucible-core/src/storage/property_store.rs:15` | 2 + 1 | 5 / 0 | yes | keep; one impl unused |
| `KnowledgeRepository` | `crucible-core/src/traits/knowledge.rs:78` | 3 + 4 | 2 / 1 | yes | keep, remove default |
| `EmbeddingProvider` | `crucible-core/src/enrichment/embedding.rs:36` | 5 + 3 | 6 / 2 | yes | keep, remove defaults |
| `BackgroundSpawner` | `crucible-core/src/background/mod.rs:22` | 1 + 5 | 4 / 0 | yes | single-impl; test double |
| `PermissionGate` | `crucible-core/src/traits/permission_gate.rs:13` | 1 + 0 | 1 / 0 | yes | single-impl, no double |
| `Undoable` | `crucible-core/src/traits/undoable.rs:15` | 1 + 0 | 3 / 0 | yes | single-impl |
| `StorageClient` | `crucible-core/src/traits/storage_client.rs:27` | 1 + gated mock | 1 / 1 | no | single-impl; method always errors |
| `EventEmitter` | `crucible-core/src/events/emitter.rs:293` | 1 + noop + mock | 1 / 2 | yes | single-impl |
| `MarkdownParser` | `crucible-core/src/parser/traits.rs:17` | 1 + 0 | 4 / 0 | yes | single-impl |
| `SyntaxExtension` | `crucible-core/src/parser/extensions.rs:18` | 8 + 1 | 5 / 4 | yes | enum-candidate |
| `HashingAlgorithm` | `crucible-core/src/hashing/algorithm.rs:48` | 2 + 0 | 3 / 4 | no | dead |
| `ContentHasher` | `crucible-core/src/storage/traits.rs:17` | 2 + 1 | 3 / 1 | no | impls dead |
| `ChangeDetectionStore` | `crucible-core/src/processing/change_detection.rs:112` | 1 + 0 | 4 / 0 | no | dead |
| `CredentialStore` | `crucible-core/src/config/credentials.rs:87` | 3 (one feature-gated) | 4 / 0 | yes | enum-candidate |
| `StorageResultExt` | `crucible-core/src/storage/error_ext.rs:6` | blanket | 1 / 0 | no | keep |
| `DelegationSpawner` | `crucible-daemon/src/delegation.rs:65` | 1 + 5 | 5 / 0 | yes | single-impl; test double |
| `SessionStorage` | `crucible-daemon/src/session_storage.rs:34` | 1 + 6 | 8 / 0 | yes | keep |
| `ToolDispatcher` | `crucible-daemon/src/tool_dispatch.rs:99` | 1 + 1 | 4 / 0 | yes | single-impl; test double |
| `FileWatcher` | `crucible-daemon/src/watch/traits.rs:10` | 3 (2 stubs) | 7 / 0 | yes | enum-candidate |
| `WatcherFactory` | `crucible-daemon/src/watch/backends/mod.rs:20` | 3 | 4 / 0 | yes | enum-candidate |
| `EventHandler` | `crucible-daemon/src/watch/traits.rs:316` | 3 | 2 / 2 | yes | keep, make required |
| `SqliteResultExt` | `crucible-daemon/src/storage/sqlite/error_ext.rs:6` | blanket | 1 / 0 | no | keep |
| `McpResultExt` | `crucible-daemon/src/tools/helpers.rs:27` | blanket | 3 / 0 | no | keep |
| `ChatResultExt` | `crucible-daemon/src/rpc_client/error_ext.rs:6` | blanket | 1 / 0 | no | keep |
| `DaemonSessionApi` | `crucible-lua/src/sessions/mod.rs:103` | 1 + 3 | 17 / 15 | yes | firewall; remove defaults |
| `DaemonToolsApi` | `crucible-lua/src/tools_api.rs:95` | 1 + 1 | 4 / 0 | yes | firewall |
| `SessionConfigRpc` | `crucible-lua/src/session_api.rs:67` | 3 + 6 | 0 / 22 | yes | violates required rule |
| `LuaResultExt` | `crucible-lua/src/error_ext.rs:6` | blanket | 1 / 0 | no | keep |
| `SendExt` (private) | `crucible-lua/src/session_api.rs:7` | blanket | 1 / 0 | no | keep |
| `App` | `crucible-cli/src/tui/oil/app.rs:68` | 1 + 0 | 3 / 2 | no | single-impl |
| `Component` | `crucible-cli/src/tui/oil/component.rs:6` | 8 + blanket | 1 / 0 | no | keep |
| `KilnStatsService` | `crucible-cli/src/commands/stats.rs:44` | 1 + 2 | 1 / 0 | yes | single-impl; test double |
| `FrameRenderer` | `crucible-oil/src/runtime.rs:7` | 1 + wrapper | 3 / 0 | no | single-impl |
| `InputStyle` | `crucible-oil/src/components/input_area.rs:9` | 1 + 1 | 2 / 2 | no | single-impl |
| `WebResultExt` | `crucible-web/src/error.rs:74` | blanket | 1 / 0 | no | keep |

Traits that break the rules:

- Defaulted methods: `AgentHandle` (41), `SessionConfigRpc` (22, zero
  required), `DaemonSessionApi` (15), `NoteStore` (5, return empty link data),
  `SyntaxExtension` (4, one blocks on Tokio), `HashingAlgorithm` (4),
  `EventEmitter` (2), `EmbeddingProvider` (2), `EventHandler` (2, every impl
  overrides both), `App` (2), `InputStyle` (2), `KnowledgeRepository` (1),
  `ContentHasher` (1), `StorageClient` (1).
- `dyn` with one impl and no test double: `PermissionGate`, `Undoable`,
  `MarkdownParser`, `EventEmitter` (the noop and mock are not production
  alternatives), `FileWatcher` and `WatcherFactory` (the other impls are stubs).
- Single impl that is neither a test double nor a firewall: `MarkdownParser`
  (both sides are workspace crates), `App`, `FrameRenderer`, `InputStyle`,
  `StorageClient`, `ChangeDetectionStore`, `HashingAlgorithm`.
- Trait where an enum would do: `SyntaxExtension` (8 impls in one crate),
  `CredentialStore` (3 impls in one file, one never compiled), `FileWatcher`,
  `WatcherFactory`.

## 7. Type families with many copies

Each family lists its members. "Canonical today" names the member that
production code uses most. The reason column is from the record when one was given.

**Truncate helpers (nine).** `crucible-core/src/background/types.rs:242`
(canonical; `delegation.rs` imports it), `workflow/stdlib.rs:74`,
`events/session_event/helpers.rs:97`, `events/session_event/internal.rs:704`
(`trunc`), `crucible-daemon/src/observe/markdown.rs:353`,
`agent_manager/attachments.rs:143` (`truncate_on_char_boundary`),
`session_bridge.rs:961` (`truncate_str`, exact copy of the core one),
`crucible-cli/src/commands/workflow.rs:282`, `commands/session/helpers.rs:76`,
plus `truncate_to_chars` / `truncate_to_width` in both `crucible-oil/src/utils.rs:35,123`
and `crucible-cli/src/tui/oil/utils/truncate.rs:32,128`. Reason: none recorded;
each module grew its own.

**Session event parallel enums.** `SessionEventMessage`
(`crucible-core/src/protocol/rpc/mod.rs:85`, canonical on the wire);
`TurnPayload` and the seven other groups (`protocol/session_events/`);
`SessionEvent` and `InternalSessionEvent` (`events/session_event/`, the
scripting side, 12 of 52 variants live); `LogEvent`
(`crucible-daemon/src/observe/events.rs:49`, 5 of 16 variants written);
`rpc_client/client/types.rs:10 SessionEvent` (client re-parse);
`crucible-web/src/events.rs:7 ChatEvent` (SSE projection, 6 dead variants);
`crucible_lua` flat tables (`handlers/conversion.rs`). Reason: the scripting
vocabulary predates the typed transport groups and the Reactor that consumed it
was removed (`events/mod.rs:14-30`); `LogEvent` is the persisted presentation
form; `ChatEvent` renames keys for the browser on purpose.

**`PermissionScope` x2.** `crucible-core/src/interaction/permission.rs:20`
(canonical; serde, part of `InteractionResponse`) and
`crucible-core/src/config/components/permissions/types.rs:5` (subset, no
serde). Converted by hand at `crucible-cli/src/tui/oil/chat_app/shell.rs:113-121`.

**`SearchResult` x3.** `crucible-core/src/storage/note_store.rs:294`
(`{note, score}`, used by `NoteStore`), `crucible-core/src/types/database.rs:147`
(`{document_id, score, highlights, snippet, kiln}`, used by
`KnowledgeRepository`, web, MCP, Lua; the crate-root re-export),
`crucible-daemon/src/storage/sqlite/fts.rs:41 FtsResult` re-parsed as
`rpc_client/client/storage_requests.rs:180 TextSearchHit`. `repository.rs:181`
converts between the two core ones.

**`SessionId` x2.** `crucible-core/src/session/types/id.rs:45` (canonical,
validated string, every RPC path) and `crucible-core/src/types/acp.rs:61`
(uuid, re-exported at `lib.rs:90`, no production user).

**`ToolCall` x2.** `crucible-core/src/traits/llm.rs:38` (OpenAI shape) and
`crucible-core/src/events/session_event/tool_call.rs:10` (`name, args, call_id`).
Also `ChatToolCall` (`traits/chat.rs:669`) and `ToolCallInfo` (`types/acp.rs:269`).

**Mock embedding providers x3 and mock repositories x3.**
`crucible-daemon/src/llm/embeddings/mock.rs:13` (hash-based, pub),
`test_support.rs:60` (constant 0.1 vectors), `enrichment/service.rs:446`
(test-local); `FixtureBasedMockProvider` (`mock.rs:291`) copies the hash
fallback verbatim (`mock.rs:67-90,346-362`). `MockKnowledgeRepository` at
`test_support.rs:28`, `multi_kiln_search.rs:109`, `agent_manager/tests/mod.rs:242`.
`test_support` exists to be canonical.

**Theme colour parsers x4.** `theme_wire::color_to_name` (`theme_wire.rs:40`) =
`hl_lua::color_name` (`hl_lua.rs:105`); `theme::parse_any_color` (`theme.rs:740`)
= `hl_lua::side` (`hl_lua.rs:40`); `theme::parse_adaptive_color` (`theme.rs:713`)
~ `hl_lua::color_from_lua` (`hl_lua.rs:18`); `theme_wire::adaptive_from_wire`
(`theme_wire.rs:80`) ~ `hl_lua::color_from_wire` (`hl_lua.rs:132`). Plus
`theme::BorderStyle` (`theme.rs:337`) vs `ui_geometry::border_from_name`
(`ui_geometry.rs:80`) vs `crucible_oil::style::Border`.

**Config provider knobs.** `ChatConfig` (`components/chat.rs:22`) and
`LlmProviderConfig` (`components/llm.rs:9`) repeat five knobs;
`CliAppConfig::chat_model` (`cli_app.rs:761`, hardcodes `llama3.2`) vs
`ChatConfig::chat_model` (`chat.rs:73`, `DEFAULT_CHAT_MODEL`); five enrichment
provider structs repeat `{model, base_url, timeout_seconds, retry_attempts,
headers}` (`enrichment.rs:79,167,281,348,410`); `BackendType` metadata
(`backend.rs:70-224`) and `components/defaults.rs` restate endpoints; VertexAI
disagrees (`backend.rs:130` vs `enrichment.rs:382`); three provider-to-model
tables in the CLI (`wizard.rs:135`, `init.rs:385`, core `DEFAULT_CHAT_MODEL`).
Canonical: `LlmProviderConfig` (64 references).

**serde_md serializers.** `crucible-core/src/serde_md/serializer.rs:21`
(`Serializer`, `SeqSerializer:210`, `MapSerializer:277`, `StructSerializer:307`;
zero callers) and `crucible-daemon/src/observe/serde_md.rs:43,216,283,313`
(`LogEventSerializer`; no production caller). Neither is canonical; the live
renderer is `observe/markdown.rs:31`.

**`SessionAgent` literals x3.** `SessionAgent::internal_from_config`
(`crucible-core/src/session/types/agent.rs:335`, canonical),
`build_default_internal_agent` (`server/session/create.rs:454`),
`crucible-cli/src/commands/session/acp.rs:527`.

**Provider detection.** `ProviderInfo` (`crucible-core/src/types/provider_info.rs:11`,
canonical, sent by the daemon) vs `DetectedProvider`
(`crucible-cli/src/provider_detect.rs:20`); `discover_env_providers`
(`agent_manager/providers.rs:93`) vs `detect_providers_inner`
(`provider_detect.rs:92`). Reason: `cru init` runs before the daemon.

**`EmbeddingResponse` x2.** `crucible-daemon/src/llm/embeddings/provider.rs:400`
(canonical) and `crucible-core/src/traits/provider.rs:16` (unused).

**Ollama `/api/tags` shapes x2.** `provider/model_listing.rs:85-92` and
`llm/embeddings/ollama.rs:43-51`, field-identical.

**Hash newtypes.** `BlockHash` (`parser/types/block_hash.rs:12`, canonical) and
`FileHash` (`types/hashing.rs:25`), same fields and methods.

**Popup rows.** `PopupEntry` (`crucible-core/src/types/popup.rs:16`) and
`PanelItem` (`crucible-core/src/interaction/types.rs:135`), field-identical;
oil `PopupItemNode` and `PopupItem`.

**`McpServerInfo` x2 plus display.** `crucible-core/src/types/mcp_status.rs:15`
(canonical on the wire), `crucible-core/src/traits/mcp.rs:138` (different
shape), `crucible-cli/src/tui/oil/chat_app/model_state.rs:11 McpServerDisplay`.

**Tool definitions.** `ToolDefinition` (`crucible-core/src/traits/tools.rs:208`,
canonical), `acp/tools.rs:29 ToolDescriptor`, `tool_discovery.rs:50 ToolSchema`,
three `rmcp::Tool` conversions each way (section 3.5).

**Agent card directories.** `agent_cards.rs:39 card_directories` (canonical)
and `crucible-cli/src/commands/agents.rs:59 collect_agent_directories`; they
already differ.

**Tilde expanders.** `project_manager.rs:68 resolve_registration_root`
(canonical), `scm.rs:156`, `scm.rs:221`, `daemon_plugins/bootstrap.rs:113`,
`kiln_manager.rs:1199`, `crucible-cli/src/commands/agents.rs:91`,
`crucible-cli/src/kiln_validate.rs expand_tilde`.

**Contexts.** `RpcContext` (`rpc/context.rs:60`, canonical) and
`ServerContext` (`server/mod.rs:950`).

**Status snapshots x3.** `StatusBar`, `StatusBarData`, `StatusComponent`
(section 3.7). **Chat params x3.** Resolved: one `ChatParams` plus a `ChatMode` enum
(`crucible-cli/src/commands/chat/mod.rs`).
**Lua tool shapes x2.** `LuaTool`/`DiscoveredTool`, `ToolParam`/`DiscoveredParam`.
**Watch factories x3.** `NotifyFactory`, `PollingFactory`, `EditorFactory`, each
`struct { capabilities }` with a constant four-method impl.
**Private-file writers x3.** `crucible-web/src/middleware/auth/api_key.rs:73`,
`crucible-core/src/config/credentials.rs:180`, `crucible-daemon/src/webhook/mod.rs:415`.
**Kiln fixtures x2.** `test_support/mod.rs:34,56` and `test_support/fixtures.rs:188,75`.

## 8. Test-support leakage

Production items that only tests use:

- `crucible-core/src/lib.rs:32 pub mod test_support;` is unconditional.
  `Cargo.toml` declares `test-utils = []` but nothing in `test_support/` checks
  it, so `EnvVarGuard` (`std::env::set_var`), `tempfile` fixtures and
  `MockEventEmitter` ship in `cru`. Only `traits/storage_client.rs:61` gates
  its mock. `crucible-core/src/parser/test_utils.rs` is a `pub mod` with no
  gate although its doc says it has one (`parser/mod.rs:37`).
- `crucible-daemon/src/test_support.rs` (`MockKnowledgeRepository`,
  `MockEmbeddingProvider`, `MockSubagentHandle`, `TempSessionStorage`) is
  always compiled; `crucible-daemon/tests/*` import it.
- `crucible-daemon/src/skills/test_utils.rs` is an empty `pub mod` in production.
- `crucible-daemon/src/acp/{mock_agent,tracing_utils,recording,replay}.rs` are
  test infrastructure in the crate; `MockAgent` has no caller and its
  `handle_request` is a TODO.
- `#[allow(dead_code)]` on test-only production methods:
  `agent_manager/permissions.rs:22,108,118`; `session_manager.rs:783,802,811`;
  `background_manager/mod.rs:195,202`; `enrichment/service.rs:61,67`;
  `subscription.rs:31`; `acp/discovery.rs:290`; `kiln_manager.rs:731` (stale,
  the function is live); `server/mod.rs:105,126,131,438,446,949`;
  `server/bind.rs:74,96,109`; `crucible-core/src/types/notification.rs:20`;
  `crucible-core/src/parser/block_extractor.rs:828`.
- `#[allow(unused_imports)]` that hide test-only re-exports:
  `server/session/mod.rs:23`, `rpc/mod.rs:15-18`, `rpc_helpers.rs:160`,
  `crucible-lua/src/fennel.rs:44`.
- Test-only constructors and builders in production: `KilnManager::new` and
  `Default` (`kiln_manager.rs:400,1086`); `NotePipeline::new`
  (`note_pipeline.rs:78`); `LlmProviderConfigBuilder`
  (`crucible-core/src/config/components/llm.rs:111`); `CliConfigBuilder`
  (`crucible-cli/src/config.rs:24`); `SqliteConfig::{with_pool_size,
  without_wal, with_cache_size}`, `SqlitePool::stats`, `FtsIndex::search_boosted`,
  `create_knowledge_repository` (section 3.3); `ReviewLedgers::restore` is
  `#[cfg(test)]` on the production struct (`review/mod.rs:246`).
- `ComponentHarness` (`crucible-cli/src/tui/oil/component.rs:19`) and
  `AppHarness` (`tui/oil/test_harness.rs:8`) live in non-test modules and are
  re-exported from `tui/mod.rs`.
- `PluginManager` test-only API: `active_plugins`, `eval_runtime`, `reload`,
  `enable`, `initialize`, `error_log`, `with_search_paths`,
  `load_plugin_spec_from_source` (`crucible-lua/src/lifecycle/`).
- `crucible-daemon/src/server/lua_plugin_suite.rs:438` reads its own source
  with `include_str!` to enumerate test arms.
- Test seams on `Server`: `shutdown_handle`, `event_sender` (`server/mod.rs:439,447`,
  23 test sites) and `bind_with_data_home*` (30 test sites); `AgentFactoryOverride`
  (`agent_manager/mod.rs:231`).
- Hermeticity gaps that tests depend on: `crucible_home()` reads env at call
  time (`crucible-core/src/config/mod.rs:40`); `EMBEDDING_PROVIDER_CACHE` is
  process-global; `crucible-lua/src/config.rs:55 CONFIG` needs `reset_config`;
  five render `RwLock`s in `crucible-cli/src/tui/oil/theme/` leak between
  tests in one process (`input_area.rs:114`); `recording.rs:263` calls
  `std::env::remove_var`.

## 9. Open concerns that are not duplicates or dead code

- `Server::run` is about 500 lines with four inline task bodies
  (`crucible-daemon/src/server/mod.rs:452-945`).
- `review/mod.rs` is 999 lines against a 1000-line budget.
- `genai_handle.rs` is about 3,200 lines on the grandfathered ledger
  (`crucible-daemon/tests/architecture_tests.rs:718`).
- Every daemon handler returns hand-spelled `serde_json::json!`; results have no
  shared type (`daemon-server-a` record).
- `AcpAgentHandle::turn` keeps the client out of its `Arc<Mutex<Option>>` after
  the consumer drops the stream; a second turn sees "busy" (`acp_handle.rs`).
- `REQUEST_ID` is one process-global counter for every ACP client
  (`crucible-daemon/src/acp/client/mod.rs:28`).
- `ProjectManager::touch` rewrites the whole projects JSON synchronously from
  RPC handlers (`crucible-daemon/src/project_manager.rs:288-308`).
- `FileEvent::new` calls `path.is_dir()` on the notify callback thread for
  every event (`crucible-daemon/src/watch/events.rs:35`); `EventMetadata.watch_id`
  is always `"default"` (`notify_backend.rs:132,152`).
- `Debouncer::emit_ready_events` returns one event per 50 ms tick unless more
  than `max_batch_size` are ready (`watch/utils/debouncer.rs:141`).
- `CopilotAuth::new` and `CopilotClient::new` `.expect()` on the reqwest builder
  (`crucible-daemon/src/provider/copilot.rs:231,373`).
- `model_listing::list_models` returns `Ok(vec![])` on a Copilot error and on a
  missing key (`provider/model_listing.rs:209-218`).
- `model_listing.rs:12-17` disables redirects against SSRF; `ollama.rs` and
  `openai.rs` embedding clients use the default policy on the same endpoint.
- `EmbeddingError` has 15 variants, 5 never built, matched only by its own
  `is_retryable` (`llm/embeddings/error.rs:10`); `watch::Error` has 21
  (`watch/error.rs:6`); `StorageError` has 22 with six from the removed
  content-addressed store (`crucible-core/src/storage/error.rs:9`);
  `ScmError::InvalidBranch` has no constructor (`scm.rs:29`).
- `llm/mod.rs:58-59` carries `#![warn(missing_docs)]` and `#![warn(clippy::all)]`
  on a submodule; `config/mod.rs:31` carries `#![allow(clippy::module_inception)]`.
- `agent/loader.rs:188` compiles a `Regex` per `is_valid_semver` call;
  `agent/loader.rs:40` `to_str().unwrap()` panics on a non-UTF-8 card path;
  agent cards load in `read_dir` order.
- The inline-metadata regex is compiled at call time four times
  (`parser/types/lists.rs:396,427`, `task.rs:257`, `inline_metadata.rs:76`).
- `SimpleBlockHasher` is `async fn` over pure CPU work with `join_all`
  (`parser/block_hasher.rs:35-112`).
- `content_category.rs:51 DOCUMENT = Self::Note` is a copy error in a dead file.
- `workflow/handler.rs:60 DispatchTable` exposes `pub handlers` and `pub default`
  beside `register`/`resolve`.
- `background/mod.rs:17` says subagents no longer use the trait, yet
  `JobKind::Subagent` remains and `delegation.rs:489` builds it.
- `register_project_in_config` uses a serde round-trip and drops comments
  (`crucible-core/src/config/registration.rs:183`).
- Two readers of `config.toml`: `CliAppConfig::load` and the daemon's own parse
  in `execution_roots.rs:90-114`; the kiln registry reads a JSON view.
- Config fields parsed and read by nothing: `acp.lazy_agent_selection`,
  `storage.idle_timeout_secs` (both documented as reserved; plan T3-B18 deleted
  `DiscoveryConfig` and `ResolveMode`),
  `ValueSource::File { path: Option }` always `Some` (`cli_app.rs:420`).
- `crucible-cli/src/main.rs:53,142` call `std::env::set_var` in production
  before the runtime starts.
- `std::process::exit` inside CLI handlers bypasses `Result` and `Drop`:
  `commands/set.rs` (8 sites), `plugin/test.rs:88,158`, `plugin/health.rs:27,105`,
  `plugin/new.rs:25`, `doctor.rs:290`, `chat/mod.rs:439,801,809`.
- `DoctorCheckResult.status` is a `String` in `{pass,fail,warn}` matched by
  string (`commands/doctor.rs:24,255`); recording mode, agent type and session
  type travel as string literals in the CLI.
- `init.rs:402` still creates `<kiln>/.crucible/sessions/` while sessions no
  longer live in a kiln. `cru session export --timestamps` does nothing on the
  fallback path (`commands/session/io.rs:53`). `session/show.rs` and
  `session/list.rs` carry three-deep fallback chains that print different shapes.
- `AgentInitParams.read_only` and `max_context_tokens` are set by every caller
  and never read (`crucible-cli/src/factories/agent.rs`).
- `ShellGateState.credentials` is `Some` exactly when `allow_remote` is true
  (`crucible-web/src/middleware/auth/shell.rs:103`).
- `SessionAgent.capabilities` and `agent_description` are written by
  `from_profile` and read by nobody. `ModeDescriptor.icon` and `.color` have no
  `Some` path (`crucible-core/src/types/mode.rs:171-211`).
- `ConversationTree::fanout` and `collect`, `NodeMeta`, `NodeContent::Marker`
  are reserved for workflows that do not exist (`crucible-core/src/turn/tree.rs:251`).
- `ToolRef` constructors are bypassed; the daemon builds the struct literal
  (`tool_dispatch.rs:206`).
- Stale docs in code: `watch/external_changes.rs:49` names `ignored_dirs`;
  `workspace/indexer.rs:3-8` says CLI indexer copies remain; `precognition/mod.rs:713`
  says k is hardcoded; `server/session/mod.rs:143` says mode has no daemon
  representation; `storage/note_store.rs:7` names a `Precognition` trait;
  `traits/knowledge.rs:30` names `ContentAddressedStorage`; `traits/mcp.rs`
  header claims traits the file lacks; `types/acp.rs:117` names
  `crucible_cli::chat::ChatSessionConfig`; `events/session_event/display.rs`
  claims `Display` impls that do not exist; `watch/mod.rs:1-35` describes Hot
  Reload handlers; `acp/tools.rs` header names a crate that no longer exists;
  `session/types/config.rs:15-19` is stale on `ContextStrategy`;
  `session_events/mod.rs` says System has 9 variants, it has 13;
  `plugin_install.rs:9` and `plugins.rs:96-105` open with dangling doc fragments.
- `use super::super::*` opens every `messaging/` file
  (`send.rs:1`, `stream.rs:23`, `tool_call.rs:1`, `permission.rs:1`,
  `review_gate.rs:40`, `precognition/mod.rs:12`), so the dependency set on
  `agent_manager/mod.rs` is invisible at the file head.
- `lifecycle/spec.rs:92-109` and `discovery.rs:290-306` duplicate the
  `#[cfg(feature = "fennel")]` branches.
- `crucible-core/src/protocol/lifecycle.rs:59` declares `extern "C" fn geteuid`
  by hand while `dirs` already pulls `libc`.

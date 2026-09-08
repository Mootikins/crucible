---
title: Architecture Gaps
description: Expected versus actual, one row per difference, with a verdict on which side is wrong.
tags: [meta, architecture]
status: as-of-7053bcfe7
---

# Architecture Gaps

This document lists the differences between [[Expected]] (the clean-room
design) and [[Actual]] (the code at `7053bcfe7`). Each difference is one row.
Each row carries a verdict. The verdict says which side should move.

## 1. Method

One writer read both documents in full. The writer walked the entities
(Expected section 3), the subsystems (section 4), the seams (section 5), the
wire surfaces (section 6), the closed sets (section 8) and the extension points
(section 9). For each item the writer looked for the matching record in
Actual.md and compared the shape, the owner, the count and the gate. Where
Actual.md did not settle a fact, the writer opened one source file to settle it;
the writer did not survey the codebase. The seam names in the `area` column are
the seven seams in Actual.md section 3 plus `config`, `crates` and `tests`.
`cost` is S, M or L for the code change a `code-wrong` verdict implies; other
verdicts carry a dash. Paths are relative to `crates/` unless the path starts
with `docs/`.

## 2. Gap table

Verdicts: `code-wrong` (the code should move), `expectation-wrong` (the clean
room guessed wrong and had no reason to guess right), `expectation-incomplete`
(the clean room lacked an input, usually a security invariant from
[[Filesystem Containment]] or a live surface the product docs do not name;
Expected.md sections 2a and 7a carry the missing input), `both-acceptable`,
`not-built`, and `closed` (a Tier 1 to 3 commit removed the difference; section
6 names the commit).

| id | area | expected | actual | verdict | cost |
|---|---|---|---|---|---|
| G1 | scope | One `CapabilityHandle` is the only path door (4.4) | `FsScope` is the door, but the `Component::Normal` whitelist loop is written six times (`server/fs/mod.rs:160,387,460`, `server/session/review/mod.rs:605`, `server/note_refactor.rs:232`, `crucible-core/src/canvas/containment.rs:215`) | code-wrong | M |
| G2 | scope | `PermissionEngine` returns `GateDecision` with the deciding `Layer`; it never prompts (4.12, 8.7) | `PermissionDecision` is `Allow`, `Deny`, `Ask` with no layer (`permissions/types.rs:98`); the prompt runs inside `messaging/permission.rs`; a second door `PermissionGate` exists as `Arc<dyn>` with one impl (`permission_bridge.rs:19`) | code-wrong | M |
| G3 | scope | One project `PatternStore` for saved allows (3.2) | Three bash allowlists and two deny lists in one module (`patterns.rs:57`, `security.rs:56,163`, `hardcoded.rs:21`); the layers have different override semantics, so T3-C9 documented the order in [[Bash Permission Layers]] instead of a merge | both-acceptable | - |
| G4 | scope | One `PermDecision` enum `AllowOnce`, `AllowSession`, `AllowProject`, `Deny` (3.15, D13) | Two `PermissionScope` enums with one name (`interaction/permission.rs:20` has `Once`, `Session`; `permissions/types.rs:5` has `Project`, `User`); the TUI maps one to the other by hand (`crucible-cli/src/tui/oil/chat_app/shell.rs:113-121`) | code-wrong | S |
| G5 | scope | The Lua permission hook has a 1 s budget (9.4) | `execute_permission_hooks_with_timeout` has no timeout; it discards a late result (`messaging/permission.rs:1108`) | code-wrong | S |
| G6 | scope | Permission requests are serialized per session (3.15) | `PermissionSerializer` serializes ACP prompts (`messaging/permission.rs:76`); the internal path is serial because tools dispatch one at a time (`messaging/tool_call.rs:647`) | both-acceptable | - |
| G7 | scope | No hand list beside `BuiltinTool` (9.1) | Six hand lists re-spell subsets: `is_core_tool_name` (`tool_dispatch.rs:181`), `KILN_BACKED_TOOLS` (`tools/mcp_server.rs:126`), `DISCOVERY_TOOL_NAMES` (`tool_dispatch.rs:30`), `PLAN_TOOL_NAMES` (`tools/tool_modes.rs:17`), `is_write_tool_name` (`provider/genai_handle.rs:103`), CLI `BUILTIN_TOOLS` (`crucible-cli/src/commands/tools.rs:34`) | code-wrong | M |
| G8 | scope | `ToolSurface` is `Daemon`, `Mcp`, `Both`: which wire serves the tool (8.1) | `ToolSurface` is `Host`, `Daemon`, `Unknown`: what an isolated session may run (`crucible-core/src/traits/tools.rs:56`) | expectation-incomplete | - |
| G9 | scope | MCP exposure derives from the enum (8.1, 6.4) | MCP exposure is the hand list `KILN_BACKED_TOOLS` (`tools/mcp_server.rs:126`) | code-wrong | S |
| G10 | scope | Twenty-four built-in tools, with `InvokeTool` (8.1, D20) | Twenty-three variants (`tools/surface.rs:60`); `invoke_tool` lives in `DISCOVERY_TOOL_NAMES` (`tool_dispatch.rs:30`), not in the enum | code-wrong | S |
| G11 | scope | Plan-mode and safe-tool classes derive from the enum (8.1) | `PLAN_TOOL_NAMES` (`tools/tool_modes.rs:17`) and `is_write_tool_name` (`genai_handle.rs:103`) are hand lists | code-wrong | S |
| G12 | scope | The daemon `KilnRegistry` is the only door from a path to a kiln (4.2.1) | The CLI builds its own `KilnRegistry` over `crucible_home()` (`crucible-cli/src/kiln_attach.rs:112`); `KilnRegistryContext::for_daemon` reads `current_dir` and `home_dir` (`kiln_registry.rs:174`) | code-wrong | M |
| G13 | scope | Containment never knows tool names (4.4) | File-tool name lists at `messaging/permission.rs:1073,1098` and `is_file_tool` (`permissions/engine.rs:193`) | code-wrong | S |
| G14 | scope | Data-class trust is enforced on every delegation (4.14) | `enforce_child_isolation` skips silently when `session_lifecycle` is unbound (`delegation.rs:181`) | code-wrong | S |
| G15 | scope | The gate order is one function with one test (4.11) | The order is statement order in `messaging/tool_call.rs`; `requires_permission_gate` holds `unreachable!` for `ToolPolicy::Deny` (`gate_decision.rs:341`) | code-wrong | S |
| G16 | scope | `AppConfig` reaches every subsystem by value at bind (S41) | `execution_roots::baseline` reads env vars and `settings.json` from disk (`execution_roots.rs`); `kiln_registry.rs:323` cites it as precedent | code-wrong | M |
| G17 | scope | The web layer holds no policy beyond SSRF (4.27, 6.2) | The web holds a credential-directory deny list (`crucible-web/src/routes/project.rs:28-76`) and enclosing-root resolution twice (`routes/canvas.rs:195`, `routes/kiln.rs:363-437`) | code-wrong | M |
| G18 | scope | `PatternStore` I/O is not on the async gate path (4.12) | `load_sync` and `save_sync` block inside the async gate (`messaging/permission.rs:732,1094`) | code-wrong | S |
| G19 | scope | The permission-engine input is built once (4.12) | The `if tool == "bash" { command } else { args }` snippet is written three times (`messaging/permission.rs:585,628,690`) | code-wrong | S |
| G20 | session | `SessionAgent` is an enum `Internal`, `Acp` (3.9, D1) | A flat struct with `agent_type: String` and `Option` fields per kind (`crucible-core/src/session/types/agent.rs:19`) | code-wrong | L |
| G21 | session | `SessionConfig` holds the session knobs apart from the agent (3.10) | The knobs are fields on `SessionAgent`; the only `SessionConfig` is an ACP type (`crucible-core/src/types/acp.rs:130`) | code-wrong | M |
| G22 | session | `AgentHandle` has three required methods; `configure` takes the whole record (4.9, D2) | 3 required, 41 defaulted; the `Box<dyn>` forwarder re-lists 44 (`crucible-core/src/traits/chat.rs:143,486`); `MockSubagentHandle` implements 3 | code-wrong | L |
| G23 | session | One handle trait (4.9) | Two contracts: `Agent::turn` yields `TurnEvent` to the runtime (`crucible-core/src/turn/mod.rs:346`); `AgentHandle` faces clients and `DaemonAgentHandle` re-implements it over RPC (`rpc_client/agent/mod.rs:29`) | expectation-wrong | - |
| G24 | session | `SessionState` is `Active`, `Paused`, `Streaming`, `Ended` (8.10) | `Active`, `Paused`, `Compacting`, `Ended`; `Compacting` is never assigned (`session/types/enums.rs:90`); no `Streaming` | code-wrong | S |
| G25 | session | `ThinkingBudget` is an enum `Off`..`Max` (8.18) | `thinking_budget: Option<i64>` (`agent.rs:55`); the setter stores `unwrap_or(0)` (`server/session/params.rs:311-325`) | code-wrong | S |
| G26 | session | `ContextStrategy::Lua { name }` (8.18) | Three arms, no Lua arm (`session/types/config.rs:8`) | not-built | - |
| G27 | session | A `Turn` record with `TurnId` and `TurnOutcome` (3.11) | No turn id; the turn is `RequestState`, `StreamContext`, `AgentStreamConfig`, `TurnEnvironment` (`agent_manager/mod.rs:133,294`, `stream_config.rs:10,110`) | both-acceptable | - |
| G28 | session | `TurnOutcome` carries `DepthCapped` (3.11) | `StopReason::MaxToolDepth` is never built (`crucible-core/src/turn/mod.rs:169`) | code-wrong | S |
| G29 | session | `ConversationTree` holds only what the prompt needs (3.12) | `fanout`, `collect`, `NodeMeta`, `NodeContent::Marker` are reserved for workflows that do not exist (`crucible-core/src/turn/tree.rs:251`) | code-wrong | S |
| G30 | session | One `SessionLog` owner behind `SessionStorage` (4.8) | `FileSessionStorage::new(...)` is rebuilt at four sites that bypass `Arc<dyn SessionStorage>` (`server/session/models.rs:207`, `server/session/messaging.rs:126`, `session_bridge.rs:416`, `server/mod.rs:486`) | code-wrong | M |
| G31 | session | Clients never read storage (4.27) | The CLI reads `session.jsonl` as a fallback (`crucible-cli/src/commands/session/io.rs:21,36`, `chat_runner/actions.rs:795-804`) | code-wrong | M |
| G32 | session | Every attached kiln is indexed at setup (3.8) | `spawn_setup_task` indexes the first kiln only (`server/session/mod.rs:126-131`) | code-wrong | S |
| G33 | session | A child ends with its parent (3.8) | `child_session_ids` turns a storage error into an empty list (`session_manager.rs:369`) | code-wrong | S |
| G34 | session | `Notification` has `created: SystemTime` (3.24) | `Session.notifications` persists an `Instant`-aged queue in `meta.json` (`crucible-core/src/types/notification.rs:20`) | code-wrong | S |
| G35 | session | `WorkspaceSnapshot` is `GitTree` or `Journal` (3.11) | Three shapes in four fields (`workspace_snapshot.rs:123`); git plumbing is duplicated with `review/git.rs:300,210-240` | code-wrong | S |
| G36 | session | `cru.session.undo`, `can_undo`, `undo_depth` read live data (F49) | The client `Undoable` impl returns constants while the RPC wrappers are unused (`rpc_client/agent/agent_handle.rs:441-464`) | code-wrong | S |
| G37 | session | `Workspace { path, kind }` with `Scratch(SessionId)` (3.3, D16) | `Session.workspace` is a path (`session/types/session.rs:31`) | both-acceptable | - |
| G38 | session | `fire_stage` is one operation (S12) | The session-VM-then-plugin-VM loop is hand-written eleven times (`tool_call.rs:358-393`, `permission.rs:331-365,502-533`, `stream.rs:1181-1199,1250-1279`, `tool_hooks.rs`, `precognition/mod.rs:159-175,253-274`) | code-wrong | M |
| G39 | session | One payload per stage name (8.2) | `post_llm_call` is emitted with two payloads (`stream.rs:1152` wire, `stream.rs:1173` Lua) | code-wrong | S |
| G40 | session | `StageId` has `SessionStart`, `TurnStart`, `ValidateOutput`, `SessionEnd`, `PermissionRequest`, `Compact` (8.2, D3) | The 11 stages are `PreToolCall`, `ToolResult`, `PreLlmCall`, `PostLlmCall`, `TransformContext`, `PrecognitionSelect`, `PrecognitionFormat`, `TurnComplete`, `ToolBeforeExecute`, `ToolDisplayStart`, `ToolDisplayComplete` (`crucible-lua/src/handlers/hook_name.rs:119`); session start and end are `SessionLifecycle`; permission hooks and validators have their own registries | expectation-incomplete | - |
| G41 | session | A `Compact` stage lets Lua replace compaction (D24) | No compact stage; `session.compact` exists as an RPC only (`rpc/dispatch.rs:115`) | not-built | - |
| G42 | session | `session.fork` copies the agent record (3.9) | `fork_session` builds a second storage and copies no agent config (`session_bridge.rs:395`) | code-wrong | S |
| G43 | session | One JSON projection of `Session` (6.1) | Three hand-built projections differ on `title` (`session_bridge.rs:89,100,118`) | code-wrong | S |
| G44 | session | `SessionAgent` is built in one place (4.10) | `internal_from_config` plus two literals (`server/session/create.rs:454`, `crucible-cli/src/commands/session/acp.rs:527`) | code-wrong | S |
| G45 | session | `DaemonAgentHandle::clear_history` is one RPC (4.7) | It re-creates the session, re-applies 8 knobs and re-subscribes on the client (`rpc_client/agent/agent_handle.rs:73-151`) | code-wrong | M |
| G46 | session | `TurnError` and `AgentError` are distinct (3.11) | Four identical variants (`crucible-core/src/turn/mod.rs:200,221`); `AgentError::PermissionNotFound` is reused for a missing interaction id (`agent_manager/interaction.rs:125`) | code-wrong | S |
| G47 | session | `ChatError` carries a constructor (4.10) | The `ChatToolResult` error literal appears seven times; core has no constructor (`crucible-core/src/traits/chat.rs:90`) | code-wrong | S |
| G48 | session | `StreamingChunk` does not exist; ACP frames map to `SessionEvent` (4.19) | `StreamingChunk` is a layer over `TurnEvent`; the loop exists twice and one path has no caller (`acp/streaming.rs:23`, `acp/client/streaming.rs`) | code-wrong | S |
| G49 | knowledge | `KilnId(Uuid)` is stable across a rename (3.1) | `KilnName` is the identity (`crucible-core/src/config/kiln_name.rs:51`); no uuid | both-acceptable | - |
| G50 | knowledge | `IndexState` and `ProcessingResult { skipped, indexed, events }` (3.1, 4.2.2) | `KilnManager::open_and_process` returns a 4-tuple of counts (`kiln_manager.rs`) | code-wrong | S |
| G51 | knowledge | `NoteKey { kiln, rel_path }` (3.4) | `NoteRecord` keyed by path inside a per-kiln database; `Scope` has one variant (`crucible-core/src/storage/scope.rs:73`) | both-acceptable | - |
| G52 | knowledge | `ParsedNote` holds each list once (3.4) | Two copies of six lists (`parsed_note.rs:39-57`, `content.rs:31-49`); the daemon DTO fills one copy and `repository.rs:111` reads the other | code-wrong | M |
| G53 | knowledge | One `ContentHash([u8; 32])` (3.4) | `BlockHash` and `FileHash` are twins (`parser/types/block_hash.rs:12`, `types/hashing.rs:25`); `hashing/` has zero callers; two meanings of "block hash" | code-wrong | S |
| G54 | knowledge | `hash_blocks` always produces `Block` rows (4.1) | The parser emits `NoteContent.blocks` on every parse, in document order with source-map spans (`parser/types/blocks.rs`). No row is stored: nothing persists a block. | partial | - |
| G55 | knowledge | One frontmatter splitter (4.1) | Eight (`implementation.rs:241`, `frontmatter_extractor.rs:111`, `workflow.rs:276`, `task.rs:213`, `crucible-cli/src/commands/workflow.rs:365`, `rpc/workflow_handlers.rs:583`, `tools/utils.rs:31`, `tools/notes/helpers.rs:91`) | code-wrong | M |
| G56 | knowledge | `LinkTarget::{Resolved, Ambiguous, Dangling}` (3.5, D17) | `LinkResolution { resolved_target: Option, target_key, is_ambiguous }` (`storage/sqlite/link_index.rs:108`); `is_ambiguous` is a flag beside a winner | both-acceptable | - |
| G57 | knowledge | `Embedding` is a separate entity with a `block` field (3.7) | Built. `note_blocks` stores one row per block (schema v7); `search_blocks` scores them; precognition retrieves through it and falls back to whole notes for a kiln with no rows yet. | built | - |
| G58 | knowledge | `EmbeddingBackend` has `embed`, `dimensions`, `batch_size` and two impls (4.2.4) | `EmbeddingProvider` has 6 required, 2 defaulted, 5 impls plus three mocks (`crucible-core/src/enrichment/embedding.rs:36`) | code-wrong | S |
| G59 | knowledge | The provider cache is injected (hermeticity) | `EMBEDDING_PROVIDER_CACHE` is process-global (`embedding.rs:19`) | code-wrong | S |
| G60 | knowledge | `expected_dimensions` comes from config (4.2.4) | `expected_dimensions_for_model` hardcodes dimensions (`llm/embeddings/config.rs:14`) | code-wrong | S |
| G61 | knowledge | The pipeline emits `note:*` events (S4) | `NoteStore::upsert` returns events the caller must announce (`note_store.rs:455,487`) | code-wrong | S |
| G62 | knowledge | No defaulted trait methods (AGENTS.md) | `NoteStore` defaults five link methods to empty (`note_store.rs:498-526`); `KnowledgeRepository::search_vectors` defaults to `Ok(vec![])` (`knowledge.rs:87`) | code-wrong | S |
| G63 | knowledge | One storage backend; an enum beats a trait (9.12) | `dyn NoteStore` with two production impls; the second is `DaemonNoteStore` in the CLI (`rpc_client/storage.rs:212`) | code-wrong | M |
| G64 | knowledge | Note tools call Knowledge as typed operations (S19) | `note_store: Option<Arc<dyn NoteStore>>` on `NoteTools`, `SearchTools`, `KilnTools` is always `None` in production (`tools/mcp_server.rs:251` is the only setter) | code-wrong | M |
| G65 | knowledge | One content search engine (F19) | `tools/grep_engine.rs` and `tools/workspace.rs:445` (shells out to `rg`) | code-wrong | S |
| G66 | knowledge | One watcher backend with one debounce (4.6) | Three backends, two stubs (`polling_backend.rs:100-116`, `editor_backend.rs:119-149`); two debounce layers in series (`notify_backend.rs:60`, `manager.rs:445`); `shutdown` stops nothing (`manager.rs:207-212`) | code-wrong | M |
| G67 | knowledge | The watcher reads its config (4.6) | `WatchConfig.debounce`, `handler_config`, `mode`, `max_concurrent_handlers`, `enable_monitoring` are never read; handler priority is never observed (`manager.rs:562-588`) | code-wrong | S |
| G68 | knowledge | The parser compiles without tokio (4.1, 7) | `MarkdownParser` is `async_trait`; `SimpleBlockHasher` is async over CPU work (`block_hasher.rs:35-112`); `process_content` blocks on a Tokio handle (`extensions.rs:59`) | code-wrong | S |
| G69 | knowledge | Enums over traits inside one crate (AGENTS.md) | `SyntaxExtension` has 8 impls in one crate behind `dyn` (`parser/extensions.rs:18`) | code-wrong | M |
| G70 | knowledge | `MarkdownParser` is not `dyn` with one impl (9.12) | `Arc<dyn MarkdownParser>` with one impl (`note_pipeline.rs:59`) | code-wrong | S |
| G71 | knowledge | `cru tasks`, `cru stats`, `cru workflow` are thin RPC callers (4.27) | `tasks.rs:55` builds `TaskGraph`, `workflow.rs:360` builds `WorkflowDoc`, three kiln walkers (`stats.rs:54`, `process.rs:329`, `workflow.rs:372`) in the CLI | code-wrong | M |
| G72 | knowledge | `FsOps` lives in the daemon (4.5) | `crucible-web/src/routes/search.rs:415`, `routes/kiln.rs:353`, `routes/canvas.rs:172` write kiln files with `tokio::fs`; `search.rs:128-171` walks the kiln | code-wrong | M |
| G73 | knowledge | No queue architecture (4.2.2) | `processing/mod.rs:44-390` and `change_detection.rs` describe one no code implements; `ChangeDetectionStore` is dead | code-wrong | S |
| G74 | knowledge | Model listing is on demand per provider (3.18) | `llm/model_discovery.rs` (608 lines) has no caller and pulls `gguf` and `shellexpand` | code-wrong | S |
| G75 | knowledge | `Proposal` is owned by `ProposalStore` inside Knowledge (3.25) | Proposals are plain files the CLI moves (`crucible-cli/src/commands/proposals.rs:44`); no daemon type | both-acceptable | - |
| G76 | knowledge | `SqlitePropertyStore` does not exist twice (4.2.2) | `SqlitePropertyStore` duplicates `impl PropertyStore for SqliteNoteStore` and has no caller (`property_store.rs:223,172`) | code-wrong | S |
| G77 | events | One typed `SessionEvent` enum on every surface (3.13) | `SessionEventMessage` is `{event, data}`; `SessionEventPayload` (70 names) is a view; the scripting `SessionEvent` + `InternalSessionEvent` has 12 of 52 live variants; `LogEvent` 5 of 16; a client `SessionEvent`; a web `ChatEvent` with 6 dead variants | code-wrong | L |
| G78 | events | Event groups are a closed set with a gate (8) | `Group::of` is a hand-maintained 70-name match; drift surfaces as `UnknownEvent` (`protocol/session_events/mod.rs:133`) | code-wrong | S |
| G79 | events | One persist predicate (3.13, 4.18) | `should_persist` decodes, then `server/core/mod.rs:465-474` matches names by string again | code-wrong | S |
| G80 | events | `ScriptingEvent` holds `segment_complete`, `mode_changed`, `title_changed` (8.4, D5) | The ten are `MessageReceived`, `TextDelta`, `AgentThinking`, `AgentResponded`, `ToolCalled`, `ToolCompleted`, `SessionEnded`, `InteractionRequested`, `InteractionCompleted`, `PrecognitionComplete` (`events/session_event/mod.rs:75`) | expectation-wrong | - |
| G81 | events | `EventName` ends with `SessionCreated`, `SessionEnded` (8.3, D4) | The eight are `FileChanged`, `FileDeleted`, `FileMoved`, four `Note*`, `WebhookReceived` (`hook_name.rs:44`) | expectation-incomplete | - |
| G82 | events | `InteractionResponse::Acknowledged` answers `Show` (3.15) | `Show` has no response variant (`crucible-core/src/interaction/types.rs:479`) | code-wrong | S |
| G83 | events | `Popup` and `Panel` share one item type (3.15) | `PopupEntry` and `PanelItem` are field-identical (`types/popup.rs:16`, `interaction/types.rs:135`) | code-wrong | S |
| G84 | events | One `InteractionBroker` pending table (4.13) | `PendingPermission` and `PendingInteraction` are separate with two inline 300 s timeouts (`agent_manager/mod.rs:288`, `interaction.rs:25`, `permission.rs:166,937`) | code-wrong | S |
| G85 | events | `respond(id, InteractionResponse)` takes any kind (S17) | Lua `sessions.interaction_respond` hardwires `respond_to_permission` (`sessions/register.rs:344`) | code-wrong | S |
| G86 | events | `Job` has `JobState::{Running, Done, Cancelled, Failed}` (3.23) | `get_job_result` returns `output: None` for a running job; callers inspect `info.status` (`background_manager/mod.rs`) | code-wrong | S |
| G87 | events | No dead event machinery (3.13) | `EventRing` is write-only (`ring.rs:74`); `events/markdown/` (1,148 lines) is unreachable; two `serde_md` serializers have no caller; `EventError` is never built | code-wrong | S |
| G88 | events | `EventEmitter` is one impl with required methods (4.18) | One production impl, two defaulted methods, `dyn` in six signatures; `EmitOutcome.cancelled` is never set, so `indexing.rs:255-263` is dead | code-wrong | S |
| G89 | events | Notifications push as a `SessionEvent` (D14) | `NotificationPayload` has two variants (`lifecycle.rs:156`) | both-acceptable | - |
| G90 | events | `StreamGap` reaches every client (4.18) | `forward_events` inserts `stream_gap` for socket clients (`core/mod.rs:109`); the persist task drops lagged events with a warning (`server/mod.rs:551-556`); the web `EventBroker` has no stated policy | code-wrong | S |
| G91 | wire | RPC errors are `{ code, message, retryable }` and idempotent calls retry on `retryable` (6.1) | `DaemonClient` flattens to `anyhow!("RPC error: {}")`; `TRANSIENT_ERROR_PATTERNS` matches text; the web sniffs `-32602` as a substring (`crucible-web/src/error.rs:88`) | code-wrong | M |
| G92 | wire | `Capabilities { methods, build_sha }` is one type (4.26) | `DaemonCapabilities` is client-only; the server emits `json!` (`rpc/dispatch.rs:1103-1118`) | code-wrong | S |
| G93 | wire | Every advertised method works (8.5) | `session.reindex` stays in `METHODS` and always fails (`dispatch.rs:175,591`); `storage.verify`, `cleanup`, `backup`, `restore` return `not_implemented` (`server/storage.rs:3-41`) | code-wrong | S |
| G94 | wire | The dispatcher derives from the table alone (8.5) | `dispatch_session_setter!` and `getter!` re-match the raw string with `_ => unreachable!()` (`dispatch.rs:273-295`) | code-wrong | S |
| G95 | wire | Method spellings follow the product docs: `session.send`, `session.respond_interaction`, `note.create`, `note.update`, `kiln.info`, `search_semantic`, `daemon.status`, `acp.discover`, `plugin.test`, `canvas.get`, `fs.read`, `fs.write`, `auth.store_key` (8.5, D10) | The table has `session.send_message`, `session.interaction_respond`, `note.upsert`, `embed.query`, `ping`, `agents.list_profiles`, `lua.run_plugin_tests`; no `kiln.info`, `canvas.*`, `fs.read`, `fs.write`, `auth.store_key`, `workflow.list`, `workflow.show`, `kiln.stats`, `property_search` (`rpc/dispatch.rs:79`) | both-acceptable | - |
| G96 | wire | One setter and one getter per knob on one field name (6.1, 9.18) | 15 one-field `SessionSet*Request` structs and 15 pairs (`rpc_client/client/agent.rs:32-144`); 16 `cached_*` fields mirror `SessionAgent` | code-wrong | M |
| G97 | wire | A handler is a thin translation with a typed result (4.26) | Every handler returns hand-spelled `json!`; `require_param!` (56 uses) and `typed_params` (9 files) coexist; 46 request structs are client-only | code-wrong | L |
| G98 | wire | One `RpcContext` (4.26) | `ServerContext` duplicates it with 8 unread fields under `#[allow(dead_code)]` (`server/mod.rs:950`) | code-wrong | S |
| G99 | wire | The daemon reads its environment at bind (S41) | `plugin_boot.rs:93,139,148` and `rpc/ui.rs:152` read `dirs::config_dir()`; `platform.rs:58,108,150` call `current_dir()`; `workflow_handlers.rs:237` reads an env var per request | code-wrong | M |
| G100 | wire | `Server::run` is a short accept loop (4.26) | About 500 lines with four inline task bodies (`server/mod.rs:452-945`); `Server::bind` has no caller; `web_config` is a stub | code-wrong | M |
| G101 | wire | Web routes mirror RPC families and a test derives the route set (6.2) | `ReconnectingDaemon` is about 95 hand wrappers over four files with six dead ones (`services/daemon.rs:57`); no route-derivation test is recorded | code-wrong | M |
| G102 | wire | The web server reaches the daemon through the RPC client only (4.27) | It imports `server::plugins::OptionAction`, `project_manager::*`, `webhook::*` (`routes/plugin.rs:8`, `routes/project.rs:8`, `routes/webhook.rs:11`) | code-wrong | M |
| G103 | wire | One error body shape on the web (6.2) | Seven hand-built error bodies; `OkResponse` beside eight `json!({"ok": true})` literals; `NoteListItem` is a 5-tuple (`routes/helpers.rs:22`) | code-wrong | S |
| G104 | wire | ACP types never leave `AcpHost` (4.19) | The CLI imports `acp::streaming::humanize_tool_title` (`crucible-cli/src/tui/oil/components/tool_render.rs:77`); `acp/tools.rs:29 ToolDescriptor` duplicates `ToolDefinition` | code-wrong | S |
| G105 | wire | Five built-in ACP profiles in one table (8.19) | The table is written twice with no gate (`acp/discovery.rs:52`, `acp_launch.rs:126`) | code-wrong | S |
| G106 | wire | `AcpAgentServer` lives in the daemon crate (7) | `CrucibleAcpAgent` lives in the CLI and proxies to the daemon over RPC (`crucible-cli/src/commands/acp/agent.rs:50`) | both-acceptable | - |
| G107 | wire | One request-id counter per ACP client (4.19) | One process-global `REQUEST_ID` (`acp/client/mod.rs:28`); timeout arithmetic is split across three files | code-wrong | S |
| G108 | wire | `Recorder` reads its config once (4.19) | `Recorder::from_env` reads env on every `with_name` (`acp/client/recording.rs:71,81`); a test calls `std::env::remove_var` | code-wrong | S |
| G109 | wire | MCP never advertises workspace tools (6.4) | `CrucibleMcpServer::get_info` lists workspace tools the router does not serve (`tools/mcp_server.rs:714`) | code-wrong | S |
| G110 | wire | One `McpGateway` built at bind from `[mcp]` (4.21) | T3-A10 (`358dd41d2`) starts the reconnect loop at daemon run; T3-A11 (`33703918c`) attaches the gateway to the served MCP surface (`crates/crucible-daemon/src/tools/extended_mcp_server.rs:123`, `crates/crucible-daemon/src/tools/mcp_gateway.rs:486`) | closed | - |
| G111 | wire | `rmcp` types map once to `ToolDefinition` and `ToolOutcome` (6.4) | Five copies of `CallToolResult -> Value`; three of `ToolDefinition -> rmcp::Tool`; three of the reverse (`tool_dispatch.rs:80,423`, `workspace.rs:564`, `gateway_executor.rs:52`, `extended_mcp_server.rs:405,421`) | code-wrong | S |
| G112 | wire | `discover_tools` reports `source` from `ToolSource` (4.11) | `discovery_tools()` advertises `["builtin","lua"]` while the classifier never returns `"lua"` (`extended_mcp_server.rs:172`, `tool_discovery.rs:76`) | code-wrong | S |
| G113 | wire | The client parses `SessionEventMessage` once (6.5) | `rpc_client/client/types.rs:10 SessionEvent` duplicates it and drops `seq` and `timestamp`; the CLI copies one into the other (`chat_runner/runner.rs:168-179`) | code-wrong | S |
| G114 | wire | `NoteRecord` crosses the wire without loss (6.1) | `NoteRecordDto::into_parsed_note` sets `offset = index` and `target_span = (0, 0)` (`rpc_client/storage.rs:100-113`); `FtsResult` is hand-serialized and re-parsed as `TextSearchHit` | code-wrong | S |
| G115 | wire | `StorageClient` exists only with a working impl (9.12) | One impl whose one method always bails (`rpc_client/storage.rs:45-52`) | code-wrong | S |
| G116 | lua | `DaemonBridge` is one trait with required methods (4.17) | `DaemonSessionApi` defaults 15 of 32 to `Err("not implemented")` (`sessions/mod.rs:288-458`); `DaemonToolsApi` is separate | code-wrong | M |
| G117 | lua | A trait requires its contract (AGENTS.md) | `SessionConfigRpc` requires 0 of 22; five impls are `impl SessionConfigRpc for X {}` (`session_api.rs:67`) | code-wrong | S |
| G118 | lua | `PluginSource` has four variants `EnvPath`, `User`, `RuntimePath`, `Runtime` (8.15, D7) | Three: `EnvPath`, `User`, `Runtime`; `runtimepath` entries and `$CRUCIBLE_RUNTIME` share `Runtime` (`crucible-lua/src/manifest.rs:295`) | expectation-wrong | - |
| G119 | lua | `spec.handlers` registers hooks (F172) | `PluginSpec.handlers` is parsed and never dispatched (`daemon_plugins/mod.rs:741`) | not-built | - |
| G120 | lua | `Capability` is one closed set with one decoder (8) | `parse_capability` hand-duplicates serde and omits `intercept_tools`, so a spec-table grant is dropped (`lifecycle/spec.rs:31`, `discovery.rs:267`) | code-wrong | S |
| G121 | lua | Modes exist in Lua only; no Rust copy of the names (8.6) | `BuiltinMode` (`crucible-core/src/types/mode.rs:85`), `BUILTIN_MODE_NAMES` (`tools/tool_modes.rs:37`), `default_internal_modes` (`mode.rs:273`) restate the three names | code-wrong | S |
| G122 | lua | `cru.log.notify` reaches a client (F134) | `cru.log.notify` is a live Lua surface (`crates/crucible-lua/src/notify.rs:30`, registered at `crates/crucible-lua/src/executor.rs:260`); the queue reaches no client (`notify.rs:78`); Expected 2a lists it | expectation-incomplete | - |
| G123 | lua | `cru.oil` nodes render somewhere (open 15) | `LuaNode` is built and nothing in the CLI consumes it (`crucible-lua/src/oil.rs:138`) | not-built | - |
| G124 | crates | `crucible-lua` and `crucible-oil` depend on `core` only (7, D19) | `crucible-lua` imports `crucible_oil::style` and node builders (Actual 4) | code-wrong | M |
| G125 | lua | One colour codec (3.29) | Four parsers across `theme.rs`, `theme_wire.rs`, `hl_lua.rs`; `ThemeLayout` and `UiLayout` are twins; `ThemeIcons`, `ThemeSpinnerStyle`, `BorderStyle`, `StatusBarPosition` are parsed and read by no renderer | code-wrong | S |
| G126 | lua | Theme state is per VM (4.17) | `CONFIG` is a process-global `OnceLock<RwLock<ConfigState>>` (`crucible-lua/src/config.rs:55`); every VM shares it | code-wrong | S |
| G127 | lua | `cru.session` and `cru.kiln` names are one list (9.3) | 28 `cru.session` names and 6 `cru.kiln` names are listed twice as strings with no check (`sessions/register.rs:26-74,187-867`, `vault/mod.rs:64,133`) | code-wrong | S |
| G128 | lua | `Skill` carries `shadowed_by`; `name == dir` is checked (3.19) | The rule is documented and not checked (`skills/types.rs:95`); `content_hash` is computed and never read; `platform.rs:66-180` copies fields by hand and drops five | code-wrong | S |
| G129 | lua | `cru.permissions.on_request` is available to plugins (9.4) | `register_permission_hook_api` is called only from `session_vm.rs:113`; the plugin loader never registers it | code-wrong | S |
| G130 | lua | Hooks are named by the name table, not by position (9.3) | `register_permission_hook_api` names hooks from `guard.len()` (`handlers/permission.rs:132`) | code-wrong | S |
| G131 | lua | `cru.defaults` exposes every default (F183) | `cru.defaults.mode` is stored but never exposed (`session_defaults.rs:92-175`) | code-wrong | S |
| G132 | lua | One plugin path computation (8.15) | `daemon_plugin_paths` and `PluginManager::with_standard_paths` both compute it (`bootstrap.rs:33`, `lifecycle/mod.rs:112`) | code-wrong | S |
| G133 | lua | A plugin spec loads once (3.20) | `load_plugin_spec` runs the file in a throwaway VM, then the daemon runs it again in the real VM (`spec.rs:140`, `discovery.rs:295`) | code-wrong | S |
| G134 | lua | No dead cross-crate path (4.17) | `SessionCommand`, `ChannelSessionRpc` and the CLI `handle_session_command` form a dead path; `with_session_command_receiver` has no caller | code-wrong | S |
| G135 | lua | One Lua tool shape (9.2) | `LuaTool`/`DiscoveredTool` and `ToolParam`/`DiscoveredParam` duplicate; `execute_tool`, `execute_file`, `execute_source` have no caller | code-wrong | S |
| G136 | lua | Plugin commands reach the web palette (9.13) | The web shows plugin commands as a count only | not-built | - |
| G137 | lua | `cru.plugin.set_status` renders in both clients (F105) | The web has no renderer for status slots | not-built | - |
| G138 | lua | `StubGenerator::verify` uses a temp dir the caller gives (tests) | It writes under `std::env::temp_dir()` (`stubs.rs:82`) | code-wrong | S |
| G139 | render | Pure display state local; everything else in the daemon (4.27) | The `config/` overlay engine (about 2,500 lines), `:set` semantics, help text and `parse_config_scalar` live in the CLI; `chat_app/shell.rs:123` writes a permission rule to a config file | code-wrong | L |
| G140 | render | `session.export` renders on the daemon (4.7) | `ExportSession` loads events and renders markdown in-process (`chat_runner/actions.rs:784-827`) | code-wrong | S |
| G141 | render | The CLI has no storage (7) | `ShellModal::save_output` writes under the session dir (`shell_modal.rs:320`) | code-wrong | S |
| G142 | render | `ToolSource` is `Builtin`, `Plugin`, `McpUpstream`, `Acp` (8.9, D9) | `Core`, `Crucible`, `Mcp`, `Plugin`, `Acp` (`crucible-core/src/types/tool_ref.rs:41`) | both-acceptable | - |
| G143 | render | `ToolSource` crosses the wire typed (8.9) | The daemon formats a `Mcp:x` string (`messaging/mod.rs:17`), the CLI parses it (`message_handlers.rs:15`) into `ToolSourceDisplay` (`viewport_cache.rs:10`) | code-wrong | S |
| G144 | render | REPL commands are one table (8.12) | Four hand-kept lists (`autocomplete.rs:201,474`, `command_handling.rs:20,99`) | code-wrong | S |
| G145 | render | Oil helpers exist once (7) | `truncate_to_width`, `truncate_to_chars`, `wrap_content`, `clamp_input_lines`, `InputArea` exist in both `crucible-oil` and `crucible-cli/src/tui/oil/utils/` | code-wrong | S |
| G146 | render | One status snapshot (F117) | `StatusBar`, `StatusBarData`, `StatusComponent` are field-identical and cloned per frame | code-wrong | S |
| G147 | render | Render state is injectable for tests (hermeticity) | Five process-wide `RwLock` theme stores leak between tests; `AdaptiveColor::resolve` reads `NO_COLOR` (`style.rs:313`) | code-wrong | S |
| G148 | render | `:set` keys have typed defaults (F110) | `RuntimeConfig::new` is never called; every `ShortcutTarget::Path` default is `""` and numbers stay strings (`config/overlay.rs:59`); `parse_bool` calls `expect` on user input | code-wrong | S |
| G149 | render | Keybindings with a readline set are remappable (F135) | Keybinding remaps remain unimplemented | not-built | - |
| G150 | render | Two markdown renderers do not exist (4.27) | `formatting/markdown_renderer.rs` and `tui/oil/markdown/`; `formatting/` imports `tui/oil` | code-wrong | S |
| G151 | render | Web types are generated from the daemon contract (9.15) | `types.ts` mirrors eleven types by hand; `FsEntry` is mirrored at `types.ts:244` | both-acceptable | - |
| G152 | render | `McpServerInfo` is one type (F124) | `types/mcp_status.rs:15`, `traits/mcp.rs:138` and `McpServerDisplay` | code-wrong | S |
| G153 | render | Dead components do not ship (7) | `template/node_spec.rs` (1008 lines) dead apart from `parse_color`; `OilRunner`, `run_sync`, `ComposerConfig`, `detect_dark_terminal`, `with_alternate_screen` have no caller | code-wrong | S |
| G154 | config | One canonical `AppConfig` (4.25) | `CliAppConfig` and `CliConfig` are re-exported under each other's names (`crucible-cli/src/config.rs:10-18`); `execution_roots.rs` parses `settings.json` on its own | code-wrong | M |
| G155 | config | `ProviderKind` has nine variants (8.11, D8) | `BackendType` has twelve: the nine plus `Burn`, `Custom`, `Mock` (`crucible-core/src/config/components/backend.rs`) | expectation-wrong | - |
| G156 | config | Provider knobs exist once (3.18) | `ChatConfig` and `LlmProviderConfig` repeat five knobs; five enrichment provider structs repeat five fields; `BackendType` metadata and `defaults.rs` restate endpoints and disagree on VertexAI | code-wrong | M |
| G157 | config | `ChatError` classifies `retryable` and `retry_after` (4.10, 9.8) | Provider error classification is planned (F73); the `ChatError` prefix list is copied verbatim (`crucible-web/src/events.rs:401`, `rpc_client/agent/convert.rs:118`) | not-built | - |
| G158 | config | `cru init` detects providers through the daemon (4.10) | `provider_detect.rs` (541 lines) duplicates `discover_env_providers` because `cru init` runs before the daemon | expectation-wrong | - |
| G159 | config | `CredentialStore` is an enum (AGENTS.md) | Three impls in one file, one never compiled (`crucible-core/src/config/credentials.rs:87`) | code-wrong | S |
| G160 | config | `project.toml` `kilns` seeds the session kiln set (3.2) | Parsed and ignored (open 7) | not-built | - |
| G161 | config | Sessions never live in a kiln (3.8) | `init.rs:402` creates `<kiln>/.crucible/sessions/`; kiln-local `config.toml` never loads | code-wrong | S |
| G162 | config | `TRACKED_FIELDS` covers every leaf (4.25) | 18 of about 60; the coverage test checks the list against itself (`cli_app.rs:5`) | code-wrong | S |
| G163 | config | Inert config fields do not exist (4.25) | `acp.lazy_agent_selection`, `storage.idle_timeout_secs` (both reserved, documented), eight of nine `[enrichment.pipeline]` fields are read by nothing; T3-B18 deleted `DiscoveryConfig` and `ResolveMode` | code-wrong | S |
| G164 | config | Storage maintenance works (F223) | `storage.verify`, `cleanup`, `backup`, `restore` return `not_implemented` | not-built | - |
| G165 | tests | Test doubles sit behind `test-utils` (7) | `crucible-core/src/lib.rs:32 pub mod test_support` is unconditional; `EnvVarGuard` ships in `cru`; `crucible-daemon/src/test_support.rs` always compiles | code-wrong | S |
| G166 | tests | No `std::env::set_var` in production (hermeticity) | `crucible-cli/src/main.rs:53,142` call it before the runtime starts | code-wrong | S |
| G167 | tests | Test harnesses live in test modules (7) | `ComponentHarness`, `AppHarness`, `KilnManager::new`, `NotePipeline::new`, eight `PluginManager` methods are test-only production API | code-wrong | S |
| G168 | tests | A closed set is proved by `EnumIter` (8) | `lua_plugin_suite.rs:438` reads its own source with `include_str!` to enumerate test arms | code-wrong | S |
| G169 | tests | Traits have test doubles when `dyn` (AGENTS.md) | `PermissionGate`, `Undoable`, `MarkdownParser`, `FileWatcher`, `WatcherFactory` are `dyn` with one impl and no double | code-wrong | S |
| G170 | knowledge | No session semantic index (F27) | No pipeline embeds transcripts | not-built | - |
| G171 | scope | Prompt-injection scanner in `SkillRegistry` (4.16, F93) | No scanner | not-built | - |
| G172 | session | Verification evidence ledger in `review.jsonl` (F94) | No second record type | not-built | - |
| G173 | session | Global estop sentinel (F74) | No sentinel | not-built | - |
| G174 | events | Agent-initiated `Ask` (F87) | No tool produces an `Ask` | not-built | - |
| G175 | knowledge | Note types and templates (F17) | None | not-built | - |
| G176 | crates | Telegram and Matrix are Lua plugins over `cru.service` (7) | Neither exists; Discord exists | not-built | - |
| G177 | session | Per-session parallel tool dispatch is a decision (open 40) | The loop dispatches one call at a time (`messaging/tool_call.rs`) | both-acceptable | - |
| G178 | session | `GenaiAgentHandle` and `AcpAgentHandle` present the same events (F125) | `GenaiAgentHandle` never yields `TurnEvent::ToolResult`; `AcpAgentHandle` does (`acp_handle.rs:629,716`) | code-wrong | S |
| G179 | session | `DEPTH_CAP_PROMPT` exists once (3.11) | A second copy of `TOOL_DEPTH_LIMIT_FINAL_PROMPT` kept in sync by comment (`genai_handle.rs:1285`) | code-wrong | S |
| G180 | scope | `Scm` expands one tilde (4.24) | Seven tilde expanders (`project_manager.rs:68`, `scm.rs:156,221`, `bootstrap.rs:113`, `kiln_manager.rs:1199`, two in the CLI) | code-wrong | S |

## 3. Verdicts

### 3.1 `code-wrong`

Each paragraph names the target shape, then the first step.

**G1.** Target: one `fn contain(root, rel) -> Result<ContainedPath>` in
`tools/fs_scope.rs` that the web, the review module and the canvas module call.
First step: move the `canvas/containment.rs:215` loop into `fs_scope.rs`, then
replace the five daemon copies with a call.

**G2.** Target: `PermissionEngine::decide -> GateDecision { verdict, layer }`,
with `Layer` an enum. The prompt moves to `ToolDispatch`. `PermissionGate`
disappears; `DaemonPermissionGate` becomes a free function. First step: add the
`layer` field and return it from the engine; leave the prompt where it is.

**G3.** Moved to `both-acceptable` on 2026-08-22. T3-C9 found that the three
allow lists have different override semantics and documented the order in
[[Bash Permission Layers]] instead of a merge. One defect stays open from that
pass: `PatternStore::matches_bash` matches a prefix on the whole command and
does not split chained statements
(`crates/crucible-core/src/config/patterns.rs:289`).

**G4.** Target: one `PermDecision` enum in `crucible-core/src/interaction/`.
First step: delete `permissions/types.rs:5 PermissionScope` and make the engine
take the interaction one.

**G5.** Target: `tokio::time::timeout(1 s, hooks)`. First step: wrap the call
at `messaging/permission.rs:1108`, then delete the elapsed-time check.

**G7, G9, G10, G11.** Target: every tool class is a method on `BuiltinTool`:
`surface()`, `served_over_mcp()`, `plan_mode_allowed()`, `is_write()`,
`is_discovery()`. First step: add `InvokeTool` to the enum, then replace
`DISCOVERY_TOOL_NAMES` with a match; repeat one list per commit. Delete the
CLI `BUILTIN_TOOLS` and read `discover_tools` over RPC instead. Status: plan
T3-C6 deleted `KILN_BACKED_TOOLS` and `BUILTIN_TOOLS` (section 6).

**G12.** Target: the CLI calls `kiln.list` and `kiln.open`; only the daemon
holds a `KilnRegistry`. First step: make `KilnRegistryContext::for_daemon` take
`cwd` and `home` as parameters from `Server::bind_with_data_home`.

**G13.** Target: one `BuiltinTool::touches_path_args()` method. First step:
replace the two lists in `permission.rs` with it, then `is_file_tool`.

**G14.** Target: `enforce_child_isolation` returns `Err` when the lifecycle is
unbound. First step: change the early `return` at `delegation.rs:181` to an
error and fix the test that relied on it.

**G15.** Target: `fn admit(call) -> Admission` in `gate_decision.rs` that runs
the seven gates in order and one test that permutes them. First step: move the
`ToolPolicy::Deny` check into `requires_permission_gate` so no arm is
`unreachable!`.

**G16, G99.** Target: `RpcContext` carries `config`, `config_home`, `cwd`,
`home`; no daemon function calls `std::env`, `dirs`, or `current_dir`. First
step: replace `execution_roots::baseline` env reads with values from
`RpcContext`, then the four `dirs::config_dir()` calls.

**G17.** Target: the credential deny list and enclosing-root resolution move to
`project_manager.rs` and `kiln_registry.rs`. First step: export
`forbidden_root_reason` as the only deny list and delete
`routes/project.rs:28-76`.

**G18, G19.** Target: async `PatternStore::load` and `save`; one
`fn engine_input(tool, args) -> &Value`. First step: add the helper and call it
at the three sites.

**G20, G21.** Target: `enum SessionAgent { Internal(InternalAgent),
Acp(AcpAgent) }` plus `struct SessionConfig` in `crucible-core/src/session/`,
with a serde `#[serde(tag = "agent_type")]` that keeps `meta.json` readable.
First step: introduce `SessionConfig` and move the ten knobs off `SessionAgent`
while the struct stays flat; the enum split comes second.

**G22.** Target: `AgentHandle { send, cancel, configure }`. First step: make
the 16 knob setters one `configure(&SessionAgent, &SessionConfig)` on
`GenaiAgentHandle`, then delete the per-knob defaults one family at a time.

**G24.** Target: `SessionState::{Active, Paused, Streaming { turn }, Ended}`.
First step: delete `Compacting`, then set `Streaming` in `send_message` and
clear it in the `TurnOutcome` path.

**G25.** Target: `enum ThinkingBudget` with six levels and a
`From<i64>`. First step: add the enum in core, keep the `i64` on the wire
through `serde(from, into)`.

**G28.** Target: build `StopReason::MaxToolDepth` at the depth cap. First step:
find the `DepthCapHit` emit in `stream.rs:751` and set the reason there.

**G29, G73, G74, G87, G153.** Target: no reserved or dead code. First step:
delete the listed items in one commit each and run `just ci`.

**G30, G31.** Target: one `Arc<dyn SessionStorage>` on `SessionManager`; the
CLI calls `session.load_events` and `session.render_markdown`. First step:
replace the four `FileSessionStorage::new` sites with `sm.storage()`; then
delete the CLI fallback readers.

**G32.** Target: `spawn_setup_task` iterates the kiln set. First step: change
the `first()` at `server/session/mod.rs:126` to a loop.

**G33.** Target: `child_session_ids -> Result<Vec<SessionId>>`. First step:
propagate the error and make `archive_session` fail loudly.

**G34.** Target: `Notification { created: SystemTime }`. First step: replace
`Instant` with `SystemTime` and drop the `#[allow(dead_code)]`.

**G35.** Target: `enum WorkspaceSnapshot { GitTree(oid), Journal(map), None }`.
First step: write the enum and a `From` for the current struct, then move
`review/git.rs` plumbing into one `git.rs`.

**G36.** Target: the client `Undoable` impl calls `session.undo`,
`session.can_undo`, `session.undo_depth`. First step: wire the three existing
wrappers and delete the constants.

**G38.** Target: one `fire_stage(stage, ctx)` in `agent_manager/hooks.rs` that
takes the session lock, runs the session VM, releases, then runs the plugin
VM. First step: extract the `tool_call.rs:358-393` copy and call it from the
other ten sites.

**G39.** Target: one `PostLlmCall` payload. First step: make the Lua emit at
`stream.rs:1173` read the wire payload.

**G42.** Target: `fork_session` copies `SessionAgent` and the kiln set. First
step: pass the parent record into the `session.create` call at
`session_bridge.rs:395`.

**G43, G44, G47.** Target: one `Session::to_json`, one
`SessionAgent::internal_from_config`, one `ChatToolResult::error(msg)`. First
step: add the constructor, replace the copies.

**G45.** Target: a `session.clear_history` RPC the daemon serves. First step:
add the row to `rpc_methods!` and move the body of
`agent_handle.rs:73-151` behind it.

**G46.** Target: `AgentError::InteractionNotFound`; `TurnError` wraps
`AgentError`. First step: add the variant and use it at
`interaction.rs:125`.

**G48.** Target: the ACP client yields `TurnEvent` directly. First step: delete
the non-callback streaming loop (`client/streaming.rs:164,687,788`), then fold
`StreamingChunk` into `TurnEvent`.

**G50.** Target: `struct ProcessingResult { skipped, indexed, failed, events }`.
First step: replace the 4-tuple return of `open_and_process`.

**G52.** Target: `ParsedNote { content: NoteContent, .. }` with no top-level
copies. First step: delete the six top-level fields and fix the DTO at
`rpc_client/storage.rs:86` to fill `content`.

**G53.** Target: one `ContentHash` newtype; `hashing/` deleted. First step:
alias `FileHash = BlockHash`, then delete `hashing/`. Status: done; plan T5-02
also deleted `types/hashing.rs` and the `FileHash` alias (section 6).

**G55.** Target: one `split_frontmatter(text) -> (Option<Frontmatter>, &str)`
in `crucible-core/src/parser/`. First step: make `implementation.rs:241` the
function, then replace the seven callers.

**G58, G62, G88.** Target: no defaulted methods on `EmbeddingProvider`,
`NoteStore`, `KnowledgeRepository`, `EventEmitter`. First step: remove the
defaults and let the compiler list the impls; make `test_support` the only mock
provider.

**G59, G60.** Target: the provider cache lives on `RpcContext`;
`expected_dimensions` reads `EmbeddingConfig::dimensions()`. First step: move
the `Lazy` into a struct field.

**G61.** Target: `NotePipeline::process` emits the events it gets from the
store. First step: grep every `upsert` caller, then move the emit into the
pipeline.

**G63, G64, G71.** Target: `NoteStore` has one impl; the CLI calls
`tasks.*`, `kiln.stats`, `workflow.*` RPCs; note tools take `Arc<dyn NoteStore>`
not `Option`. First step: wire `NoteTools.note_store` at daemon bind, then make
the field required; then move `TaskGraph` and the kiln walkers behind RPC
rows; then delete `DaemonNoteStore`.

**G65.** Target: `grep_engine.rs` serves both the workspace `grep` tool and
`search_grep`. First step: make `workspace.rs:445` call `grep_engine`.

**G66, G67.** Target: `enum WatchBackend { Notify }`, one debounce, a config
the manager reads. First step: delete the polling and editor stubs and the
`FileWatcher` and `WatcherFactory` traits; then remove one of the two debounce
layers. Status: plan T3-B3 replaced the two traits with the `Backend` enum
(section 6).

**G68, G70.** Target: `fn parse(text) -> ParsedNote`, sync, no trait. First
step: drop `async_trait` from `MarkdownParser` and make `NotePipeline` hold
`CrucibleParser` by value.

**G69.** Target: `enum Extension` with eight variants and one `fn run`. First
step: replace `ExtensionRegistry: Vec<Arc<dyn SyntaxExtension>>` with
`Vec<Extension>`. Status: done in plan T3-B1 (section 6).

**G72.** Target: `fs.read`, `fs.write`, `canvas.get`, `canvas.put`,
`note.resolve` RPC rows. First step: add `fs.write` and route
`routes/kiln.rs:353` through it.

**G76.** Target: one `PropertyStore` impl. First step: delete
`SqlitePropertyStore`.

**G77.** Target: `SessionEventMessage { event: SessionEventPayload, .. }` typed
end to end; the scripting `SessionEvent` is a projection of it; `LogEvent`
derives from it; the client and the web deserialize it, not a copy. First step:
delete the 40 dead scripting variants and the `identifier`, `priority`,
`category`, `estimate_tokens` families; then make `SessionEventMessage::new`
take a `SessionEventPayload`.

**G78.** Target: `Group` derives from the eight enums through one `rename_all`
table. First step: generate `Group::of` from `strum::EnumIter` over each
payload enum.

**G79.** Target: `should_persist` is the only predicate. First step: delete the
string match at `server/core/mod.rs:465-474`.

**G82, G83.** Target: `InteractionResponse::Acknowledged`; one `ListItem` type.
First step: add the variant and alias `PanelItem = PopupEntry`.

**G84, G85.** Target: one `PendingInteraction` table keyed by `RequestId` with
a per-kind timeout; `interaction_respond` takes any `InteractionResponse`.
First step: route permissions through `PendingInteraction`.

**G86.** Target: `JobResult` is an enum by state. First step: return
`Err(JobStillRunning)` from `get_job_result`.

**G90.** Target: the web `EventBroker` forwards `stream_gap` and the persist
task writes a gap marker on lag. First step: emit a `StreamGap` event at
`server/mod.rs:551`.

**G91.** Target: `RpcError { code, message, retryable }` is one type in
`protocol/rpc/`, and `DaemonClient::call` returns it. First step: add the
`retryable` field server-side and delete `TRANSIENT_ERROR_PATTERNS`.

**G92, G93, G94.** Target: `Capabilities` in `protocol/`; `METHODS` holds only
served methods; the setter macros match the enum. First step: delete
`SessionReindex`, then the four `storage.*` rows until they work.

**G96.** Target: one `session.set_config { knob, value }` and
`session.get_config` pair over `SessionConfig`. First step: add the pair, then
retire the 15 structs one at a time.

**G97, G103.** Target: every handler returns a serde struct shared with the
client. First step: convert the `kiln.rs` and `observe.rs` `require_param!`
handlers to `typed_params`, then move the 46 client-only structs to
`protocol/`.

**G98, G100.** Target: one `RpcContext`; `Server::run` spawns four named
functions. First step: delete `ServerContext` and pass `Arc<RpcContext>`.

**G101, G102.** Target: a `routes` table that derives from `METHODS`, and the
web imports `crucible_daemon::rpc_client` only. First step: write the test
that asserts every `session.set_*` method has a route; then move
`OptionAction` and `forbidden_root_reason` into `rpc_client` types.

**G104, G105, G107, G108.** Target: the ACP module exports `AcpAgentHandle`
only; one `BUILTIN_AGENTS`; one counter per client; one env read. First step:
move `humanize_tool_title` to `crucible-core/src/types/tool_ref.rs`.

**G109, G111, G112.** Target: `get_info` derives from `served_over_mcp()`;
one `impl From<CallToolResult> for Value`; one `From<ToolDefinition> for
rmcp::Tool`. First step: write the two `From` impls in `tools/helpers.rs`.

**G113, G114, G115.** Target: the client deserializes `SessionEventMessage`;
`NoteRecord` is the DTO; `StorageClient` is deleted. First step: delete
`rpc_client/client/types.rs:10 SessionEvent`.

**G116, G117.** Target: `DaemonSessionApi` with 32 required methods;
`SessionConfigRpc` with 22 required. First step: remove the defaults and let
the six test doubles fail to compile; give them one shared mock.

**G120.** Target: `Capability` is decoded by serde only. First step: delete
`parse_capability` and deserialize the spec-table list with `serde_json`.

**G121.** Target: no Rust list of mode names. First step: delete
`BUILTIN_MODE_NAMES` and `default_internal_modes`; read `session.list_modes`.

**G124.** Target: `AdaptiveColor`, `Color`, `Border`, `Padding` move to
`crucible-core/src/types/style.rs`; `cru.oil` is withdrawn (G123). First step:
move the style types and re-export them from `crucible-oil`.

**G125, G126.** Target: one colour codec in `theme_wire.rs`; `ConfigState` on
`LuaExecutor`. First step: make `hl_lua.rs` call `theme_wire`.

**G127.** Target: the stub list derives from the registration list. First
step: build both from one `const NAMES: &[&str]`.

**G128.** Target: `Skill` crosses `platform.rs` through serde; the name rule is
checked. First step: replace the hand copy at `platform.rs:66-180` with
`serde_json::to_value`.

**G129, G130.** Target: `cru.permissions.on_request` registered in both VMs
with ids from the handler registry. First step: call
`register_permission_hook_api` from `DaemonPluginLoader`.

**G131, G132, G133, G134, G135, G138.** Target: expose `cru.defaults.mode`;
one plugin path list; load a spec once; delete the dead channel path, the
duplicate tool shapes and the temp-dir write. First step: one commit per item.

**G139.** Target: `:set` parses locally and sends `session.set_config`; the
overlay engine shrinks to the TUI-local keys; the permission rule write becomes
an RPC. First step: move the `shell.rs:123` config write behind
`project.add_pattern`.

**G140, G141.** Target: `:export` calls `session.export_to_file`; the shell
modal keeps output in memory or asks the daemon. First step: delete the
in-process render at `actions.rs:784-827`.

**G143, G152.** Target: `ToolSource` crosses the wire as JSON; `McpServerInfo`
is one type. First step: delete `ToolSourceDisplay` and the `Mcp:x` formatter.

**G144.** Target: one `REPL_COMMANDS` table with name, aliases, help. First
step: build the four lists from it.

**G145, G146, G147, G148, G150.** Target: oil helpers live in oil only; one
status struct; theme stores on `OilChatApp`; typed `:set` defaults; one
markdown renderer. First step: delete `crucible-cli/src/tui/oil/utils/truncate.rs`.

**G154, G156.** Target: one `AppConfig` and one `ProviderConfig` struct. First
step: fix the swapped re-exports at `crucible-cli/src/config.rs:10-18`, then
delete the `execution_roots.rs` second parse.

**G159.** Target: `enum CredentialStore { Keyring, File, Env }`. First step:
replace the trait with the enum.

**G161, G162, G163.** Target: `cru init` writes no `sessions/`;
`TRACKED_FIELDS` derives from the struct; inert fields are deleted. First step:
delete the `init.rs:402` mkdir.

**G165, G166, G167, G168, G169.** Target: `test_support` behind `test-utils`;
no `set_var`; harnesses under `#[cfg(test)]`; closed-set tests on `EnumIter`;
a test double for each `dyn` trait or no `dyn`. First step: gate
`crucible-core/src/lib.rs:32` with `#[cfg(any(test, feature = "test-utils"))]`.

**G178, G179, G180.** Target: both handles yield `ToolResult`; one depth
prompt; one `expand_tilde` in `crucible-core/src/config/`. First step: yield
`TurnEvent::ToolResult` from `GenaiAgentHandle` after dispatch.

### 3.2 `expectation-wrong`

Five rows remain here after the re-check of 2026-08-22: G23, G80, G118, G155
and G158. In each the clean room had the inputs and picked a shape the code
does not need. No security invariant explains the difference.

**G23.** The clean room folded two contracts into one trait. The runtime needs
`Agent::turn -> Stream<TurnEvent>` so the stream loop owns the tool round. The
client needs `AgentHandle` so the TUI can drive a session over RPC without the
runtime. The two traits are not the same seam. Expected.md section 4.9 should
keep `Agent` as the runtime contract and shrink `AgentHandle` to the
client contract.

**G80.** D5 picked the ten `ScriptingEvent` names from the docs. The set is
defined by what the scripting vocabulary and the transport vocabulary share in
code. `segment_complete`, `mode_changed` and `title_changed` are transport-only.
Expected.md section 8.4 should copy the ten names from
`crucible-core/src/events/session_event/mod.rs:75`.

**G118.** D7 wanted a `RuntimePath(PathBuf)` variant so a `runtimepath` entry
carries its own provenance. The code resolves every `runtimepath` entry and
`$CRUCIBLE_RUNTIME` through one search list and stores the directory on
`Plugin.dir`. The path is the provenance. Three variants suffice.

**G155.** D8 fixed nine provider kinds from the product text. The code needs
`Custom` for an OpenAI-compatible endpoint with a user URL, `Mock` for tests
and `Burn` for a local backend. Expected.md section 3.18 should say "at least
nine" and name `Custom`.

**G158.** The clean room assumed every client calls the daemon. `cru init`
runs before any daemon exists, so it must detect providers in-process. The
constraint is "no daemon yet". The fix is one shared detection function in
`crucible-core`, not a daemon call.

### 3.2a `expectation-incomplete`

Three of the eight rows first filed as `expectation-wrong` moved here on
2026-08-22. In each the clean room lacked an input, not judgement: the product
documents carry no threat model, and Expected.md section 7a now states the
invariants. Two `not-built` rows also moved here because the code has a live
surface the product documents do not name (Expected.md section 2a).

**G8.** The clean room used `ToolSurface` for "which wire serves this tool".
The code needs "what may run in an isolated session": `Host`, `Daemon`,
`Unknown`, and the isolation gate refuses `Unknown` as it refuses `Host`.
That is invariant I3 (an unclassified tool surface is refused). MCP exposure
is a second axis; since T3-C6 it derives from `BuiltinTool` too
(`crates/crucible-daemon/src/tools/surface.rs:220`). Expected.md section 8.1
should name two predicates.

**G40.** The clean room made the permission hook, output validation and
session start and end into turn-loop stages. The code keeps the permission
hook in its own synchronous registry with a budget, and session start and end
as lifecycle hooks that can refuse a session before a turn exists. Invariant
I4 (gate order; `handled` returns before the permission gate) needs the
permission hook outside the stage list, and invariant I8 needs session start
as a refusal point. The six stages the clean room did not see (`PreLlmCall`,
`PostLlmCall`, `PrecognitionFormat`, `ToolBeforeExecute`, `ToolDisplayStart`,
`ToolDisplayComplete`) are plain omissions. Expected.md section 8.2 should
list the eleven real stages.

**G81.** D4 added `session_created` and `session_ended` as broadcast events.
The code has `FileDeleted` and `FileMoved` instead, because a plugin that
keeps an index must see a delete (invariant I8). Session start and end are
lifecycle hooks (G40). Expected.md section 8.3 should list the eight real
names.

**G122.** `cru.log.notify`, `cru.log.notify_once` and `cru.log.messages.*`
are registered on every VM (`crates/crucible-lua/src/executor.rs:260`). The
product documents name toasts (F134) but not the Lua call, so the clean room
had no row for it. The sink is still missing: the queue reaches no client
(`crates/crucible-lua/src/notify.rs:78`). The code-side fix stays open.

**G110.** Closed; see section 6.

### 3.3 `both-acceptable`

G3, G6, G27, G37, G49, G51, G56, G75, G89, G95, G106, G142, G151 and G177 are
legitimate alternatives. G3 moved here on 2026-08-22 (section 3.1). The table says why in each row. Two need a note.
G95: the RPC spellings differ from the product docs in about fifteen names;
the code's names are the contract clients use today, so the docs should move.
G142: `Core` and `Crucible` split what the clean room called `Builtin`; the
split costs nothing and lets a badge rule change later.

### 3.4 `not-built`

G26, G41, G54, G57, G119, G123, G136, G137, G149, G157, G160, G164,
G170 to G176. Seven of these are marked *(planned)* in Expected.md already
(G26, G54, G57, G170 to G175). Four are documented as shipped and have no
working code: G119 (`spec.handlers`), G136 and G137 (web plugin surfaces),
G164 (storage maintenance). Those four are the ones to fix or to remove from
the docs. G110 closed when T3-A10 and T3-A11 wired the gateway; G122 moved to
`expectation-incomplete` (section 3.2a). G123 stays here: `cru.oil` is
registered (`crates/crucible-lua/src/oil.rs:191`) and `Product.md` lists it,
but no client renders a `LuaNode`, and the product entry itself asks whether
to wire it or withdraw it.

## 4. Patterns

1. **One concept, N types.** The clean room expected one type per concept. The
   code has two `PermissionScope`, two `SessionId`, two `ToolCall`, three
   `SearchResult`, three status snapshots, two hash newtypes, two `McpServerInfo`,
   six event enums for one stream, nine truncate helpers, eight frontmatter
   splitters, seven tilde expanders. The copies arise at crate seams (core to
   daemon to CLI) and at wire seams (daemon to client, daemon to web). Rows:
   G4, G43, G44, G47, G52, G53, G55, G77, G83, G113, G125, G143, G145, G146,
   G150, G152, G156, G180.
2. **Closed sets with a side list.** Every enumerated table the clean room
   asked for exists. Beside each one the code keeps a hand list that re-spells a
   subset: six beside `BuiltinTool`, four REPL lists, two ACP tables, two
   `cru.session` lists, three Rust copies of the mode names, a 70-name
   `Group::of`. Rows: G7, G9, G10, G11, G78, G105, G121, G127, G144.
3. **Traits where the clean room expected enums, and defaults where it expected
   required methods.** `AgentHandle` 41 defaulted, `SessionConfigRpc` 22,
   `DaemonSessionApi` 15, `NoteStore` 5. `dyn` with one impl and no double:
   `PermissionGate`, `MarkdownParser`, `FileWatcher`, `WatcherFactory`,
   `Undoable`. Eight `SyntaxExtension` impls in one crate. Rows: G2, G22, G58,
   G62, G66, G69, G70, G88, G116, G117, G159, G169.
4. **Flat records where the clean room expected sums.** `SessionAgent` with
   `agent_type: String`, `WorkspaceSnapshot` with three shapes in four fields,
   `ThinkingBudget` as `Option<i64>`, `LinkResolution` as an `Option` plus a
   flag, `JobResult` with `output: None`. Rows: G20, G21, G25, G35, G56, G86.
5. **Wire types shared where the clean room expected privacy.** ACP helpers
   reach the CLI; `rmcp` conversions exist five times; the web imports daemon
   server modules; the CLI reads the session directory and builds a
   `KilnRegistry`. Rows: G12, G31, G102, G104, G111.
6. **Business logic below the daemon.** The CLI holds the `:set` engine, the
   task graph, the kiln walkers, provider detection, kiln validation, export
   rendering and a permission-rule writer. The web writes kiln files and walks
   the kiln. Rows: G17, G63, G71, G72, G139, G140, G141.
7. **Environment reads instead of injection.** `execution_roots`,
   `plugin_boot`, `platform.rs`, `Recorder`, `EMBEDDING_PROVIDER_CACHE`, Lua
   `CONFIG`, `NO_COLOR`, `main.rs set_var`. Rows: G16, G59, G99, G108, G126,
   G147, G166.
8. **Machinery with no consumer.** 40 scripting event variants, `EventRing`,
   `events/markdown/`, two `serde_md`, `hashing/`, `processing/`,
   `model_discovery.rs`, two watcher stubs, `cru.oil`, `cru.log.notify`,
   `spec.handlers`, the MCP gateway half. Rows: G29, G53, G66, G73, G74, G87,
   G110, G119, G122, G123, G153. Re-checked 2026-08-22 against Expected.md
   section 2a: G110 and G122 were real features with a live surface, not
   machinery; the rest stand. Section 6 lists which rows the consolidation
   closed.
9. **The clean room under-counted the live sets.** `StageId`, `EventName`,
   `ScriptingEvent`, `BackendType` and `ToolSurface` all differ from the docs.
   In each case the code's set answers a question the docs did not ask (isolation,
   file deletes, custom endpoints). Rows: G8, G40, G80, G81, G155. Where the
   question is a security invariant (G8, G40, G81) the verdict is now
   `expectation-incomplete`; Expected.md section 7a states the invariant.

## 5. Agreements worth keeping

These points match between Expected.md and Actual.md. A future change must not
break them.

- **`SessionEventMessage` is the only type the four wire bindings share.** No
  codec, framing, correlation id or error class is shared
  (`crucible-core/src/protocol/rpc/mod.rs:85`; Expected 5 rule 1, 6.5).
- **The parser is an island.** `ParsedNote` carries raw text and byte offsets,
  no resolution. Link resolution lives in `storage/sqlite/link_index.rs`
  (Expected 3.4, 3.5, 4.1).
- **`BuiltinTool` is the exemplar closed set**: exhaustive match, two
  module-level `#![deny]`, no `Default`, a test that derives its expectation
  from the running system (`tools/surface.rs:46-60`; Expected 8).
- **`RpcMethod` and `METHODS` come from one `rpc_methods!` table** with a
  contract test (`rpc/dispatch.rs:54,79`; Expected 8.5, 9.17).
- **`StageId` and `EventName` are two types with two contracts.** `cru.on`
  validates against the enum through `HookName` (`hook_name.rs:193`; Expected
  3.21, 9.3). A stage result decides the next step; an event reads only `cancel`.
- **Modes live in Lua only.** `BUILTIN_INIT_LUA` is the one definition of the
  three modes, the deny hook, the default prompt and the precognition
  formatter; `ModeRegistry` has no Rust fallback (`crucible-lua/src/lib.rs:164`;
  Expected 8.6).
- **`handled` returns before the permission gate, and gate order is the
  protection.** The order is plan bar, active set, card policy, review gate,
  `pre_tool_call`, isolation gate, permission gate, dispatch
  (`messaging/tool_call.rs`; Expected 4.11, 9.5). G15 asks for a test, not a
  change of order.
- **Project and kiln have separate registries and separate config files**
  (`project_manager.rs`, `kiln_registry.rs`; Expected 3.2, D23). A session
  attaches a flat kiln set with no primary (`session/types/session.rs:31`).
- **Transcripts live under the daemon data root, never in a kiln**
  (`session_storage.rs:92`; Expected 3.8, 4.8). G161 removes the one stale
  mkdir.
- **Containment proves a path before a tool touches it.** `ContainedPath` and
  `WritablePath` are the proof types; `FsScope` is the door
  (`tools/fs_scope.rs:92,130,162`; Expected 4.4).
- **Seven interaction kinds with one arm per kind in each client**, gated by
  `KINDS` and `interaction-coverage.test.ts` (`interaction/types.rs:380`;
  Expected 8.8). `Popup` carries items (D12).
- **Notifications push as events and store on the session** (`lifecycle.rs:156`;
  Expected D14, D22). `NotificationKind` is `Toast`, `Progress`, `Warning`
  (`types/notification.rs:90`; Expected 8.14).
- **Per-uid 0700 socket** from `$CRUCIBLE_SOCKET`, `$XDG_RUNTIME_DIR`, else a
  tmpdir (`crucible-core/src/protocol/lifecycle.rs:77`; Expected 6.1).
- **ACP wire types come from `agent_client_protocol`; MCP types from `rmcp`.**
  Neither is vendored. The in-process MCP host binds `127.0.0.1:0` and serves
  `/mcp` (`mcp_host.rs:63`; Expected 4.19, 4.21).
- **Oil depends on no workspace crate** (Actual 4). This is stronger than the
  clean room asked for. Keep it.
- **The web is behind a default-on `web` feature** and the CLI imports it only
  there (Actual 4; Expected 7).
- **Canvas round trip keeps unknown keys** through `extra` maps
  (`crucible-core/src/canvas/mod.rs:38`; Expected 3.6).
- **Rename splices inbound links and reports skips** (`server/note_refactor.rs:48,35`;
  Expected 4.2.3).
- **Skill scopes are four** and the bundled scope sits below every user scope
  (`skills/types.rs:14`; Expected 8.17).
- **Luau is the runtime and `require` is the host's** (`crucible-lua/src/modules.rs`,
  `luau_compat.rs`; Fennel is removed). Expected open question 27 is now about
  typed plugins, which `cru plugin check` answers in part.
- **Delegation limits** (depth, allowlist, concurrency, timeout) and the model
  chain card, `[llm.models]`, parent are in place (`delegation.rs:93`;
  Expected 4.14).
- **Precognition runs on the daemon and sends to the provider, not to a
  client**, with `PrecognitionSelect` and `PrecognitionFormat` as seams
  (`precognition/mod.rs:556`; Expected 4.15).
- **`WorkspaceSnapshot` gives turn-level undo** (`workspace_snapshot.rs:123`;
  Expected 3.11). G35 changes the shape, not the seam.

## 6. Status after consolidation, 2026-08-22

Tier 1, Tier 2 and Tier 3 of [[Consolidation Plan]] landed in `b31aa0b00`
to `7fcd3b9f4` (`git log --oneline 4cc9cf2af..HEAD`, 88 commits). This
section matches the commit subjects, which carry the plan id, to the gap rows.
"Closed" means the difference the row names no longer exists. "Part" means the
commit removed some of it; the row stays open for the rest. Rows this section
does not name are unchanged.

| Row | Plan entry | Commit | Status |
|---|---|---|---|
| G2 | T3-B4 | `0e194bb53` | part: `PermissionGate` is a concrete type; the prompt still runs inside `messaging/permission.rs` and `PermissionDecision` has no layer |
| G4 | T3-B13 | `28be1a511` | closed: `TryFrom` between the two scopes replaces the hand map |
| G7 | T3-C6 | `11ca718e2` | part: `KILN_BACKED_TOOLS` and the CLI `BUILTIN_TOOLS` are deleted (`BuiltinTool::needs_kiln`, `BuiltinTool::ALL`); `DISCOVERY_TOOL_NAMES`, `PLAN_TOOL_NAMES` and `is_write_tool_name` stay |
| G9 | T3-C6 | `11ca718e2` | closed |
| G21 | T3-B11 | `c8394afe8` | part: the ACP `SessionConfig` is gone; the knobs stay on `SessionAgent` |
| G22 | T3-A1 | `0fbef4943` | closed: `AgentHandle` plus `SessionKnobs`, all required |
| G35 | T3-C14 | `1cdddfd63` | part: one `run_git`; the snapshot shape is unchanged |
| G36 | T3-B4 | `0e194bb53` | part: `Undoable` is gone; the client constants remain |
| G44 | T3-C5 | `b4e2fcf37` | closed for the daemon; the CLI ACP literals at `crucible-cli/src/factories/agent.rs` stay |
| G48 | T3-B8 | `d429a886d` | part: the non-callback path is deleted; `StreamingChunk` still translates to `TurnEvent` in a stateful loop |
| G53 | T1-B17, T3-B6, T3-B14, T5-02 | `6c6e8608a`, `1d5461468`, `e67ec3e7e`, `757f73ed1` | closed: `hashing/`, `ContentHasher` and `types/hashing.rs` deleted; `BlockHash` is the one hash newtype |
| G62 | T3-A4, T3-B5 | `44da8714e`, `cdbb6b440` | closed: the `NoteStore`, `KnowledgeRepository`, `EmbeddingProvider`, `EventHandler`, `StorageClient` and `EventEmitter` methods are required |
| G64 | T3-A12 | `76b944d94` | closed: the `note_store` branch is deleted |
| G66 | T3-B3, T3-C23, T3-C24 | `fbe49077c`, `c9973d969`, `ef3f26663` | part: one `Backend` enum with a capability table; `DebounceConfig` reaches `Debouncer`; the polling and editor backends are still stubs |
| G69 | T3-B1 | `61e7a1d67` | closed: `Extension` is one enum |
| G70 | T3-B4 | `0e194bb53` | closed |
| G73 | T1-B17 | `6c6e8608a` | closed: `processing/` and `change_detection.rs` deleted |
| G77 | T3-B7 | `2b4a0c71f` | part: 42 dead scripting variants and 5 dead `LogEvent` variants deleted; six enums remain |
| G87 | T1-B18, T3-B17 | `c8bdacacc`, `5638b2be1` | part: `events/markdown/` and both `serde_md` serializers deleted; `EventRing` stays |
| G88 | T3-B5 | `cdbb6b440` | closed for the trait; the `EmitOutcome.cancelled` dead branch stays |
| G98 | T3-B21 | `37e4a8b8d` | closed |
| G110 | T3-A10, T3-A11 | `358dd41d2`, `33703918c` | closed |
| G113 | T3-B9 | `6a8080880` | closed: the client reads `SessionEventMessage` |
| G114 | T3-B12 | `bb5b39591` | part: `FtsResult` is the one text-search shape; the DTO offsets stay |
| G116 | T3-A3 | `81eb69ca3` | closed |
| G117 | T3-A2 | `1d60a80ed` | closed |
| G125 | T3-C4 | `9434cc15d` | part: one set of colour parsers; `BorderStyle` maps onto oil; the unread theme fields stay |
| G127 | T3-B24 | `80d003052` | part: `cru.session` from one list with a set-equality test; `cru.kiln` still twice |
| G132 | T3-C8 | `f36093d2b` | closed: `crucible_core::paths` |
| G134 | T3-A5 | `ca9473cf4` | closed |
| G144 | T3-C20 | `bcf424dd3` | closed: one `ReplCommand` table |
| G145 | T1-B24, T3-C1 | `62839af8b`, `c07a2b7fc` | part: `crucible_core::text` holds the truncate helpers; oil keeps its own |
| G148 | T1-B20, T3-C21 | `2a2f9e4cd`, `8b259b03e` | part: `parse_bool` no longer panics; the overlay defaults stay strings |
| G152 | T3-C13 | `7fbc9f481` | part: `From` impls; the three types stay |
| G153 | T2-B3, T3-C22 | `5fb48681d`, `2d01c51d3` | part: dead strategies and `ComputedLayout` deleted; `template/node_spec.rs` stays |
| G156 | T3-C3, T3-B25 | `420472f32`, `9fb6b8849` | part: one `BackendType` table for defaults and one Ollama tags shape; `ChatConfig` and `LlmProviderConfig` still repeat knobs |
| G159 | T3-B2 | `782d6f664` | closed: the trait and the keyring store are gone |
| G163 | T3-B18, T3-B19 | `728d2641f`, `2385cee7a` | part: `DiscoveryConfig`, `ResolveMode`, `pool_size` deleted; the enrichment pipeline fields stay |
| G165 | T3-A6, T3-A7 | `0ce9742d4`, `bed420580` | closed: the helpers sit behind `test-utils` |
| G169 | T3-B3, T3-B4 | `fbe49077c`, `0e194bb53` | closed: none of the five is a trait now |
| G180 | T3-C2 | `3011f8e02` | closed: one `expand_tilde` in `crucible_core::config` |

Not touched by design: C11, C17, C19 and C29 were deferred, so G125's
`ThemeLayout` twin, the web policy rows (G17) and the `PermissionHook` versus
`RuntimeHandler` pair are unchanged. The follow-ups the Tier 3 agents noted
are in [[Consolidation Plan]] section "Tier 5".

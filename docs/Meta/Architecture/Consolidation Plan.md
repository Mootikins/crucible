---
title: Consolidation Plan
description: Ranked dead-code and duplicate items in four tiers, with the gate for each batch.
tags: [meta, architecture, plan]
status: as-of-7053bcfe7
---

# Consolidation Plan

This plan turns the audit in [[Actual]] into work batches. Each batch compiles as one unit. Each item has a file, a line and one action.

## 1. Method

The audit produced two lists: dead code and duplicates. A skeptic pass checked most items. This plan sorts the checked items into four tiers.

**Definition.** An item is SAFE MECHANICAL when all three hold:

1. The compiler or a zero-caller grep proves the code unused, or two types are field-for-field identical.
2. No public wire type changes (RPC, ACP, MCP, web JSON).
3. No behaviour change that a test would need to cover.

An item that fails one condition needs a decision. An item that nobody checked needs a second skeptic pass.

Line numbers are as of `7053bcfe7`. Open the file before you edit it; the line may move.

| Tier | What | Batches | Items |
|---|---|---|---|
| 1 | Safe mechanical, this session | 24 | 254 rows: the 256 skeptic-confirmed dead items (deduped) plus the 52 exact and 29 near duplicates; 4 rows excluded for protected paths |
| 2 | Mechanically dead, no skeptic | 3 | 23 items in 20 rows (the other 94 of the 117 are in Tier 1 or Tier 3) |
| 3 | Needs a decision | — | 66 entries: the 25 dead items, the 8 exact duplicates, the 98 near duplicates grouped into families, 7 trait groups, the section 7 type families |
| 4 | Unverified | — | 70 checklist items (unverified identifiers not ruled on elsewhere, plus the weak items); 79 + 54 refuted claims listed so nobody re-raises them |

The gate for every Tier 1 and Tier 2 batch is the same:

1. Run `cargo check --workspace --all-targets`.
2. Run `just test quick`.
3. Commit the batch alone.

Rules that apply to every batch:

- Delete the tests that only exercise the deleted item. Do not keep a test that tests nothing.
- When you delete a `pub use`, delete the item it re-exported in the same batch.
- When a batch removes the last reader of a field, remove the field in the same batch.
- Do not touch `protocol/`, `rpc/dispatch.rs` or `crucible-web/src/routes/`. Items in those files are listed at the end of Tier 1.

## 1a. Result, 2026-08-22

Tier 1 and Tier 2 ran the same day, one commit per batch, `b31aa0b00` to
`5fb48681d`. 266 items were removed or merged: 248 in Tier 1, 18 in Tier 2.
Net change: about 280 files, −14,300 lines. `just test quick` passed after
every batch (8492 tests before, 8210 after; the difference is tests of removed
items). `just ci` passed at the end, after `1f7a555ae` fixed three clippy
findings the removals exposed.

Skipped items, with the reason, now live in Tier 3 or Tier 4:

| Batch | Item | Why | Now |
|---|---|---|---|
| T1-B3 | `with_debounce` calls at `external_changes.rs`, `kiln_manager.rs` | Those are `WatchConfig::with_debounce`, a live method; the plan conflated two methods | dropped |
| T1-B5 | `impl Default` for `ClientId`, `SubscriptionManager` | clippy `new_without_default`; `ClientId::new` draws from a counter, so a derived Default changes behaviour | Tier 3 |
| T1-B7 | `KILN_BACKED_TOOLS` | deferred by the plan | Tier 3 |
| T1-B11 | `SessionManager::remove_session` | live test callers in two test modules | Tier 4 |
| T1-B11 | `KilnRegistry::iter` | live caller `server/session/list.rs:109`; the grep missed it | dropped |
| T1-B13 | `Component::Normal` loop in `core/canvas/containment.rs` | different crate from the daemon helper | Tier 3 |
| T1-B18 | `ModelCapability`, `UnifiedModelInfo`, `McpTransportConfig` | carry `serde` attributes; rule 2 | Tier 3 |
| T1-B22 | `kiln_validate::is_temp_directory` | not a duplicate; `starts_with` flags subdirectories, and a test depends on it | dropped |
| T1-B23 | `parse_capability` → serde | behaviour differs (case fold, nine names) | Tier 3 |
| T2-B1 | `resolve_path` parameter | B22 removed the function | done |
| T2-B3 | `InputArea::with_popup` | B24 removed it | done |

Two follow-ons that the batches applied under the Method rules: T1-B2 removed
`PerformanceStats`, `QueueStats` and their readers once `get_status` went; T1-B5
removed three `Server` fields that lost their last reader with `ServerContext`.

## 1b. Tier 3 result, 2026-08-22

Tier 3 ran after Tier 2, one commit per entry, `2a07d01e4` to `7fcd3b9f4`.
57 of the 66 entries landed, each commit subject carrying its plan id
(`git log --oneline 2a07d01e4^..HEAD | grep 'plan T3-'`). Net change under
`crates/`: 342 files, +6,695 and −13,714 lines. `just ci` passed at the end.
[[Gaps]] section 6 maps the commits to the gap rows they closed.

The nine entries with no commit:

| Entry | Why |
|---|---|
| B23 | The plan said keep; `InputNode` versus `LayoutContent::Input` is documented design |
| C11 | Deferred by design: each TUI pair changes visible output and needs a snapshot review |
| C15, C18 | Covered by T3-B7 |
| C17 | Deferred by design: `PermissionHook` and `RuntimeHandler` differ in first-match semantics |
| C19 | Deferred by design: under `crucible-web/src/routes/` or on the SSE wire; a web session owns it |
| C25 | Covered by T3-B17 |
| C26 | Covered by T1-B4 |
| C29 | Deferred: the `acp/mod.rs` re-exports are a Tier 4 check first |

Entries whose outcome differs from the recommendation: B2 replaced the trait
with a concrete `SecretsFile` store and deleted the keyring store, instead of
an enum; B4 replaced `PermissionGate`, `Undoable`, `MarkdownParser`, `App`
and `InputStyle` with concrete types and left `FrameRenderer`; B7 took option
(a) and left (b) open; B18 kept `idle_timeout_secs` and
`lazy_agent_selection` as reserved; C9 produced [[Bash Permission Layers]]
and no merge. The agents recorded 160 follow-up notes; section 5a holds them
after deduplication.

## 2. Tier 1 — safe mechanical, this session

Actions: `delete` removes the item. `narrow` changes visibility. `merge-into X` keeps X and deletes the other copy. `call X` replaces an inline body with a call to X.

The batch labels are `T1-B1` to `T1-B24`. The commit subjects from 2026-08-22 carry the older `plan Bn` form; Band B in section 4 uses `B1` to `B25` for different entries.

### T1-B1 — crucible-daemon, `watch/` builders

Files: `crates/crucible-daemon/src/watch/traits.rs`, `watch/manager.rs`, `watch/backends/factory.rs`, `crates/crucible-daemon/tests/watch_notify_filter_tests.rs`.

| Item | Location | Action |
|---|---|---|
| `HandlerConfig::with_buffer_size` | `watch/traits.rs:215` | delete |
| `HandlerConfig::with_max_concurrent` | `watch/traits.rs:221` | delete |
| `HandlerConfig::with_order_preservation` | `watch/traits.rs:227` | delete |
| `HandlerConfig::with_timeout`, `HandlerConfig`, `WatchMode` | `watch/traits.rs:247` | delete; edit the struct literal at `watch_notify_filter_tests.rs:70-71` |
| `DebounceConfig::with_deduplication` | `watch/traits.rs:175` | delete |
| `WatchConfig::with_handler_config` | `watch/traits.rs:121` | delete |
| `WatchConfig::with_backend_option` | `watch/traits.rs:133` | delete; keep the `backend_options` field (`editor_backend.rs:192` reads it) |
| `default_low_frequency_interval` | `watch/traits.rs:266` | delete |
| `WatchManagerConfig::with_queue_capacity` | `watch/manager.rs:58` | delete |
| `WatchManagerConfig::with_debounce_delay` | `watch/manager.rs:64` | delete |
| `WatchManagerConfig::with_monitoring` | `watch/manager.rs:70` | delete |
| `WatchManagerConfig::with_default_handlers` | `watch/manager.rs:76` | delete |
| `WatcherRequirements::with_max_latency` | `watch/backends/factory.rs:284` | delete |
| `WatcherRequirements::with_resource_priority` | `watch/backends/factory.rs:290` | delete |

### T1-B2 — crucible-daemon, `watch/` manager, monitor, factory

Files: `watch/manager.rs`, `watch/utils/monitor.rs`, `watch/backends/factory.rs`, `watch/backends/mod.rs`, `watch/mod.rs`.

| Item | Location | Action |
|---|---|---|
| `WatchManager::remove_watch` | `manager.rs:361` | delete |
| `WatchManager::unregister_handler` | `manager.rs:397` | delete the wrapper only; check `handlers.unregister` separately |
| `WatchManager::emitter` | `manager.rs:155` | delete |
| `WatchManager::get_performance_stats` | `manager.rs:407` | delete |
| `WatchManager::get_status`, `ManagerStatus` | `manager.rs:413`, `:419`, `:602` | delete both |
| `WatchManagerConfig.max_concurrent_handlers`, `.enable_monitoring` | `manager.rs:39-41` | delete the fields and their `Default` lines |
| `PerformanceStats::is_good_performance` | `utils/monitor.rs:165` | delete |
| `PerformanceStats::performance_score` | `utils/monitor.rs:173` | delete |
| `WatcherRequirements::low_frequency` | `factory.rs:245` | delete; keep `WatcherUseCase::LowFrequency` |
| `WatcherRequirements::editor_integration` | `factory.rs:258` | delete |
| `WatcherRequirements::compatibility` | `factory.rs:271` | delete |
| `ResourcePriority` and the `resource_priority` field | `factory.rs:187`, `:205` | delete with its re-export |
| `ExtendedBackendRegistry::inner` | `factory.rs:24` | delete |
| `default_backend` | `backends/mod.rs:111` | delete |
| `watch::prelude` | `watch/mod.rs:86-92` | delete the module |

### T1-B3 — crucible-daemon, `watch/` backends, handlers, events

Files: `watch/backends/notify_backend.rs`, `watch/backends/polling_backend.rs`, `watch/backends/editor_backend.rs`, `watch/handlers/indexing.rs`, `watch/handlers/composite.rs`, `watch/handlers/mod.rs`, `watch/events.rs`, `watch/mod.rs`, `watch/external_changes.rs`, `kiln_manager.rs`.

| Item | Location | Action |
|---|---|---|
| `NotifyWatcher::update_debounce_config` | `notify_backend.rs:158` | delete |
| `PollingWatcher::update_interval` | `polling_backend.rs:198` | delete |
| `EditorWatcher::update_editor_config` | `editor_backend.rs:152` | delete with its unit test at `:412` |
| `EditorWatcher::with_default_config` | `editor_backend.rs` | inline into `new()` |
| `IndexingHandler::initialize_database` | `handlers/indexing.rs:54` | delete |
| `IndexingHandler::new` | `handlers/indexing.rs:27` | delete; drop the `NoOpEmitter` import at `:16` |
| `IndexingHandler::set_emitter` | `handlers/indexing.rs:39` | delete |
| `IndexingHandler::emitter`, `with_debounce`, `index_debounce` field | `handlers/indexing.rs:22`, `:34`, `:49` | delete; remove the `with_debounce` calls at `external_changes.rs:504` and in `kiln_manager.rs` |
| `CompositeHandler`, `CoordinationStrategy`, `HandlerState` | `handlers/composite.rs` | delete the module, the `pub use` at `handlers/mod.rs:7` and `watch/mod.rs:67` |
| `FileEventKind::affects_content`, `is_removal`; `FileEvent::file_name` | `events.rs:93` | delete with their tests at `events.rs:312` |
| `EventFilter::exclude_extension`, `include_dir`, `with_size_limits`, `with_custom_filter` | `events.rs` | delete |

### T1-B4 — crucible-daemon, `storage/sqlite/`

Files: `storage/sqlite/connection.rs`, `config.rs`, `fts.rs`, `repository.rs`, `adapters.rs`, `mod.rs`, `property_store.rs`, `schema/tests.rs`, `kiln_manager/tests/mod.rs`.

| Item | Location | Action |
|---|---|---|
| `SqlitePool::with_connection_mut` | `connection.rs:75` | delete |
| `SqlitePool::stats`, `DbStats` | `connection.rs:107`, `:121` | delete with the test at `:228` |
| `SqliteConfig::with_pool_size`, `without_wal`, `with_cache_size` | `config.rs:82`, `:88`, `:94` | delete with the test at `:122-124` |
| `FtsIndex::search_boosted` | `fts.rs:339` | delete with the test at `:515` |
| `FtsIndex::is_empty` | `fts.rs:255` | merge-into `count()`; change `kiln_manager/tests/mod.rs:915`, `:962` to `count().await == 0` |
| `create_knowledge_repository` | `repository.rs:201`, `mod.rs:56` | delete; point the test at `:442` at `SqliteKnowledgeRepository::new` |
| `create_knowledge_repository_with_kiln` | `repository.rs:205`, `mod.rs:56` | delete |
| `SqliteClientHandle::kiln_path` getter | `adapters.rs:55` | delete; keep the field and `with_kiln_path` |
| `SqliteResult` alias | `mod.rs:51` | delete |
| `SqlitePropertyStore` | `property_store.rs:223`, `mod.rs:54` | merge-into `impl PropertyStore for SqliteNoteStore` (`:235-280`); rewrite `schema/tests.rs:150`, `:165` against `SqliteNoteStore` |
| `repository.rs` Scope match x3 | `repository.rs:71`, `:123`, `:169` | call one private `fn scope_for(kiln_path: Option<&Path>) -> Scope` |

### T1-B5 — crucible-daemon, `server/` and `subscription.rs`

Files: `server/bind.rs`, `server/mod.rs`, `server/core/mod.rs`, `server/lua_plugin_suite.rs`, `server/plugins.rs`, `server/session/review/mod.rs`, `server/fs/mod.rs`, `server/session/mod.rs`, `rpc_helpers.rs`, `subscription.rs`, `crates/crucible-cli/src/main.rs`, `crates/crucible-cli/src/commands/daemon.rs`, `daemon_plugins/mod.rs`, `tests/isolation_param.rs`.

| Item | Location | Action |
|---|---|---|
| `Server::bind(path, mcp_config)` | `bind.rs:75` | delete; change the doc comments at `daemon_plugins/mod.rs:299` and `tests/isolation_param.rs:170` to name `bind_with_plugin_config` |
| `Server.web_config`, `web_cancel` | `server/mod.rs:127`, `:425` | delete |
| `BindWithPluginConfigParams.web_config` | `bind.rs:27`, `:63` | delete; remove the `None` at `main.rs:167` and `daemon.rs:134` |
| `Server.background_manager` | `server/mod.rs:106`, `:307`, `:407` | delete the field; `AgentManager` holds its own `Arc` |
| `ServerContext` dead fields (`kiln_manager`, `session_manager`, `agent_manager`, `project_manager`, `lua_sessions`, `plugin_loader`, `llm_config`, `mcp_server_manager`) | `server/mod.rs:950` | delete the eight fields and the `#[allow(dead_code)]`; keep `authorized_uid`, `subscription_manager`, `dispatcher`, `shutdown` |
| `collect_plugin_test_files` | `lua_plugin_suite.rs:309` | narrow to private |
| `find_owning_plugin` | `plugins.rs:580` | narrow to private |
| `reject_hunk` | `review/mod.rs:171` | narrow to private |
| `DirListing` | `fs/mod.rs:88`, `:141` | narrow to private with its returning fn |
| `#[allow(unused_imports)]` re-export | `session/mod.rs:23` | replace with `#[cfg(test)]` |
| `require_session_id!` | `rpc_helpers.rs:72`, `:133`, `:161` | delete the macro, the `pub use` and the doc line; update `scripts/probe-dispatch-preambles.py:33` |
| `impl Default for ClientId`, `SubscriptionManager` | `subscription.rs:37`, `:207` | delete; if clippy `new_without_default` fires, derive `Default` and delete `new()` instead |
| `sweep_review_refs` pass-through | `server/core/mod.rs:556` | delete; `server/mod.rs:691` calls `crate::review::sweep_review_refs` directly |

### T1-B6 — crucible-daemon, `rpc_client/`

Files: `rpc_client/client/agent.rs`, `storage.rs`, `lua.rs`, `session.rs`, `workflow.rs`, `rpc_client/mod.rs`, `rpc/workflow_handlers.rs`.

| Item | Location | Action |
|---|---|---|
| `DaemonClient::session_can_undo` | `agent.rs:771` | delete; the daemon handler stays |
| `DaemonClient::session_undo_depth` | `agent.rs:778` | delete |
| `DaemonClient::session_cache_stats` | `agent.rs:412` | delete; the RPC stays alive for Lua |
| `DaemonClient::kiln_set_classification` | `storage.rs:41` | delete |
| `DaemonClient::note_move` | `storage.rs:703` | delete; the RPC stays alive for web |
| `DaemonClient::lua_register_commands`, `LuaRegisterCommandsResponse` | `lua.rs:190`, `:134`, `rpc_client/mod.rs:28` | delete both and the two `pub use` entries |
| `WorkflowSessionRequest` | `workflow.rs:29`, `rpc_client/mod.rs:20` | merge-into `SessionIdRequest` (`session.rs:219`) |
| local `Params { session_id }` | `rpc/workflow_handlers.rs:164-167` | merge-into `SessionIdRequest` |
| `extract_yaml_frontmatter` | `rpc/workflow_handlers.rs:583` and `crucible-cli/src/commands/workflow.rs:365` | merge-into one `pub fn` in `crucible-core/src/parser/types/workflow.rs` next to `body_start_offset`; keep the body as is, do not swap it for `extract_frontmatter` (that is Tier 3) |

### T1-B7 — crucible-daemon, `tools/` and `mcp/`

Files: `tools/mcp_client.rs`, `tools/mcp_gateway.rs`, `tools/extended_mcp_server.rs`, `tools/mcp_server.rs`, `mcp/config.rs`, `agent_factory.rs`, `acp_handle.rs`, `tools/mcp_server/tests.rs`, `tests/acp_integration*`.

| Item | Location | Action |
|---|---|---|
| `RmcpExecutor::get_tool` | `mcp_client.rs:93` | delete |
| `RmcpExecutor::server_info` and the `server_info` field | `mcp_client.rs:83`, `:48`, `:61`, `:76` | delete the getter, the field and the `convert_server_info` call |
| `RmcpExecutor::refresh_tools` | `mcp_client.rs:98` | delete |
| `ExtendedMcpService::refresh_tools` | `extended_mcp_server.rs:473` | delete |
| `ExtendedMcpServer::kiln_server` getter | `extended_mcp_server.rs:120` | delete; keep the field |
| `ExtendedMcpServer::emit_event` identity stub | `extended_mcp_server.rs:254-256` | inline `(event, false)` at the six callers; delete the `if cancelled` branches and the `effective_args` match |
| `McpGatewayManager::remove_upstream` | `mcp_gateway.rs:337` | delete |
| `McpGatewayManager::upstream_names` | `mcp_gateway.rs:420` | delete |
| `read_mcp_servers` | `mcp/config.rs:71`, `:16`, `:97-148` | delete with its five tests and the doc line |
| `DelegationContext.depth`, `.data_classification` | `mcp_server.rs:77`, `:82` | delete the fields; edit the writers at `agent_factory.rs:123`, `:128`, `acp_handle.rs:173`, `:178`, `mcp_server/tests.rs:128` and the three integration tests |
| `KILN_BACKED_TOOLS` names | `mcp_server.rs:126` | no change in this batch; see Tier 3 |

### T1-B8 — crucible-daemon, `acp/`

Files: `acp/mock_agent.rs`, `acp/client/mod.rs`, `acp/tracing_utils.rs`, `acp/discovery.rs`, `acp/mod.rs`, `acp_launch.rs`, `acp/tools.rs`.

| Item | Location | Action |
|---|---|---|
| `MockAgent::add_response` | `mock_agent.rs:70` | delete |
| `CrucibleAcpClient::with_recorder` | `client/mod.rs:168` | delete; `Recorder::from_env` at `:141` stays |
| `TraceContext`, `LogCapture`, `CapturedLog`, `trace_span!`, `trace_event!` | `tracing_utils.rs` | delete the file and the two `pub use` lines in `acp/mod.rs` |
| `get_agent_help` | `discovery.rs:297`, `:39`, `acp/mod.rs:29` | delete; fix the doc comment at `:39` |
| `known` agent table | `acp_launch.rs:126-132` | call `BUILTIN_AGENTS` (`discovery.rs:52`) |
| `ToolDescriptor` | `acp/tools.rs:29` | merge-into `crucible_core::ToolDefinition`; map `category: String` to `Some(category)` and `input_schema` to `parameters`; keep `ToolRegistry` and `discover_tools` (integration tests use them) |

### T1-B9 — crucible-daemon, `llm/` and `provider/`

Files: `llm/embeddings/mock.rs`, `llm/embeddings/provider.rs`, `llm/embeddings/error.rs`, `llm/embeddings/mod.rs`, `llm/mod.rs`, `provider/copilot.rs`, `provider/model_listing.rs`.

| Item | Location | Action |
|---|---|---|
| `CopilotClient::complete_device_flow` | `copilot.rs:322` | delete |
| `CopilotClient::api_token` | `copilot.rs:492` | delete; `ensure_token` stays |
| `CopilotClient::oauth_token` getter | `copilot.rs:383` | delete; keep the field |
| `create_mock_provider` | `embeddings/mod.rs:88` | delete |
| `ModelInfo::formatted_size` | `provider.rs:255` | delete |
| `EmbeddingFixtures`, `FixtureBasedMockProvider` and the copied `hash_text` + sine loop | `mock.rs:133`, `:291`, `:346-362` | delete the two types and their impls |
| `MockEmbeddingProvider::with_model` | `mock.rs:39` | delete |
| `MockEmbeddingProvider::set_embedding` | `mock.rs:51` | delete |
| `openai_small` | `mock.rs:320` | delete |
| `New*` alias clauses (`NewEmbeddingProviderConfig`, `NewFastEmbedConfig`, `NewMockConfig`, `NewOllamaConfig`, `NewOpenAIConfig`) | `llm/mod.rs:62-64` | delete the `as New*` clauses |
| `EmbeddingError::{CircuitBreakerOpen, ModelDiscoveryNotSupported, ModelNotFound, InvalidModelMetadata, InferenceFailed}` | `error.rs:52-81`, `:127-135` | delete the variants and their `is_retryable` arms |
| `anthropic::parse_models_response` vs `openai_compat::parse_models_response` | `model_listing.rs:56-78`, `:148-181` | merge-into one `parse_models_response` that reads `data[] \| models[]` with `id \| name`; the anthropic shape is a subset |

### T1-B10 — crucible-daemon, skills, enrichment, pipeline, workflow, observe

Files: `skills/types.rs`, `skills/error.rs`, `skills/test_utils.rs`, `skills/mod.rs`, `enrichment/service.rs`, `pipeline/note_pipeline.rs`, `workflow_registry.rs`, `observe/events.rs`, `observe/session.rs`, `lib.rs`.

| Item | Location | Action |
|---|---|---|
| `Skill::id` | `skills/types.rs:78` | delete |
| `SkillError::NotFound` | `skills/error.rs:29` | delete |
| `skills::test_utils` | `skills/test_utils.rs` | delete the file and the `pub mod` line |
| `EnrichmentService::has_embedding_provider` | `enrichment/service.rs:81` | delete |
| `Enricher::with_min_words`, `with_max_batch_size`, `min_words_for_embedding`, `max_batch_size`, `DEFAULT_MIN_WORDS_FOR_EMBEDDING`, `DEFAULT_MAX_BATCH_SIZE` | `enrichment/service.rs:15`, `:18`, `:62-77` | delete the two builders and two getters; inline the consts into the struct defaults |
| `NotePipeline::new` | `pipeline/note_pipeline.rs:83` | delete; all callers use `with_config` |
| `storage_key` inline copies | `note_pipeline.rs:221-228`, `:331-335`, `:375-379` | call `storage_key` (`:511`) |
| `request_scope` copy | `note_pipeline.rs:665` | merge-into `server/kiln.rs:20` `request_scope`, moved to a shared `pub(crate)` location |
| `WorkflowRegistry::len`, `is_empty`, `prune_terminal` | `workflow_registry.rs:50`, `:63-69` | delete |
| `LogEvent::permission_with_reason` | `observe/events.rs:348` | delete |
| `LogEvent::summary_with_count` | `observe/events.rs:373` | delete |
| `observe::list_sessions` | `observe/session.rs:49`, `:254-355`, `lib.rs:112` | delete with its module tests and the re-export |

### T1-B11 — crucible-daemon, session, scm, agent_manager, kiln, platform, bootstrap

Files: `session_manager.rs`, `session_manager/tests.rs`, `scm.rs`, `agent_manager/mod.rs`, `agent_manager/messaging/send.rs`, `agent_manager/tests/*`, `agent_manager/models.rs`, `kiln_registry.rs`, `kiln_manager.rs`, `server/platform.rs`, `daemon_plugins/bootstrap.rs`, `project_manager.rs`, `session_bridge.rs`, `server/session/models.rs`, `server/session/messaging.rs`.

| Item | Location | Action |
|---|---|---|
| `SessionManager::remove_session` | `session_manager.rs:783` | delete with `test_remove_session` (`tests.rs:322`) |
| `SessionManager::active_count`, `total_count` | `session_manager.rs:802`, `:811` | delete with the tests at `tests.rs:350-371` |
| `archive_session` vs `unarchive_session` 15-line block | `session_manager.rs:699-714`, `:726-741` | call one private `fn set_archived(session_id, bool)` |
| `list_sessions_filtered` predicate x3 | `session_manager.rs:471`, `:498`, `:521` | call one predicate fn over the shared fields |
| `FileSessionStorage::new(sm.sessions_root()).with_registry(...)` x4 | `server/session/models.rs:207`, `server/session/messaging.rs:126`, `session_bridge.rs:416` and one more | call the `SessionManager`'s `Arc<dyn SessionStorage>` |
| `session_bridge::truncate_str` | `session_bridge.rs:961` | call `crucible_core::background::types::truncate` |
| `scm::discover_workdir`, `ScmError::NotARepo`, `ScmError::InvalidBranch` | `scm.rs:52`, `:26`, `:29` | delete all three |
| `RequestState.started_at` | `agent_manager/mod.rs:137`, `:203`, `send.rs:91`, five tests | delete the field and seven writes |
| `AgentManager::invalidate_model_cache` | `agent_manager/mod.rs:934` | delete; the test at `tests/models_discovery.rs:772` calls `model_cache.clear()` |
| `ResolvedProvider.api_key` | `agent_manager/mod.rs:358`, `models.rs:23` | delete the field and three test asserts; it carries a secret nobody reads |
| `KilnRegistry::iter` | `kiln_registry.rs:418` | delete |
| stale `#[allow(dead_code)]` on `KilnManager::get` | `kiln_manager.rs:731` | delete the attribute and its comment |
| `NoteRecord -> NoteInfo` closure x2 | `kiln_manager.rs:133-144`, `:209-220` | call one `impl From<&NoteRecord> for NoteInfo` |
| skills discovery block x3 | `server/platform.rs:55-62`, `:105-112`, `:148-155` | call one `async fn discover_skills(kiln_path)` |
| `bootstrap::expand_tilde`, `kiln_manager::expand_tilde_path`, `scm.rs:156`, `scm.rs:221` tilde copies | `bootstrap.rs:113`, `kiln_manager.rs:1199`, `scm.rs:158-167`, `:229-238` | call `project_manager::resolve_registration_root` (`project_manager.rs:68`); make it `pub(crate)` |

### T1-B12 — crucible-daemon, `agent_manager/messaging/`

Files: `agent_manager/messaging/permission.rs`, `tool_call.rs`, `stream.rs`, `tool_hooks.rs`, `crucible-core/src/traits/chat.rs`.

| Item | Location | Action |
|---|---|---|
| permission-engine input snippet x3 | `permission.rs:583-590`, `:626-633`, `:688-695` | call one `fn engine_input(tool_name, args) -> &str` |
| file-tool name list x2 | `permission.rs:1073`, `:1098` | call one `const FILE_TOOLS: &[&str]` or `BuiltinTool` predicate; leave `core engine.rs:193 is_file_tool` alone (different list, Tier 3) |
| `ChatToolResult` error literal x7 | `tool_call.rs:44`, `:499`, `:517`, `:815`, `:875`, `stream.rs:619`, `:697` | add `ChatToolResult::error(name, call_id, msg)` in `crucible-core/src/traits/chat.rs`; call it |
| inline `deny_tool_call` body | `tool_call.rs:485-506` | call `deny_tool_call` (`:23`) |
| `resolve_display_start_hints` vs `resolve_display_complete_hints` | `tool_hooks.rs:22-71`, `:73-122` | call one generic `fn resolve_hints<E, H>(hook, event) -> H` |

### T1-B13 — crucible-daemon, server path and review helpers; tool listing

Files: `server/fs/mod.rs`, `server/session/review/mod.rs`, `server/note_refactor.rs`, `server/canvas/containment.rs`, `tools/workspace.rs`, `tool_dispatch.rs`, `tools/gateway_executor.rs`, `replay.rs`, `crucible-cli/src/tui/oil/local_replay.rs`.

| Item | Location | Action |
|---|---|---|
| `Component::Normal` whitelist loop x6 | `fs/mod.rs:155-165`, `:381-391`, `:455-464`, `review/mod.rs:605`, `note_refactor.rs:228`, `canvas/containment.rs:215` | call one `pub(crate) fn reject_non_normal(path) -> Result<()>` in `tools/containment.rs` |
| `parse_state`, `state_reason`, `parse_author` | `review/mod.rs:490-512` | call `serde_json::from_value` on `ReviewState` / `CommentAuthor` (`rename_all = snake_case` yields the same strings) |
| `rmcp::Tool -> ToolDefinition` x3 | `workspace.rs:655-671`, `tool_dispatch.rs:575-591`, `gateway_executor.rs:68` | call one `fn tool_definition_from_rmcp(tool, category) -> ToolDefinition` |
| `is_core_tool_name` | `tool_dispatch.rs:181-186` | call `BuiltinTool::parse(name).map(\|t\| t.surface() == ToolSurface::Host)` |
| `is_keypress_event` | `replay.rs:199`, `crucible-cli/src/tui/oil/local_replay.rs:62-65` | make the daemon fn `pub`; the CLI calls it |

### T1-B14 — crucible-core, `parser/types/` accessors

Files: `parser/types/callout.rs`, `parsed_note.rs`, `frontmatter.rs`, `content.rs`, `ast.rs`, `blocks.rs`, `parser/blockquotes.rs`, `parser/basic_markdown_it.rs`, `parser/block_hasher.rs`.

| Item | Location | Action |
|---|---|---|
| `Callout::display_type`, `is_standard_type` | `callout.rs:175`, `:170` | delete |
| `LatexExpression::expression_type` | `callout.rs:227` | delete |
| `Callout::start_offset`, `LatexExpression::start_offset` | `callout.rs:184`, `:236` | delete; callers use `.offset` |
| `ParsedNote::first_heading` | `parsed_note.rs:226` | delete |
| `Frontmatter::get_bool`, `get_number`, `get_date`, `get_object`, `has` | `frontmatter.rs:66`, `:70`, `:79`, `:109`, `:114` | delete; check the `chrono` import after |
| `CodeBlock::is_language` | `content.rs:191` | delete |
| `ASTBlock::is_heading`, `with_depth`, `with_hierarchy` | `ast.rs:269`, `:247`, `:254` | delete |
| `ASTBlock::type_name` | `ast.rs:204-216` | merge-into `ASTBlockType::as_str` (`:39-50`); `block_hasher.rs:134` calls `block.block_type.as_str()` |
| `HorizontalRule::detect_style` | `blocks.rs:94` | delete |
| `supports_blockquotes` | `blockquotes.rs:163` | delete |
| `BasicMarkdownItExtension::disabled` | `basic_markdown_it.rs:40` | delete (feature `markdown-it-parser`) |

### T1-B15 — crucible-core, `parser/` lists, links, extensions, traits

Files: `parser/types/lists.rs`, `links.rs`, `parser/extensions.rs`, `parser/traits.rs`, `parser/implementation.rs`, `parser/block_extractor.rs`, `parser/mod.rs`.

| Item | Location | Action |
|---|---|---|
| `ListItem::with_metadata`, `new_task_with_metadata` | `lists.rs:312` and sibling | delete |
| `ListItem::content_without_task`, `effective_indent`, `set_nested` | `lists.rs:367`, `:361`, `:346` | delete |
| `ListBlock::items_at_level`, `nested_items`, `with_marker_style` | `lists.rs:99`, `:107`, `:60` | delete |
| `FootnoteMap::add_reference`, `orphaned_references`, `unused_definitions` | `links.rs:270`, `:280`, `:288` | delete |
| `Wikilink::with_alias` | `links.rs:73` | delete |
| `SyntaxExtension::process_content`, `capabilities`, `ExtensionCapabilities` | `extensions.rs:59`, `:70-81`, `parser/mod.rs` re-export | delete the two defaulted methods, the type and the re-export |
| `ParserRequirements::links_and_tags_only` | `traits.rs:136` | delete |
| `ParserCapabilities::full` vs `CrucibleParser::capabilities` | `traits.rs:54-73`, `implementation.rs:493-512` | `capabilities()` calls `ParserCapabilities::full()` then sets `max_file_size` |
| `ExtractionType` | `block_extractor.rs:827-840` | merge-into `ASTBlockType`; delete the private enum and its `#[allow(dead_code)]` |

### T1-B16 — crucible-core, `types/`

Files: `types/undo_tree.rs`, `types/undo.rs`, `types/database.rs`, `types/tool_ref.rs`, `types/mode.rs`, `types/notification.rs`, `types/acp.rs`, `types/popup.rs`, `types/mod.rs`, `turn/mod.rs`, `lib.rs`, `interaction/types.rs`.

| Item | Location | Action |
|---|---|---|
| `undo_tree.rs` whole file (`UndoTree`, `TreeNode`, `TreeSummary`, `UndoNodeId`, `NodeId` alias, `render_ascii`, `current_item`, `current_item_mut`, `iter_nodes`, `tree_summary`) | `undo_tree.rs`, `types/mod.rs:84` | delete the file and its re-exports; this also ends the `NodeId` name collision with `turn/tree.rs:38` |
| `UndoEntry` | `undo.rs:14`, `types/mod.rs:81` | delete; `UndoSummary` stays |
| `DbError`, `DbResult` | `database.rs:11`, `:15`, `types/mod.rs:39`, `lib.rs:126` | delete |
| `UnifiedSearchResult` | `database.rs:189`, `:210-328` | delete with its tests and re-exports |
| `QueryResult::empty`, `with_records`; `Record::with_id` | `database.rs:108`, `:118` | delete |
| `ToolRef::core`, `crucible`, `from_mcp`, `from_plugin`, `searchable_text`, `is_core`, `is_crucible`, `is_mcp`, `is_plugin`, `with_always_available` | `tool_ref.rs:74-134` | delete with the in-file tests that use them |
| `ModeDescriptor::new`, `with_icon`, `with_color` | `mode.rs:171` | delete |
| `NotificationQueue::expire_old`, `notifications`; `#[allow(dead_code)]` on `created_at` | `notification.rs:137`, `:20` | delete the methods; then remove `created_at` if nothing reads it |
| `SessionId::as_uuid` | `acp.rs:75` | delete |
| `SessionConfig::with_enrichment_count` | `acp.rs:186` | delete |
| `FileMetadata::as_directory` | `acp.rs:603` | delete |
| `SharedAgent` | `turn/mod.rs:424` | delete |
| `PopupEntry` vs `PanelItem` | `types/popup.rs:16`, `interaction/types.rs:135` | merge-into `PopupEntry`; `pub type PanelItem = PopupEntry` in `interaction/types.rs` (same serde shape) |

### T1-B17 — crucible-core, dead modules

Files: `content_category.rs`, `note.rs`, `properties.rs`, `processing/` (whole directory), `hashing/` (whole directory), `storage/traits.rs`, `test_support/mocks/storage.rs`, `test_support/mocks/mod.rs`, `test_support/MOCKS.md`, `lib.rs`, `types/mod.rs`, `storage/mod.rs`.

| Item | Location | Action |
|---|---|---|
| `ContentCategory` and its const aliases (`IMAGE`, `VIDEO`, `AUDIO`, `WEB`, `WIKIPEDIA`, `YOUTUBE`, `DOCUMENT`) | `content_category.rs` | delete the file, `lib.rs:9` and `lib.rs:44` |
| `NoteNode`, `ViewportState` | `note.rs`, `lib.rs:103`, `types/mod.rs:43` | delete |
| `AttributeValue`, `PropertyMap` | `properties.rs`, `lib.rs:124`, `types/mod.rs:46` | delete |
| `ProcessedNote`, `ProcessingContext`, `ProcessingSource`, `ProcessingPriority`, `ProcessingMetadata`, `NoteProcessingJob`, `JobConfiguration`, `JobStats`, `NoteProcessingResult`, `PipelineMetrics`, `ChangeDetectionStore`, `InMemoryChangeDetectionStore`, `FileState`, `ChangeDetectionError`, `record_skip`, `set_priority` | `processing/` | delete the directory, `lib.rs:29`, `:61` and the re-exports |
| `hashing/` (`HashingAlgorithm`, `Blake3Algorithm`, `Sha256Algorithm`, `Blake3Hasher`, `SHA256Hasher`, the two `*_CONTENT_HASHER` consts) | `hashing/` | delete the directory; update `CONTENT_HASHER.md` and `test_support/MOCKS.md` prose |
| `StorageBackend`, `StorageStats`, `QuotaUsage` | `storage/traits.rs:48-90`, `storage/mod.rs` | delete; keep `ContentHasher` for now (Tier 3) |
| `MockStorage`, `MockStorageStats` | `test_support/mocks/storage.rs:68`, `mocks/mod.rs:24-40` | delete |
| `hash_computation` | `storage/error.rs:97` | delete |

### T1-B18 — crucible-core, events, enrichment, traits, config, project, serde_md, test_support

Files: `events/markdown/` (whole directory), `events/session_event/display.rs`, `events/session_event/mod.rs`, `events/session_event/payloads.rs`, `events/ring.rs`, `enrichment/embedding.rs`, `enrichment/types.rs`, `enrichment/mod.rs`, `traits/context_ops/mod.rs`, `traits/provider.rs`, `traits/mcp.rs`, `traits/tools.rs`, `traits/mod.rs`, `workflow/engine.rs`, `project/types.rs`, `config/enrichment.rs`, `config/components/llm.rs`, `config/config/cli_app.rs`, `config/credentials.rs`, `config/value_source.rs`, `serde_md/serializer.rs`, `test_support/mod.rs`, `test_support/fixtures.rs`.

| Item | Location | Action |
|---|---|---|
| `events/markdown/` (`to_markdown_block`, `from_markdown_block`, `format.rs`, `parse.rs`, `SessionEvent::event_type_name`, the hand-rolled calendar arithmetic) | `events/markdown/` | delete the directory and its `pub mod`; this removes the `type_name` duplicate |
| `events/session_event/display.rs` | `display.rs` | delete the file and the `pub mod` at `session_event/mod.rs:33` |
| `NotePayload::with_word_count` | `payloads.rs:109` | delete |
| `EventRing::would_overflow_unflushed` | `ring.rs:176` | delete |
| `CachedEmbedding` | `enrichment/embedding.rs:142`, `enrichment/mod.rs:10`, `lib.rs:53` | delete |
| `EmbeddingResponse` (core) | `traits/provider.rs:16` | delete; the daemon `llm/embeddings/provider.rs:400` type is canonical |
| `ModelCapability`, `UnifiedModelInfo` | `traits/provider.rs:34` | delete with their tests |
| `McpTransportConfig` | `traits/mcp.rs:156`, `:246`, `traits/mod.rs:38` | delete |
| `ToolDefinition::with_example` | `traits/tools.rs:258` | delete; keep `ToolExample` (serde field) |
| `estimate_messages_tokens` | `traits/context_ops/mod.rs:139` | delete with `context_ops_tests.rs:83`, `:88` |
| `WorkflowStatus::is_awaiting_gate` | `workflow/engine.rs:54` | delete |
| `repository_id`, `with_named_kiln` | `project/types.rs:97`, `:78` | delete |
| `embedding_count` | `enrichment/types.rs:58` | delete |
| `BurnEmbedConfig::all_search_paths`, `default_search_paths` | `config/enrichment.rs:561`, `:541` | delete |
| `LlmProviderConfigBuilder::maybe_timeout_secs` | `config/components/llm.rs:172` | delete |
| `CliAppConfig::session_kiln_path` | `config/config/cli_app.rs:879` | delete; `session_kiln_name` is live |
| `AutoStore::with_file_path` | `config/credentials.rs:381` | delete |
| `track_value!` | `config/value_source.rs:141-144` | delete; it names a field no struct has |
| `Serializer::into_output` | `serde_md/serializer.rs:33` | delete |
| `create_kiln_with_files`, `create_basic_kiln` (pub copies) | `test_support/mod.rs:34`, `:56` | merge-into `fixtures.rs:188`, `:75`; make those `pub` and re-export |

### T1-B19 — crucible-cli, `chat_runner/`, `chat_app/`, runner, composer

Files: `tui/oil/chat_runner/mod.rs`, `chat_runner/runner.rs`, `chat_app/state.rs`, `chat_app/mod.rs`, `chat_app/popup_state.rs`, `chat_app/message_handlers.rs`, `chat_app/model_state.rs`, `chat_app/autocomplete.rs`, `chat_app/command_handling.rs`, `tui/oil/runner.rs`, `tui/oil/composer.rs`, `tui/oil/mod.rs`, `tui/mod.rs`, `tui/oil/utils/width.rs`, `tui/oil/utils/mod.rs`, `commands/chat/mod.rs`.

| Item | Location | Action |
|---|---|---|
| `OilChatRunner::context_limit_handle` | `chat_runner/mod.rs:177` | delete |
| `OilChatRunner::with_available_models` | `chat_runner/mod.rs:206` | delete; keep `OilChatApp::set_available_models` (test uses it) |
| `recording_mode`, `recording_path` write-only fields | `chat_runner/mod.rs:97-98`, `:153-154`, `:245`, `:250`, `commands/chat/mod.rs:470-471` | delete |
| `Role` enum | `chat_app/state.rs:5`, `chat_app/mod.rs:40`, `tui/oil/mod.rs:40`, `tui/mod.rs:8` | delete with three re-exports |
| `PrecognitionState::last_notes`, `last_notes_count` | `popup_state.rs:75-86`, `message_handlers.rs:274-275` | delete |
| `ModelListState::Failed(String)` payload | `model_state.rs:7` | change to `Failed`; callers already match `Failed(_)` |
| `AutocompleteKind::CommandArg.arg_index` | `state.rs:80`, `autocomplete.rs:135-137`, `:243-244` | delete the field and the writers |
| `OilRunner`, `run_sync`, `with_message_channel` | `tui/oil/runner.rs`, `oil/mod.rs:49`, `tui/mod.rs:5`, `:7` | delete the file and the re-exports; this also removes the `run` / `run_sync` loop duplicate |
| `ComposerConfig`, `pad_popup_region` | `composer.rs:28-133`, `oil/mod.rs:45`, `tui/mod.rs` | delete with tests and re-exports |
| `cursor_position` | `utils/width.rs:38`, `utils/mod.rs:19` | delete |
| `AutocompleteKind::Command` two-entry palette | `autocomplete.rs:181-187` | call `known_slash_commands` (`commands/chat/mod.rs:929`) |
| `levenshtein` | `command_handling.rs:44-58` | merge-into one `pub fn levenshtein(&str, &str)` in `crucible-core`; `crucible-lua/src/handlers/crucible_on.rs:12-25` calls it too (char-indexed version wins) |

### T1-B20 — crucible-cli, `tui/oil/config/`

Files: `tui/oil/config/overlay.rs`, `presets.rs`, `value.rs`, `shortcuts.rs`, `stack.rs`.

| Item | Location | Action |
|---|---|---|
| `RuntimeConfig::get_or_default` | `overlay.rs:103` | delete |
| `RuntimeConfig::new` | `overlay.rs:59` | delete; tests use `empty()` |
| `RuntimeConfig::enable` | `overlay.rs:156` | delete with the test at `:509`; keep `disable` |
| `SetError::NotFound`, `TypeMismatch` | `overlay.rs:21` | delete; only `NotBoolean` is built |
| `ThinkingPreset::render_soft_prompt` | `presets.rs:83`, `:190-238` | delete with tests |
| `ConfigValue::parse_bool`, `as_float` | `value.rs:127`, `:441` | delete with the test; keep `try_parse_bool` |
| `ShortcutRegistry::is_virtual`, `target_path` | `shortcuts.rs:208`, `:281-298` | delete with tests |
| `ConfigStack::current_source`, `base`, `modification_count`, `reset` | `stack.rs:149`, tests `:210-385` | delete with tests |

### T1-B21 — crucible-cli, components, theme, viewport cache, status bar

Files: `tui/oil/component.rs`, `viewport_cache.rs`, `components/interaction_modal/mod.rs`, `components/shell_modal.rs`, `components/thinking_component.rs`, `components/status_bar.rs`, `components/status_items.rs`, `components/notification_component.rs`, `components/input_component.rs`, `theme/global.rs`, `theme/groups.rs`, `theme/geometry.rs`, `theme/bars.rs`, `theme/exprs.rs`, `theme/mod.rs`, `markdown/mod.rs`.

| Item | Location | Action |
|---|---|---|
| `ComponentHarness::focus_mut` | `component.rs:65` | delete |
| `CachedToolCall::last_n_lines`, `set_auto_approved` | `viewport_cache.rs:148`, `:136` | delete |
| `InteractionModalOutput::Close`, `Notify` | `interaction_modal/mod.rs:44`, `:58` | delete with their match arms |
| `InteractionModal::other_text_preserved`, `filter` | `interaction_modal/mod.rs:80`, `:70`, `:123`, `:128` | delete; `panel.rs` uses `PanelState::filter` |
| `ShellModal::{command, status, output_lines, working_dir, duration, output_path, set_output_path}` | `shell_modal.rs:468` | delete |
| `ThinkingComponent::replace`, `is_graduated` | `thinking_component.rs:38`, `:48` | delete with the unit test; fix the doc at `:37` |
| `theme::is_initialized` | `theme/global.rs:50`, `:93`, `:101`, `theme/mod.rs:22` | delete with tests and re-export |
| `theme::exprs::clear` | `theme/exprs.rs:66` | delete with its test |
| four `RwLock<Option<&'static T>>` + `OnceLock` stores | `global.rs:20-47`, `groups.rs:14-35`, `geometry.rs:10-35`, `bars.rs:11-31` | call one generic `struct RenderSlot<T>` with `set`, `get`, `reset` |
| `StatusBarData` | `status_items.rs:19-29` | merge-into `StatusBar` (`status_bar.rs:36-48`); `item_data()` returns `&self` |
| `InputComponent::view` cursor/content match | `input_component.rs:115-126` | call `InputStyle::display_content` / `display_cursor` (already imported at `:9`) |
| `NotificationEntry::kind_label` | `notification_component.rs:38-44` | call `NotificationToastKind::label` (`status_bar.rs:25`) |
| `RenderStyle::Natural` | `markdown/mod.rs:64-133` | merge-into `RenderStyle::Viewport`; the two carry the same data and compute the same widths; keep a `natural()` constructor if tests call it |

### T1-B22 — crucible-cli, commands and utils

Files: `commands/plugin/list.rs`, `commands/plugin/update.rs`, `commands/plugin/plugin_ops.rs`, `commands/agents.rs`, `kiln_discover.rs`, `kiln_validate.rs`, `main.rs`, `cli/mod.rs`, `tui/oil/utils/truncate.rs`, `tui/oil/utils/mod.rs`, `config/config/cli_app.rs` (core), `commands/session/acp.rs`.

| Item | Location | Action |
|---|---|---|
| `plugins.toml` path x2 | `plugin/list.rs:28-43`, `plugin/update.rs:13-28` | call `plugin_ops::plugins_toml_path` and `plugins_dir` |
| `agents.rs::resolve_path(path, _config_dir)` | `agents.rs:91-101` | call `kiln_validate::expand_tilde` (`:66`); drop the unused parameter at four call sites |
| `is_temp_directory` x2, `is_temp_root` | `kiln_discover.rs:105`, `kiln_validate.rs:216`, `:237` | keep one `pub(crate) fn is_temp_root` in `kiln_validate.rs` |
| `parse_log_level` | `main.rs:15-25` | call `LogLevel::from_str` then `LevelFilter::from(LogLevel)` (`cli/mod.rs:31-58`) |
| `truncate_to_width`, `truncate_to_chars` CLI copies | `tui/oil/utils/truncate.rs:32`, `:128` | merge-into `crucible_oil::utils`; the CLI re-exports them from `utils/mod.rs:18`; keep `truncate_lines` in the CLI |
| `resolved_kiln_path` inline match | `cli_app.rs:853-858` | call `KilnEntry::path()` (`registry.rs:40`) |
| `acp.rs` send loop vs replay loop | `commands/session/acp.rs:440-606`, `:650-820` | call one `fn print_event(event) -> Option<Ended>`; the `ended` arm differs, keep it in the callers |

### T1-B23 — crucible-lua

Files: `theme_wire.rs`, `hl_lua.rs`, `theme.rs`, `fs.rs`, `http.rs`, `handlers/before_execute.rs`, `handlers/registry.rs`, `lifecycle/spec.rs`, `manifest.rs`, `shell.rs`.

| Item | Location | Action |
|---|---|---|
| `hl_lua::color_name` | `hl_lua.rs:105-127` | call `theme_wire::color_to_name` (`theme_wire.rs:40`) |
| `hl_lua::side` | `hl_lua.rs:40-47` | call `theme::parse_any_color` (`theme.rs:740`) |
| ensure-parent block x4 | `fs.rs:58-68`, `:75-85`, `:127-137`, `:145-155` | call one `fn ensure_parent(path) -> Result<()>` |
| `get`/`post`/`put`/`delete`/`patch` closures | `http.rs:50-116` | loop over `[("get", HttpMethod::Get), ...]` and register one closure per row |
| `execute_runtime_json_handler` | `handlers/before_execute.rs:83-118` | call `execute_runtime_handler` (`registry.rs:192`) |
| `parse_capability` | `lifecycle/spec.rs:31-44` | call `serde_json::from_value::<Capability>` (`manifest.rs:78` derives it with the same aliases) |
| `exec_command` vs `spawn_command` setup | `shell.rs:159-185`, `:289-312` | call one `fn prepare_command(policy, cmd, args, opts) -> Result<Command>` |

### T1-B24 — crucible-oil and crucible-web (non-route)

Files: `crucible-oil/src/cell_grid.rs`, `overlay.rs`, `popup_node.rs`, `layout/types.rs`, `components/popup.rs`, `components/input_area.rs`, `components/mod.rs`, `lib.rs`, `node.rs`, `taffy_layout.rs`, `crucible-web/src/events.rs`, `crucible-daemon/src/rpc_client/agent/convert.rs`.

| Item | Location | Action |
|---|---|---|
| `overlay::StyledCell` | `overlay.rs:17-20` | merge-into `cell_grid::StyledCell`; move `is_transparent()` onto it |
| `overlay::cells_to_string` | `overlay.rs:87-114` | call `cell_grid::cells_to_string` (make it `pub(crate)`) |
| `PopupItem` | `layout/types.rs:204-211` | merge-into `PopupItemNode` (`popup_node.rs:37`) |
| `components::popup::popup_item` | `components/popup.rs:149-155` | delete; `lib.rs:53` re-exports `popup_node::popup_item` |
| `wrap_content`, `clamp_input_lines`, `InputArea` (oil copies, no callers) | `input_area.rs:29`, `:143`, `:166`, `components/mod.rs:7`, `lib.rs:53` | delete; the CLI `InputComponent` is the live one; keep the `InputStyle` trait |
| web `PREFIXES` + strip loop | `crucible-web/src/events.rs:398-418` | call a `pub fn strip_event_prefix` in `crucible-daemon/src/rpc_client/agent/convert.rs:116` |
| web error body hand copies (middleware only) | `middleware/auth/mod.rs:394-417`, `middleware/auth/shell.rs:178-201` | call `error.rs:61` body builder; the `routes/webhook.rs:147` copy is excluded |

### Excluded from Tier 1

These items meet the definition but live in a protected path. Do them in a session that owns that path.

| Item | Location | Reason |
|---|---|---|
| `handle_session_list` parameter `data_home` | `server/session/list.rs:25`, `:138`; argument at `rpc/dispatch.rs` | edit touches `rpc/dispatch.rs` |
| `DaemonCapabilities` + `CapabilityFlags` vs inline `json!` | `rpc_client/client/types.rs:18`, `rpc/dispatch.rs:1103-1118` | reply literal lives in `rpc/dispatch.rs` |
| `OkResponse` vs `json!({"ok": true})` x8 | `crucible-web/src/routes/session/mod.rs:21`, `chat.rs:188`, `plugin.rs:116`, `:124`, `layout.rs:52`, `:61`, `project.rs:214`, `kiln.rs:357`, `canvas.rs:174` | all under `crucible-web/src/routes/` |
| web error body copy | `crucible-web/src/routes/webhook.rs:147-154` | under `routes/` |

## 3. Tier 2 — mechanically dead, no skeptic

Read the definition first. Skip an item if it carries `serde`, `mlua` or `rpc` attributes. Skip an item if a string literal with the same name appears in a `.lua`, `.fnl` or `.ts` file. Report every skip.

The 117 mechanically dead items overlap the skeptic lists. 94 are already in Tier 1 or Tier 3. The 23 below have no skeptic verdict.

### T2-B1 — crucible-cli

Files: `config.rs`, `commands/session/io.rs`, `commands/session/export.rs`, `commands/agents.rs`, `commands/process.rs`.

| Item | Location | Action |
|---|---|---|
| `EmbeddingConfigSection` alias | `config.rs:21` | delete |
| `format_events_markdown` `_include_timestamps` parameter | `commands/session/io.rs:53` | the flag is a no-op; either implement it or delete the parameter and the `--timestamps` pass at `export.rs:35`. Report which. |
| `resolve_path` `_config_dir` parameter | `commands/agents.rs:91` | delete (T1-B22 also touches this fn; do T1-B22 first) |
| `run_watch_mode` `_verbose` parameter | `commands/process.rs:247` | delete |

### T2-B2 — crucible-lua

Files: `executor.rs`, `session_api.rs`, `statusline_exprs.rs`, `schema.rs`, `statusline_lua.rs`, `lifecycle/queries.rs`, `types.rs`.

| Item | Location | Action |
|---|---|---|
| `add_session_start_hook` | `executor.rs:94` | delete |
| `clear_current` | `session_api.rs:511` | delete |
| `unbind` | `session_api.rs:350` | delete |
| `is_forbidden` | `statusline_exprs.rs:87` | delete |
| `is_optional` | `schema.rs:188` | delete |
| `is_status_item` | `statusline_lua.rs:242` | delete |
| `PluginManager::load_errors` | `lifecycle/queries.rs:34` | delete |
| `PluginManager::plugin_has_capability` | `lifecycle/queries.rs:28` | delete |
| `ok_with_metadata` | `types.rs:100` | delete |

### T2-B3 — crucible-oil

Files: `proptest_strategies.rs`, `style.rs`, `cell_grid.rs`, `planning.rs`, `terminal.rs`, `components/input_area.rs`.

| Item | Location | Action |
|---|---|---|
| `arb_gap`, `arb_size`, `arb_popup`, `assert_lines_exact_width` | `proptest_strategies.rs:54`, `:59`, `:130`, `:294` | delete |
| `detect_dark_terminal` | `style.rs:333` | delete |
| `CellGrid::extract_rows` | `cell_grid.rs:192` | delete |
| `FrameSnapshot::screen_with_overlays` | `planning.rs:81` | delete |
| `Terminal::show_cursor_at` | `terminal.rs:311` | delete |
| `Terminal::with_alternate_screen` and the `use_alternate_screen` field | `terminal.rs:168` | delete both; the field is always false |
| `InputArea::with_popup` | `components/input_area.rs:57` | delete (T1-B24 deletes `InputArea`; do T1-B24 first) |

## 4. Tier 3 — needs a decision

Each entry gives the options, a recommendation, the payoff for extension, the risk and the cost (S: under an hour; M: half a day; L: a day or more). Entries are ranked by payoff over risk: band A first.

### Band A — high payoff, low risk

**A1. `AgentHandle` defaulted methods (41 of 44).** `crucible-core/src/traits/chat.rs:143`.
Options: (a) make every knob required; (b) split into `AgentHandle` (3 required) plus `SessionKnobs` (required); (c) keep.
Recommend (b). Payoff: a new client or provider that forgets a knob fails to compile; today it compiles and silently does nothing (see "Session-scoped vs TUI-local" in AGENTS.md). Risk: `CLI Noop` and `Mock` need 41 stubs; generate them with one macro. Cost: M.

**A2. `SessionConfigRpc` (0 required / 22 defaulted).** `crucible-lua/src/session_api.rs:67`.
Options: (a) make all required; (b) delete the trait and call `DaemonSessionApi`; (c) keep.
Recommend (a) now, (b) after A5. Payoff: a Lua knob cannot be half-wired. Risk: 9 impls (3 + 6 test) gain stubs. Cost: M.

**A3. `DaemonSessionApi` 15 defaulted methods.** `crucible-lua/src/sessions/mod.rs:103`.
Recommend: make them required; it is a firewall trait and the one production impl already overrides them. Payoff: same as A1 for the Lua surface. Risk: 3 test impls gain stubs. Cost: S.

**A4. `NoteStore` 5 defaulted methods that return empty link data; `KnowledgeRepository` 1; `EmbeddingProvider` 2; `EventHandler` 2 (every impl overrides both); `ContentHasher` 1; `StorageClient` 1 (always errors).** Locations in Actual.md section 6.
Recommend: make all required. Payoff: a new storage backend must answer the link queries; today a backend that omits them returns "no links" and passes. Risk: 6 test `NoteStore` impls gain stubs. Cost: S per trait, M total.

**A5. `ChannelSessionRpc` + `SessionCommand` + `with_session_command_receiver` chain.** `crucible-lua/src/session_api.rs:156`, `crucible-cli/src/tui/oil/chat_runner/mod.rs:163`, `runner.rs:551`, `:387`, `commands.rs:15`.
Options: (a) delete the whole channel path (TUI side is dead; Lua side has no consumer); (b) wire `with_session_command_receiver` at the one call site; (c) keep.
Recommend (a). Payoff: one fewer session RPC binding; the Lua session API then has one transport. Risk: `SessionCommand` is live Lua surface in `session_api.rs`; check `runtime/` scripts for `session.*` calls that route through it. Cost: M.

**A6. Test-support gating.** `crucible-core/src/lib.rs:32 pub mod test_support` unconditional; `crucible-daemon/src/test_support.rs` always compiled; `parser/test_utils.rs` ungated; `ComponentHarness`, `AppHarness` re-exported from `tui/mod.rs`; `PluginManager` test-only API (`active_plugins`, `eval_runtime`, `reload`, `enable`, `initialize`, `error_log`, `with_search_paths`, `load_plugin_spec_from_source`).
Options: (a) gate all under `#[cfg(any(test, feature = "test-utils"))]` and enable the feature from dev-dependencies; (b) leave.
Recommend (a). Payoff: `EnvVarGuard` and `set_var` leave the `cru` binary; `#[allow(dead_code)]` on 20 test-only production methods goes away because the compiler sees the real callers. Risk: integration tests in `crucible-daemon/tests/*` must enable the feature; one Cargo edit per crate. Cost: M.

**A7. `webhook::sign`.** `crucible-daemon/src/webhook/mod.rs:333`; used by `crucible-web` tests.
Options: (a) gate under `test-utils` feature (with A6); (b) web tests re-implement HMAC; (c) keep pub.
Recommend (a). Payoff: follows A6. Risk: none. Cost: S.

**A8. `AgentManager::await_permission`, `get_pending_permission`, `list_pending_permissions`** (`agent_manager/permissions.rs:23`, `:110`, `:120`; 22 tests).
Options: (a) move tests to `slot.insert_permission` / `list_all_pending_permissions` and delete; (b) gate under `cfg(test)`.
Recommend (a). Payoff: one permission path instead of two; a new gate hooks one place. Risk: 22 test rewrites. Cost: M.

**A9. `BackgroundJobManager::{get_job_result_for_session, cancel_job_for_session, running_count, total_running_count}`** (`background_manager/mod.rs:195`, `:202`).
Recommend: delete `running_count` and `total_running_count`; fold the two `_for_session` bodies into their wrappers. Payoff: small. Risk: none. Cost: S.

**A10. `McpGatewayManager::start_reconnect_loop`, `reconnect`, `upstreams_needing_reconnect`; `auto_reconnect` config is inert.** `tools/mcp_gateway.rs:465-526`; `docs/Help/Extending/MCP Gateway.md:144`.
Options: (a) spawn the loop at daemon bind when `auto_reconnect` is true; (b) delete the loop and the config key and fix the doc.
Recommend (a); the config and doc promise it. Payoff: a new upstream MCP server gets reconnect for free. Risk: a new background task in `Server::run` (already 500 lines; see section 9). Cost: M.

**A11. `ExtendedMcpServer::with_gateway` and the gateway arm.** `tools/extended_mcp_server.rs:126`, `:133`, `:150`, `:266`, `:366-403`.
Options: (a) wire `with_gateway` where the daemon builds `ExtendedMcpServer`; (b) delete the arm.
Recommend (a) together with A10; the gateway itself is live. Payoff: gateway tools appear on the MCP host surface, which is the design. Risk: tools/list output grows; pin it with a test. Cost: M.

**A12. `CrucibleMcpServer::with_note_store`, `list_notes_via_store`, `property_search_via_store`.** `tools/mcp_server.rs`, `notes/list.rs:13`, `search.rs:318`.
Options: (a) delete the `note_store` branch (production never sets it); (b) wire it.
Recommend (a). Payoff: one code path per tool; a new note tool implements one branch. Risk: tests that set the store need a rewrite. Cost: M.

### Band B — medium payoff or medium risk

**B1. `SyntaxExtension` trait (8 impls, 4 defaulted, one blocks on Tokio).** `crucible-core/src/parser/extensions.rs:18`.
Options: (a) `enum Extension { Wikilink, Tag, ... }` with one `match`; (b) keep the trait, make methods required.
Recommend (a). Payoff: a new syntax is one variant and the compiler lists every match to update. Risk: `ExtensionRegistry` API changes; parser is an island so blast radius is one crate. Cost: M.

**B2. `CredentialStore` trait (3 impls in one file, `KeyringStore` never compiled).** `crucible-core/src/config/credentials.rs:87`, `:279`.
Options: (a) `enum CredentialStore { File, Env, Keyring }` and delete the `keyring` feature; (b) keep and compile `keyring` in CI.
Recommend (a) and drop `KeyringStore` until someone enables it. Payoff: `AutoStore`'s keyring-first fallback becomes one match. Risk: the optional `keyring` dependency and `Cargo.toml` feature go. Cost: S.

**B3. `FileWatcher` + `WatcherFactory` traits (3 impls, 2 stubs) and the three `*Factory { capabilities }` structs.** `watch/traits.rs:10`, `watch/backends/mod.rs:20`.
Options: (a) `enum Backend { Notify, Polling, Editor }` with a `capabilities()` const table; (b) keep.
Recommend (a). Payoff: a new backend is one variant plus one row. Risk: `select_optimal_backend` rewrites; polling and editor backends are stubs today. Cost: M.

**B4. Single-impl traits with no test double: `PermissionGate`, `Undoable`, `MarkdownParser`, `App`, `FrameRenderer`, `InputStyle`.**
Recommend: replace each `dyn` with the concrete type; keep the trait only where a second impl is planned in writing. `Undoable` goes with `session_can_undo` (B6): the daemon handle returns constants. Payoff: fewer indirections to read. Risk: low; compiler-driven. Cost: S each.

**B5. `EventEmitter` (1 impl + noop + mock; 2 defaulted).** `crucible-core/src/events/emitter.rs:293`.
Recommend: make `emit_recursive` and `is_available` required; keep the trait because `MockEventEmitter` is a real double. Payoff: small. Cost: S.

**B6. `ContentHasher`, `HashingAlgorithm`, `ChangeDetectionStore`.** T1-B17 deletes `hashing/` and `processing/`. `ContentHasher` (`storage/traits.rs:17`) remains with dead impls.
Recommend: delete `ContentHasher` too once T1-B17 lands and `rg ContentHasher` shows only the trait. Cost: S.

**B7. Session event parallel enums.** `SessionEventMessage` (wire, canonical), `TurnPayload` and seven groups (`protocol/session_events/`), `SessionEvent` + `InternalSessionEvent` (scripting, 12 of 52 variants live), `LogEvent` (5 of 16 written), `rpc_client/client/types.rs:10 SessionEvent`, web `ChatEvent` (6 dead variants). Families from the duplicate audit: `PostLlmCall` x2, `SessionEnded` vs `Ended`, `Delegation*` x2, `BashTask*` vs `BashJob*`, `Interaction*` x2, `Subagent*` x2, file/note variants vs `SystemPayload`.
Options: (a) delete the 40 dead scripting variants and the 11 dead `LogEvent` variants; keep the rest; (b) make the scripting enum a projection of the wire enum (`From<SessionEventMessage>`); (c) leave.
Recommend (a) now, (b) as a follow-up after T1-B18 removes the markdown transcript format. Payoff: a new event is one wire variant plus one projection arm, not five enums. Risk: Lua handlers read the scripting names (`handlers/conversion.rs`); a deleted variant that a script matches fails silently at runtime. Grep `runtime/` for each name before deletion. Cost: L.
Result, 2026-08-22: (a) landed. Removed 11 `SessionEvent` and 31 `InternalSessionEvent` variants, and the types only they carried (`SessionEventConfig`, `NotePayload`, `events::ToolCall`, `EntityType`, `InputType`, `TerminalStream`, `ToolProvider`). `LogEvent` had 11 live variants, not 5: `wire_to_log_event` yields `Thinking`, and the daemon writes `Subagent*`. Removed the 5 with no writer (`Permission`, `Summary`, `Bash*`) and `PermissionOutcome`. `runtime/` matched none of the names. (b) stays open.

**B8. `StreamingChunk` vs `TurnEvent`.** `acp/streaming.rs:23`, `turn/mod.rs:41`.
Recommend: add `name` to `StreamingChunk::ToolEnd` and `impl From<StreamingChunk> for TurnEvent`; then delete `send_prompt_with_streaming`, `process_streaming_message`, `apply_session_update` and the four diff-extraction copies (no production caller). Payoff: ACP becomes one translation. Risk: M; `acp_handle.rs` is the only consumer. Cost: M.

**B9. `rpc_client SessionEvent` vs `SessionEventMessage`.** `rpc_client/client/types.rs:10`, `protocol/rpc/mod.rs:85`.
Recommend: `pub type SessionEvent = SessionEventMessage` after renaming `event_type` readers (15 CLI files). Payoff: a new client reads the wire type once. Risk: wide mechanical rename. Cost: M. Note: the `runner.rs:168` copy task goes with it.

**B10. `ToolCall` x4** (`traits/llm.rs:38` OpenAI shape, `events/session_event/tool_call.rs:10`, `ChatToolCall` `traits/chat.rs:669`, `ToolCallInfo` `types/acp.rs:269`).
Recommend: keep `llm::ToolCall` (provider wire) and `events::ToolCall` (event wire); add `From` between them; delete `ToolCallInfo` with the dead `types/acp.rs` family (below). Payoff: one conversion. Risk: both are serialized. Cost: S.

**B11. `types/acp.rs` family: `SessionConfig`, `ToolInvocation`, `ToolOutput`, `StreamChunk`, `ChunkType`, `StreamMetadata`, `FileMetadata`, `SessionId` (uuid).** `crucible-core/src/types/acp.rs:61`, `:130`.
Recommend: delete; edit the `acp/tools.rs` test module and `error_propagation.rs` import. `SessionId` x2 ends. Payoff: one `SessionId`. Risk: test-only. Cost: S.

**B12. `SearchResult` x3.** `storage/note_store.rs:294`, `types/database.rs:147`, `FtsResult`/`TextSearchHit`.
Recommend: give `FtsResult` `Serialize`/`Deserialize` and use it at `server/kiln.rs:329` and `rpc_client/client/storage.rs:113`; delete `TextSearchHit`. Leave the two core types (different concepts: `{note, score}` vs wire hit). Payoff: one text-search shape across daemon and client. Risk: the JSON keys are identical, so the wire does not change; pin with a test. Cost: S.

**B13. `PermissionScope` x2.** `interaction/permission.rs:20`, `config/components/permissions/types.rs:5`.
Recommend: `impl TryFrom<interaction::PermissionScope> for config::PermissionScope`; delete the hand map at `chat_app/shell.rs:113-121`. Payoff: a new scope fails to compile at the conversion. Cost: S.

**B14. `BlockHash` vs `FileHash`; `HashError` and the dead `types/hashing.rs` methods.** `parser/types/block_hash.rs:12`, `types/hashing.rs:25`, `:136`.
Recommend: `pub type FileHash = BlockHash` after `FileHash`'s `FromStr` error becomes the `BlockHash` one; delete `HashError`, `zero`, `from_hex`, `FileHashInfo::new`, `content_matches`, `metadata_matches`, `BlockHashInfo::new`, `HashAlgorithm::Sha256`, `output_size`. Risk: `FromStr` error type changes for callers who match it (none found). Cost: S.

**B15. `ParsedNote` vs `NoteContent` six lists; daemon reads `note.wikilinks`, repository writes `note.content.wikilinks`.** `parser/types/parsed_note.rs:30`, `storage/sqlite/repository.rs:111`.
Recommend: keep the top-level fields (serialized at `tools/kiln.rs:217`); make `parse_content` move, not clone; fix the repository write to the field the daemon reads. Payoff: one source of truth for links. Risk: the write fix is a behaviour change; add a test. Cost: M.

**B16. `ThematicBreak` (`ExtractionType`, `extract_thematic_break`, `ASTBlockType::ThematicBreak`).**
Recommend: delete. `ASTBlockType` is not on any wire. Cost: S.

**B17. `serde_md` serializers (core, zero callers) and daemon `LogEventSerializer` copy; `observe/markdown.rs` is the live renderer.**
Options: (a) delete core `serde_md` except `Error`/`Result`, delete `observe/serde_md.rs`, and make `observe_e2e.rs` test `observe/markdown.rs`; (b) add an extension point in core and make the daemon use it.
Recommend (a). Payoff: one transcript renderer. Risk: `observe_e2e.rs:135`, `:286` rewrite. Cost: M.

**B18. `DiscoveryConfig` `[discovery.hooks]`; `ResolveMode::Strict`; `acp.lazy_agent_selection`; `storage.idle_timeout_secs`.** Config fields parsed and read by nobody.
Recommend: delete `DiscoveryConfig` and `ResolveMode::Strict`; keep `idle_timeout_secs` (`cru status` prints it) and `lazy_agent_selection` until someone decides the feature. Document both as reserved. Risk: a user config with `[discovery]` would fail only if `deny_unknown_fields` is set; check. Cost: S.

**B19. `SqliteConfig.pool_size` field (serde, default) and `ClientConfig.max_retries` (serde, no default).**
Recommend: delete `pool_size` (nobody loads the struct from a file); keep `max_retries` until `ClientConfig` gets `#[serde(default)]` (then delete). Cost: S.

**B20. `ExternalChangeTracker::tracked_roots`** (tests observe through it).
Recommend: keep as `#[cfg(test)] pub(crate)`. Cost: S.

**B21. `ServerContext` vs `RpcContext`.** After T1-B5 deletes the dead fields, `ServerContext` holds four fields that `RpcContext` also holds.
Recommend: delete `ServerContext`; `server/core/mod.rs` reads `RpcContext`. Payoff: one context for a new RPC handler. Cost: S.

**B22. `StreamContext` rebuilt field by field although it derives `Clone`** (`stream.rs:1098`).
Recommend: keep the exhaustive literal; it is a compile-time checklist for per-turn reset. Add a comment that says so. Cost: S.

**B23. `InputNode` vs `LayoutContent::Input`** (oil, 13 match sites).
Recommend: keep; documented design (layout strips hints). Cost: none.

**B24. `register_sessions_module` stubs vs `_with_api` names.**
Recommend: one shared `const SESSION_FN_NAMES: &[&str]` both paths iterate; a test asserts the sets agree. Payoff: a new session Lua function cannot land in one path only. Cost: S.

**B25. Ollama `/api/tags` shapes x2** (`model_listing.rs:85`, `embeddings/ollama.rs:43`).
Recommend: move the two structs to `crucible-core/src/config/components/backend.rs` next to `BackendType`; both clients deserialize into them; keep the two `list_models` bodies. Cost: S.

### Band C — design decisions; lower payoff or higher risk

**C1. Truncate helpers (nine) plus oil/CLI `truncate_to_*`.** Three distinct behaviours (byte cap, char cap with ellipsis, width cap). 
Recommend: one `crucible-core::text` module with `truncate_chars(s, n, ellipsis: bool)` and `truncate_bytes`; oil keeps `truncate_to_width` (needs `unicode-width`). Replace the core copies (`workflow/stdlib.rs:74`, `session_event/helpers.rs:97`, `internal.rs:704 trunc`) first; they are mechanical. The daemon `observe/markdown.rs:353` copy passes `max_len 0` through; decide whether that is a bug. Cost: M.

**C2. Tilde expanders (seven).** T1-B11 and T1-B22 collapse them to `resolve_registration_root` and `expand_tilde`. Remaining decision: one `pub fn expand_tilde` in `crucible-core::config` that both crates call. Cost: S after T1-B11/T1-B22.

**C3. Provider knobs: `ChatConfig` vs `LlmProviderConfig`; five enrichment provider configs; `BackendType` vs `defaults.rs` endpoints; VertexAI disagreement; three provider-to-model tables (`wizard.rs:135`, `init.rs:385`, `DEFAULT_CHAT_MODEL`); `ollama_endpoint` x2; `provider_type_label` vs `keyed_backend_display_name`; `ProviderInfo` vs `DetectedProvider`; `discover_env_providers` vs `detect_providers_inner`.**
Options: (a) one `ProviderDefaults` table in `crucible-core/src/config/components/backend.rs` keyed by `BackendType` (endpoint, default model, label, env var) that config, wizard, init, model listing and detection all read; (b) leave.
Recommend (a). Payoff: a new provider is one row; today it is eight edits across three crates. Risk: the VertexAI endpoint must be decided (`backend.rs:130` vs `enrichment.rs:382`); wizard defaults for anthropic and openai change to match `DEFAULT_CHAT_MODEL`. Cost: L.

**C4. Theme colour parsers.** T1-B23 collapses two exact pairs. `parse_adaptive_color` vs `color_from_lua` and `adaptive_from_wire` vs `color_from_wire` differ in the String arm (palette names).
Recommend: share the Integer/Table/Object arms through one helper; keep two String arms. `BorderStyle` vs `ui_geometry::border_from_name` vs oil `Border`: map `BorderStyle` onto oil `Border` and delete `border_from_name` once `Ascii` has an oil mapping. Cost: M.

**C5. `SessionAgent` literals x3** (`agent.rs:335` canonical, `create.rs:454`, `acp.rs:527`).
Recommend: both callers call `internal_from_config` then apply overrides. Payoff: a new agent field defaults in one place. Risk: `create.rs:471-511` override order must stay. Cost: S.

**C6. Tool definition shapes.** `ToolDefinition` (canonical), `ToolSchema` (`tool_discovery.rs:50`), `discovery_tools` vs `bridge_tool_defs` (drifted), `tool_ref_from_definition` / `build_tool_discovery` / `mcp_tool_from_plugin` (three conversions), `KILN_BACKED_TOOLS` vs `BuiltinTool`, CLI `BUILTIN_TOOLS` (stale), `LuaTool`/`DiscoveredTool`, `ToolParam`/`DiscoveredParam`.
Recommend: (1) make `needs_kiln()` a method on `BuiltinTool` and delete `KILN_BACKED_TOOLS`; (2) delete CLI `BUILTIN_TOOLS` and query the daemon; (3) one `fn tool_definition_for_discovery()` that both `discovery_tools` and `bridge_tool_defs` call; (4) leave `LuaTool` until Lua tool discovery is settled. Payoff: a new tool is one `BuiltinTool` variant (the closed-set gate already exists) plus one executor arm. Risk: `discovery_tools` output is served as MCP `tools/list`; pin it. Cost: M.

**C7. Agent card directories** (`card_directories` vs `collect_agent_directories`, already diverged).
Recommend: daemon honours `config.agent_directories`; CLI calls the daemon list through RPC. Payoff: one search path. Risk: CLI behaviour change before a daemon runs. Cost: S.

**C8. `daemon_plugin_paths` vs `PluginManager::with_standard_paths`.** `crucible-lua` cannot depend on the daemon.
Recommend: move the list to `crucible-core::paths` and both read it. Cost: S.

**C9. Three bash allowlists (`BashPatterns.allowed_prefixes`, `PermissionConfig.allow`, `ShellPolicy.whitelist`) run in series; `is_hardcoded_denied` overlaps `ShellPolicy::default_blacklist`.**
Recommend: document the layer order in `docs/Meta/`; do not merge. The layers have different override semantics. Cost: S (doc). Done: [[Bash Permission Layers]].

**C10. `MockEmbeddingProvider` x3, `MockKnowledgeRepository` x3.**
Recommend: `test_support` copies become canonical; delete `enrichment/service.rs:446` and `multi_kiln_search.rs:109` after adding scripted results to the canonical ones. (The third copy, once at `agent_manager/tests/mod.rs:242`, is already gone; `rg -w MockEmbeddingProvider` finds only `test_support.rs:101` and `llm/embeddings/mock.rs`.) `mock.rs:13` stays (production config can select it). Cost: M.

**C11. `StatusBar` / `StatusComponent` / per-frame clones; `ShellHistoryItem` vs `CachedShellExecution`; `prettify_tool_args` vs `format_tool_args`; `table::wrap_text` vs `wrap_words`; `CellGrid::blit_line` vs `parse_line_to_cells`; `ansi.rs skip_until_st_or_bel` vs the OSC skip.** All change visible TUI output or lifetimes.
Recommend: defer; each needs a snapshot review. Cost: M each.

**C12. `ExecuteParams` / `RunInteractiveChatParams` / `RunOneshotChatParams` (13 copied fields).**
Recommend: one `ChatParams` struct plus a `mode` enum. Payoff: a new chat flag is one field. Risk: `execute` destructuring rewrite. Cost: M.

**C13. `McpServerDisplay` vs `McpServerInfo`; `RenderState` vs `ViewContext`; `ToolSourceDisplay` vs `ToolSource`.**
Recommend: keep the display projections; add `From` impls where missing. Cost: S.

**C14. `review/git.rs git()` vs `workspace_snapshot.rs run_git()`; `copy_dir` vs `copy_dir_recursive`; three markdown walkers; `cosine_similarity` x2; `parse_yaml_frontmatter` + `extract_content_without_frontmatter` vs core `extract_frontmatter`; five frontmatter splitters.**
Recommend: one `run_git(args, index_file: Option)` in `scm.rs`; leave the rest until the parser exposes one `split_frontmatter(&str) -> (Option<&str>, &str)` that all five call. Cost: M.

**C15. `LogEvent::Bash*` vs `bash_job_*`; `LogEvent::Subagent*` vs `InternalSessionEvent::Subagent*`.** Covered by B7.

**C16. Private-file writers x3** (`api_key.rs:73`, `credentials.rs:180`, `webhook/mod.rs:415`). Two set mode only at creation.
Recommend: one `crucible_core::fs::write_private(path, bytes)` that always sets `0o600`. Payoff: security fix. Risk: none. Cost: S.

**C17. `PermissionHook` vs `RuntimeHandler`; `retain_other_owners` vs `clear_plugin_auth_hooks`.**
Recommend: defer; first-match-wins semantics differ. Cost: M.

**C18. `SessionEvent::SessionEnded` vs `TurnPayload::Ended`, `PostLlmCall` x2, `Interaction*` x2.** Covered by B7.

**C19. Web `map_grep_err` vs `daemon_err` (400 vs 422); `enclosing_root` vs `resolve_enclosing_root`; `PrecognitionNote` vs `PrecognitionNoteInfo`.** Under `routes/` or SSE wire.
Recommend: defer to a web session. Cost: S each.

**C20. Four hand-kept REPL command lists (`KNOWN_REPL_COMMANDS`, popup, `:pick`, `help_text`), already out of sync.**
Recommend: one enumerated `ReplCommand` table with `strum::EnumIter` and a test that derives the help text from it (the `surface.rs` pattern). Payoff: a new REPL command is one row. Cost: S.

**C21. `ConfigValue::try_parse_bool` vs `set.rs parse_bool`** (token sets differ).
Recommend: `set.rs` calls `try_parse_bool`; accept y/n in `cru set`. Cost: S.

**C22. `AgentCardFrontmatter` vs `AgentCard`; `FileState` x2; `FileChangeKind` vs `FileEventKind`; `TurnError` vs `AgentError`; `TaskStatus` vs `CheckboxStatus`; `ComputedLayout` vs `Rect`; `ThemeLayout` vs `UiLayout`.**
Recommend: keep; each pair is a documented raw-vs-resolved or wire-vs-internal split. Delete `ComputedLayout` (no caller). Cost: S.

**C23. `DebounceConfig` vs `Debouncer` vs `WatchManagerConfig.debounce_delay`; `DebounceConfig` never reaches `Debouncer`.**
Recommend: pass `DebounceConfig` into `Debouncer::new`; delete `debounce_delay`. Cost: S.

**C24. `PollingWatcher` vs `EditorWatcher` `unwatch`/`active_watches` identical bodies.** Goes with B3 (enum backend). Cost: S after B3.

**C25. `observe/markdown.rs render_to_markdown` vs `serde_md::to_string`.** Covered by B17.

**C26. `SqlitePropertyStore`** is in T1-B4 (duplicate audit found it safe; the dead-code skeptic wanted the test rewrite named). The rewrite is in T1-B4.

**C27. `session_bridge.rs CommentSpec::parse` vs `ReviewCommentRequest`; `parse_range` vs `context_ops::Range`.**
Recommend: derive `Deserialize` on `Range` with `serde(tag = "type")` and delete `parse_range`; `CommentSpec` becomes `ReviewCommentRequest` with `#[serde(default)]` on author. Cost: S.

**C28. "Session VM under lock, then plugin VM" two-pass loop x9.**
Recommend: one `async fn for_each_vm(session, plugins, f)` helper in `crucible-lua`. Cost: M.

**C29. `acp/mod.rs` re-exports of `agent_client_protocol` message types** (unverified use).
Recommend: Tier 4 check first.

## 5. Tier 4 — unverified


### 5.0 Result, 2026-08-23

A second skeptic pass ran over the 70 items with a batch of seven per agent.
Verdicts: 45 dead (29 delete, 16 narrow to `cfg(test)`), 25 keep. Two
commits applied the 45: `0f623f215` (crucible-cli) and `3d109b106` (the rest),
41 files, −1,614 lines. `just ci` passed after.

Items the pass kept, with the reason:

- [narrow-to-cfg(test)] CliStorageHandle::list_notes (crates/crucible-cli/src/factories/storage.rs:32) — Only the integration test file uses it. An integration test under tests/ cannot see a cfg(test) item, so narrowing means: either keep it, or rewrite those tests to call as_daemon_client() / note_store
- [keep] SyntaxHighlighter::supports_language (crates/crucible-cli/src/formatting/syntax.rs:183) — Live in the markdown code block renderer and the diff view.
- [keep] PluginManager::error_log (lifecycle/error_log.rs:67) — Already narrowed to test/test-utils. The underlying capture_plugin_error and the log field are production. Nothing to delete unless the tests go.
- [keep] PluginManager::active_plugins (lifecycle/queries.rs:14) — Already gated to tests. Deleting requires rewriting loading.rs:189 to filter PluginManager::list() by state; low value.
- [narrow-to-cfg(test)] PluginErrorLog::clear, is_empty (lifecycle/error_log.rs:55) — clear is test-only; narrow it to #[cfg(test)]. Keep is_empty: pub len() without is_empty trips clippy::len_without_is_empty, which just ci lints.
- [keep] load_plugin_spec_from_source (lifecycle/spec.rs:136) — The claim is wrong. Only the pub(crate) re-export in lifecycle/mod.rs:30 is test-only; it could become #[cfg(test)] but that is trivial.
- [keep] PluginSpec.handlers, DiscoveredHandler (lifecycle/spec.rs:23) — Handlers are parsed but never dispatched; the daemon warns about this on purpose. The count appears in an RPC response (server/plugins.rs:55), so removal changes a wire payload.
- [keep] cru.tbl_get, cru.tbl_deep_extend, cru.on_error (lua_stdlib/qol.rs:123) — These are documented public Lua API for user plugins, not internal code. No bundled runtime plugin uses them. cru.on_error is documented as a reserved slot that nothing invokes; deleting it means remo
- [keep] compile_fennel (fennel.rs:105) — The claim is wrong. The function is the only Fennel compile path for spec extraction and discovery.
- [keep] McpGatewayManager::upstream_status tools/mcp_gateway.rs:454 (weak: own tests only) — The item is already gone. Nothing to remove. Strike the claim from the plan.
- [keep-protected-path] RpcMethod::SessionReindex rpc/dispatch.rs:175 (weak; protected path; retired name in METHODS) — Referenced by a handler arm, a CLI test and the changelog. Protected path rpc/dispatch.rs. Removal is a wire change (METHODS list).
- [keep] PluginManager::eval_runtime (crates/crucible-lua/src/lifecycle/lua_integration.rs:54) — Already test-gated; nothing further to narrow. Tests that use it verify reload and load/unload hooks, not only eval_runtime itself.
- [keep] PluginManager::enable (crates/crucible-lua/src/lifecycle/loading.rs:216) — Already test-gated; no change needed.
- [keep] PluginManager::initialize (crates/crucible-lua/src/lifecycle/mod.rs:136) — Method no longer exists; it was replaced by discover_only (lifecycle/mod.rs:128) and load_all (loading.rs:93). Nothing to remove. Optionally reword the four stale doc comments that still reference ini
- [keep-protected-path] KeepAlive.shell Some path (crucible-web routes/terminal.rs:56) — The field is the test injection seam the doc comment at terminal.rs:49-51 describes; the production const sets None on purpose. Under routes/. Keep.
- [delete] NodeSpec, spec_to_node, NodeSpecError, NodeAttrs, parse_* (crucible-oil template/node_spec.rs) — Partial. NodeSpec, NodeAttrs, spec_to_node and every parse_* except parse_color/parse_hex_color/parse_rgb_color are dead. NodeSpecError and NodeSpecResult stay because parse_color returns them. Move p
- [keep] 30 of 38 `InternalSessionEvent` variants — done in Tier 3 B7 — Already done. 38 - 31 = 7 matches the current enum. No further action.
- [keep] 8 of 14 `SessionEvent` variants — done in Tier 3 B7 — Already done. 14 - 11 = 3 versus 4 present; one variant is `Internal(Box<InternalSessionEvent>)`, the wrapper. No further action.
- [keep] `SessionState::Compacting` (session/types/enums.rs:89) — Not dead: it is written, parsed and shown. It is a design gap (Gaps.md G24, Product.md 'Session Compaction'), not dead code. Removal changes the `session.list` wire value and needs the G24 plan.
- [keep] `StopReason::MaxToolDepth` (turn/mod.rs:166) — Never built, so dead by the rule. Gaps.md G28 plans to build it at the depth cap ('code-wrong'). Keep if the G28 fix lands; delete if the team drops G28. Update the translate.rs:78 comment either way.
- [keep] EventRing read side (get, range, iter, oldest_sequence, newest_sequence, write_sequence, len, is_emp — The claim bundles a live type with dead methods. AgentEventBridge is live: keep. The nine read methods plus the whole overflow/flush API (set_overflow_callback, clear_overflow_callback, set_overflow_b
- [keep] InputType, TerminalStream (events/session_event/types.rs:113) — The items do not exist in the tree; the claim is stale. Nothing to remove.
- [keep] SessionEventConfig, NotePayload builders (events/session_event/payloads.rs:31) — The file and the items do not exist; the claim is stale. Nothing to remove.
- [keep] Format::name (crates/crucible-lua/src/json_query.rs:76) — Live: it is the return value of the Lua `oq.detect(str)` binding.
- [keep] `LayoutEngine::compute`, `ComputedLayout` `taffy_layout.rs` — Already resolved. Gaps.md G153 records `ComputedLayout` deleted in 5fb48681d/2d01c51d3. Nothing left to remove; the open checkbox in Consolidation Plan.md:889 is stale. Docs Actual.md:1026 and Consoli

### 5.1 Second skeptic pass checklist (done; kept for the record)

For each item: run `rg -nw <name>` over `crates/ runtime/ docs/ scripts/ examples/` for `.rs .lua .fnl .ts .tsx .json .toml .md`. Then open every hit. Record: definition only / test only / production. Items already ruled on elsewhere in this plan are not repeated here.

**crucible-daemon**
- [ ] `acp/mod.rs:9` re-exports of `agent_client_protocol` message types
- [ ] `McpGatewayManager::upstream_status` `tools/mcp_gateway.rs:454` (weak: own tests only)
- [ ] `RpcMethod::SessionReindex` `rpc/dispatch.rs:175` (weak; protected path; retired name in `METHODS`)

**crucible-cli**
- [ ] `CliConfigBuilder` `config.rs:24`
- [ ] `AgentInitParams::with_provider` `factories/agent.rs:94`
- [ ] `AgentInitParams.read_only`, `.max_context_tokens` `factories/agent.rs:36`
- [ ] `CliStorageHandle::query_raw` `factories/storage.rs:22`
- [ ] `CliStorageHandle::list_notes` `factories/storage.rs:33`
- [ ] `CliStorageHandle::as_knowledge_repository` `factories/storage.rs:50`
- [ ] `SyntaxHighlighter::supports_language` `formatting/syntax.rs:178`
- [ ] `impl Default for McpArgs` `commands/mcp.rs:46`
- [x] `RunOneshotChatParams.initial_mode` `commands/chat/mod.rs:68` — removed by T3-C12
- [x] `RunInteractiveChatParams.replay`, `.replay_speed`, `.replay_auto_exit` `commands/chat/mod.rs:59` — removed by T3-C12
- [ ] `generate_initial_config` `embedding_provider` parameter `commands/wizard.rs:135`

**crucible-lua**
- [ ] `LuaExecutor::execute_file`, `execute_source`, `execute_tool` `executor.rs:306`
- [ ] `fennel_available`, `session_start_hooks` `executor.rs:78`
- [ ] `FunctionSignature`, `TypedParam`, `LuauType`, `type_to_string` `schema.rs:31`
- [ ] `PluginManager::eval_runtime` `lifecycle/lua_integration.rs:53`
- [ ] `PluginManager::reload` `lifecycle/loading.rs:198`
- [ ] `PluginManager::enable` `lifecycle/loading.rs:219`
- [ ] `PluginManager::initialize` `lifecycle/mod.rs:136`
- [ ] `PluginManager::error_log` `lifecycle/error_log.rs:67`
- [ ] `PluginManager::active_plugins` `lifecycle/queries.rs:14` (weak)
- [ ] `PluginErrorLog::clear`, `is_empty` `lifecycle/error_log.rs:55`
- [ ] `load_plugin_spec_from_source` `lifecycle/spec.rs:136`
- [ ] `PluginSpec.handlers`, `DiscoveredHandler` `lifecycle/spec.rs:23`
- [ ] `cru.tbl_get`, `cru.tbl_deep_extend`, `cru.on_error` `lua_stdlib/qol.rs:123` (grep `runtime/` Lua and Fennel)
- [ ] `compile_fennel` `fennel.rs:105`
- [ ] `get_pending_notifications`, `get_messages_action` `notify.rs:241`, `:262`
- [ ] `ModeRegistry::is_empty` `modes.rs:183`
- [ ] `StatuslineExprRegistry::forget` `statusline_exprs.rs:188`
- [ ] `ThemeSpinnerStyle::frames` `theme.rs:315`
- [ ] `Capability::all`, `Capability::description` `manifest.rs:106`, `:121`
- [ ] `Format::name` `json_query.rs:76`

**crucible-core events**
- [ ] `EventRing` overflow API (`set_overflow_callback`, `clear_overflow_callback`, `set_overflow_batch_size`, `overflow_batch_size`, `flushed_sequence`, ...) `events/ring.rs`
- [ ] `EventRing` read side (`get`, `range`, `iter`, `oldest_sequence`, `newest_sequence`, `write_sequence`, `len`, `is_empty`, `capacity`) and `AgentEventBridge`
- [ ] `SessionEvent::identifier`, `priority`, `payload`, `estimate_tokens`, `category` `events/session_event/mod.rs:366`
- [ ] `InternalSessionEvent::identifier`, `priority`, `estimate_content_len`, `payload_content` `internal.rs:502` (weak)
- [ ] `helpers::identifier_for_event`, `payload_for_event`, `estimate_content_len` `events/session_event/helpers.rs:10`
- [ ] `EventCategory` `events/session_event/types.rs:262`
- [ ] `InputType`, `TerminalStream` `events/session_event/types.rs:113`
- [ ] `SessionEventConfig`, `NotePayload` builders `events/session_event/payloads.rs:31`
- [x] 30 of 38 `InternalSessionEvent` variants (list in plan_data) — done in Tier 3 B7 (31 removed)
- [x] 8 of 14 `SessionEvent` variants — done in Tier 3 B7 (11 removed)

**crucible-core other**
- [ ] `Session::is_granular`, `recording_jsonl_path`, `artifacts_path`, `can_access_kiln` `session/types/session.rs:398`
- [ ] `SessionState::Compacting` `session/types/enums.rs:90`
- [ ] `AgentError::AgentUnavailable`, `Internal` `turn/mod.rs:228`
- [ ] `StopReason::MaxToolDepth` `turn/mod.rs:169` (weak)
- [ ] `BoxAgent` `turn/mod.rs:374` (weak; two skeptic notes disagree)
- [ ] `ConversationTree::fanout`, `collect`, `common_ancestor`, `add_child_with_meta`, `NodeContent::Marker`, `NodeMeta` `turn/tree.rs:251`
- [ ] `PanelAction` `interaction/types.rs:304`
- [ ] `InteractionRequest::expects_response`, `PermRequest::pattern_at`, `AskBatch::with_id`, `AskBatchResponse::cancelled`, `QuestionAnswer::*` constructors `interaction/`

**crucible-web**
- [ ] `ReconnectingDaemon::capabilities` `services/daemon.rs:241`
- [ ] `ReconnectingDaemon::note_upsert` `services/daemon.rs:324`
- [ ] `ReconnectingDaemon::lua_discover_plugins` `services/daemon.rs:408` (weak)
- [ ] `ReconnectingDaemon::lua_plugin_health` `services/daemon.rs:418`
- [ ] `ReconnectingDaemon::session_create` `services/daemon.rs:477`
- [ ] `ReconnectingDaemon::agents_resolve_profile` `services/daemon.rs:902`
- [ ] `ChatEvent::{ToolResultDelta, ToolResultComplete, SubagentSpawned, SubagentCompleted, SubagentFailed, ContextUsage}` `events.rs` (SSE wire; grep `web/src/`)
- [ ] `KeepAlive.shell` Some path `routes/terminal.rs:56` (protected path)

**crucible-oil**
- [ ] `Terminal::cursor_style` `terminal.rs:173`
- [ ] `bounded`, `bounded_head` `bounded.rs:43`
- [ ] `NodeSpec`, `spec_to_node`, `NodeSpecError`, `NodeAttrs`, `parse_*` `template/node_spec.rs:7`
- [x] `LayoutEngine::compute`, `ComputedLayout` `taffy_layout.rs:40` — `ComputedLayout` removed by T2-B3 and T3-C22; `LayoutEngine` stays (section 5a)
- [ ] `CellGrid::blit_string`, `to_lines`, `get`, `set` `cell_grid.rs:175`
- [ ] `PopupOverlay::move_selection_up_wrap`, `move_selection_down_wrap`, `selected_label` `components/popup.rs:100`
- [ ] `InputNode::placeholder`, `focused`; `TextNode::{fg, bg, bold, dim}` `node.rs:510`
- [ ] `Padding::horizontal`, `vertical` `style.rs:389`
- [ ] `LayoutTree::empty`, `LayoutBox::empty` `layout/types.rs:37`
- [ ] `overlay_from_bottom_right`, `OverlayAnchor::FromBottomRight` `node.rs:265` (weak)

### 5.2 Refuted dead-code claims (alive; do not re-raise)

- `session_id_field` — `server/observe.rs`, `session/lifecycle.rs` call it.
- `default_socket_path` — `rpc_client/lifecycle.rs:97`, `:103` production.
- `VersionCheck::is_match` — client tests call it.
- `ClientId::as_u64` — own tests.
- `DeferredShutdown::subscribe` — dispatch and core tests.
- `RpcMethod::SessionReindex` — retired name kept in `METHODS` on purpose.
- `FtsIndex::is_empty` — kiln_manager tests (now merged in T1-B4).
- `handle_lua_discover_plugins` `kiln_path` — serde field on a wire type.
- `InProcessMcpHost::{address, shutdown}` — acp integration tests.
- `llm/model_discovery.rs` — `examples/llm_discover_models.rs`.
- `EmbeddingResponse::{is_compatible_dimensions, with_metadata, cosine_similarity}` — provider tests.
- `ModelFamily`, `ParameterSize`, `ModelInfoBuilder` — `fastembed.rs`.
- `default_daemon_plugin_paths` — daemon_plugins tests.
- `McpGatewayManager::upstream_status` — own tests.
- `McpGatewayManager::reconnect`, `upstreams_needing_reconnect` — `start_reconnect_loop` (Tier 3 A10).
- `PerformanceMonitor` / `PerformanceStats` — `manager.rs:130`, `:593` write side.
- `ParsedNote::legacy`, block-hash helpers — test modules.
- `MockAgent` / `MockAgentConfig` — own tests under `test-utils`.
- `StreamConfig`, `StreamHandler` — `concurrent_sessions.rs`.
- `ToolDescriptor`, `ToolRegistry`, `discover_tools`, `AcpToolExecutor` — `acp_integration_e2e.rs` (shape merge in T1-B8).
- `discover_agent`, `discover_agent_uncached` — CLI and tests.
- `resolve_agent_from_config` — own tests.
- `reset_agent_cache` — own and integration tests.
- `CrucibleAcpClient::{connect, disconnect, is_connected, send_message, agent_supports_sse_mcp, config, active_session}` — tests.
- `send_prompt_with_streaming` — see Tier 3 B8.
- `TransportConfig` — `session.rs:65` takes it.
- `KilnRegistry::eager` — registry tests.
- `SnapshotMap::len` — messaging tests.
- `ListBlock::stats`, `ListStats`, `CheckboxStatus::to_char` — `commands/tasks.rs:95`.
- `Blockquote::new` — enrichment tests.
- `ValueSourceMap::to_serializable`, `has_source`, `values_from_source`, `ValueInfo` — own tests.
- `ValueSource::Environment`, `Profile`, `Included` — matched in `short()`/`detail()`.
- `CliAppConfig::database_path_str` — config tests.
- `PipelineConfig::optimize_for_*` — own tests.
- `LlmConfig::all_provider_models` — own tests.
- `DiscoveryPathsConfig`, `TypeDiscoveryConfig`, `get_type_config*` — own tests.
- `PatternStore::load`, `save` — doctest.
- `AcpConfig::lazy_agent_selection` — serde field with default.
- `StorageConfig::idle_timeout_secs` — `cru status` prints it.
- `Skill.content_hash`, `indexed_at`, `compatibility`, `metadata` — tests read them.
- `LogEvent` constructors — `session/messaging.rs:119-121` use three.
- `observe/serde_md.rs` — `observe_e2e.rs` (Tier 3 B17).
- `observe/indexer.rs` — CLI command uses it.
- `ExtensionRegistry::{unregister, get, all_extensions, apply_extensions, stats}` — own tests.
- `ParserRequirements`, `ParserCapabilities::supports_all` — own test; `crucible_kiln()` used.
- `ParserError::{unsupported, is_recoverable, is_fatal}` and variants — own tests.
- `Wikilink::embed`, `display`; `FootnoteMap::get_definition`; `InlineLink::{is_external, is_relative}`; `Tag::*` — footnotes tests.
- `KilnAttachment::effective_classification` — own tests.
- `LlmProviderConfigBuilder` — `agent_factory.rs:501`.
- `require_test_env`, `test_env` — `llm_backend_comparison.rs`.
- `hermetic_env_pairs` — CLI e2e helpers.
- `NoteContent::{add_heading, add_code_block, outline}` — block_extractor tests.
- `ASTBlock::{with_hash, length, content_length, is_heading_level, is_code_language, is_callout_type, with_parent}` — `parser/types/mod.rs` tests.
- `BlockHash::as_bytes` — `note_store.rs:393` production.
- `CrucibleParser::with_*` — implementation tests.
- `FrontmatterExtractor::with_config`, `FrontmatterExtractorConfig`, `FrontmatterResult` fields — `new()` calls it.
- `TaskItem::with_id`, `TaskFile::system_prompt`, `TaskGraph::dependencies_of` — task tests.
- `InlineMetadata::{new, new_array, is_array}` — tests.
- `test_utils::parse_note` — `tests/dev_kiln.rs` (gate it in Tier 3 A6).
- `KilnFixture`, `create_kiln` — `process_command_tests.rs`.
- `ToolExample` — serde field on `ToolDefinition`.
- `with_session_path` / `JobInfo.session_path` — `delegation.rs:495` sets it.
- `StatusBar::emergency_view` — component isolation tests.
- `markdown_to_node`, `markdown_to_node_with_widths` — markdown tests and fuzz tests.
- `wrap_to_width_indented` — doctest and unit tests.
- `truncate_lines` — doctest, unit tests, runtime plugins.
- `terminal_height`, `terminal_size` — unit tests.
- `CachedToolCall::render_compact`, `render_compact_with_frame` — `tool_render_tests.rs`.
- `format_tool_args` — `tool_render_tests.rs`.
- `NotificationArea::{show, hide, is_visible, history}` — `chat_app/mod.rs:205`.
- `RenderStyle::natural` — markdown tests.
- `DiffOptions::layout` Some path — diff_view tests.
- `EventError` — `EmitResult` in the `EventEmitter` signature.
- `EmitOutcome::with_errors`, `cancelled`, `HandlerErrorInfo` — `MockEventEmitter`.
- `EventEmitter::emit_recursive`, `is_available` — overridden in `DaemonEventBridge` and the mock.
- `ToolCall::default`, `SessionEvent::default` — `SessionEvent::default()` called in core events.
- `ScriptingEvent::ALL`, `TurnPayload::as_scripting_event` — protocol tests and the `ALL` completeness proof.
- `ToolResultBody::error` — `chat_runner/commands.rs:335`.

### 5.3 Refuted duplicate claims (distinct; do not re-raise)

Same name, different type: `Session` x3 · `ToolOutput` x2 · `SearchResult` (database vs note_store) · `McpServerInfo` x2 · `Node` (canvas vs oil) · `Color` (canvas vs oil) · `Op` x2 · `RenderContext` x2 · `SetError` x2 · `ValidationResult` x2 · `SessionError` x2 · `ParseError` x2 · `ToolCall` (events vs llm; see Tier 3 B10) · `AgentError` x2 · `Direction` x2 · `SessionEvent` (client vs core; see Tier 3 B9) · `ModelsResponse` x2 · `DelegationSpawned` x2 · `ToolSchema` vs `McpToolInfo` · `SearchResultWithScore` vs `SearchResult` · `lua_to_json` x2 · `JobStats` vs `JobInfo` · `SessionDefaultValues` vs agent config · `StoredOption` / `PluginOptionCallRequest` / `OptionRequest` · `SkillFrontmatter` vs `Skill` · `AgentInfo` vs `AgentProfile` · `ClientError` vs `AcpError` · `BackendCapabilities` vs `WatcherRequirements` · `FileEventKind` vs `FileChangeKind` · `FileEventKind` vs web `FsEvent` · `ShellPolicy` vs `PluginShellPolicy` · `DaemonAgentHandle.cached_*` vs `SessionAgent`.

Already a re-export, not a copy: `PluginStatusEntry` · `Drawer` · `TokenUsage` · `ProviderInfo` · `BackendType`.

Different behaviour, not a copy: `discover_env_providers` vs `detect_providers_inner` · `PendingPermission` vs `PendingInteraction` · `keep_ref` / `update_keep` / `drop_keep` vs workspace_snapshot · `validate_grep_root` vs `resolve_root` · `LuaSessionState` construction x2 · `DEPTH_CAP_PROMPT` vs `TOOL_DEPTH_LIMIT_FINAL_PROMPT` (the second does not exist) · 15 `SessionSet*Request` structs (each is a distinct wire shape) · `resolve_agent_trust` forwarding wrapper · `scoped_neighbors` (no `graph.rs` exists) · `handle_session_event` vs `session_event_to_chat_msgs` · `validate_note_name` vs `reject_path_traversal`.

Near-duplicate, kept on purpose: five frontmatter splitters (Tier 3 C14) · `SerializableMetadata` (adds a serde tag) · inline-metadata regex compiled five times (performance item in Actual.md section 9, not a duplicate) · five `CallToolResult` conversions (Tier 3 C6).

## 5a. Tier 5 — follow-ups from Tier 3

The Tier 3 agents noted 160 follow-ups in their journals. This section keeps
one line per distinct item, grouped by crate. The tag in brackets names the
entry that found it. None of these is committed. Doc-only notes are in the
last group.

### crucible-core

- `traits/chat.rs`: `clear_history` still defaults to `Ok(())`; make it required. `GenaiAgentHandle` holds a `thinking_budget` field but its `SessionKnobs` answers `NotSupported`; `Genai` and `Acp` handles answer empty for `max_iterations`, `execution_timeout` and precognition because the daemon session owns them. [A1]
- `storage/traits.rs`: the `StorageClient` doc names `DirectStorageClient` and `crucible-rpc`, which do not exist; `MockStorageClient` under `test-utils` has no caller. [A4, A6]
- `types/hashing.rs`: `FileHashInfo`, `BlockHashInfo`, `HashAlgorithm` and the `FileHash` alias have no callers outside re-exports; delete the file (Gaps G53). [B14]
- `types/acp.rs`: `ToolCallInfo` is still re-exported from `acp/streaming.rs`; `FileDiff` is live in `TurnEvent::ToolCall`, so re-check B11's premise before deleting the family; `test_tool_definition_from_traits` belongs in `traits/tools.rs`. [B10, B11]
- `events/session_event/`: done in T5-04. `identifier`, `priority`, `category`, `estimate_tokens`, `payload`, `Priority`, `EventCategory` and the manual `Deserialize` had no production reader; T5-04 deleted them. `MessageReceived` stays: the CLI `EventRing` push produces it. `ScriptingEvent` still names seven events with no `SessionEvent` variant. B7 option (b) stays open as a plan note: make `SessionEvent` a projection of the wire enum, so `SessionEventMessage` is the one producer and the scripting names come from `TurnPayload::as_scripting_event`. [B7]
- `events/emitter.rs`: the `Event` doc says the type is `SessionEvent` from `crucible-lua`; verify against `DaemonEventBridge`. The trait doc example is not a doctest. [B5]
- `parser/extensions.rs`: replace the u8 `priority` with variant order and delete the sort; `BasicMarkdownItExtension::parse` uses `eprintln!` on a markdown-it panic. `ParserCapabilities::supports_all` and `ParserRequirements` have no production caller. `ASTBlockType::HorizontalRule` may have no producer. [B1, B16]
- `parser/types/parsed_note.rs`: `NoteContent` still carries the six list fields with serde; `ParsedNote::legacy` and several `with_*` and `has_*` helpers look unused. [B15]
- `config/credentials.rs`: `CredentialSource::Store` displays as "file" (printed by `cru auth`); `resolve_copilot_oauth_token` builds its own `SecretsFile` and reads the real config dir in tests. [B2]
- `config/components/backend.rs`: `DEFAULT_ANTHROPIC_MODEL` is `claude-3-5-sonnet-20241022`, older than the value the wizard wrote before; enrichment `MockConfig::default_model` disagrees with the table; `default_max_concurrent` is the one property still in a `match`. `ProviderInfo` versus `DetectedProvider` and `discover_env_providers` versus `detect_providers_inner` still duplicate the credential scan. [C3]
- `config/components/discovery.rs`: `DiscoveryPathsConfig` and `TypeDiscoveryConfig` under `[discovery]` were not checked for readers. [B18]
- `config/patterns.rs:289`: `PatternStore::matches_bash` matches a prefix on the whole command and does not split chained statements, so a saved `git ` allows `git log; curl evil`. Needs a failing test first. [C9]
- `config/cli_app.rs`: the `agent_directories` example could say the daemon honours it. [C7]
- `text.rs`: daemon `background/types.rs::truncate` and CLI `commands/session/helpers.rs::truncate` cap with ASCII `...`; they can move to `truncate_chars` once the `…` output change is accepted. [C1]
- `Cargo.toml`: `rust-version = 1.75` is stale; the tree uses `LazyLock`. [C28]

### crucible-daemon

- `tools/mcp_server.rs:695`, `tool_dispatch.rs:553`: a job cancel over MCP or RPC does not check session ownership; if cross-session cancel must be denied, the gate belongs in the caller. [A9]
- `agent_manager/messaging/permission.rs`: the perm-id, oneshot, `PendingPermission`, `insert_permission` sequence is written twice plus a test helper; a `SessionSlot::register_permission` would fold them. A `User` scope grant from the daemon path is not persisted (only `Project`). `load_sync` and `save_sync` still block inside the async gate (Gaps G18). [A8, C9]
- `server/lua.rs`, `session_lifecycle.rs`: done in T5-15. The `NoopSessionRpc` alias is gone; both sites bind `UnsupportedSessionRpc` by name. `session:set_variable` and `get_variable` store on `SessionSlot` through `crucible_lua::SessionVariables`, and `Session.variables` persists the map in `meta.json`. The plugin-loader sessions (`session_lifecycle.rs`) still report variables as unsupported. [A2, A5]
- `rpc_client/storage.rs`: `DaemonNoteStore::get_by_hash` and `content_hash` scan `list`; `NoteRecordDto` builds `ParsedNote` with placeholder spans and the `get_note_by_name` reply has no `wikilinks` key, so the client always sees an empty list; a `pub use` of `FtsResult` from `rpc_client` would spare clients the `storage::sqlite` path. [A4, B12, B15]
- `tools/mcp_gateway.rs`: nothing detects a mid-session disconnect (a failed `call_tool` leaves `Connected`), so `auto_reconnect` retries only startup failures. `crucible-cli/src/tui/oil/chat_runner/runner.rs` builds a second throwaway gateway for TUI status. [A10]
- `tools/extended_mcp_server.rs`: `tool_count` counts `delegate_session` while `list_tools` filters it without a delegation context. `mcp_server.rs` `McpServerManager::start` hardcodes `EmptyEmbeddingProvider`. [A11]
- `tools/notes/`: done in T5-19. `NoteTools` holds the session's `KnowledgeRepository` as a required field; `read_metadata` and `list_notes` answer from the index row when `get_note_by_path` finds one, and from disk when it does not. [A12]
- `acp/tools.rs`: the test `PermissionedToolBridge` double mirrors no production type. `acp_handle.rs` discards the formatted content from `send_prompt_with_callback`, so `StreamingState::formatted_output` may serve only `streaming_chat.rs` tests. The `StreamingChunk` to `TurnEvent` translation is a total `From` since ACP W4; the client names every `ToolEnd` from its per-turn table. [B8, B11]
- `tool_dispatch.rs`: `DISCOVERY_TOOL_NAMES` is still a hand list (a test ties it to the definitions); `DiscoverToolsParams.source` is a free `String`, a `ToolSourceFilter` enum would make the schema and handler agree; `ToolSchema` versus `ToolDefinition` and the three conversions wait on a `ToolRef` owner. `a_provider_that_never_lists_tools_does_not_hang_has_tool` failed once under parallel load. [C6, B16]
- `watch/`: `WatchConfig.debounce` is set by two callers and read by no backend; `DebounceConfig::with_max_batch_size` has no caller; `Backend::unwatch`, `active_watches`, `capabilities` are test-only; `EditorWatchState` wraps one `PathBuf`; `PollingWatcher::start_polling` holds a dead snapshot; `NotifyWatcher::unwatch` removes by path while the others remove by id; `watch/mod.rs` keeps a module-level `#![allow(clippy::ptr_arg)]`. [B3, C23, C24]
- `server/mod.rs:106`: `Server` keeps its own clones of `subscription_manager` and `event_tx` beside `rpc_context`; `RpcContext::new` takes 16 arguments under an allow. [B21]
- `acp/client/types.rs`: `ClientConfig` lacks `#[serde(default)]`; add it, then delete `max_retries` and shorten 20 test literals. `SqliteConfig` derives serde with no loader. [B19]
- `llm/embeddings/ollama.rs`: `test_list_models_response_deserialization` parses into `Value` with a stale comment; parse into the shared struct. [B25]
- `scm.rs`: `clone_repo` spawns git itself to keep the last 10 stderr lines; four test-only `git(...)` helpers could share `test_support::git`. The rest of C14 (frontmatter splitters, markdown walkers, `cosine_similarity`, `copy_dir`) waits on a parser `split_frontmatter`. [C14]
- `session_bridge.rs`, `rpc_client/`: `DaemonClient::review_comment` takes a `Value` and merges `session_id` itself; `Range` has no `Serialize`. [C27]
- `agent_manager/vm_pass.rs`: `apply_transform_context_handlers` carries an `Option` accumulator with an unreachable `None` arm; the `Vm` label is read by two of nine sites; two structs spell the tuple instead of `PluginHandlers`. [C28]
- `server/session/create.rs`: `build_default_internal_agent` has no test that pins the override order; `cru session configure` has no test. [C5]
- `test_support.rs`: `tests/common/mod.rs` re-exports the canonical mocks and could go; `llm/embeddings/mock.rs::MockEmbeddingProvider` shares the name with the `test_support` one. `server/tests/truncation.rs::truncate_utf8_safe` re-implements `truncate_bytes`. [C10, C1]
- `tests/replay_e2e.rs:73` rebuilds a `SessionEventMessage` to set `msg_type`; mutate in place. [B9]
- `runtime_defaults.rs`, `execution_roots.rs`: could read `crucible_core::paths::env_plugin_paths` directly. [C8]
- `rpc/dispatch.rs`: done in T5-29. `agents.list_cards` answers with the cards `agent_cards::discover_agent_cards_in` resolves for a workspace and kiln path; `dispatch_agents_list_cards_pins_the_card_json` pins the reply. `cru agents list` asks a running daemon first and reads disk only when none answers. [C7]
- Existing `0o600` assertions in `credentials.rs`, `api_key.rs`, `session.rs` and `webhook/tests.rs` now test the shared helper through each caller; they could thin. [C16]

### crucible-cli

- `commands/chat/`: no test covers `ChatMode::from_flags` or the `--replay` exclusivity; `cru chat -q --plan` does not apply plan mode (needs `--mode` plumbing); `--record` with a piped stdin query is silently dropped. [C12]
- `tui/oil/chat_app/command_handling.rs`: the slash-command arms are a fifth hand-kept set; no test proves every `ReplCommand` dispatches (the match has a wildcard arm); `suggest_command` still takes `&[&str]`; the `parse_bool` error says "Use true/false"; line 760 gets y/n with no test. [C20, C21]
- `tui/oil/chat_app/shell.rs`: the toast path branches on `PermissionScope::User`; `write_permission_rule` owns the real path. [B13]
- `commands/session/acp.rs`: raw output uses the key `event_type` while the wire key is `event`. [B9]
- `factories/agent.rs:164`, `trust_resolution.rs:157`: full `SessionAgent` literals for the ACP path. [C5]
- `kiln_validate::expand_tilde` shares the core name; rename to `expand_tilde_home`. `collect_agent_directories` reads `dirs::config_dir()` at call time; a `CardRoots` built once would make its tests hermetic. [C2, C7]
- `commands/doctor.rs` builds the Ollama `/api/tags` URL and does not parse the reply. [B25]
- `tui/oil/mod.rs` re-exports the ungated `crucible_oil::runtime::TestRuntime`. `main.rs:273` uses `OpenOptions` for a non-credential file. `ThemeDecorations.border_style` has no consumer. [A6, C16, C4]

### crucible-lua

- `ui.rs`, `tools_api.rs`: `register_ui_module_with_api` and `register_tools_module_with_api` keep the stub-then-overwrite pattern with two hand lists; apply the B24 constant plus gate. The older `sessions_module_registers_in_namespace` test is redundant. [B24]
- `session_defaults.rs`: `SessionDefaultsRpc` delegates `get_model` and `switch_model` to the unsupported backing; a hook that picks a model needs a `model` field on `SessionDefaultValues`. [A2]
- `sessions/register.rs:839`: no Lua-side test pins the serde error texts for a missing `path`, `body` or `line_start`. [C27]
- `theme_wire::border_style_name` and `ui_geometry::border_to_wire` emit different names for one border (sharp versus single, thick versus heavy); unifying changes a wire payload. [C4]
- `manifest.rs:290`, `lifecycle/mod.rs:103`: doc comments describe the old path copies as history and could name `crucible_core::paths`. `fs.rs:82` uses `OpenOptions` for a non-credential file. [C8, C16]

### crucible-web and crucible-oil

- `crucible-web/src/events.rs`: `ChatEvent::SessionEvent` carries `event_type` in web JSON; a rename needs a pinned test. Its dev-dependency on `crucible-daemon` now pulls `test-utils` from daemon and core. [B9, A7]
- `crucible-oil/src/utils.rs::truncate_to_chars` and `render_helpers.rs::truncate_with_ellipsis` duplicate the char cap; oil has no core dependency. `taffy_layout.rs` `LayoutEngine` keeps an unused `usize` context type. `TestRuntime` is ungated. [C1, C22, A6]

### Docs and this plan

- Stale references after Tier 3: `ContentHasher`, `FileHash`, `SyntaxExtension`, `FileWatcher`, `WatcherFactory`, `serde_md`, `resolve_registration_root`, `KILN_BACKED_TOOLS`, `BUILTIN_TOOLS`, `ComputedLayout`, `SessionCommand` in [[Actual]], [[Gaps]], [[Type Flows]] and `Product.md` (`start_reconnect_loop` has call sites now; `source: index` no longer exists). `7fcd3b9f4` fixed the analysis docs; Gaps.md section 6 records the closed rows.
- This plan: done in T5-38. The Tier 1 batches are `T1-B1` to `T1-B24`, so they no longer collide with Band B; the C10 cite names the two copies that exist; section 5.1 marks the `LayoutEngine::compute`, `ComputedLayout` row done. Line numbers for `types/acp.rs` in B10 and B11 are still stale.
- `docs/Help/Config/storage.md` and `acp.md` describe the two kept fields as unread but do not use the word "reserved"; `Product.md:1106` on `cru init` writing `[storage] backend` is a separate cleanup.

### 5a.0 Result, 2026-08-23

All 38 items ran, one agent each, commits `c9e6ddda1` to `443e1c20c`: 37
committed, one partial (T5-20; the ACP client work W1-W4 closed it on
2026-08-23 — the client emits the resolved name on `ToolEnd`, and the
stateful loop left `acp_handle.rs`; see the Tier 6 entry). Decisions applied: cross-session job cancel is
denied; Lua `session:set_variable` has daemon storage; the raw JSON key
`event_type` is now `event` in the CLI and web surfaces; border names are
canonical `sharp`/`thick` with the old names accepted on read; the ellipsis is
`…`; `DEFAULT_ANTHROPIC_MODEL` is `claude-sonnet-5`. The bash allowlist now
matches every chained statement. `just ci` and the web unit tests pass.

### 5b. Tier 6 — follow-ups from Tier 5

- [T5-01] The DaemonAgentHandle mirror path for thinking_budget (session.set_thinking_budget RPC to the Genai handle) was not traced end to end; verify the daemon forwards the RPC value into GenaiAgentHandle::set_thinking_budget rather than only into AgentConfig.
- [T5-02] docs/Meta/Architecture/Gaps.md and Actual.md still cite types/hashing.rs; a docs pass should mark G53 fully closed (file deleted).
- [T5-02] docs/Meta/Analysis/Systems.md still cites crucible-core/src/hashing/algorithm.rs, which no longer exists.
- [T5-03] types/mod.rs still re-exports the ACP schema types and traits::tools types at the types:: level; a later pass could check which of those re-export paths have callers.
- [T5-04] B7 option (b) stays open: make SessionEvent a projection of the wire enum so ScriptingEvent stops naming seven events with no SessionEvent variant.
- [T5-04] InternalSessionEvent still derives Deserialize; nothing outside tests deserializes it, so a later pass can drop the derive if its serde round-trip test goes too.
- [T5-05] `ExtensionRegistry::register` is now public API whose order depends on the caller; if a second constructor appears, consider deriving Ord on the inner extension structs and inserting by variant order.
- [T5-06] ParsedNote::add_block_hash, block_hash_count, get_merkle_root and clear_hash_data have callers in tests only (and get_merkle_root in implementation.rs tests). A later pass can inline them the same way.
- [T5-07] The daemon still calls SecretsFile::new() (real config dir) in agent_factory.rs; a daemon-level test of the Copilot path would need the store injected one level higher.
- [T5-08] Decide whether the daemon should pass the SecretsFile to discover_credentials so `cru auth set` keys show in providers.list without a config include.
- [T5-08] crucible-lua emits 5 unused-import/dead-code warnings under plain `cargo check` (pre-existing, not hit by clippy -D warnings in this gate).
- [T5-09] The acp.rs and storage.rs tests for lazy_agent_selection and idle_timeout_secs still exist; they test parse only. Remove when someone decides the features.
- [T5-09] Gaps.md G163 may want the T5-09 commit recorded.
- [T5-10] `PermissionEngine` (engine.rs) and `PatternStore` both split bash lines but decide differently on unmodellable constructs (engine falls to the configured default; the store returns false). Consider one shared decision point.
- [T5-11] crates/crucible-daemon/src/tools/mcp_server.rs passes `delegation.result_max_bytes` to a char cap; a multibyte result can exceed the byte budget. Decide whether that field should become a char budget or use `truncate_bytes`.
- [T5-11] crates/crucible-daemon/src/observe/markdown.rs has its own byte `truncate` without a marker; it could use `text::truncate_bytes`.
- [T5-12] Install rustup toolchain 1.94 and run cargo check --workspace --all-targets to confirm the declared MSRV builds.
- [T5-12] Consider a CI job that builds with the declared rust-version so the MSRV does not drift again.
- [T5-13] get_job_result and list_jobs are not gated the same way: get_job_result returns any job by ID regardless of owner (read-only, but it leaks another session's job output).
- [T5-13] The ownership check reads the owner then cancels in two steps; a job that finishes between the two steps is harmless but the trait could take session_id to make ownership a spawner-level invariant.
- [T5-14] `PatternStore::load_sync_in`/`load_user_sync_in`/`save_file` still block inside the async tool gate (Gaps G18).
- [T5-14] The ACP gate (`build_acp_permission_handler`) never persists a Project or User grant; only the internal tool gate does.
- [T5-14] Resolved: the tool gate reads `StreamContext::whitelists_dir`, which the daemon derives from the injected config home. `PatternStore::whitelists_dir()` still reads `dirs::config_dir()` for a caller with no injected home.
- [T5-15] Give the plugin-loader sessions (session_lifecycle.rs fire_session_start/fire_session_end) access to the slot's SessionVariables so plugin hooks share the same store as user hooks.
- [T5-15] crates/crucible-lua/src/executor.rs carry two unused imports (`LuaExecutionResult`, `Instant`) under the daemon's feature set; pre-existing, warnings only.
- [T5-15] docs/Meta/Architecture/Actual.md,855 and docs/Meta/Product.md still mention `NoopSessionRpc`; the Actual/Product doc rewrites are owned by other agents.
- [T5-16] `links_to` and `wikilinks` now duplicate the same data on the wire; a later WIRE item could drop `links_to` after the web reader moves to `wikilinks`.
- [T5-16] `links_to` holds raw link targets (for example `target`), not resolved paths; the client DTO builds Wikilink with placeholder offsets and spans.
- [T5-17] `UpstreamClient::call_tool` keeps the old executor after it marks the upstream Disconnected; `connect()` replaces it on reconnect, but a call in that window still goes to the dead executor.
- [T5-17] The rmcp `McpError` to `McpError::ServerError` mapping changes the error text seen by tool callers from `Execution error:` to `Server error:`; no reader matched on it, but a user-facing message may look different.
- [T5-18] No test covers McpServerManager::start's provider wiring, because start spawns a live stdio/SSE transport; a test would need a transport seam.
- [T5-18] Resolving the embedding provider from enrichment_config is now duplicated in agent_manager/mod.rs, server/kiln.rs, precognition, messaging/send.rs and server/platform.rs; a KilnManager helper could replace the five copies.
- [T5-19] `SqliteKnowledgeRepository::get_note_by_name` is still a substring match that rebuilds a fake frontmatter; callers that want a precise row can move to `get_note_by_path`.
- [T5-19] The disk path of `read_metadata` counts frontmatter words in `word_count`; the index path has no word count at all. A parser `split_frontmatter` (plan C14) would let both report the body count.
- [T5-19] The `get_note_by_name` RPC reply carries no `properties`; if a CLI-side reader ever needs an index row, add it to the wire (WIRE item) and implement `DaemonStorageClient::get_note_by_path` for real.
- [T5-20] Done in ACP W1-W4 (2026-08-23). The per-turn `ToolCallTable` in `acp/client/tool_table.rs` replaced `OrphanedResults`; `StreamingChunk::ToolEnd` carries the name; `acp_handle/translate.rs` has a total `From<StreamingChunk> for TurnEvent`. Deviation: a bare orphaned result is now announced under the placeholder label, as Zed does, instead of dropped. The same work moved the ACP SDK to `agent-client-protocol` 2.0.0 in the daemon and the CLI; `crucible-core` pins `agent-client-protocol-schema` =1.5.0, the same build the SDK pulls, so the dual schema copy is gone.
- [T5-20] cargo check -p crucible-lua prints 4 pre-existing warnings on the clean tree (unused imports and dead methods in executor.rs and lifecycle/lua_integration.rs); clippy -D warnings passed, so they are likely cfg-gated, but someone should look.
- [T5-21] The MCP `tools/list` pin in extended_mcp_server.rs still carries a literal `["builtin","just","upstream"]`; it is a wire pin by design, but the wire shape there is a separate literal from `discovery_tool_definitions()`.
- [T5-21] `ToolDiscovery::classify_source` classifies by name prefix (`just_`, `gh_`, `mcp_`, `::`), not by the executor that serves the tool; a gateway tool without those prefixes reports as `builtin`.
- [T5-22] The polling backend's tick loop still scans nothing (only `watch` scans once); it delivers no change events after the initial scan.
- [T5-22] The debounce-delay test relies on timing (no event within 250 ms with a 600 ms delay); it is stable locally but a heavily loaded CI could need the margin widened.
- [T5-23] The cfg(test) helpers RpcContext::for_test and for_test_with_plugin_loader still carry #[allow(clippy::too_many_arguments)]; they could take a small test params struct too.
- [T5-23] Server still stores kiln_manager, session_manager, agent_manager, project_manager and plugin_loader that rpc_context also holds; the same dedup could apply.
- [T5-25] review/git.rs `git_stdin` still spawns git itself (needs piped stdin); run_git could grow a stdin option to remove that last copy.
- [T5-25] cargo check (without clippy) shows 4 pre-existing unused-import/dead-code warnings in crates/crucible-lua/src/executor.rs and lifecycle/lua_integration.rs on HEAD; clippy -D warnings passes, so they are cfg-gated, but they are noise.
- [T5-26] Nothing serializes `Range` in production yet; the derive exists for the item only.
- [T5-26] Plan line 1089 (Lua-side test for serde error texts on a missing path/body/line_start) is still open.
- [T5-27] crucible-lua has 4 pre-existing `cargo check` warnings (executor.rs unused imports, execute_lua and clear_plugin_modules unused) that clippy with -D warnings did not flag at workspace level; check whether a feature gate hides them.
- [T5-27] A daemon round-trip test for `session.configure_agent` (store then `session.get_*`) still does not exist.
- [T5-28] execution_roots.rs and daemon_plugins/bootstrap.rs both still carry the CRUCIBLE_RUNTIME read inline; a core helper could own that too.
- [T5-28] webhook/tests.rs::minted_secrets_file_is_private_to_its_owner tests the tighten path that fs.rs already covers; drop it if the helper is the only writer.
- [T5-29] crucible-lua emits 4 pre-existing cargo check warnings (unused imports and dead methods in executor.rs, lifecycle/lua_integration.rs); clippy with -D warnings passed so they are cfg-gated, but they predate this change.
- [T5-29] No out-of-process test in tests/rpc_platform_e2e.rs covers agents.list_cards; the dispatch test covers the handler in-process.
- [T5-30] Add a `--mode <name>` flag to `cru chat` so user-defined modes are reachable from the command line, then retire `--plan` as a shorthand of it.
- [T5-30] The `--plan` oneshot apply is covered by pure helper tests only; no test drives a mock AgentHandle to assert `set_mode_str` is called.
- [T5-31] `:` alone (empty word) suggests `:quit` because the empty string is within distance 2 of alias `q`; consider skipping suggestions for an empty word.
- [T5-31] `:set` still receives the raw `command` string and re-strips its own `set` prefix in `SetCommand::parse`; could take the argument directly.
- [T5-32] collect_agent_directories still reads dirs::config_dir()/home_dir() at call time for the three production callers; the hermetic seam is card_roots, not a CardRoots passed down from the command entry.
- [T5-32] check_providers in doctor.rs has no test that crosses HTTP; probe_reply is unit-tested only.
- [T5-33] `impl FromLua for BorderStyle` in crucible-lua/src/theme.rs has no caller (it had none before this change); delete it in a cleanup pass.
- [T5-33] Plan line 1090 (theme_wire::border_style_name vs ui_geometry::border_to_wire name mismatch) is now moot on the theme_wire side; update the plan entry.
- [T5-34] The plugin-loader session VMs (session_lifecycle.rs) still bind UnsupportedSessionRpc, so session.model = "x" there raises "not supported"; only the on_session_start hook path reaches SessionAgent.
- [T5-34] SessionDefaultsRpc::list_models still answers like the unsupported backing; a hook cannot enumerate models before picking one.
- [T5-35] crucible-oil template parsers (template/node_spec.rs, template/html.rs) still only accept the oil spellings `single`/`heavy`; they are a separate vocabulary, not the geometry wire, and were left as-is.
- [T5-36] The e2e Playwright fixtures were only grepped for event_type (no hits); `just web-test e2e` was not run.
- [T5-38] Expected.md section 2 still lacks feature rows (F-ids) for the eight code-only features; section 2a now says so.
- [T5-38] Lua Notifications stays `[-]`: no daemon code drains the cru.log.notify queue into a session event (Gaps G122).
- [T5-38] The TUI `:export` renders events client-side with render_to_markdown instead of calling session.export_to_file, so the write-protection in observe.rs does not cover it.
- [T5-38] Consolidation Plan line numbers for types/acp.rs in Band B B10 and B11 remain stale (noted in section 5a).

## 6. Extension seams now

"Before" lists the files a contributor touched at `7053bcfe7`. "Now" lists the files at `7fcd3b9f4`, after Tier 1 to Tier 3 landed. Every path in a "Now" line exists at that commit.

### Add a tool

Before: `tools/surface.rs` (`BuiltinTool` + `ToolSurface`, gated) · `tools/workspace_defs.rs` or `tools/notes/` · `tool_dispatch.rs` (`is_core_tool_name`, executor arm) · `tools/mcp_server.rs` (`KILN_BACKED_TOOLS`) · `tools/extended_mcp_server.rs` (`discovery_tools`) · `provider/genai_handle.rs` (`bridge_tool_defs`) · `crucible-cli/src/commands/tools.rs` (`BUILTIN_TOOLS`) · `agent_manager/messaging/permission.rs` (file-tool list x2) · `runtime/defaults/init.lua` if the permission mode must know it.
Now (Tier 1 T1-B12, T1-B13; Tier 3 C6): `crates/crucible-daemon/src/tools/surface.rs` (variant, surface, the kiln predicate) · the tool module under `crates/crucible-daemon/src/tools/` · `crates/crucible-daemon/src/tool_dispatch.rs` (one executor arm; `DISCOVERY_TOOL_NAMES` is still a hand list, see section 5a) · `runtime/defaults/init.lua` if a mode must know it. `KILN_BACKED_TOOLS` and the CLI `BUILTIN_TOOLS` are gone. The compiler lists every `match` that must grow.

### Add a provider

Before: `crucible-core/src/config/components/backend.rs` (`BackendType`) · `components/defaults.rs` · `components/llm.rs` · `components/chat.rs` · `components/enrichment.rs` (one struct per provider) · `crucible-daemon/src/agent_factory.rs` · `provider/model_listing.rs` · `agent_manager/providers.rs` · `crucible-cli/src/provider_detect.rs` · `commands/wizard.rs` · `commands/init.rs` · `crucible-daemon/src/llm/embeddings/<provider>.rs`.
Now (Tier 3 C3, B25): `crates/crucible-core/src/config/components/backend.rs` (variant plus one table row with endpoint, default model, label and env var) · `crates/crucible-daemon/src/agent_factory.rs` (one client constructor arm) · `crates/crucible-daemon/src/llm/embeddings/` if it embeds. Wizard, init and model listing read the row. Detection still has two copies (`provider_detect.rs`, `discover_env_providers`; section 5a).

### Add a client

Before: `crucible-core/src/protocol/rpc/mod.rs` (`SessionEventMessage`) · `crucible-daemon/src/rpc_client/client/types.rs` (second `SessionEvent`) · `rpc_client/agent/convert.rs` (prefix strip) · `crucible-core/src/traits/chat.rs` (`AgentHandle`, 41 defaulted knobs a client can silently skip) · `crucible-web/src/events.rs` (`ChatEvent`, own prefix copy) · `crucible-lua/src/handlers/conversion.rs`.
Now (Tier 3 A1, B9; Tier 1 T1-B24): `crates/crucible-core/src/protocol/rpc/mod.rs` (read only) · `crates/crucible-core/src/traits/chat.rs` (`AgentHandle` plus `SessionKnobs`, all required) · one projection module for the client's own render type. The second `SessionEvent` is gone; `crates/crucible-web/src/events.rs` still holds `ChatEvent`. A client that omits a knob does not compile.

### Add a hook stage

Before: `crucible-lua/src/handlers/hook_name.rs` (`StageId`, gated by `EnumIter`) · `crucible-daemon/src/agent_manager/messaging/tool_call.rs` or `send.rs` (the call site, and the gate order) · `handlers/registry.rs` (`execute_runtime_handler`) · `handlers/before_execute.rs` (`execute_runtime_json_handler`, copy) · `tool_hooks.rs` (`resolve_display_*`, copy per hook) · `runtime/defaults/init.lua` (if a default mode reacts) · `docs/Help/Extending/`.
Now (Tier 1 T1-B12, T1-B23; Tier 3 C28): `crates/crucible-lua/src/handlers/hook_name.rs` (variant) · one call site in `crates/crucible-daemon/src/agent_manager/messaging/` · one `fold_vms` pass in `crates/crucible-daemon/src/agent_manager/vm_pass.rs` · `runtime/defaults/init.lua` if needed · `docs/Help/Extending/`. The gate order in `crates/crucible-daemon/src/agent_manager/messaging/tool_call.rs` stays the single place that decides `cancel` versus `handled`.

### Add a storage backend

Before: `crucible-core/src/storage/note_store.rs` (`NoteStore`, 5 defaulted link methods) · `storage/property_store.rs` (`PropertyStore`) · `traits/knowledge.rs` (`KnowledgeRepository`, 1 default) · `crucible-daemon/src/storage/sqlite/` (`adapters.rs`, `repository.rs` with the Scope match x3, `property_store.rs` with two impls) · `storage/sqlite/link_index.rs` · `storage/mod.rs` re-exports · `crucible-core/src/storage/traits.rs` (`StorageBackend`, dead).
Now (Tier 1 T1-B4, T1-B17; Tier 3 A4): `crates/crucible-core/src/storage/note_store.rs` (all methods required) · `crates/crucible-core/src/storage/property_store.rs` · `crates/crucible-core/src/traits/knowledge.rs` · one new directory beside `crates/crucible-daemon/src/storage/sqlite/` with one `impl` per trait · `crates/crucible-daemon/src/storage/sqlite/adapters.rs` (one constructor arm). `StorageBackend` and `ContentHasher` are gone. A backend that omits link queries does not compile.

### Add an RPC method

Before: `crucible-daemon/src/rpc/dispatch.rs` (`rpc_methods!` row, gated; dispatch arm) · `server/<area>.rs` (handler; hand-spelled `json!` reply) · `rpc_helpers.rs` · `rpc_client/client/<area>.rs` (client wrapper; request type, often a fresh `{session_id}` struct) · `rpc_client/mod.rs` (re-export) · `crucible-web/src/services/daemon.rs` (`ReconnectingDaemon` wrapper) · `crucible-lua/src/sessions/` (`DaemonSessionApi`, defaulted) · `rpc/missing_session_contract.rs` if the method takes a session.
Now (Tier 1 T1-B6; Tier 3 A3, B21): `crates/crucible-daemon/src/rpc/dispatch.rs` (row plus arm) · one handler under `crates/crucible-daemon/src/server/` that takes `RpcContext` (`crates/crucible-daemon/src/rpc/context.rs`; `ServerContext` is gone) · `crates/crucible-daemon/src/rpc_client/client/` using `SessionIdRequest` · `crates/crucible-web/src/services/daemon.rs` if the web needs it · `crates/crucible-lua/src/sessions/mod.rs` (`DaemonSessionApi`, required, so Lua cannot miss it). The reply-shape problem (Actual.md section 9, "hand-spelled `json!`") is outside this plan.

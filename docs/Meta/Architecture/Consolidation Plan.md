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
| B3 | `with_debounce` calls at `external_changes.rs`, `kiln_manager.rs` | Those are `WatchConfig::with_debounce`, a live method; the plan conflated two methods | dropped |
| B5 | `impl Default` for `ClientId`, `SubscriptionManager` | clippy `new_without_default`; `ClientId::new` draws from a counter, so a derived Default changes behaviour | Tier 3 |
| B7 | `KILN_BACKED_TOOLS` | deferred by the plan | Tier 3 |
| B11 | `SessionManager::remove_session` | live test callers in two test modules | Tier 4 |
| B11 | `KilnRegistry::iter` | live caller `server/session/list.rs:109`; the grep missed it | dropped |
| B13 | `Component::Normal` loop in `core/canvas/containment.rs` | different crate from the daemon helper | Tier 3 |
| B18 | `ModelCapability`, `UnifiedModelInfo`, `McpTransportConfig` | carry `serde` attributes; rule 2 | Tier 3 |
| B22 | `kiln_validate::is_temp_directory` | not a duplicate; `starts_with` flags subdirectories, and a test depends on it | dropped |
| B23 | `parse_capability` → serde | behaviour differs (case fold, nine names) | Tier 3 |
| T2-B1 | `resolve_path` parameter | B22 removed the function | done |
| T2-B3 | `InputArea::with_popup` | B24 removed it | done |

Two follow-ons that the batches applied under the Method rules: B2 removed
`PerformanceStats`, `QueueStats` and their readers once `get_status` went; B5
removed three `Server` fields that lost their last reader with `ServerContext`.

## 2. Tier 1 — safe mechanical, this session

Actions: `delete` removes the item. `narrow` changes visibility. `merge-into X` keeps X and deletes the other copy. `call X` replaces an inline body with a call to X.

### B1 — crucible-daemon, `watch/` builders

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

### B2 — crucible-daemon, `watch/` manager, monitor, factory

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

### B3 — crucible-daemon, `watch/` backends, handlers, events

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

### B4 — crucible-daemon, `storage/sqlite/`

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

### B5 — crucible-daemon, `server/` and `subscription.rs`

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

### B6 — crucible-daemon, `rpc_client/`

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

### B7 — crucible-daemon, `tools/` and `mcp/`

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

### B8 — crucible-daemon, `acp/`

Files: `acp/mock_agent.rs`, `acp/client/mod.rs`, `acp/tracing_utils.rs`, `acp/discovery.rs`, `acp/mod.rs`, `acp_launch.rs`, `acp/tools.rs`.

| Item | Location | Action |
|---|---|---|
| `MockAgent::add_response` | `mock_agent.rs:70` | delete |
| `CrucibleAcpClient::with_recorder` | `client/mod.rs:168` | delete; `Recorder::from_env` at `:141` stays |
| `TraceContext`, `LogCapture`, `CapturedLog`, `trace_span!`, `trace_event!` | `tracing_utils.rs` | delete the file and the two `pub use` lines in `acp/mod.rs` |
| `get_agent_help` | `discovery.rs:297`, `:39`, `acp/mod.rs:29` | delete; fix the doc comment at `:39` |
| `known` agent table | `acp_launch.rs:126-132` | call `BUILTIN_AGENTS` (`discovery.rs:52`) |
| `ToolDescriptor` | `acp/tools.rs:29` | merge-into `crucible_core::ToolDefinition`; map `category: String` to `Some(category)` and `input_schema` to `parameters`; keep `ToolRegistry` and `discover_tools` (integration tests use them) |

### B9 — crucible-daemon, `llm/` and `provider/`

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

### B10 — crucible-daemon, skills, enrichment, pipeline, workflow, observe

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

### B11 — crucible-daemon, session, scm, agent_manager, kiln, platform, bootstrap

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

### B12 — crucible-daemon, `agent_manager/messaging/`

Files: `agent_manager/messaging/permission.rs`, `tool_call.rs`, `stream.rs`, `tool_hooks.rs`, `crucible-core/src/traits/chat.rs`.

| Item | Location | Action |
|---|---|---|
| permission-engine input snippet x3 | `permission.rs:583-590`, `:626-633`, `:688-695` | call one `fn engine_input(tool_name, args) -> &str` |
| file-tool name list x2 | `permission.rs:1073`, `:1098` | call one `const FILE_TOOLS: &[&str]` or `BuiltinTool` predicate; leave `core engine.rs:193 is_file_tool` alone (different list, Tier 3) |
| `ChatToolResult` error literal x7 | `tool_call.rs:44`, `:499`, `:517`, `:815`, `:875`, `stream.rs:619`, `:697` | add `ChatToolResult::error(name, call_id, msg)` in `crucible-core/src/traits/chat.rs`; call it |
| inline `deny_tool_call` body | `tool_call.rs:485-506` | call `deny_tool_call` (`:23`) |
| `resolve_display_start_hints` vs `resolve_display_complete_hints` | `tool_hooks.rs:22-71`, `:73-122` | call one generic `fn resolve_hints<E, H>(hook, event) -> H` |

### B13 — crucible-daemon, server path and review helpers; tool listing

Files: `server/fs/mod.rs`, `server/session/review/mod.rs`, `server/note_refactor.rs`, `server/canvas/containment.rs`, `tools/workspace.rs`, `tool_dispatch.rs`, `tools/gateway_executor.rs`, `replay.rs`, `crucible-cli/src/tui/oil/local_replay.rs`.

| Item | Location | Action |
|---|---|---|
| `Component::Normal` whitelist loop x6 | `fs/mod.rs:155-165`, `:381-391`, `:455-464`, `review/mod.rs:605`, `note_refactor.rs:228`, `canvas/containment.rs:215` | call one `pub(crate) fn reject_non_normal(path) -> Result<()>` in `tools/containment.rs` |
| `parse_state`, `state_reason`, `parse_author` | `review/mod.rs:490-512` | call `serde_json::from_value` on `ReviewState` / `CommentAuthor` (`rename_all = snake_case` yields the same strings) |
| `rmcp::Tool -> ToolDefinition` x3 | `workspace.rs:655-671`, `tool_dispatch.rs:575-591`, `gateway_executor.rs:68` | call one `fn tool_definition_from_rmcp(tool, category) -> ToolDefinition` |
| `is_core_tool_name` | `tool_dispatch.rs:181-186` | call `BuiltinTool::parse(name).map(\|t\| t.surface() == ToolSurface::Host)` |
| `is_keypress_event` | `replay.rs:199`, `crucible-cli/src/tui/oil/local_replay.rs:62-65` | make the daemon fn `pub`; the CLI calls it |

### B14 — crucible-core, `parser/types/` accessors

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

### B15 — crucible-core, `parser/` lists, links, extensions, traits

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

### B16 — crucible-core, `types/`

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

### B17 — crucible-core, dead modules

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

### B18 — crucible-core, events, enrichment, traits, config, project, serde_md, test_support

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

### B19 — crucible-cli, `chat_runner/`, `chat_app/`, runner, composer

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

### B20 — crucible-cli, `tui/oil/config/`

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

### B21 — crucible-cli, components, theme, viewport cache, status bar

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

### B22 — crucible-cli, commands and utils

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

### B23 — crucible-lua

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

### B24 — crucible-oil and crucible-web (non-route)

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
| `resolve_path` `_config_dir` parameter | `commands/agents.rs:91` | delete (B22 also touches this fn; do B22 first) |
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
| `InputArea::with_popup` | `components/input_area.rs:57` | delete (B24 deletes `InputArea`; do B24 first) |

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

**B6. `ContentHasher`, `HashingAlgorithm`, `ChangeDetectionStore`.** B17 deletes `hashing/` and `processing/`. `ContentHasher` (`storage/traits.rs:17`) remains with dead impls.
Recommend: delete `ContentHasher` too once B17 lands and `rg ContentHasher` shows only the trait. Cost: S.

**B7. Session event parallel enums.** `SessionEventMessage` (wire, canonical), `TurnPayload` and seven groups (`protocol/session_events/`), `SessionEvent` + `InternalSessionEvent` (scripting, 12 of 52 variants live), `LogEvent` (5 of 16 written), `rpc_client/client/types.rs:10 SessionEvent`, web `ChatEvent` (6 dead variants). Families from the duplicate audit: `PostLlmCall` x2, `SessionEnded` vs `Ended`, `Delegation*` x2, `BashTask*` vs `BashJob*`, `Interaction*` x2, `Subagent*` x2, file/note variants vs `SystemPayload`.
Options: (a) delete the 40 dead scripting variants and the 11 dead `LogEvent` variants; keep the rest; (b) make the scripting enum a projection of the wire enum (`From<SessionEventMessage>`); (c) leave.
Recommend (a) now, (b) as a follow-up after B18 removes the markdown transcript format. Payoff: a new event is one wire variant plus one projection arm, not five enums. Risk: Lua handlers read the scripting names (`handlers/conversion.rs`); a deleted variant that a script matches fails silently at runtime. Grep `runtime/` for each name before deletion. Cost: L.
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

**B21. `ServerContext` vs `RpcContext`.** After B5 deletes the dead fields, `ServerContext` holds four fields that `RpcContext` also holds.
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

**C2. Tilde expanders (seven).** B11 and B22 collapse them to `resolve_registration_root` and `expand_tilde`. Remaining decision: one `pub fn expand_tilde` in `crucible-core::config` that both crates call. Cost: S after B11/B22.

**C3. Provider knobs: `ChatConfig` vs `LlmProviderConfig`; five enrichment provider configs; `BackendType` vs `defaults.rs` endpoints; VertexAI disagreement; three provider-to-model tables (`wizard.rs:135`, `init.rs:385`, `DEFAULT_CHAT_MODEL`); `ollama_endpoint` x2; `provider_type_label` vs `keyed_backend_display_name`; `ProviderInfo` vs `DetectedProvider`; `discover_env_providers` vs `detect_providers_inner`.**
Options: (a) one `ProviderDefaults` table in `crucible-core/src/config/components/backend.rs` keyed by `BackendType` (endpoint, default model, label, env var) that config, wizard, init, model listing and detection all read; (b) leave.
Recommend (a). Payoff: a new provider is one row; today it is eight edits across three crates. Risk: the VertexAI endpoint must be decided (`backend.rs:130` vs `enrichment.rs:382`); wizard defaults for anthropic and openai change to match `DEFAULT_CHAT_MODEL`. Cost: L.

**C4. Theme colour parsers.** B23 collapses two exact pairs. `parse_adaptive_color` vs `color_from_lua` and `adaptive_from_wire` vs `color_from_wire` differ in the String arm (palette names).
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
Recommend: `test_support` copies become canonical; delete `enrichment/service.rs:446` and `multi_kiln_search.rs:109`, `agent_manager/tests/mod.rs:242` after adding scripted results to the canonical ones. `mock.rs:13` stays (production config can select it). Cost: M.

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

**C26. `SqlitePropertyStore`** is in B4 (duplicate audit found it safe; the dead-code skeptic wanted the test rewrite named). The rewrite is in B4.

**C27. `session_bridge.rs CommentSpec::parse` vs `ReviewCommentRequest`; `parse_range` vs `context_ops::Range`.**
Recommend: derive `Deserialize` on `Range` with `serde(tag = "type")` and delete `parse_range`; `CommentSpec` becomes `ReviewCommentRequest` with `#[serde(default)]` on author. Cost: S.

**C28. "Session VM under lock, then plugin VM" two-pass loop x9.**
Recommend: one `async fn for_each_vm(session, plugins, f)` helper in `crucible-lua`. Cost: M.

**C29. `acp/mod.rs` re-exports of `agent_client_protocol` message types** (unverified use).
Recommend: Tier 4 check first.

## 5. Tier 4 — unverified

### 5.1 Second skeptic pass checklist

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
- [ ] `LayoutEngine::compute`, `ComputedLayout` `taffy_layout.rs:40`
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
- `FtsIndex::is_empty` — kiln_manager tests (now merged in B4).
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
- `ToolDescriptor`, `ToolRegistry`, `discover_tools`, `AcpToolExecutor` — `acp_integration_e2e.rs` (shape merge in B8).
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

## 6. Extension seams after consolidation

"Today" lists the files a contributor touches at `7053bcfe7`. "After Tier 3" lists the files once the named entries land.

### Add a tool

Today: `tools/surface.rs` (`BuiltinTool` + `ToolSurface`, gated) · `tools/workspace_defs.rs` or `tools/notes/` · `tool_dispatch.rs` (`is_core_tool_name`, executor arm) · `tools/mcp_server.rs` (`KILN_BACKED_TOOLS`) · `tools/extended_mcp_server.rs` (`discovery_tools`) · `provider/genai_handle.rs` (`bridge_tool_defs`) · `crucible-cli/src/commands/tools.rs` (`BUILTIN_TOOLS`) · `agent_manager/messaging/permission.rs` (file-tool list x2) · `runtime/defaults/init.lua` if the permission mode must know it.
After Tier 1 B12, B13 and Tier 3 C6: `tools/surface.rs` (variant, surface, `needs_kiln`) · the tool module · `tool_dispatch.rs` (one executor arm) · `init.lua` if needed. The compiler lists every `match` that must grow.

### Add a provider

Today: `crucible-core/src/config/components/backend.rs` (`BackendType`) · `components/defaults.rs` · `components/llm.rs` · `components/chat.rs` · `components/enrichment.rs` (one struct per provider) · `crucible-daemon/src/agent_factory.rs` · `provider/model_listing.rs` · `agent_manager/providers.rs` · `crucible-cli/src/provider_detect.rs` · `commands/wizard.rs` · `commands/init.rs` · `crucible-daemon/src/llm/embeddings/<provider>.rs`.
After Tier 3 C3 and B25: `backend.rs` (variant plus one `ProviderDefaults` row) · `agent_factory.rs` (one client constructor arm) · `llm/embeddings/<provider>.rs` if it embeds. Wizard, init, detection and model listing read the row.

### Add a client

Today: `crucible-core/src/protocol/rpc/mod.rs` (`SessionEventMessage`) · `crucible-daemon/src/rpc_client/client/types.rs` (second `SessionEvent`) · `rpc_client/agent/convert.rs` (prefix strip) · `crucible-core/src/traits/chat.rs` (`AgentHandle`, 41 defaulted knobs a client can silently skip) · `crucible-web/src/events.rs` (`ChatEvent`, own prefix copy) · `crucible-lua/src/handlers/conversion.rs`.
After Tier 3 A1, B9 and Tier 1 B24: `protocol/rpc/mod.rs` (read only) · `traits/chat.rs` (`AgentHandle` + `SessionKnobs`, all required) · one projection module for the client's own render type. A client that omits a knob does not compile.

### Add a hook stage

Today: `crucible-lua/src/handlers/hook_name.rs` (`StageId`, gated by `EnumIter`) · `crucible-daemon/src/agent_manager/messaging/tool_call.rs` or `send.rs` (the call site, and the gate order) · `handlers/registry.rs` (`execute_runtime_handler`) · `handlers/before_execute.rs` (`execute_runtime_json_handler`, copy) · `tool_hooks.rs` (`resolve_display_*`, copy per hook) · `runtime/defaults/init.lua` (if a default mode reacts) · `docs/Help/Extending/`.
After Tier 1 B12, B23: `hook_name.rs` (variant) · one call site in `messaging/` · one generic `resolve_hints` call · `init.lua` if needed · docs. The gate order in `tool_call.rs` stays the single place that decides `cancel` vs `handled`.

### Add a storage backend

Today: `crucible-core/src/storage/note_store.rs` (`NoteStore`, 5 defaulted link methods) · `storage/property_store.rs` (`PropertyStore`) · `traits/knowledge.rs` (`KnowledgeRepository`, 1 default) · `crucible-daemon/src/storage/sqlite/` (`adapters.rs`, `repository.rs` with the Scope match x3, `property_store.rs` with two impls) · `storage/sqlite/link_index.rs` · `storage/mod.rs` re-exports · `crucible-core/src/storage/traits.rs` (`StorageBackend`, dead).
After Tier 1 B4, B17 and Tier 3 A4: `note_store.rs` (all methods required) · `property_store.rs` · `knowledge.rs` · one `storage/<backend>/` directory with one `impl` per trait · `adapters.rs` (one constructor arm). A backend that omits link queries does not compile.

### Add an RPC method

Today: `crucible-daemon/src/rpc/dispatch.rs` (`rpc_methods!` row, gated; dispatch arm) · `server/<area>.rs` (handler; hand-spelled `json!` reply) · `rpc_helpers.rs` · `rpc_client/client/<area>.rs` (client wrapper; request type, often a fresh `{session_id}` struct) · `rpc_client/mod.rs` (re-export) · `crucible-web/src/services/daemon.rs` (`ReconnectingDaemon` wrapper) · `crucible-lua/src/sessions/` (`DaemonSessionApi`, defaulted) · `rpc/missing_session_contract.rs` if the method takes a session.
After Tier 1 B6 and Tier 3 A3, B21: `rpc/dispatch.rs` (row + arm) · one handler in `server/` taking `RpcContext` · `rpc_client/client/<area>.rs` using `SessionIdRequest` · `ReconnectingDaemon` if the web needs it · `DaemonSessionApi` (required, so Lua cannot miss it). The reply-shape problem (Actual.md section 9, "hand-spelled `json!`") is outside this plan.

// GENERATED FILE. Do not hand-edit.
//
// Regenerate with:
//   cargo run -p crucible-core --example gen_rpc_methods_ts -- \
//     crates/crucible-web/web/src/lib/api-schema.d.ts \
//     > crates/crucible-web/web/src/lib/rpc-methods.d.ts
//
// Source of truth: the `rpc_methods!` table in
// crates/crucible-core/src/protocol/rpc/method.rs. Each entry names the
// wire params and reply shape of one RpcMethod row. A cell is `unknown`
// when the row's type is `serde_json::Value`, or when `api-schema.d.ts`
// has no schema for it yet (no route emits it through utoipa today).
import type { components } from './api-schema';

type Schemas = components['schemas'];

/** A session-scoped request body: the daemon reads `session_id` at the
 * top level, flattened beside the method's own fields (`Scoped<T>` on the
 * Rust side). */
export type WithSessionId<T> = { session_id: string } & T;

export interface RpcMethods {
  'ping': { params: null; result: string };
  'daemon.capabilities': { params: null; result: Schemas['DaemonCapabilities'] };
  'shutdown': { params: null; result: string };
  'kiln.open': { params: Schemas['KilnOpenRequest']; result: Schemas['KilnOpenReply'] };
  'kiln.close': { params: Schemas['PathRequest']; result: Schemas['StatusReply'] };
  'kiln.list': { params: null; result: (Schemas['KilnRow'])[] };
  'kiln.register': { params: Schemas['KilnRegisterRequest']; result: Schemas['KilnRegisterReply'] };
  'kiln.registry_list': { params: null; result: unknown };
  'kiln.forget': { params: Schemas['NameRequest']; result: Schemas['KilnForgetReply'] };
  'llm.register_provider': { params: Schemas['LlmRegisterProviderRequest']; result: Schemas['LlmRegisterProviderReply'] };
  'search_vectors': { params: Schemas['SearchVectorsRequest']; result: (Schemas['VectorHit'])[] };
  'search_text': { params: Schemas['SearchTextRequest']; result: (Schemas['FtsResult'])[] };
  'search_grep': { params: Schemas['GrepSearchRequest']; result: Schemas['GrepSearchResponse'] };
  'embed.query': { params: Schemas['EmbedQueryRequest']; result: Schemas['EmbedQueryReply'] };
  'list_notes': { params: Schemas['ListNotesRequest']; result: (Schemas['NoteListRow'])[] };
  'get_note_by_name': { params: Schemas['NoteRef']; result: (Schemas['NoteByNameReply'] | null) };
  'base.list': { params: unknown; result: unknown };
  'base.views': { params: unknown; result: unknown };
  'base.query': { params: unknown; result: unknown };
  'base.create_entry': { params: unknown; result: unknown };
  'base.set_property': { params: unknown; result: unknown };
  'base.reorder_groups': { params: unknown; result: unknown };
  'get_backlinks': { params: Schemas['NoteRef']; result: (Schemas['GetBacklinksReply'] | null) };
  'kiln.graph': { params: Schemas['KilnRef']; result: Schemas['KilnGraphReply'] };
  'note.upsert': { params: Schemas['NoteUpsertRequest']; result: Schemas['NoteUpsertReply'] };
  'note.get': { params: Schemas['NotePathRequest']; result: (Schemas['NoteRecord'] | null) };
  'note.delete': { params: Schemas['NotePathRequest']; result: Schemas['StatusReply'] };
  'note.list': { params: Schemas['KilnRef']; result: (Schemas['NoteRecord'])[] };
  'process_file': { params: Schemas['ProcessFileRequest']; result: Schemas['ProcessFileReply'] };
  'process_batch': { params: Schemas['ProcessBatchRequest']; result: Schemas['ProcessBatchReply'] };
  'session.create': { params: Schemas['SessionCreateRequest']; result: Schemas['SessionSummary'] };
  'session.list': { params: Schemas['SessionListRequest']; result: Schemas['SessionListReply'] };
  'session.get': { params: WithSessionId<Record<string, never>>; result: Schemas['SessionDetail'] };
  'session.pause': { params: WithSessionId<Record<string, never>>; result: Schemas['SessionTransitionReply'] };
  'session.resume': { params: WithSessionId<Record<string, never>>; result: Schemas['SessionTransitionReply'] };
  'session.resume_from_storage': { params: WithSessionId<Schemas['Page']>; result: Schemas['SessionHistoryReply'] };
  'session.history': { params: WithSessionId<Schemas['Page']>; result: Schemas['SessionHistoryReply'] };
  'session.end': { params: WithSessionId<Record<string, never>>; result: Schemas['SessionEndReply'] };
  'session.archive': { params: WithSessionId<Record<string, never>>; result: Schemas['SessionArchiveReply'] };
  'session.unarchive': { params: WithSessionId<Record<string, never>>; result: Schemas['SessionArchiveReply'] };
  'session.delete': { params: WithSessionId<Record<string, never>>; result: Schemas['SessionDeleteReply'] };
  'session.compact': { params: WithSessionId<Record<string, never>>; result: Schemas['SessionCompactReply'] };
  'session.subscribe': { params: Schemas['SessionSubscribeRequest']; result: Schemas['SessionSubscribeReply'] };
  'session.unsubscribe': { params: Schemas['SessionSubscribeRequest']; result: Schemas['SessionUnsubscribeReply'] };
  'session.configure_agent': { params: WithSessionId<Schemas['AgentConfig']>; result: Schemas['SessionConfigureAgentReply'] };
  'session.send_message': { params: WithSessionId<Schemas['MessageInput']>; result: Schemas['SendOutcome'] };
  'session.cancel': { params: WithSessionId<Record<string, never>>; result: Schemas['SessionCancelResponse'] };
  'session.clear': { params: WithSessionId<Record<string, never>>; result: Schemas['SessionClearReply'] };
  'session.connect_kiln': { params: WithSessionId<Schemas['NamedKiln']>; result: Schemas['SessionScopeReply'] };
  'session.disconnect_kiln': { params: WithSessionId<Schemas['NamedKiln']>; result: Schemas['SessionScopeReply'] };
  'session.set_workspace': { params: WithSessionId<Schemas['WorkspaceChoice']>; result: Schemas['SessionScopeReply'] };
  'session.list_models': { params: WithSessionId<Record<string, never>>; result: Schemas['SessionListModelsReply'] };
  'session.list_modes': { params: WithSessionId<Record<string, never>>; result: Schemas['SessionModes'] };
  'session.commands': { params: WithSessionId<Record<string, never>>; result: Schemas['SessionCommandsReply'] };
  'session.list_knobs': { params: WithSessionId<Record<string, never>>; result: Schemas['SessionKnobSupport'] };
  'session.knob.set': { params: WithSessionId<Schemas['KnobValue']>; result: Schemas['SessionKnobSetReply'] };
  'session.knob.get': { params: WithSessionId<Schemas['KnobRef']>; result: Schemas['KnobValue'] };
  'session.list_agent_options': { params: WithSessionId<Record<string, never>>; result: Schemas['SessionListAgentOptionsReply'] };
  'session.set_agent_option': { params: Schemas['SessionSetAgentOptionRequest']; result: Schemas['PluginAck'] };
  'session.cache_stats': { params: WithSessionId<Record<string, never>>; result: Schemas['SessionCacheStatsReply'] };
  'session.add_notification': { params: WithSessionId<Schemas['NewNotification']>; result: Schemas['SessionAddNotificationReply'] };
  'session.list_notifications': { params: WithSessionId<Record<string, never>>; result: Schemas['SessionListNotificationsReply'] };
  'session.dismiss_notification': { params: WithSessionId<Schemas['NotificationKey']>; result: Schemas['SessionDismissNotificationReply'] };
  'notification.list': { params: Schemas['NotificationListRequest']; result: Schemas['NotificationListResponse'] };
  'notification.dismiss': { params: Schemas['NotificationDismissRequest']; result: Schemas['NotificationDismissResponse'] };
  'session.interaction_respond': { params: WithSessionId<Schemas['InteractionAnswer']>; result: Schemas['SessionInteractionRespondReply'] };
  'session.pending_interactions': { params: null; result: Schemas['SessionPendingInteractionsReply'] };
  'session.set_plugin_approval': { params: WithSessionId<Schemas['PluginApprovalChange']>; result: Schemas['PluginApprovalReply'] };
  'session.get_plugin_approval': { params: WithSessionId<Schemas['PluginRef']>; result: Schemas['PluginApprovalReply'] };
  'session.list_plugin_approvals': { params: WithSessionId<Record<string, never>>; result: Schemas['SessionListPluginApprovalsReply'] };
  'session.inject_context': { params: WithSessionId<Schemas['ContextInjection']>; result: Schemas['SessionInjectContextReply'] };
  'session.test_interaction': { params: WithSessionId<Schemas['TestInteraction']>; result: Schemas['SessionTestInteractionReply'] };
  'session.fork': { params: WithSessionId<Schemas['ForkPoint']>; result: Schemas['SessionForkReply'] };
  'session.set_title': { params: WithSessionId<Schemas['Title']>; result: Schemas['SessionTitleReply'] };
  'session.generate_title': { params: WithSessionId<Record<string, never>>; result: Schemas['SessionTitleReply'] };
  'session.search': { params: Schemas['SessionSearchRequest']; result: Schemas['SessionSearchResponse'] };
  'session.events_after': { params: WithSessionId<Schemas['EventCursor']>; result: (Schemas['SessionEventMessage'])[] };
  'session.list_persisted': { params: Schemas['SessionListPersistedRequest']; result: unknown };
  'session.render_markdown': { params: WithSessionId<Schemas['MarkdownOptions']>; result: Schemas['SessionRenderMarkdownResponse'] };
  'session.export_to_file': { params: WithSessionId<Schemas['ExportOptions']>; result: Schemas['SessionExportToFileResponse'] };
  'session.replay': { params: Schemas['SessionReplayRequest']; result: Schemas['SessionReplayStartedReply'] };
  'session.cleanup': { params: Schemas['SessionCleanupRequest']; result: Schemas['SessionCleanupReply'] };
  'session.reindex': { params: null; result: null };
  'session.undo': { params: WithSessionId<Schemas['UndoCount']>; result: Schemas['SessionUndoReply'] };
  'session.can_undo': { params: WithSessionId<Record<string, never>>; result: Schemas['SessionCanUndoReply'] };
  'session.undo_depth': { params: WithSessionId<Record<string, never>>; result: Schemas['SessionUndoDepthReply'] };
  'plugin.reload': { params: Schemas['NameRequest']; result: Schemas['PluginReloadReply'] };
  'plugin.list': { params: null; result: Schemas['PluginListReply'] };
  'plugin.commands': { params: null; result: Schemas['PluginCommandsReply'] };
  'plugin.publications': { params: Schemas['PluginPublicationsRequest']; result: Schemas['PluginPublicationsReply'] };
  'surface.list': { params: Schemas['SurfaceRequest']; result: Schemas['SurfaceListReply'] };
  'surface.get': { params: Schemas['SurfaceRequest']; result: Schemas['SurfaceGetReply'] };
  'plugin.options': { params: Schemas['PluginOptionsRequest']; result: Schemas['PluginOptionsReply'] };
  'plugin.option_get': { params: Schemas['PluginOptionCallRequest']; result: Schemas['PluginOptionValue'] };
  'plugin.option_set': { params: Schemas['PluginOptionCallRequest']; result: Schemas['PluginAck'] };
  'plugin.option_execute': { params: Schemas['PluginOptionCallRequest']; result: Schemas['PluginAck'] };
  'session.status': { params: WithSessionId<Record<string, never>>; result: Schemas['SessionStatusReply'] };
  'plugin.run_command': { params: Schemas['PluginRunCommandRequest']; result: Schemas['PluginRunCommandReply'] };
  'plugin.install': { params: Schemas['PluginInstallRequest']; result: Schemas['PluginInstallReply'] };
  'plugin.remove': { params: Schemas['PluginRemoveRequest']; result: Schemas['PluginRemoveReply'] };
  'lua.init_session': { params: WithSessionId<Schemas['LuaSessionInit']>; result: Schemas['LuaInitSessionResponse'] };
  'lua.shutdown_session': { params: WithSessionId<Record<string, never>>; result: Schemas['LuaShutdownSessionResponse'] };
  'lua.discover_plugins': { params: Schemas['LuaDiscoverPluginsRequest']; result: Schemas['LuaDiscoverPluginsResponse'] };
  'lua.plugin_health': { params: Schemas['LuaPluginHealthRequest']; result: Schemas['LuaPluginHealthResponse'] };
  'lua.generate_stubs': { params: Schemas['LuaGenerateStubsRequest']; result: Schemas['LuaGenerateStubsResponse'] };
  'lua.run_plugin_tests': { params: Schemas['LuaRunPluginTestsRequest']; result: Schemas['LuaRunPluginTestsResponse'] };
  'lua.register_commands': { params: WithSessionId<Schemas['LuaCommands']>; result: Schemas['LuaRegisterCommandsReply'] };
  'lua.eval': { params: Schemas['LuaEvalRequest']; result: unknown };
  'config.get': { params: Schemas['ConfigLookupRequest']; result: unknown };
  'config.set': { params: Schemas['ConfigValuesRequest']; result: Schemas['ConfigSetReply'] };
  'config.save': { params: Schemas['ConfigValuesRequest']; result: Schemas['ConfigSaveReply'] };
  'config.reset': { params: Schemas['ConfigKeyRequest']; result: unknown };
  'config.pop': { params: Schemas['ConfigKeyRequest']; result: unknown };
  'config.unset': { params: Schemas['ConfigKeyRequest']; result: unknown };
  'config.origin': { params: Schemas['ConfigLookupRequest']; result: unknown };
  'config.effective': { params: null; result: unknown };
  'config.controls': { params: null; result: unknown };
  'ui.config': { params: Schemas['UiConfigRequest']; result: unknown };
  'ui.set_theme': { params: Schemas['UiSetThemeRequest']; result: Schemas['UiSetThemeReply'] };
  'project.register': { params: Schemas['PathRequest']; result: Schemas['Project'] };
  'project.unregister': { params: Schemas['PathRequest']; result: Schemas['StatusReply'] };
  'project.list': { params: null; result: (Schemas['Project'])[] };
  'project.get': { params: Schemas['PathRequest']; result: (Schemas['Project'] | null) };
  'project.open_kilns': { params: Schemas['PathRequest']; result: Schemas['ProjectOpenKilnsReply'] };
  'project.registry_list': { params: null; result: unknown };
  'scm.clone': { params: Schemas['ScmCloneRequest']; result: Schemas['ScmCloneResponse'] };
  'fs.list_dir': { params: Schemas['FsListDirRequest']; result: Schemas['FsListing'] };
  'diff.get': { params: Schemas['DiffsetRef']; result: Schemas['Diffset'] };
  'diff.file': { params: Schemas['DiffFileRequest']; result: Schemas['DiffFileText'] };
  'diff.comment': { params: Schemas['DiffCommentRequest']; result: Schemas['DiffCommentReply'] };
  'diff.resolve_comment': { params: Schemas['DiffCommentKey']; result: Schemas['DiffResolveCommentReply'] };
  'diff.delete_comment': { params: Schemas['DiffCommentKey']; result: Schemas['DiffDeleteCommentReply'] };
  'diff.comments': { params: Schemas['DiffsetRef']; result: Schemas['DiffCommentsReply'] };
  'proposal.list': { params: Schemas['ProposalListRequest']; result: (Schemas['Proposal'])[] };
  'proposal.get': { params: Schemas['ProposalIdRequest']; result: Schemas['Proposal'] };
  'proposal.accept': { params: Schemas['ProposalAcceptRequest']; result: Schemas['Proposal'] };
  'proposal.reject': { params: Schemas['ProposalRejectRequest']; result: Schemas['Proposal'] };
  'proposal.dismiss': { params: Schemas['ProposalIdRequest']; result: Schemas['Proposal'] };
  'proposal.resolve': { params: Schemas['ProposalResolveRequest']; result: Schemas['Proposal'] };
  'fs.read': { params: Schemas['FileReadRequest']; result: unknown };
  'fs.write': { params: Schemas['FileWriteRequest']; result: unknown };
  'fs.move': { params: Schemas['FsMoveRequest']; result: Schemas['FsMoveReply'] };
  'fs.mkdir': { params: Schemas['FsPathRequest']; result: Schemas['FsMkdirReply'] };
  'fs.trash': { params: Schemas['FsPathRequest']; result: Schemas['FsTrashReply'] };
  'note.rename': { params: Schemas['NoteRenameRequest']; result: Schemas['NoteRenameReply'] };
  'note.move': { params: Schemas['NoteRenameRequest']; result: Schemas['NoteRenameReply'] };
  'storage.verify': { params: Schemas['KilnPathRequest']; result: Schemas['NotImplementedReply'] };
  'storage.cleanup': { params: Schemas['KilnPathRequest']; result: Schemas['NotImplementedReply'] };
  'storage.backup': { params: Schemas['StorageBackupRequest']; result: Schemas['NotImplementedReply'] };
  'storage.restore': { params: Schemas['StorageRestoreRequest']; result: Schemas['NotImplementedReply'] };
  'mcp.start': { params: Schemas['McpStartRequest']; result: Schemas['McpStartReply'] };
  'mcp.stop': { params: null; result: Schemas['McpStopReply'] };
  'mcp.status': { params: null; result: Schemas['McpStatus'] };
  'skills.list': { params: Schemas['SkillsListRequest']; result: Schemas['SkillsReply'] };
  'skills.get': { params: Schemas['SkillsGetRequest']; result: Schemas['SkillDetail'] };
  'skills.search': { params: Schemas['SkillsSearchRequest']; result: Schemas['SkillsReply'] };
  'agents.list_profiles': { params: null; result: Schemas['AgentProfilesReply'] };
  'agents.list_cards': { params: Schemas['AgentsListCardsRequest']; result: Schemas['AgentCardsListReply'] };
  'agents.resolve_profile': { params: Schemas['NameRequest']; result: (Schemas['AgentProfileResolved'] | null) };
  'models.list': { params: Schemas['ListAllModelsRequest']; result: Schemas['ModelsListReply'] };
  'providers.list': { params: Schemas['ListProvidersRequest']; result: Schemas['ProvidersListReply'] };
  'embeddings.models': { params: Schemas['EmbeddingModelsRequest']; result: Schemas['EmbeddingCatalog'] };
  'subagent.collect': { params: Schemas['SubagentCollectRequest']; result: unknown };
  'webhook.receive': { params: Schemas['WebhookReceiveRequest']; result: Schemas['WebhookReceiveReply'] };
  'suggest_links': { params: Schemas['SuggestLinksRequest']; result: Schemas['SuggestLinksReply'] };
  'workflow.start': { params: WithSessionId<Schemas['WorkflowSource']>; result: Schemas['WorkflowRunReply'] };
  'workflow.approve_gate': { params: WithSessionId<Schemas['GateRef']>; result: Schemas['WorkflowRunReply'] };
  'workflow.status': { params: WithSessionId<Record<string, never>>; result: Schemas['WorkflowStatusReply'] };
  'workflow.cancel': { params: WithSessionId<Record<string, never>>; result: Schemas['WorkflowCancelReply'] };
}

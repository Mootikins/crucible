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
  'daemon.capabilities': { params: null; result: unknown };
  'shutdown': { params: null; result: unknown };
  'kiln.open': { params: unknown; result: unknown };
  'kiln.close': { params: unknown; result: unknown };
  'kiln.list': { params: null; result: (Schemas['KilnRow'])[] };
  'kiln.register': { params: unknown; result: unknown };
  'kiln.registry_list': { params: null; result: unknown };
  'kiln.forget': { params: unknown; result: unknown };
  'llm.register_provider': { params: unknown; result: unknown };
  'search_vectors': { params: unknown; result: (unknown)[] };
  'search_text': { params: unknown; result: (unknown)[] };
  'search_grep': { params: Schemas['GrepSearchRequest']; result: Schemas['GrepSearchResponse'] };
  'embed.query': { params: unknown; result: unknown };
  'list_notes': { params: unknown; result: (Schemas['NoteListRow'])[] };
  'get_note_by_name': { params: unknown; result: (Schemas['NoteByNameReply'] | null) };
  'base.list': { params: unknown; result: unknown };
  'base.views': { params: unknown; result: unknown };
  'base.query': { params: unknown; result: unknown };
  'base.create_entry': { params: unknown; result: unknown };
  'base.set_property': { params: unknown; result: unknown };
  'base.reorder_groups': { params: unknown; result: unknown };
  'get_backlinks': { params: unknown; result: (unknown | null) };
  'kiln.graph': { params: unknown; result: Schemas['KilnGraphReply'] };
  'note.upsert': { params: unknown; result: unknown };
  'note.get': { params: unknown; result: (unknown | null) };
  'note.delete': { params: unknown; result: unknown };
  'note.list': { params: unknown; result: (unknown)[] };
  'process_file': { params: unknown; result: unknown };
  'process_batch': { params: unknown; result: unknown };
  'session.create': { params: unknown; result: Schemas['SessionSummary'] };
  'session.list': { params: unknown; result: Schemas['SessionListReply'] };
  'session.get': { params: WithSessionId<Record<string, never>>; result: Schemas['SessionDetail'] };
  'session.pause': { params: WithSessionId<Record<string, never>>; result: unknown };
  'session.resume': { params: WithSessionId<Record<string, never>>; result: unknown };
  'session.resume_from_storage': { params: WithSessionId<unknown>; result: unknown };
  'session.history': { params: WithSessionId<unknown>; result: unknown };
  'session.end': { params: WithSessionId<Record<string, never>>; result: unknown };
  'session.archive': { params: WithSessionId<Record<string, never>>; result: unknown };
  'session.unarchive': { params: WithSessionId<Record<string, never>>; result: unknown };
  'session.delete': { params: WithSessionId<Record<string, never>>; result: unknown };
  'session.compact': { params: WithSessionId<Record<string, never>>; result: unknown };
  'session.subscribe': { params: unknown; result: unknown };
  'session.unsubscribe': { params: unknown; result: unknown };
  'session.configure_agent': { params: WithSessionId<unknown>; result: unknown };
  'session.send_message': { params: WithSessionId<unknown>; result: Schemas['SendOutcome'] };
  'session.cancel': { params: WithSessionId<Record<string, never>>; result: unknown };
  'session.clear': { params: WithSessionId<Record<string, never>>; result: unknown };
  'session.connect_kiln': { params: WithSessionId<Schemas['NamedKiln']>; result: unknown };
  'session.disconnect_kiln': { params: WithSessionId<Schemas['NamedKiln']>; result: unknown };
  'session.set_workspace': { params: WithSessionId<Schemas['WorkspaceChoice']>; result: unknown };
  'session.list_models': { params: WithSessionId<Record<string, never>>; result: unknown };
  'session.list_modes': { params: WithSessionId<Record<string, never>>; result: unknown };
  'session.commands': { params: WithSessionId<Record<string, never>>; result: unknown };
  'session.list_knobs': { params: WithSessionId<Record<string, never>>; result: unknown };
  'session.knob.set': { params: WithSessionId<Schemas['KnobValue']>; result: unknown };
  'session.knob.get': { params: WithSessionId<unknown>; result: Schemas['KnobValue'] };
  'session.list_agent_options': { params: WithSessionId<Record<string, never>>; result: unknown };
  'session.set_agent_option': { params: unknown; result: unknown };
  'session.cache_stats': { params: WithSessionId<Record<string, never>>; result: unknown };
  'session.add_notification': { params: WithSessionId<unknown>; result: unknown };
  'session.list_notifications': { params: WithSessionId<Record<string, never>>; result: unknown };
  'session.dismiss_notification': { params: WithSessionId<unknown>; result: unknown };
  'notification.list': { params: unknown; result: unknown };
  'notification.dismiss': { params: unknown; result: unknown };
  'session.interaction_respond': { params: WithSessionId<unknown>; result: unknown };
  'session.pending_interactions': { params: null; result: unknown };
  'session.set_plugin_approval': { params: WithSessionId<unknown>; result: unknown };
  'session.get_plugin_approval': { params: WithSessionId<unknown>; result: unknown };
  'session.list_plugin_approvals': { params: WithSessionId<Record<string, never>>; result: unknown };
  'session.inject_context': { params: WithSessionId<unknown>; result: unknown };
  'session.test_interaction': { params: WithSessionId<unknown>; result: unknown };
  'session.fork': { params: WithSessionId<unknown>; result: unknown };
  'session.set_title': { params: WithSessionId<Schemas['Title']>; result: unknown };
  'session.generate_title': { params: WithSessionId<Record<string, never>>; result: unknown };
  'session.search': { params: unknown; result: Schemas['SessionSearchResponse'] };
  'session.events_after': { params: WithSessionId<unknown>; result: (Schemas['SessionEventMessage'])[] };
  'session.list_persisted': { params: unknown; result: unknown };
  'session.render_markdown': { params: WithSessionId<unknown>; result: unknown };
  'session.export_to_file': { params: WithSessionId<unknown>; result: unknown };
  'session.replay': { params: unknown; result: unknown };
  'session.cleanup': { params: unknown; result: unknown };
  'session.reindex': { params: null; result: unknown };
  'session.undo': { params: WithSessionId<unknown>; result: unknown };
  'session.can_undo': { params: WithSessionId<Record<string, never>>; result: unknown };
  'session.undo_depth': { params: WithSessionId<Record<string, never>>; result: unknown };
  'plugin.reload': { params: unknown; result: Schemas['PluginReloadReply'] };
  'plugin.list': { params: null; result: unknown };
  'plugin.commands': { params: null; result: Schemas['PluginCommandsReply'] };
  'plugin.publications': { params: unknown; result: Schemas['PluginPublicationsReply'] };
  'surface.list': { params: unknown; result: Schemas['SurfaceListReply'] };
  'surface.get': { params: unknown; result: unknown };
  'plugin.options': { params: unknown; result: Schemas['PluginOptionsReply'] };
  'plugin.option_get': { params: unknown; result: Schemas['PluginOptionValue'] };
  'plugin.option_set': { params: unknown; result: Schemas['PluginAck'] };
  'plugin.option_execute': { params: unknown; result: Schemas['PluginAck'] };
  'session.status': { params: WithSessionId<Record<string, never>>; result: unknown };
  'plugin.run_command': { params: Schemas['PluginRunCommandRequest']; result: Schemas['PluginRunCommandReply'] };
  'plugin.install': { params: Schemas['PluginInstallRequest']; result: Schemas['PluginInstallReply'] };
  'plugin.remove': { params: unknown; result: Schemas['PluginRemoveReply'] };
  'lua.init_session': { params: WithSessionId<unknown>; result: unknown };
  'lua.shutdown_session': { params: WithSessionId<Record<string, never>>; result: unknown };
  'lua.discover_plugins': { params: unknown; result: unknown };
  'lua.plugin_health': { params: unknown; result: unknown };
  'lua.generate_stubs': { params: unknown; result: unknown };
  'lua.run_plugin_tests': { params: unknown; result: unknown };
  'lua.register_commands': { params: WithSessionId<unknown>; result: unknown };
  'lua.eval': { params: unknown; result: unknown };
  'config.get': { params: unknown; result: unknown };
  'config.set': { params: unknown; result: unknown };
  'config.save': { params: unknown; result: unknown };
  'config.reset': { params: unknown; result: unknown };
  'config.pop': { params: unknown; result: unknown };
  'config.unset': { params: unknown; result: unknown };
  'config.origin': { params: unknown; result: unknown };
  'config.effective': { params: null; result: unknown };
  'config.controls': { params: null; result: unknown };
  'ui.config': { params: unknown; result: unknown };
  'ui.set_theme': { params: unknown; result: unknown };
  'project.register': { params: unknown; result: Schemas['Project'] };
  'project.unregister': { params: unknown; result: unknown };
  'project.list': { params: null; result: (Schemas['Project'])[] };
  'project.get': { params: unknown; result: (Schemas['Project'] | null) };
  'project.open_kilns': { params: unknown; result: unknown };
  'project.registry_list': { params: null; result: unknown };
  'scm.clone': { params: Schemas['ScmCloneRequest']; result: Schemas['ScmCloneResponse'] };
  'fs.list_dir': { params: unknown; result: Schemas['FsListing'] };
  'diff.get': { params: unknown; result: Schemas['Diffset'] };
  'diff.file': { params: unknown; result: Schemas['DiffFileText'] };
  'diff.comment': { params: Schemas['DiffCommentRequest']; result: Schemas['DiffCommentReply'] };
  'diff.resolve_comment': { params: Schemas['DiffCommentKey']; result: Schemas['DiffResolveCommentReply'] };
  'diff.delete_comment': { params: Schemas['DiffCommentKey']; result: Schemas['DiffDeleteCommentReply'] };
  'diff.comments': { params: unknown; result: Schemas['DiffCommentsReply'] };
  'proposal.list': { params: unknown; result: (Schemas['Proposal'])[] };
  'proposal.get': { params: unknown; result: Schemas['Proposal'] };
  'proposal.accept': { params: unknown; result: Schemas['Proposal'] };
  'proposal.reject': { params: unknown; result: Schemas['Proposal'] };
  'proposal.dismiss': { params: unknown; result: Schemas['Proposal'] };
  'proposal.resolve': { params: unknown; result: Schemas['Proposal'] };
  'fs.read': { params: unknown; result: unknown };
  'fs.write': { params: unknown; result: unknown };
  'fs.move': { params: Schemas['FsMoveRequest']; result: Schemas['FsMoveReply'] };
  'fs.mkdir': { params: Schemas['FsPathRequest']; result: unknown };
  'fs.trash': { params: Schemas['FsPathRequest']; result: Schemas['FsTrashReply'] };
  'note.rename': { params: unknown; result: unknown };
  'note.move': { params: unknown; result: unknown };
  'storage.verify': { params: unknown; result: unknown };
  'storage.cleanup': { params: unknown; result: unknown };
  'storage.backup': { params: unknown; result: unknown };
  'storage.restore': { params: unknown; result: unknown };
  'mcp.start': { params: unknown; result: unknown };
  'mcp.stop': { params: null; result: unknown };
  'mcp.status': { params: null; result: Schemas['McpStatus'] };
  'skills.list': { params: unknown; result: Schemas['SkillsReply'] };
  'skills.get': { params: unknown; result: Schemas['SkillDetail'] };
  'skills.search': { params: unknown; result: Schemas['SkillsReply'] };
  'agents.list_profiles': { params: null; result: unknown };
  'agents.list_cards': { params: unknown; result: unknown };
  'agents.resolve_profile': { params: unknown; result: (unknown | null) };
  'models.list': { params: unknown; result: unknown };
  'providers.list': { params: unknown; result: unknown };
  'embeddings.models': { params: unknown; result: unknown };
  'subagent.collect': { params: unknown; result: unknown };
  'webhook.receive': { params: unknown; result: Schemas['WebhookReceiveReply'] };
  'suggest_links': { params: unknown; result: unknown };
  'workflow.start': { params: WithSessionId<unknown>; result: unknown };
  'workflow.approve_gate': { params: WithSessionId<unknown>; result: unknown };
  'workflow.status': { params: WithSessionId<Record<string, never>>; result: unknown };
  'workflow.cancel': { params: WithSessionId<Record<string, never>>; result: unknown };
}

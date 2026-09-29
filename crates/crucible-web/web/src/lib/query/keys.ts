import type { InvalidateQueryFilters, QueryKey } from '@tanstack/solid-query';
import { diffsetKey } from '@/lib/diffset';

/**
 * One key factory for every server entity, from the plan's Part F.
 *
 * A factory, not a literal at the call site, because a mutation and an SSE
 * event must invalidate the same array the read used. A session key is a
 * prefix of every key under it, so one `invalidateQueries` on `session(id)`
 * reaches that session's history, models, modes, knobs and scope.
 */
export const keys = {
  bases: () => ['bases'] as const,
  baseQuery: (request: unknown) => ['bases', request] as const,
  kilns: () => ['kilns'] as const,
  config: () => ['config'] as const,
  projects: () => ['projects'] as const,
  project: (path: string) => ['project', path] as const,
  targetProviders: (axis: 'workspace' | 'runtime') => ['targets', 'providers', axis] as const,
  // The workspace belongs in the key: a provider answers PER project — a
  // branch list belongs to a repository — so one entry per (plugin, axis)
  // would serve project A's branches for project B, and a session would be
  // created against a worktree of the wrong repository.
  providerTargets: (plugin: string, axis: string, workspace?: string) =>
    ['targets', 'provider', plugin, axis, workspace] as const,
  workspaceTargets: (workspace?: string) => ['targets', 'workspace', workspace] as const,

  sessions: (includeArchived: boolean) => ['sessions', { includeArchived }] as const,
  session: (id: string) => ['session', id] as const,
  sessionHistory: (id: string) => ['session', id, 'history'] as const,
  sessionModels: (id: string) => ['session', id, 'models'] as const,
  sessionModes: (id: string) => ['session', id, 'modes'] as const,
  sessionStatus: (id: string) => ['session', id, 'status'] as const,
  sessionKnobs: (id: string) => ['session', id, 'knobs'] as const,
  sessionAgentOptions: (id: string) => ['session', id, 'config/agent-options'] as const,
  // One key shape for every session knob — model, mode, context strategy,
  // precognition, plugin turn limit — so a knob added later needs no sibling
  // key here.
  sessionKnob: (id: string, knob: string) => ['session', id, 'knob', knob] as const,
  sessionPluginApprovals: (id: string) => ['session', id, 'config/plugin-approvals'] as const,
  sessionScope: (id: string) => ['session', id, 'scope'] as const,
  allModels: () => ['models', 'all'] as const,
  providers: () => ['providers'] as const,
  pendingInteractions: () => ['interactions', 'pending'] as const,

  agents: () => ['agents'] as const,
  pluginList: () => ['plugins', 'list'] as const,
  pluginOptions: () => ['plugins', 'options'] as const,
  pluginOption: (plugin: string, path: readonly string[]) =>
    ['plugins', 'option', plugin, ...path] as const,
  // Base key for the whole publications family, and the specific key an
  // SSE `publication_changed` event invalidates. The order is (plugin, key)
  // everywhere, as Part F reconciled it.
  pluginPublications: (plugin?: string, key?: string) =>
    ['plugins', 'publications', plugin, key] as const,
  pluginCommands: () => ['plugins', 'commands'] as const,
  surfaces: () => ['surfaces'] as const,
  skillsList: (kiln: string) => ['skills', 'list', kiln] as const,
  skillsSearch: (kiln: string, query: string) => ['skills', 'search', kiln, query] as const,
  skillDetail: (name: string, kiln: string) => ['skills', 'detail', name, kiln] as const,
  slashCommands: (sessionId?: string) =>
    sessionId ? (['commands', 'slash', sessionId] as const) : (['commands', 'slash'] as const),
  mcpStatus: () => ['mcp', 'status'] as const,

  fsDir: (path: string) => ['fs', 'dir', path] as const,
  fsFile: (path: string) => ['fs', 'file', path] as const,
  notesList: (kiln: string) => ['notes', 'list', kiln] as const,
  notesResolve: (kiln: string, name: string) => ['notes', 'resolve', kiln, name] as const,
  // Every held resolution, whatever kiln it belongs to. It is a factory like
  // the rest, because an invalidation that spells its own prefix is a literal
  // that no rename of `notesResolve` can reach.
  notesResolvePrefix: () => ['notes', 'resolve'] as const,
  notesBacklinks: (kiln: string, note: string) => ['notes', 'backlinks', kiln, note] as const,
  notesGraph: (kiln: string) => ['notes', 'graph', kiln] as const,
  notesKiln: (kiln: string) => ['notes', 'kiln', kiln] as const,
  // `/api/kiln/files` is the sibling of `/api/kiln/notes`, so it keys into
  // the `notes` family: the filesystem stream drops a kiln's held answers
  // by walking that family, and a key outside it would never be dropped.
  kilnFiles: (kiln: string) => ['notes', 'kiln-files', kiln] as const,
  canvas: (path: string) => ['canvas', path] as const,
  searchSemantic: (kiln: string, q: string) => ['search', 'semantic', kiln, q] as const,
  searchGrep: (root: string, q: string, glob?: string) =>
    ['search', 'grep', root, q, glob] as const,
  searchSessions: (q: string, kiln?: string) => ['search', 'sessions', q, kiln] as const,
  // One diffset, by the client key of its source (`diffsetKey`). The file
  // texts sit under it, so one invalidation of the diffset reaches them too.
  diffset: (key: string) => ['diff', key] as const,
  // A session record can span more than one root, and two roots can hold
  // the same relative path. The root is therefore part of the file key.
  diffFile: (key: string, root: string, path: string, from?: string) =>
    ['diff', key, 'file', root, path, from] as const,
  diffComments: (key: string) => ['diff', key, 'comments'] as const,
  proposalFamily: () => ['proposal'] as const,
  publicationsFamily: () => ['plugins', 'publications'] as const,
  fsFamily: () => ['fs'] as const,
  notesFamily: () => ['notes'] as const,
  diffFamily: () => ['diff'] as const,
  proposal: (id: string) => ['proposal', id] as const,
  // The proposals in the Inbox. A proposal belongs to no session, so the
  // list has no session in its key.
  proposals: () => ['proposals'] as const,
  recents: () => ['recents'] as const,
} as const;

/** The text that starts the diffset key of each proposal. */
const PROPOSAL_DIFFSET_PREFIX = diffsetKey({ kind: 'proposal', id: '' });

/**
 * Every proposal diffset, and nothing else under `diff`. A branch diff, a
 * working-tree diff and their comments share that family, and a proposal
 * change says nothing about them.
 */
const proposalDiffsets: InvalidateQueryFilters = {
  queryKey: keys.diffFamily(),
  predicate: query => {
    const key = query.queryKey[1];
    return typeof key === 'string' && key.startsWith(PROPOSAL_DIFFSET_PREFIX);
  },
};

/**
 * What a proposal change makes stale: the Inbox list, the proposal and its
 * diffset. Without ids it names every proposal and every proposal diffset.
 * The system route reconciles the family; a decision and an event name ids.
 */
export function proposalRefreshTargets(ids?: readonly string[]): (QueryKey | InvalidateQueryFilters)[] {
  if (ids === undefined) return [keys.proposals(), keys.proposalFamily(), proposalDiffsets];
  return [
    keys.proposals(),
    ...ids.flatMap(id => [keys.proposal(id), keys.diffset(diffsetKey({ kind: 'proposal', id }))]),
  ];
}

/**
 * What the system stream reconciles at each open and each gap: every
 * proposal and every plugin publication, which are the two kinds of change
 * that the stream carries.
 */
export function systemReconcileTargets(): (QueryKey | InvalidateQueryFilters)[] {
  return [...proposalRefreshTargets(), keys.publicationsFamily()];
}

/**
 * One key factory for every server entity, from the plan's Part F.
 *
 * A factory, not a literal at the call site, because a mutation and an SSE
 * event must invalidate the same array the read used. A session key is a
 * prefix of every key under it, so one `invalidateQueries` on `session(id)`
 * reaches that session's history, models, modes, knobs and scope.
 */
export const keys = {
  kilns: () => ['kilns'] as const,
  config: () => ['config'] as const,
  projects: () => ['projects'] as const,
  project: (path: string) => ['project', path] as const,
  targetProviders: (axis: 'workspace' | 'runtime') =>
    ['targets', 'providers', axis] as const,
  providerTargets: (plugin: string, axis: string) =>
    ['targets', 'provider', plugin, axis] as const,
  workspaceTargets: (workspace?: string) =>
    ['targets', 'workspace', workspace] as const,

  sessions: (includeArchived: boolean) =>
    ['sessions', { includeArchived }] as const,
  session: (id: string) => ['session', id] as const,
  sessionHistory: (id: string) => ['session', id, 'history'] as const,
  sessionModels: (id: string) => ['session', id, 'models'] as const,
  sessionModes: (id: string) => ['session', id, 'modes'] as const,
  sessionStatus: (id: string) => ['session', id, 'status'] as const,
  sessionKnobs: (id: string) => ['session', id, 'knobs'] as const,
  sessionAgentOptions: (id: string) =>
    ['session', id, 'config/agent-options'] as const,
  sessionPrecognition: (id: string) =>
    ['session', id, 'config/precognition'] as const,
  sessionContextStrategy: (id: string) =>
    ['session', id, 'config/context-strategy'] as const,
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
  skillsSearch: (kiln: string, query: string) =>
    ['skills', 'search', kiln, query] as const,
  skillDetail: (name: string, kiln: string) =>
    ['skills', 'detail', name, kiln] as const,
  slashCommands: () => ['commands', 'slash'] as const,
  mcpStatus: () => ['mcp', 'status'] as const,

  fsDir: (path: string) => ['fs', 'dir', path] as const,
  fsFile: (path: string) => ['fs', 'file', path] as const,
  notesList: (kiln: string) => ['notes', 'list', kiln] as const,
  notesResolve: (kiln: string, name: string) =>
    ['notes', 'resolve', kiln, name] as const,
  notesBacklinks: (kiln: string, note: string) =>
    ['notes', 'backlinks', kiln, note] as const,
  notesGraph: (kiln: string) => ['notes', 'graph', kiln] as const,
  notesKiln: (kiln: string) => ['notes', 'kiln', kiln] as const,
  canvas: (path: string) => ['canvas', path] as const,
  searchSemantic: (kiln: string, q: string) =>
    ['search', 'semantic', kiln, q] as const,
  searchGrep: (root: string, q: string, glob?: string) =>
    ['search', 'grep', root, q, glob] as const,
  searchSessions: (q: string, kiln?: string) =>
    ['search', 'sessions', q, kiln] as const,
  review: (sessionId: string) => ['review', sessionId] as const,
  recents: () => ['recents'] as const,
} as const;

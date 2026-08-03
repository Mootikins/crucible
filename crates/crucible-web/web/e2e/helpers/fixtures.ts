// RawSession shape (what Axum returns, before mapSession() transforms it)
export const MOCK_SESSION = {
  session_id: 'test-session-001',
  type: 'chat' as const,
  kiln: '/home/user/notes',
  workspace: '/home/user/project',
  state: 'active' as const,
  title: 'Test Session',
  agent_model: 'llama3.2',
  started_at: '2026-01-01T00:00:00Z',
  event_count: 0,
};

// The single-session GET (session.get) shape, distinct from the list shape
// above: model/mode live in a nested `agent` object, there is NO top-level
// `agent_model`, and there's `connected_kilns`/`continued_from` (see
// handle_session_get). Returning this from the GET mock keeps getSession()
// mapping bugs (e.g. reading model from the wrong level) from hiding.
export const MOCK_SESSION_DETAIL = {
  session_id: 'test-session-001',
  type: 'chat' as const,
  kiln: '/home/user/notes',
  workspace: '/home/user/project',
  connected_kilns: ['/home/user/notes'],
  state: 'active' as const,
  title: 'Test Session',
  continued_from: null,
  agent: { model: 'llama3.2', mode: 'chat' },
  started_at: '2026-01-01T00:00:00Z',
};
export const MOCK_SESSION_2 = {
  session_id: 'test-session-002',
  type: 'chat' as const,
  kiln: '/home/user/notes',
  workspace: '/home/user/project',
  state: 'active' as const,
  title: 'Second Session',
  agent_model: 'llama3.2',
  started_at: '2026-01-01T01:00:00Z',
  event_count: 5,
};

export const MOCK_PROVIDERS = {
  providers: [
    {
      name: 'ollama',
      provider_type: 'ollama',
      available: true,
      default_model: 'llama3.2',
      models: ['llama3.2', 'mistral'],
      endpoint: 'http://localhost:11434',
    },
  ],
};

// Real wire shape: kiln.list returns entries, not bare path strings.
export const MOCK_KILNS = {
  kilns: [{ path: '/home/user/notes', name: 'My Kiln' }],
};

export const MOCK_CONFIG = {
  kiln_path: '/home/user/notes',
};

// What plugins published about themselves, keyed by contribution kind then
// plugin. The composer reads its isolation offer from here rather than from
// plugin config, so a second isolating plugin needs no frontend change.
/**
 * Target providers on both axes, as plugins publish them.
 *
 * The targets themselves are not here — they are enumerated on demand through
 * `targets_command`, which `MOCK_PLUGIN_COMMAND` answers.
 */
export const MOCK_PUBLICATIONS = {
  publications: {
    targets: {
      oci: { axis: 'runtime', label: 'Container', targets_command: 'oci.targets' },
      worktree: {
        axis: 'workspace',
        label: 'Worktree',
        targets_command: 'worktree.targets',
        resolve_command: 'worktree.resolve',
      },
    },
  },
};

/** What each provider's `targets_command` answers, by command name. */
export const MOCK_PLUGIN_TARGETS: Record<string, { targets: object[] }> = {
  'oci.targets': {
    targets: [
      { value: '', label: 'Default', hint: 'alpine:latest' },
      { value: 'rust', label: 'rust', hint: 'rust:1-bookworm' },
      { value: 'throwaway', label: 'throwaway' },
    ],
  },
  'worktree.targets': {
    targets: [
      { value: 'master', label: 'master', hint: 'current' },
      { value: 'feat/x', label: 'feat/x', hint: 'new worktree' },
    ],
  },
};

export const MOCK_PROJECT = {
  path: '/home/user/project',
  name: 'project',
  kilns: [{ path: '/home/user/notes', name: 'My Kiln' }],
  last_accessed: '2026-01-01T00:00:00Z',
};

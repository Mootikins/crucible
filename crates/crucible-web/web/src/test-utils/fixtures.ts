import type {
  Session,
  SessionState,
  SessionType,
  ProviderInfo,
  NoteEntry,
} from '@/lib/types';

/**
 * Mock Session object for testing.
 * Matches the Session interface from lib/types.ts
 */
export const mockSession = {
  id: 'session-001',
  session_type: 'chat' as SessionType,
  kiln: 'default',
  workspace: 'workspace-001',
  state: 'active' as SessionState,
  title: 'Test Session',
  agent_model: 'ollama:neural-chat',
  started_at: '2026-03-10T10:00:00Z',
  event_count: 42,
} satisfies Session;

/**
 * Mock array of provider information for testing.
 * Includes Ollama and OpenAI providers.
 */
export const mockProviders: ProviderInfo[] = [
  {
    name: 'ollama',
    provider_type: 'ollama',
    available: true,
    default_model: 'neural-chat',
    models: ['neural-chat', 'mistral', 'llama2'],
    endpoint: 'http://localhost:11434',
  },
  {
    name: 'openai',
    provider_type: 'openai',
    available: true,
    default_model: 'gpt-4',
    models: ['gpt-4', 'gpt-3.5-turbo'],
  },
];

/**
 * Mock array of available model names for testing.
 */
export const mockModels: string[] = [
  'ollama:neural-chat',
  'ollama:mistral',
  'ollama:llama2',
  'openai:gpt-4',
  'openai:gpt-3.5-turbo',
];

/**
 * Mock array of note metadata for testing.
 */
export const mockNotes: NoteEntry[] = [
  {
    name: 'Getting Started',
    path: '/docs/getting-started.md',
    title: 'Getting Started with Crucible',
    tags: ['guide', 'intro'],
    updated_at: '2026-03-09T15:30:00Z',
  },
  {
    name: 'Architecture',
    path: '/docs/architecture.md',
    title: 'System Architecture',
    tags: ['architecture', 'design'],
    updated_at: '2026-03-08T12:00:00Z',
  },
  {
    name: 'API Reference',
    path: '/docs/api-reference.md',
    title: 'API Reference',
    tags: ['api', 'reference'],
    updated_at: '2026-03-07T09:45:00Z',
  },
];

/**
 * Mock session search results for testing.
 */
export const mockSearchResults = {
  sessions: [
    {
      id: 'session-001',
      title: 'Test Session',
      kiln: 'default',
      workspace: 'workspace-001',
      state: 'active' as SessionState,
      session_type: 'chat' as SessionType,
      started_at: '2026-03-10T10:00:00Z',
      agent_model: 'ollama:neural-chat',
      event_count: 42,
    },
    {
      id: 'session-002',
      title: 'Another Session',
      kiln: 'default',
      workspace: 'workspace-001',
      state: 'paused' as SessionState,
      session_type: 'agent' as SessionType,
      started_at: '2026-03-09T14:30:00Z',
      agent_model: 'openai:gpt-4',
      event_count: 28,
    },
  ],
};

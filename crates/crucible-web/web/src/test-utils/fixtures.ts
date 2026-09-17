import type { SessionSearchResponse } from '@/lib/types';
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
  session_id: 'session-001',
  type: 'chat' as SessionType,
  kilns: ['default'],
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
    is_local: true,
  },
  {
    name: 'openai',
    provider_type: 'openai',
    available: true,
    default_model: 'gpt-4',
    models: ['gpt-4', 'gpt-3.5-turbo'],
    is_local: false,
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
    properties: {},
    tags: ['guide', 'intro'],
    updated_at: '2026-03-09T15:30:00Z',
  },
  {
    name: 'Architecture',
    path: '/docs/architecture.md',
    title: 'System Architecture',
    properties: {},
    tags: ['architecture', 'design'],
    updated_at: '2026-03-08T12:00:00Z',
  },
  {
    name: 'API Reference',
    path: '/docs/api-reference.md',
    title: 'API Reference',
    properties: {},
    tags: ['api', 'reference'],
    updated_at: '2026-03-07T09:45:00Z',
  },
];

/**
 * Mock session search results for testing.
 *
 * Matched LINES, not sessions: `GET /api/sessions/search` answers the
 * transcript line it matched on, and a caller that wants the session reads
 * `session_id` and asks for it.
 */
export const mockSearchResults: SessionSearchResponse = {
  matches: [
    { session_id: 'session-001', line: 12, context: 'the refactor landed' },
    { session_id: 'session-002', line: 0, context: 'Another Session' },
  ],
  total: 2,
};

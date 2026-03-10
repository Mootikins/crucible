import { describe, it, expect, afterEach } from 'vitest';
import { createMockFetch } from '@/test-utils';
import {
  sendChatMessage,
  createSession,
  listSessions,
  getSession,
  executeCommand,
  listProviders,
  switchModel,
  setThinkingBudget,
  getThinkingBudget,
  saveNote,
  respondToInteraction,
  searchSessions,
  listModels,
} from '../api';

// Preserve original fetch so we can restore it after each test
const originalFetch = global.fetch;

afterEach(() => {
  global.fetch = originalFetch;
});

// Raw session as the backend would return it (snake_case, different field names)
const rawSession = {
  session_id: 'ses-abc',
  type: 'chat',
  kiln: 'default',
  workspace: 'ws-1',
  state: 'active',
  title: 'My Session',
  agent_model: 'ollama:mistral',
  started_at: '2026-03-10T10:00:00Z',
  event_count: 5,
};

// =============================================================================
// sendChatMessage
// =============================================================================

describe('sendChatMessage', () => {
  it('sends POST to /api/chat/send and returns message_id', async () => {
    const mockFetch = createMockFetch({
      'POST /api/chat/send': { body: { message_id: 'msg-001' } },
    });
    global.fetch = mockFetch;

    const result = await sendChatMessage('ses-1', 'Hello world');

    expect(result).toBe('msg-001');
    expect(mockFetch).toHaveBeenCalledOnce();
    const [url, init] = mockFetch.mock.calls[0];
    expect(url).toBe('/api/chat/send');
    expect(init.method).toBe('POST');
    expect(JSON.parse(init.body)).toEqual({ session_id: 'ses-1', content: 'Hello world' });
  });

  it('throws on non-ok response', async () => {
    const mockFetch = createMockFetch({
      'POST /api/chat/send': { status: 500 },
    });
    global.fetch = mockFetch;

    await expect(sendChatMessage('ses-1', 'fail')).rejects.toThrow('Failed to send message: HTTP 500');
  });
});

// =============================================================================
// createSession + mapSession field mapping
// =============================================================================

describe('createSession', () => {
  it('sends POST to /api/session and maps raw response to Session', async () => {
    const mockFetch = createMockFetch({
      'POST /api/session': { body: rawSession },
    });
    global.fetch = mockFetch;

    const session = await createSession({ kiln: 'default' });

    // Verify mapSession field mapping: session_id → id, type → session_type
    expect(session.id).toBe('ses-abc');
    expect(session.session_type).toBe('chat');
    expect(session.kiln).toBe('default');
    expect(session.workspace).toBe('ws-1');
    expect(session.state).toBe('active');
    expect(session.title).toBe('My Session');
    expect(session.agent_model).toBe('ollama:mistral');
    expect(session.started_at).toBe('2026-03-10T10:00:00Z');
    expect(session.event_count).toBe(5);
  });

  it('maps null agent_model and missing event_count with defaults', async () => {
    const rawNoOptionals = {
      ...rawSession,
      agent_model: null,
      event_count: undefined,
    };
    const mockFetch = createMockFetch({
      'POST /api/session': { body: rawNoOptionals },
    });
    global.fetch = mockFetch;

    const session = await createSession({ kiln: 'default' });

    expect(session.agent_model).toBeNull();
    expect(session.event_count).toBe(0); // ?? 0 fallback
  });

  it('throws on non-ok response', async () => {
    const mockFetch = createMockFetch({
      'POST /api/session': { status: 422 },
    });
    global.fetch = mockFetch;

    await expect(createSession({ kiln: 'x' })).rejects.toThrow('Failed to create session: HTTP 422');
  });
});

// =============================================================================
// listSessions
// =============================================================================

describe('listSessions', () => {
  it('fetches /api/session/list without filters and maps sessions', async () => {
    const mockFetch = createMockFetch({
      'GET /api/session/list': { body: { sessions: [rawSession], total: 1 } },
    });
    global.fetch = mockFetch;

    const sessions = await listSessions();

    expect(sessions).toHaveLength(1);
    expect(sessions[0].id).toBe('ses-abc');
    expect(sessions[0].session_type).toBe('chat');
    // Verify URL had no query string
    const [url] = mockFetch.mock.calls[0];
    expect(url).toBe('/api/session/list');
  });

  it('appends filter query params when provided', async () => {
    const mockFetch = createMockFetch({
      'GET /api/session/list': { body: { sessions: [], total: 0 } },
    });
    global.fetch = mockFetch;

    await listSessions({ kiln: 'my-kiln', state: 'active' });

    const [url] = mockFetch.mock.calls[0];
    expect(url).toContain('kiln=my-kiln');
    expect(url).toContain('state=active');
  });

  it('throws on non-ok response', async () => {
    const mockFetch = createMockFetch({
      'GET /api/session/list': { status: 503 },
    });
    global.fetch = mockFetch;

    await expect(listSessions()).rejects.toThrow('Failed to list sessions: HTTP 503');
  });
});

// =============================================================================
// getSession
// =============================================================================

describe('getSession', () => {
  it('fetches /api/session/{id} and maps response', async () => {
    const mockFetch = createMockFetch({
      'GET /api/session/ses-abc': { body: rawSession },
    });
    global.fetch = mockFetch;

    const session = await getSession('ses-abc');

    expect(session.id).toBe('ses-abc');
    expect(session.session_type).toBe('chat');
  });
});

// =============================================================================
// executeCommand
// =============================================================================

describe('executeCommand', () => {
  it('sends POST to /api/session/{id}/command and returns result', async () => {
    const mockFetch = createMockFetch({
      'POST /api/session/ses-1/command': { body: { result: 'Done!', type: 'success' } },
    });
    global.fetch = mockFetch;

    const result = await executeCommand('ses-1', ':help');

    expect(result).toEqual({ result: 'Done!', type: 'success' });
    const [, init] = mockFetch.mock.calls[0];
    expect(JSON.parse(init.body)).toEqual({ command: ':help' });
  });

  it('throws on non-ok response', async () => {
    const mockFetch = createMockFetch({
      'POST /api/session/ses-1/command': { status: 400 },
    });
    global.fetch = mockFetch;

    await expect(executeCommand('ses-1', 'bad')).rejects.toThrow('Failed to execute command: HTTP 400');
  });
});

// =============================================================================
// listProviders
// =============================================================================

describe('listProviders', () => {
  it('fetches /api/providers and returns provider array', async () => {
    const providers = [
      { name: 'ollama', provider_type: 'ollama', available: true, default_model: 'mistral', models: ['mistral'] },
    ];
    const mockFetch = createMockFetch({
      'GET /api/providers': { body: { providers } },
    });
    global.fetch = mockFetch;

    const result = await listProviders();

    expect(result).toEqual(providers);
  });

  it('throws on non-ok response', async () => {
    const mockFetch = createMockFetch({
      'GET /api/providers': { status: 500 },
    });
    global.fetch = mockFetch;

    await expect(listProviders()).rejects.toThrow('Failed to list providers: HTTP 500');
  });
});

// =============================================================================
// switchModel
// =============================================================================

describe('switchModel', () => {
  it('sends POST to /api/session/{id}/model with model_id', async () => {
    const mockFetch = createMockFetch({
      'POST /api/session/ses-1/model': { body: {} },
    });
    global.fetch = mockFetch;

    await switchModel('ses-1', 'openai:gpt-4');

    const [, init] = mockFetch.mock.calls[0];
    expect(init.method).toBe('POST');
    expect(JSON.parse(init.body)).toEqual({ model_id: 'openai:gpt-4' });
  });
});

// =============================================================================
// setThinkingBudget / getThinkingBudget
// =============================================================================

describe('setThinkingBudget', () => {
  it('sends PUT to config/thinking-budget with budget value', async () => {
    const mockFetch = createMockFetch({
      'PUT /api/session/ses-1/config/thinking-budget': { body: {} },
    });
    global.fetch = mockFetch;

    await setThinkingBudget('ses-1', 4096);

    const [, init] = mockFetch.mock.calls[0];
    expect(init.method).toBe('PUT');
    expect(JSON.parse(init.body)).toEqual({ thinking_budget: 4096 });
  });

  it('sends null budget to disable thinking', async () => {
    const mockFetch = createMockFetch({
      'PUT /api/session/ses-1/config/thinking-budget': { body: {} },
    });
    global.fetch = mockFetch;

    await setThinkingBudget('ses-1', null);

    const [, init] = mockFetch.mock.calls[0];
    expect(JSON.parse(init.body)).toEqual({ thinking_budget: null });
  });
});

describe('getThinkingBudget', () => {
  it('fetches thinking budget and returns number', async () => {
    const mockFetch = createMockFetch({
      'GET /api/session/ses-1/config/thinking-budget': { body: { thinking_budget: 2048 } },
    });
    global.fetch = mockFetch;

    const result = await getThinkingBudget('ses-1');
    expect(result).toBe(2048);
  });

  it('returns null when thinking is disabled', async () => {
    const mockFetch = createMockFetch({
      'GET /api/session/ses-1/config/thinking-budget': { body: { thinking_budget: null } },
    });
    global.fetch = mockFetch;

    const result = await getThinkingBudget('ses-1');
    expect(result).toBeNull();
  });
});

// =============================================================================
// saveNote
// =============================================================================

describe('saveNote', () => {
  it('sends PUT to /api/notes/{name} with kiln and content in body', async () => {
    const mockFetch = createMockFetch({
      'PUT /api/notes/My%20Note': { body: {} },
    });
    global.fetch = mockFetch;

    await saveNote('My Note', 'default', '# Hello\nWorld');

    const [, init] = mockFetch.mock.calls[0];
    expect(init.method).toBe('PUT');
    expect(JSON.parse(init.body)).toEqual({ kiln: 'default', content: '# Hello\nWorld' });
  });
});

// =============================================================================
// respondToInteraction
// =============================================================================

describe('respondToInteraction', () => {
  it('sends POST to /api/interaction/respond with session, request, and response', async () => {
    const mockFetch = createMockFetch({
      'POST /api/interaction/respond': { body: {} },
    });
    global.fetch = mockFetch;

    await respondToInteraction('ses-1', 'req-42', { allowed: true, scope: 'session' });

    const [, init] = mockFetch.mock.calls[0];
    expect(init.method).toBe('POST');
    expect(JSON.parse(init.body)).toEqual({
      session_id: 'ses-1',
      request_id: 'req-42',
      response: { allowed: true, scope: 'session' },
    });
  });

  it('throws on non-ok response', async () => {
    const mockFetch = createMockFetch({
      'POST /api/interaction/respond': { status: 404 },
    });
    global.fetch = mockFetch;

    await expect(respondToInteraction('x', 'y', {})).rejects.toThrow('Failed to respond: HTTP 404');
  });
});

// =============================================================================
// searchSessions
// =============================================================================

describe('searchSessions', () => {
  it('sends query params and maps raw sessions', async () => {
    const mockFetch = createMockFetch({
      'GET /api/sessions/search': { body: [rawSession] },
    });
    global.fetch = mockFetch;

    const sessions = await searchSessions('refactor');

    expect(sessions).toHaveLength(1);
    expect(sessions[0].id).toBe('ses-abc');
    const [url] = mockFetch.mock.calls[0];
    expect(url).toContain('q=refactor');
  });
});

// =============================================================================
// listModels
// =============================================================================

describe('listModels', () => {
  it('fetches models for a session and returns string array', async () => {
    const mockFetch = createMockFetch({
      'GET /api/session/ses-1/models': { body: { models: ['a', 'b', 'c'] } },
    });
    global.fetch = mockFetch;

    const models = await listModels('ses-1');
    expect(models).toEqual(['a', 'b', 'c']);
  });
});

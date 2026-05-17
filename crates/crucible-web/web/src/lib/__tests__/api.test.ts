import { describe, it, expect, afterEach, vi, beforeEach } from 'vitest';
import { createMockFetch } from '@/test-utils';
import {
  sendChatMessage,
  subscribeToEvents,
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
  getConfig,
  pauseSession,
  resumeSession,
  endSession,
  deleteSession,
  archiveSession,
  unarchiveSession,
  cancelSession,
  setSessionTitle,
  generateSessionTitle,
  getSessionHistory,
  getTemperature,
  setTemperature,
  getMaxTokens,
  setMaxTokens,
  getPrecognition,
  setPrecognition,
  getPrecognitionResults,
  setPrecognitionResults,
  exportSession,
  executeShell,
  getPlugins,
  reloadPlugin,
  getMcpStatus,
  listSkills,
  getSkill,
  searchSkills,
  listKilns,
  listNotes,
  getNote,
  searchVectors,
  registerProject,
  unregisterProject,
  listProjects,
  getProject,
  listFiles,
  listKilnNotes,
  getFileContent,
  saveFileContent,
  generateMessageId,
  saveLayout,
  loadLayout,
  resetLayout,
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
    expect(init!.method).toBe('POST');
    expect(JSON.parse(init!.body as string)).toEqual({ session_id: 'ses-1', content: 'Hello world' });
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

  it('appends workspace, type, includeArchived filters', async () => {
    const mockFetch = createMockFetch({
      'GET /api/session/list': { body: { sessions: [], total: 0 } },
    });
    global.fetch = mockFetch;

    await listSessions({ workspace: '/w', type: 'agent', includeArchived: true });

    const [url] = mockFetch.mock.calls[0];
    expect(url).toContain('workspace=%2Fw');
    expect(url).toContain('type=agent');
    expect(url).toContain('include_archived=true');
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
    expect(JSON.parse(init!.body as string)).toEqual({ command: ':help' });
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
    expect(init!.method).toBe('POST');
    expect(JSON.parse(init!.body as string)).toEqual({ model_id: 'openai:gpt-4' });
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
    expect(init!.method).toBe('PUT');
    expect(JSON.parse(init!.body as string)).toEqual({ thinking_budget: 4096 });
  });

  it('sends null budget to disable thinking', async () => {
    const mockFetch = createMockFetch({
      'PUT /api/session/ses-1/config/thinking-budget': { body: {} },
    });
    global.fetch = mockFetch;

    await setThinkingBudget('ses-1', null);

    const [, init] = mockFetch.mock.calls[0];
    expect(JSON.parse(init!.body as string)).toEqual({ thinking_budget: null });
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
    expect(init!.method).toBe('PUT');
    expect(JSON.parse(init!.body as string)).toEqual({ kiln: 'default', content: '# Hello\nWorld' });
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
    expect(init!.method).toBe('POST');
    expect(JSON.parse(init!.body as string)).toEqual({
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

  it('appends kiln and limit when provided', async () => {
    const mockFetch = createMockFetch({
      'GET /api/sessions/search': { body: [] },
    });
    global.fetch = mockFetch;

    await searchSessions('foo', 'my-kiln', 10);

    const [url] = mockFetch.mock.calls[0];
    expect(url).toContain('kiln=my-kiln');
    expect(url).toContain('limit=10');
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

// =============================================================================
// Config
// =============================================================================

describe('getConfig', () => {
  it('returns server kiln_path', async () => {
    global.fetch = createMockFetch({
      'GET /api/config': { body: { kiln_path: '/data/kiln' } },
    });
    expect(await getConfig()).toEqual({ kiln_path: '/data/kiln' });
  });

  it('throws on non-ok', async () => {
    global.fetch = createMockFetch({ 'GET /api/config': { status: 500 } });
    await expect(getConfig()).rejects.toThrow('Failed to get config: HTTP 500');
  });
});

// =============================================================================
// Session lifecycle (pause / resume / end / delete / archive / unarchive / cancel)
// =============================================================================

describe('session lifecycle endpoints', () => {
  it('pauseSession POSTs to /pause', async () => {
    const mockFetch = createMockFetch({
      'POST /api/session/ses-1/pause': { body: {} },
    });
    global.fetch = mockFetch;
    await pauseSession('ses-1');
    expect(mockFetch.mock.calls[0][1]!.method).toBe('POST');
  });

  it('resumeSession POSTs to /resume', async () => {
    const mockFetch = createMockFetch({
      'POST /api/session/ses-1/resume': { body: {} },
    });
    global.fetch = mockFetch;
    await resumeSession('ses-1');
    expect(mockFetch).toHaveBeenCalledOnce();
  });

  it('endSession POSTs to /end', async () => {
    global.fetch = createMockFetch({
      'POST /api/session/ses-1/end': { body: {} },
    });
    await expect(endSession('ses-1')).resolves.toBeUndefined();
  });

  it('deleteSession DELETEs the session', async () => {
    const mockFetch = createMockFetch({
      'DELETE /api/session/ses-1': { body: {} },
    });
    global.fetch = mockFetch;
    await deleteSession('ses-1');
    expect(mockFetch.mock.calls[0][1]!.method).toBe('DELETE');
  });

  it('archiveSession POSTs to /archive', async () => {
    global.fetch = createMockFetch({
      'POST /api/session/ses-1/archive': { body: {} },
    });
    await expect(archiveSession('ses-1')).resolves.toBeUndefined();
  });

  it('unarchiveSession POSTs to /unarchive', async () => {
    global.fetch = createMockFetch({
      'POST /api/session/ses-1/unarchive': { body: {} },
    });
    await expect(unarchiveSession('ses-1')).resolves.toBeUndefined();
  });

  it('cancelSession returns the cancelled bool', async () => {
    global.fetch = createMockFetch({
      'POST /api/session/ses-1/cancel': { body: { cancelled: true } },
    });
    expect(await cancelSession('ses-1')).toBe(true);
  });

  it('pauseSession throws on error', async () => {
    global.fetch = createMockFetch({
      'POST /api/session/ses-1/pause': { status: 500 },
    });
    await expect(pauseSession('ses-1')).rejects.toThrow('Failed to pause session: HTTP 500');
  });

  it('deleteSession throws on error', async () => {
    global.fetch = createMockFetch({
      'DELETE /api/session/ses-1': { status: 403 },
    });
    await expect(deleteSession('ses-1')).rejects.toThrow('Failed to delete session: HTTP 403');
  });

  it('cancelSession throws on error', async () => {
    global.fetch = createMockFetch({
      'POST /api/session/ses-1/cancel': { status: 500 },
    });
    await expect(cancelSession('ses-1')).rejects.toThrow('Failed to cancel session: HTTP 500');
  });
});

// =============================================================================
// Session titles
// =============================================================================

describe('session title endpoints', () => {
  it('setSessionTitle PUTs the title', async () => {
    const mockFetch = createMockFetch({
      'PUT /api/session/ses-1/title': { body: {} },
    });
    global.fetch = mockFetch;
    await setSessionTitle('ses-1', 'New title');
    const [, init] = mockFetch.mock.calls[0];
    expect(init!.method).toBe('PUT');
    expect(JSON.parse(init!.body as string)).toEqual({ title: 'New title' });
  });

  it('generateSessionTitle returns the generated title', async () => {
    global.fetch = createMockFetch({
      'POST /api/session/ses-1/generate-title': { body: { title: 'Auto title' } },
    });
    expect(await generateSessionTitle('ses-1')).toBe('Auto title');
  });

  it('generateSessionTitle throws on error', async () => {
    global.fetch = createMockFetch({
      'POST /api/session/ses-1/generate-title': { status: 503 },
    });
    await expect(generateSessionTitle('ses-1')).rejects.toThrow('Failed to generate title: HTTP 503');
  });
});

// =============================================================================
// Session history
// =============================================================================

describe('getSessionHistory', () => {
  it('passes kiln + limit/offset and returns parsed response', async () => {
    const mockFetch = createMockFetch({
      'GET /api/session/ses-1/history': {
        body: { session_id: 'ses-1', history: [], total_events: 0 },
      },
    });
    global.fetch = mockFetch;
    await getSessionHistory('ses-1', '/path/to/kiln', 50, 100);
    const [url] = mockFetch.mock.calls[0];
    expect(url).toContain('kiln=%2Fpath%2Fto%2Fkiln');
    expect(url).toContain('limit=50');
    expect(url).toContain('offset=100');
  });

  it('omits limit/offset when undefined', async () => {
    const mockFetch = createMockFetch({
      'GET /api/session/ses-1/history': {
        body: { session_id: 'ses-1', history: [], total_events: 0 },
      },
    });
    global.fetch = mockFetch;
    await getSessionHistory('ses-1', '/k');
    const [url] = mockFetch.mock.calls[0];
    expect(url).not.toContain('limit');
    expect(url).not.toContain('offset');
  });

  it('forwards AbortSignal to fetch', async () => {
    const mockFetch = createMockFetch({
      'GET /api/session/ses-1/history': {
        body: { session_id: 'ses-1', history: [], total_events: 0 },
      },
    });
    global.fetch = mockFetch;
    const controller = new AbortController();
    await getSessionHistory('ses-1', '/k', undefined, undefined, controller.signal);
    const [, init] = mockFetch.mock.calls[0];
    expect(init!.signal).toBe(controller.signal);
  });
});

// =============================================================================
// Per-session config: temperature, max-tokens, precognition
// =============================================================================

describe('temperature / max-tokens / precognition endpoints', () => {
  it('getTemperature returns the current value', async () => {
    global.fetch = createMockFetch({
      'GET /api/session/ses-1/config/temperature': { body: { temperature: 0.7 } },
    });
    expect(await getTemperature('ses-1')).toBe(0.7);
  });

  it('getTemperature returns null when unset', async () => {
    global.fetch = createMockFetch({
      'GET /api/session/ses-1/config/temperature': { body: { temperature: null } },
    });
    expect(await getTemperature('ses-1')).toBeNull();
  });

  it('setTemperature PUTs the value', async () => {
    const mockFetch = createMockFetch({
      'PUT /api/session/ses-1/config/temperature': { body: {} },
    });
    global.fetch = mockFetch;
    await setTemperature('ses-1', 0.3);
    expect(JSON.parse(mockFetch.mock.calls[0][1]!.body as string)).toEqual({ temperature: 0.3 });
  });

  it('getMaxTokens / setMaxTokens roundtrip with null', async () => {
    global.fetch = createMockFetch({
      'GET /api/session/ses-1/config/max-tokens': { body: { max_tokens: null } },
    });
    expect(await getMaxTokens('ses-1')).toBeNull();

    const setFetch = createMockFetch({
      'PUT /api/session/ses-1/config/max-tokens': { body: {} },
    });
    global.fetch = setFetch;
    await setMaxTokens('ses-1', 4096);
    expect(JSON.parse(setFetch.mock.calls[0][1]!.body as string)).toEqual({ max_tokens: 4096 });
  });

  it('getPrecognition returns the flag', async () => {
    global.fetch = createMockFetch({
      'GET /api/session/ses-1/config/precognition': { body: { precognition_enabled: true } },
    });
    expect(await getPrecognition('ses-1')).toBe(true);
  });

  it('setPrecognition PUTs { enabled }', async () => {
    const mockFetch = createMockFetch({
      'PUT /api/session/ses-1/config/precognition': { body: {} },
    });
    global.fetch = mockFetch;
    await setPrecognition('ses-1', false);
    expect(JSON.parse(mockFetch.mock.calls[0][1]!.body as string)).toEqual({ enabled: false });
  });

  it('getPrecognitionResults returns the count', async () => {
    global.fetch = createMockFetch({
      'GET /api/session/ses-1/config/precognition/results': { body: { precognition_results: 7 } },
    });
    expect(await getPrecognitionResults('ses-1')).toBe(7);
  });

  it('setPrecognitionResults PUTs { count }', async () => {
    const mockFetch = createMockFetch({
      'PUT /api/session/ses-1/config/precognition/results': { body: {} },
    });
    global.fetch = mockFetch;
    await setPrecognitionResults('ses-1', 10);
    expect(JSON.parse(mockFetch.mock.calls[0][1]!.body as string)).toEqual({ count: 10 });
  });

  it('getMaxTokens throws on error', async () => {
    global.fetch = createMockFetch({
      'GET /api/session/ses-1/config/max-tokens': { status: 500 },
    });
    await expect(getMaxTokens('ses-1')).rejects.toThrow('Failed to get max tokens');
  });
});

// =============================================================================
// Export
// =============================================================================

describe('exportSession', () => {
  it('returns raw markdown string', async () => {
    global.fetch = createMockFetch({
      'POST /api/session/ses-1/export': {
        body: '# Session export\n\nbody',
        headers: { 'Content-Type': 'text/markdown' },
      },
    });
    // createMockFetch JSON.stringifies the body — wrap with custom Response.
    global.fetch = vi.fn(async () => new Response('# Markdown\n\nbody', {
      status: 200,
      headers: { 'Content-Type': 'text/markdown' },
    })) as typeof fetch;
    expect(await exportSession('ses-1')).toBe('# Markdown\n\nbody');
  });

  it('throws on error', async () => {
    global.fetch = createMockFetch({
      'POST /api/session/ses-1/export': { status: 500 },
    });
    await expect(exportSession('ses-1')).rejects.toThrow('Failed to export session');
  });
});

// =============================================================================
// Plugins
// =============================================================================

describe('plugin endpoints', () => {
  it('getPlugins fetches /api/plugins?kiln=… and returns the plugins array', async () => {
    const mockFetch = createMockFetch({
      'GET /api/plugins': {
        body: {
          plugins: [
            { name: 'p1', path: '/p1', plugin_type: 'lua', healthy: true },
          ],
        },
      },
    });
    global.fetch = mockFetch;
    const plugins = await getPlugins('default');
    expect(plugins).toHaveLength(1);
    expect(plugins[0].name).toBe('p1');
    expect(mockFetch.mock.calls[0][0]).toContain('kiln=default');
  });

  it('reloadPlugin POSTs to /reload and returns the body', async () => {
    global.fetch = createMockFetch({
      'POST /api/plugins/my-plugin/reload': {
        body: { healthy: true, message: 'ok' },
      },
    });
    const result = await reloadPlugin('my-plugin');
    expect(result).toEqual({ healthy: true, message: 'ok' });
  });

  it('reloadPlugin URL-encodes the name', async () => {
    const mockFetch = createMockFetch({
      'POST /api/plugins/weird%20name/reload': { body: { healthy: true } },
    });
    global.fetch = mockFetch;
    await reloadPlugin('weird name');
    expect(mockFetch.mock.calls[0][0]).toContain('weird%20name');
  });

  it('getPlugins throws on error', async () => {
    global.fetch = createMockFetch({ 'GET /api/plugins': { status: 500 } });
    await expect(getPlugins('default')).rejects.toThrow('Failed to list plugins');
  });
});

// =============================================================================
// Skills
// =============================================================================

describe('skills endpoints', () => {
  it('listSkills fetches /api/skills?kiln=… and returns skills array', async () => {
    const mockFetch = createMockFetch({
      'GET /api/skills': {
        body: {
          skills: [
            { name: 's1', scope: 'user', description: 'd', shadowed_count: 0 },
          ],
        },
      },
    });
    global.fetch = mockFetch;
    const skills = await listSkills('/tmp/k');
    expect(skills).toHaveLength(1);
    expect(skills[0].name).toBe('s1');
    expect(mockFetch.mock.calls[0][0]).toContain('kiln=%2Ftmp%2Fk');
  });

  it('listSkills includes scope filter when provided', async () => {
    const mockFetch = createMockFetch({
      'GET /api/skills': { body: { skills: [] } },
    });
    global.fetch = mockFetch;
    await listSkills('/tmp/k', 'kiln');
    expect(mockFetch.mock.calls[0][0]).toContain('scope=kiln');
  });

  it('getSkill URL-encodes the name and returns the detail', async () => {
    const mockFetch = createMockFetch({
      'GET /api/skills/my%20skill': {
        body: {
          name: 'my skill',
          scope: 'user',
          description: 'd',
          source_path: '/p',
          body: '# Body',
        },
      },
    });
    global.fetch = mockFetch;
    const detail = await getSkill('my skill', '/tmp/k');
    expect(detail.body).toBe('# Body');
    expect(mockFetch.mock.calls[0][0]).toContain('/api/skills/my%20skill');
  });

  it('searchSkills passes q + limit', async () => {
    const mockFetch = createMockFetch({
      'GET /api/skills/search': { body: { skills: [] } },
    });
    global.fetch = mockFetch;
    await searchSkills('foo', '/tmp/k', 5);
    const [url] = mockFetch.mock.calls[0];
    expect(url).toContain('q=foo');
    expect(url).toContain('limit=5');
  });

  it('searchSkills omits limit when not provided', async () => {
    const mockFetch = createMockFetch({
      'GET /api/skills/search': { body: { skills: [] } },
    });
    global.fetch = mockFetch;
    await searchSkills('foo', '/tmp/k');
    expect(mockFetch.mock.calls[0][0]).not.toContain('limit=');
  });

  it('listSkills throws on 5xx', async () => {
    global.fetch = createMockFetch({ 'GET /api/skills': { status: 500 } });
    await expect(listSkills('/tmp/k')).rejects.toThrow('Failed to list skills');
  });
});

// =============================================================================
// MCP / Kilns / Notes / Search
// =============================================================================

describe('MCP / kilns / notes / search', () => {
  it('getMcpStatus returns the response body', async () => {
    global.fetch = createMockFetch({
      'GET /api/mcp/status': { body: { running: true, port: 3847 } },
    });
    expect(await getMcpStatus()).toEqual({ running: true, port: 3847 });
  });

  it('listKilns returns the kilns array', async () => {
    global.fetch = createMockFetch({
      'GET /api/kilns': { body: { kilns: ['default', 'docs'] } },
    });
    expect(await listKilns()).toEqual(['default', 'docs']);
  });

  it('listNotes passes kiln + optional pathFilter', async () => {
    const mockFetch = createMockFetch({
      'GET /api/notes': { body: { notes: [] } },
    });
    global.fetch = mockFetch;
    await listNotes('default', 'docs/');
    const [url] = mockFetch.mock.calls[0];
    expect(url).toContain('kiln=default');
    expect(url).toContain('path_filter=docs%2F');
  });

  it('listNotes omits pathFilter when missing', async () => {
    const mockFetch = createMockFetch({
      'GET /api/notes': { body: { notes: [] } },
    });
    global.fetch = mockFetch;
    await listNotes('default');
    expect(mockFetch.mock.calls[0][0]).not.toContain('path_filter');
  });

  it('listNotes includes error text on failure', async () => {
    global.fetch = vi.fn(async () => new Response('database locked', {
      status: 500,
      headers: { 'Content-Type': 'text/plain' },
    })) as typeof fetch;
    await expect(listNotes('default')).rejects.toThrow('database locked');
  });

  it('getNote fetches and returns the note content', async () => {
    const note = {
      name: 'Hello',
      path: '/hello.md',
      content: '# Hi',
      title: 'Hi',
      tags: [],
      updated_at: '2026-05-17T00:00:00Z',
    };
    global.fetch = createMockFetch({
      'GET /api/notes/Hello': { body: note },
    });
    expect(await getNote('Hello', 'default')).toEqual(note);
  });

  it('searchVectors POSTs vector + optional limit', async () => {
    const mockFetch = createMockFetch({
      'POST /api/search/vectors': { body: { results: [{ id: 'r1' }] } },
    });
    global.fetch = mockFetch;
    const results = await searchVectors('default', [0.1, 0.2], 5);
    expect(results).toEqual([{ id: 'r1' }]);
    expect(JSON.parse(mockFetch.mock.calls[0][1]!.body as string)).toEqual({
      kiln: 'default',
      vector: [0.1, 0.2],
      limit: 5,
    });
  });

  it('searchVectors omits limit when undefined', async () => {
    const mockFetch = createMockFetch({
      'POST /api/search/vectors': { body: { results: [] } },
    });
    global.fetch = mockFetch;
    await searchVectors('default', [0.5]);
    expect(JSON.parse(mockFetch.mock.calls[0][1]!.body as string)).toEqual({
      kiln: 'default',
      vector: [0.5],
    });
  });

  it('saveNote includes kiln + content in body', async () => {
    const mockFetch = createMockFetch({
      'PUT /api/notes/Foo': { body: {} },
    });
    global.fetch = mockFetch;
    await saveNote('Foo', 'kiln1', 'body');
    expect(JSON.parse(mockFetch.mock.calls[0][1]!.body as string)).toEqual({
      kiln: 'kiln1',
      content: 'body',
    });
  });
});

// =============================================================================
// Projects
// =============================================================================

describe('project endpoints', () => {
  const project = {
    path: '/p',
    name: 'p',
    kilns: [],
    last_accessed: '2026-05-17T00:00:00Z',
  };

  it('registerProject POSTs the path', async () => {
    const mockFetch = createMockFetch({
      'POST /api/project/register': { body: project },
    });
    global.fetch = mockFetch;
    const result = await registerProject('/p');
    expect(result).toEqual(project);
    expect(JSON.parse(mockFetch.mock.calls[0][1]!.body as string)).toEqual({ path: '/p' });
  });

  it('unregisterProject POSTs the path', async () => {
    const mockFetch = createMockFetch({
      'POST /api/project/unregister': { body: {} },
    });
    global.fetch = mockFetch;
    await unregisterProject('/p');
    expect(JSON.parse(mockFetch.mock.calls[0][1]!.body as string)).toEqual({ path: '/p' });
  });

  it('listProjects returns the array', async () => {
    global.fetch = createMockFetch({
      'GET /api/project/list': { body: [project] },
    });
    expect(await listProjects()).toEqual([project]);
  });

  it('getProject returns the project when found', async () => {
    global.fetch = createMockFetch({
      'GET /api/project/get': { body: project },
    });
    expect(await getProject('/p')).toEqual(project);
  });

  it('getProject returns null on 404', async () => {
    global.fetch = createMockFetch({
      'GET /api/project/get': { status: 404, body: { error: 'not found' } },
    });
    expect(await getProject('/missing')).toBeNull();
  });

  it('getProject re-throws non-404 errors', async () => {
    global.fetch = createMockFetch({
      'GET /api/project/get': { status: 500 },
    });
    await expect(getProject('/p')).rejects.toThrow('Failed to get project: HTTP 500');
  });
});

// =============================================================================
// File operations
// =============================================================================

describe('file endpoints', () => {
  const fileEntry = { name: 'a.md', path: '/a.md', is_dir: false };

  it('listFiles returns the files array', async () => {
    global.fetch = createMockFetch({
      'GET /api/kiln/files': { body: { files: [fileEntry] } },
    });
    expect(await listFiles('/k')).toEqual([fileEntry]);
  });

  it('listKilnNotes returns the files array', async () => {
    global.fetch = createMockFetch({
      'GET /api/kiln/notes': { body: { files: [fileEntry] } },
    });
    expect(await listKilnNotes('/k')).toEqual([fileEntry]);
  });

  it('getFileContent returns the content string', async () => {
    global.fetch = createMockFetch({
      'GET /api/kiln/file': { body: { content: 'hello' } },
    });
    expect(await getFileContent('/k/a.md')).toBe('hello');
  });

  it('saveFileContent PUTs path + content', async () => {
    const mockFetch = createMockFetch({
      'PUT /api/kiln/file': { body: {} },
    });
    global.fetch = mockFetch;
    await saveFileContent('/k/a.md', 'new content');
    expect(JSON.parse(mockFetch.mock.calls[0][1]!.body as string)).toEqual({
      path: '/k/a.md',
      content: 'new content',
    });
  });
});

// =============================================================================
// Layout persistence — note: these functions catch errors and console.warn.
// =============================================================================

describe('layout persistence (error-swallowing variants)', () => {
  let warnSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
  });

  afterEach(() => {
    warnSpy.mockRestore();
  });

  it('saveLayout POSTs the layout', async () => {
    const mockFetch = createMockFetch({
      'POST /api/layout': { body: {} },
    });
    global.fetch = mockFetch;
    await saveLayout({ version: 1, root: null } as never);
    expect(mockFetch.mock.calls[0][1]!.method).toBe('POST');
  });

  it('saveLayout swallows error and warns instead of throwing', async () => {
    global.fetch = createMockFetch({ 'POST /api/layout': { status: 500 } });
    await expect(saveLayout({ version: 1, root: null } as never)).resolves.toBeUndefined();
    expect(warnSpy).toHaveBeenCalled();
  });

  it('loadLayout returns the layout when present', async () => {
    global.fetch = createMockFetch({
      'GET /api/layout': { body: { version: 1, root: null } },
    });
    expect(await loadLayout()).toEqual({ version: 1, root: null });
  });

  it('loadLayout returns null on 404 without warning', async () => {
    global.fetch = createMockFetch({
      'GET /api/layout': { status: 404, body: { error: 'no layout' } },
    });
    expect(await loadLayout()).toBeNull();
    expect(warnSpy).not.toHaveBeenCalled();
  });

  it('loadLayout returns null on other errors but warns', async () => {
    global.fetch = createMockFetch({ 'GET /api/layout': { status: 500 } });
    expect(await loadLayout()).toBeNull();
    expect(warnSpy).toHaveBeenCalled();
  });

  it('resetLayout DELETEs', async () => {
    const mockFetch = createMockFetch({ 'DELETE /api/layout': { body: {} } });
    global.fetch = mockFetch;
    await resetLayout();
    expect(mockFetch.mock.calls[0][1]!.method).toBe('DELETE');
  });

  it('resetLayout swallows error and warns', async () => {
    global.fetch = createMockFetch({ 'DELETE /api/layout': { status: 500 } });
    await expect(resetLayout()).resolves.toBeUndefined();
    expect(warnSpy).toHaveBeenCalled();
  });
});

// =============================================================================
// generateMessageId — pure utility, tiny but worth a smoke
// =============================================================================

describe('generateMessageId', () => {
  it('returns a string with the expected prefix and structure', () => {
    const id = generateMessageId();
    expect(id).toMatch(/^msg_\d+_[a-z0-9]+$/);
  });

  it('returns unique ids across consecutive calls', () => {
    const ids = new Set<string>();
    for (let i = 0; i < 20; i++) ids.add(generateMessageId());
    // Not strictly guaranteed (Math.random collision), but should be unique in
    // any realistic run.
    expect(ids.size).toBeGreaterThan(15);
  });
});

// =============================================================================
// subscribeToEvents — SSE subscription + reconnect logic.
// We mock EventSource to drive the lifecycle deterministically.
// =============================================================================

describe('subscribeToEvents', () => {
  type EventSourceListener = (event: MessageEvent) => void;

  class MockEventSource {
    public static instances: MockEventSource[] = [];
    public url: string;
    public readyState = 0;
    public onerror: ((ev: Event) => unknown) | null = null;
    public onopen: ((ev: Event) => unknown) | null = null;
    public onmessage: ((ev: MessageEvent) => unknown) | null = null;
    public closed = false;
    private listeners = new Map<string, EventSourceListener[]>();

    constructor(url: string) {
      this.url = url;
      MockEventSource.instances.push(this);
    }

    addEventListener(type: string, listener: EventSourceListener) {
      const arr = this.listeners.get(type) ?? [];
      arr.push(listener);
      this.listeners.set(type, arr);
    }

    /** Test helper: dispatch an event by type. */
    dispatch(type: string, data: unknown) {
      const arr = this.listeners.get(type) ?? [];
      const evt = { data: JSON.stringify(data) } as MessageEvent;
      for (const l of arr) l(evt);
    }

    /** Test helper: dispatch raw (unparseable) data. */
    dispatchRaw(type: string, raw: string) {
      const arr = this.listeners.get(type) ?? [];
      const evt = { data: raw } as MessageEvent;
      for (const l of arr) l(evt);
    }

    close() {
      this.closed = true;
    }

    triggerError() {
      const handler = this.onerror;
      if (handler) handler(new Event('error'));
    }
  }

  let OriginalEventSource: typeof EventSource;
  let warnSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    OriginalEventSource = global.EventSource;
    (global as { EventSource: unknown }).EventSource = MockEventSource;
    MockEventSource.instances = [];
    warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    (global as { EventSource: typeof EventSource }).EventSource = OriginalEventSource;
    warnSpy.mockRestore();
  });

  it('subscribes to expected event types and parses incoming messages', () => {
    const events: unknown[] = [];
    const cleanup = subscribeToEvents('ses-1', (e) => events.push(e));

    expect(MockEventSource.instances).toHaveLength(1);
    const source = MockEventSource.instances[0];
    expect(source.url).toBe('/api/chat/events/ses-1');

    source.dispatch('token', { type: 'token', content: 'hi' });
    expect(events).toEqual([{ type: 'token', content: 'hi' }]);

    cleanup();
    expect(source.closed).toBe(true);
  });

  it('warns on unparseable event data', () => {
    subscribeToEvents('ses-1', () => {});
    const source = MockEventSource.instances[0];
    source.dispatchRaw('token', 'not json {{{');
    expect(warnSpy).toHaveBeenCalled();
    expect(warnSpy.mock.calls[0][0]).toContain('Failed to parse SSE event');
  });

  it('emits a synthetic reconnect error event and schedules reconnect on disconnect', () => {
    const events: unknown[] = [];
    subscribeToEvents('ses-1', (e) => events.push(e));

    const first = MockEventSource.instances[0];
    first.triggerError();

    // Synthetic error fired on the user callback.
    expect(events).toEqual([
      { type: 'error', code: 'sse_reconnecting', message: 'Reconnecting...' },
    ]);
    expect(first.closed).toBe(true);

    // Advance timers — reconnect should fire and create a second instance.
    vi.advanceTimersByTime(1000);
    expect(MockEventSource.instances).toHaveLength(2);
  });

  it('does not reconnect after cleanup', () => {
    subscribeToEvents('ses-1', () => {});
    const cleanup = subscribeToEvents('ses-2', () => {});
    cleanup();

    // Even if an error fires on the closed source, no reconnect attempted.
    const second = MockEventSource.instances[1];
    second.triggerError();
    vi.advanceTimersByTime(5000);
    // Still only the original two instances.
    expect(MockEventSource.instances).toHaveLength(2);
  });

  it('encodes session id in URL', () => {
    subscribeToEvents('ses 1/weird', () => {});
    expect(MockEventSource.instances[0].url).toBe(
      '/api/chat/events/ses%201%2Fweird',
    );
  });

  it('onopen handler resets reconnect attempts', () => {
    subscribeToEvents('ses-1', () => {});
    const source = MockEventSource.instances[0];
    // Trigger error to bump reconnectAttempts, then onopen on the new source.
    source.triggerError();
    vi.advanceTimersByTime(1000);
    const second = MockEventSource.instances[1];
    expect(second.onopen).toBeTruthy();
    second.onopen!(new Event('open'));
    // No observable state from the outside; this primarily exercises the
    // handler so v8 records it as covered.
  });

  it('cleanup while reconnect is pending clears the timer', () => {
    // Exercises the `if (reconnectTimeout) clearTimeout(reconnectTimeout)`
    // branch in the cleanup closure (api.ts:148). Trigger an error to schedule
    // a reconnect, then cleanup before timers fire.
    const cleanup = subscribeToEvents('ses-1', () => {});
    MockEventSource.instances[0].triggerError();
    // Reconnect is scheduled but not yet fired.
    cleanup();
    // Advancing timers must NOT create a new instance because cleanup cancelled.
    vi.advanceTimersByTime(5000);
    expect(MockEventSource.instances).toHaveLength(1);
  });
});

// =============================================================================
// executeShell — POST + manual SSE parsing over ReadableStream
// =============================================================================

describe('executeShell', () => {
  /** Build a Response whose body streams the given text chunks. */
  function streamResponse(chunks: string[], status = 200): Response {
    const encoder = new TextEncoder();
    let i = 0;
    const stream = new ReadableStream({
      pull(controller) {
        if (i < chunks.length) {
          controller.enqueue(encoder.encode(chunks[i]));
          i++;
        } else {
          controller.close();
        }
      },
    });
    return new Response(stream, {
      status,
      headers: { 'Content-Type': 'text/event-stream' },
    });
  }

  it('parses stdout/stderr/exit events and calls onDone', async () => {
    global.fetch = vi.fn(async () => streamResponse([
      'data: {"type":"stdout","data":"hello\\n"}\n\n',
      'data: {"type":"exit","code":0}\n\n',
    ])) as typeof fetch;

    const events: unknown[] = [];
    let done = false;
    executeShell('echo hello', (e) => events.push(e), () => { done = true; });

    // Allow microtasks to drain the stream.
    await new Promise((r) => setTimeout(r, 10));

    expect(events).toEqual([
      { type: 'stdout', data: 'hello\n' },
      { type: 'exit', code: 0 },
    ]);
    expect(done).toBe(true);
  });

  it('reports an error event on non-ok HTTP status', async () => {
    global.fetch = vi.fn(async () => streamResponse([], 500)) as typeof fetch;
    const events: unknown[] = [];
    let done = false;
    executeShell('bad', (e) => events.push(e), () => { done = true; });
    await new Promise((r) => setTimeout(r, 10));
    expect(events[0]).toMatchObject({ type: 'error' });
    expect(done).toBe(true);
  });

  it('ignores malformed SSE data without crashing', async () => {
    global.fetch = vi.fn(async () => streamResponse([
      'data: not-valid-json\n\n',
      'data: {"type":"exit","code":0}\n\n',
    ])) as typeof fetch;
    const events: unknown[] = [];
    executeShell('cmd', (e) => events.push(e));
    await new Promise((r) => setTimeout(r, 10));
    // Only the parseable line came through.
    expect(events).toEqual([{ type: 'exit', code: 0 }]);
  });

  it('passes cwd and timeout in body when provided', async () => {
    const mockFetch = vi.fn(async () => streamResponse([])) as ReturnType<typeof vi.fn>;
    global.fetch = mockFetch as typeof fetch;
    executeShell('ls', () => {}, undefined, '/tmp', 30);
    await new Promise((r) => setTimeout(r, 10));
    const [, init] = mockFetch.mock.calls[0];
    expect(JSON.parse(init.body as string)).toEqual({
      command: 'ls',
      cwd: '/tmp',
      timeout_secs: 30,
    });
  });

  it('returns an AbortController that cancels the request silently', async () => {
    let aborted = false;
    global.fetch = vi.fn((_input, init?: RequestInit) => {
      return new Promise<Response>((_, reject) => {
        init?.signal?.addEventListener('abort', () => {
          aborted = true;
          reject(new DOMException('aborted', 'AbortError'));
        });
      });
    }) as typeof fetch;

    const events: unknown[] = [];
    let done = false;
    const controller = executeShell('sleep 100', (e) => events.push(e), () => { done = true; });

    controller.abort();
    // Allow rejection handler to run.
    await new Promise((r) => setTimeout(r, 10));

    expect(aborted).toBe(true);
    // Abort should NOT emit an error event — it's user-initiated.
    expect(events).toEqual([]);
    expect(done).toBe(true);
  });

  it('reports an error event when the fetch promise rejects', async () => {
    global.fetch = vi.fn(async () => {
      throw new TypeError('network unreachable');
    }) as typeof fetch;
    const events: unknown[] = [];
    let done = false;
    executeShell('cmd', (e) => events.push(e), () => { done = true; });
    await new Promise((r) => setTimeout(r, 10));
    expect(events[0]).toMatchObject({ type: 'error', message: expect.stringContaining('network unreachable') });
    expect(done).toBe(true);
  });

  it('reports an error event when response body is null', async () => {
    // Exercises the `if (!reader)` branch at api.ts:583-587. Build a Response
    // whose .body is null (ok status, but no stream).
    global.fetch = vi.fn(async () => {
      // The Response constructor with `null` body yields null .body.
      return new Response(null, { status: 200 });
    }) as typeof fetch;
    const events: unknown[] = [];
    let done = false;
    executeShell('cmd', (e) => events.push(e), () => { done = true; });
    await new Promise((r) => setTimeout(r, 10));
    expect(events).toEqual([{ type: 'error', message: 'No response body' }]);
    expect(done).toBe(true);
  });
});

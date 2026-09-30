import { describe, it, expect, afterEach, vi, beforeEach } from 'vitest';
import { createMockFetch } from '@/test-utils';
import { FakeEventSource, installFakeEventSource } from '@/test-utils/sse';
import {
  sendChatMessage,
  subscribeToEvents,
  resetEventsConnectionForTests,
  createSession,
  listSessions,
  executeCommand,
  listProviders,
  switchModel,
  respondToInteraction,
  searchSessions,
  listModels,
  getConfig,
  getPluginPublications,
  runPluginCommand,
  PLUGIN_CALLER_HEADER,
  APP_CALLER,
  getTargetProviders,
  getProviderTargets,
  pauseSession,
  resumeSession,
  endSession,
  deleteSession,
  archiveSession,
  getPrecognition,
  setPrecognition,
  exportSession,
  getPlugins,
  reloadPlugin,
  installPlugin,
  removePlugin,
  listDir,
  listNotes,
  registerProject,
  listFiles,
  listKilnNotes,
  getFileContent,
  saveFileContent,
  saveFileIfUnchanged,
  saveLayout,
  loadLayout,
  resetLayout,
  subscribeToSurfaceEvents,
  subscribeToFsEvents,
  subscribeToSystemEvents,
  fetchRawFile,
} from '../api';
import { generateMessageId } from '../turn';

// Preserve original fetch so we can restore it after each test
const originalFetch = global.fetch;

afterEach(() => {
  global.fetch = originalFetch;
});

// Raw session as the backend would return it (snake_case, different field names)
const rawSession = {
  session_id: 'ses-abc',
  type: 'chat',
  kilns: ['default'],
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
  it('sends POST to /api/rpc/session.send_message and returns the outcome', async () => {
    const mockFetch = createMockFetch({
      'POST /api/rpc/session.send_message': { body: { outcome: 'turn', message_id: 'msg-001' } },
    });
    global.fetch = mockFetch;

    const result = await sendChatMessage('ses-1', 'Hello world');

    expect(result).toEqual({ outcome: 'turn', message_id: 'msg-001' });
    expect(mockFetch).toHaveBeenCalledOnce();
    const sent = await mockFetch.sent(0);
    expect(sent.path).toBe('/api/rpc/session.send_message');
    expect(sent.method).toBe('POST');
    expect(sent.body).toEqual({ session_id: 'ses-1', content: 'Hello world' });
  });

  it('refuses a blank message with no comment before the daemon ever sees it', async () => {
    const mockFetch = createMockFetch({});
    global.fetch = mockFetch;

    await expect(sendChatMessage('ses-1', '  ')).rejects.toThrow(
      'Failed to send message: Message cannot be empty',
    );
    expect(mockFetch).not.toHaveBeenCalled();
  });

  it('throws on non-ok response', async () => {
    const mockFetch = createMockFetch({
      'POST /api/rpc/session.send_message': { status: 500 },
    });
    global.fetch = mockFetch;

    await expect(sendChatMessage('ses-1', 'fail')).rejects.toThrow(
      /RPC `session.send_message` failed/,
    );
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

    const session = await createSession({ kilns: ['default'] });

    // Verify mapSession field mapping: session_id → id, type → session_type
    expect(session.session_id).toBe('ses-abc');
    expect(session.type).toBe('chat');
    expect(session.kilns).toEqual(['default']);
    expect(session.workspace).toBe('ws-1');
    expect(session.state).toBe('active');
    expect(session.title).toBe('My Session');
    expect(session.agent_model).toBe('ollama:mistral');
    expect(session.started_at).toBe('2026-03-10T10:00:00Z');
    expect(session.event_count).toBe(5);
  });

  // The create route does not send `event_count` or `agent_model`: the
  // contract marks both optional, and the client no longer substitutes a
  // zero or a null for them. A reader that wants a number says so itself.
  it('passes the create reply through, absences included', async () => {
    const mockFetch = createMockFetch({
      'POST /api/session': {
        body: { ...rawSession, agent_model: null, event_count: undefined },
      },
    });
    global.fetch = mockFetch;

    const session = await createSession({ kilns: ['default'] });

    expect(session.agent_model).toBeNull();
    expect(session.event_count).toBeUndefined();
  });

  // A session nobody has named comes back from the create route with NO title
  // field, and from every other route with an explicit null. `title` is
  // optional AND nullable in the document for exactly that reason.
  it('leaves an absent title absent rather than minting a null', async () => {
    const mockFetch = createMockFetch({
      'POST /api/session': { body: { ...rawSession, title: undefined } },
    });
    global.fetch = mockFetch;

    const session = await createSession({ kilns: ['default'] });

    expect(session.title).toBeUndefined();
  });

  it('throws on non-ok response', async () => {
    const mockFetch = createMockFetch({
      'POST /api/session': { status: 422 },
    });
    global.fetch = mockFetch;

    await expect(createSession({ kilns: ['x'] })).rejects.toThrow(
      'Failed to create session: HTTP 422',
    );
  });

  it('forwards isolation untouched, and omits it when unset', async () => {
    const mockFetch = createMockFetch({ 'POST /api/session': { body: rawSession } });
    global.fetch = mockFetch;

    // A profile name rides through as-is — the client never interprets it.
    await createSession({ kilns: ['default'], isolation: 'throwaway' });
    expect((await mockFetch.sent(0)).body).toMatchObject({ isolation: 'throwaway' });

    // `false` ("no sandbox even if the project has one") must survive: it is
    // an instruction, not a falsy value to drop.
    await createSession({ kilns: ['default'], isolation: false });
    expect((await mockFetch.sent(1)).body).toMatchObject({ isolation: false });

    // Unset stays absent — absent means "resolve normally", which is a
    // different instruction from false.
    await createSession({ kilns: ['default'] });
    expect((await mockFetch.sent(2)).body).not.toHaveProperty('isolation');
  });
});

// =============================================================================
// listSessions
// =============================================================================

describe('listSessions', () => {
  it('calls session.list without filters and maps sessions', async () => {
    const mockFetch = createMockFetch({
      'POST /api/rpc/session.list': { body: { sessions: [rawSession], total: 1 } },
    });
    global.fetch = mockFetch;

    const sessions = await listSessions();

    expect(sessions).toHaveLength(1);
    expect(sessions[0].session_id).toBe('ses-abc');
    expect(sessions[0].type).toBe('chat');
    const sent = await mockFetch.sent(0);
    expect(sent.path).toBe('/api/rpc/session.list');
    expect(sent.body).toEqual({});
  });

  it.each([
    {
      name: 'passes filter fields when provided',
      args: { kiln: 'my-kiln', state: 'active' },
      expectedBody: { kilns: ['my-kiln'], state: 'active' },
    },
    {
      name: 'passes workspace, type, includeArchived filters',
      args: { workspace: '/w', type: 'agent', includeArchived: true },
      expectedBody: { workspace: '/w', type: 'agent', include_archived: true },
    },
  ])(
    '$name',
    async ({
      args,
      expectedBody,
    }: {
      args: Parameters<typeof listSessions>[0];
      expectedBody: Record<string, unknown>;
    }) => {
      const mockFetch = createMockFetch({
        'POST /api/rpc/session.list': { body: { sessions: [], total: 0 } },
      });
      global.fetch = mockFetch;
      await listSessions(args);
      const { body } = await mockFetch.sent(0);
      expect(body).toEqual(expect.objectContaining(expectedBody));
    },
  );

  it('throws on non-ok response', async () => {
    const mockFetch = createMockFetch({
      'POST /api/rpc/session.list': { status: 503 },
    });
    global.fetch = mockFetch;

    await expect(listSessions()).rejects.toThrow(/RPC `session.list` failed/);
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
    expect((await mockFetch.sent(0)).body).toEqual({ command: ':help' });
  });

  it('throws on non-ok response', async () => {
    const mockFetch = createMockFetch({
      'POST /api/session/ses-1/command': { status: 400 },
    });
    global.fetch = mockFetch;

    await expect(executeCommand('ses-1', 'bad')).rejects.toThrow(
      'Failed to execute command: HTTP 400',
    );
  });
});

// =============================================================================
// listProviders
// =============================================================================

describe('listProviders', () => {
  it('calls providers.list and returns provider array', async () => {
    const providers = [
      {
        name: 'ollama',
        provider_type: 'ollama',
        available: true,
        default_model: 'mistral',
        models: ['mistral'],
      },
    ];
    const mockFetch = createMockFetch({
      'POST /api/rpc/providers.list': { body: { providers } },
    });
    global.fetch = mockFetch;

    const result = await listProviders();

    expect(result).toEqual(providers);
  });

  it('throws on non-ok response', async () => {
    const mockFetch = createMockFetch({
      'POST /api/rpc/providers.list': { status: 500 },
    });
    global.fetch = mockFetch;

    await expect(listProviders()).rejects.toThrow(/RPC `providers.list` failed/);
  });
});

// =============================================================================
// switchModel
// =============================================================================

describe('switchModel', () => {
  it('calls session.knob.set with the model knob', async () => {
    const mockFetch = createMockFetch({
      'POST /api/rpc/session.knob.set': { body: {} },
    });
    global.fetch = mockFetch;

    await switchModel('ses-1', 'openai:gpt-4');

    const sent = await mockFetch.sent(0);
    expect(sent.method).toBe('POST');
    expect(sent.body).toEqual({ session_id: 'ses-1', knob: 'model', value: 'openai:gpt-4' });
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

    const sent = await mockFetch.sent(0);
    expect(sent.method).toBe('POST');
    expect(sent.body).toEqual({
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
  // The route answers MATCHED LINES under `matches`, not a bare array of
  // sessions. Reading it as sessions is what left every hit untitled: each
  // field the panel read was undefined.
  it('reads the matched lines the route answers', async () => {
    const mockFetch = createMockFetch({
      'POST /api/rpc/session.search': {
        body: { matches: [{ session_id: 'ses-abc', line: 12, context: 'the refactor' }], total: 1 },
      },
    });
    global.fetch = mockFetch;

    const found = await searchSessions('refactor');

    expect(found.matches).toHaveLength(1);
    expect(found.matches[0].session_id).toBe('ses-abc');
    expect(found.matches[0].line).toBe(12);
    expect(found.matches[0].context).toBe('the refactor');
    expect(found.total).toBe(1);
    const { body } = await mockFetch.sent(0);
    expect(body).toEqual(expect.objectContaining({ query: 'refactor' }));
  });

  // An unscoped search searches nothing, and the daemon says so in a sentence
  // the panel has to be able to show.
  it('carries the daemon note an unscoped search answers', async () => {
    global.fetch = createMockFetch({
      'POST /api/rpc/session.search': {
        body: { matches: [], total: 0, note: "Specify 'kilns' to scope the search" },
      },
    });

    const found = await searchSessions('refactor');

    expect(found.matches).toEqual([]);
    expect(found.note).toBe("Specify 'kilns' to scope the search");
  });

  it('sends kiln and limit when provided', async () => {
    const mockFetch = createMockFetch({
      'POST /api/rpc/session.search': { body: { matches: [], total: 0 } },
    });
    global.fetch = mockFetch;

    await searchSessions('foo', 'my-kiln', 10);

    const { body } = await mockFetch.sent(0);
    expect(body).toEqual({ query: 'foo', kilns: ['my-kiln'], limit: 10 });
  });

  // Scope is kiln-set overlap, so a caller cleared for several kilns states
  // all of them — the whole array travels in one JSON body now, not a
  // repeated query key.
  it('sends every kiln in the scope', async () => {
    const mockFetch = createMockFetch({
      'POST /api/rpc/session.search': { body: { matches: [], total: 0 } },
    });
    global.fetch = mockFetch;

    await searchSessions('foo', ['/kilns/a', '/kilns/b']);

    const { body } = await mockFetch.sent(0);
    expect(body).toEqual(expect.objectContaining({ kilns: ['/kilns/a', '/kilns/b'] }));
  });
});

// =============================================================================
// listModels
// =============================================================================

describe('listModels', () => {
  it('calls session.list_models and returns string array', async () => {
    const mockFetch = createMockFetch({
      'POST /api/rpc/session.list_models': { body: { models: ['a', 'b', 'c'] } },
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
// Target providers — the workspace and runtime axes
// =============================================================================

const publications = (targets: unknown) => ({
  'POST /api/rpc/plugin.publications': { body: { publications: { targets } } },
});

describe('getTargetProviders', () => {
  // The whole reason the axes are separate: a worktree provider and a container
  // provider both publish here, and asking for one must never surface the
  // other. Selecting a workspace target on the runtime chip would send a branch
  // name down the isolation channel, where `oci` raises on names it does not
  // know.
  it('returns only the providers on the axis asked for', async () => {
    global.fetch = createMockFetch(
      publications({
        worktree: { axis: 'workspace', label: 'Worktree', targets_command: 'worktree.targets' },
        oci: { axis: 'runtime', label: 'Container', targets_command: 'oci.targets' },
        ssh: { axis: 'runtime', label: 'Remote Machines', targets_command: 'ssh.targets' },
      }),
    );

    expect(await getTargetProviders('workspace')).toEqual([
      {
        plugin: 'worktree',
        axis: 'workspace',
        label: 'Worktree',
        targets_command: 'worktree.targets',
      },
    ]);

    const runtime = await getTargetProviders('runtime');
    // Sorted by label, so the menu does not reshuffle between loads.
    expect(runtime.map((p) => p.plugin)).toEqual(['oci', 'ssh']);
  });

  it('falls back to the plugin name when a provider published no label', async () => {
    global.fetch = createMockFetch(publications({ worktree: { axis: 'workspace' } }));
    const [provider] = await getTargetProviders('workspace');
    expect(provider.label).toBe('worktree');
    expect(provider.targets_command).toBeUndefined();
  });

  // Publications are opaque plugin JSON. One malformed declaration must cost
  // only itself: a throw here is swallowed by swrLocal and the whole chip
  // silently never renders, so a bad plugin would hide every good one.
  it('skips a malformed declaration without losing the others', async () => {
    global.fetch = createMockFetch(
      publications({
        broken: null,
        alsoBroken: { axis: 'nonsense' },
        worktree: { axis: 'workspace', label: 'Worktree' },
      }),
    );
    expect((await getTargetProviders('workspace')).map((p) => p.plugin)).toEqual(['worktree']);
  });

  it('offers nothing when no plugin published a targets key', async () => {
    global.fetch = createMockFetch({
      'POST /api/rpc/plugin.publications': { body: { publications: {} } },
    });
    expect(await getTargetProviders('runtime')).toEqual([]);
  });
});

describe('getProviderTargets', () => {
  const worktree = {
    plugin: 'worktree',
    axis: 'workspace' as const,
    label: 'Worktree',
    targets_command: 'worktree.targets',
  };

  it('reads a provider’s targets from its command', async () => {
    global.fetch = createMockFetch({
      'POST /api/rpc/plugin.run_command': {
        body: {
          targets: [
            { value: 'feat/x', label: 'feat/x', hint: 'new worktree' },
            { value: 'main', label: 'main' },
          ],
        },
      },
    });

    expect(await getProviderTargets(worktree, '/repo')).toEqual([
      {
        value: 'feat/x',
        label: 'feat/x',
        hint: 'new worktree',
        disabled: false,
        // Built once, here — no caller reassembles `provider:target`.
        spec: 'worktree:feat/x',
        path: undefined,
        current: undefined,
      },
      {
        value: 'main',
        label: 'main',
        hint: undefined,
        disabled: false,
        spec: 'worktree:main',
        path: undefined,
        current: undefined,
      },
    ]);
  });

  it('accepts a bare array as readily as a wrapped one', async () => {
    global.fetch = createMockFetch({
      'POST /api/rpc/plugin.run_command': { body: [{ value: 'main', label: 'main' }] },
    });
    expect(await getProviderTargets(worktree)).toEqual([
      {
        value: 'main',
        label: 'main',
        hint: undefined,
        disabled: false,
        spec: 'worktree:main',
        path: undefined,
        current: undefined,
      },
    ]);
  });

  // The only place this mapping exists now: the session tree labels a checkout
  // with its branch from it, and the files-pane picker jumps to it.
  it('carries an existing checkout path through', async () => {
    global.fetch = createMockFetch({
      'POST /api/rpc/plugin.run_command': {
        body: {
          targets: [
            { value: 'master', label: 'master', path: '/repo', current: true },
            { value: 'feat/x', label: 'feat/x' },
          ],
        },
      },
    });
    const rows = await getProviderTargets(worktree, '/repo');
    expect(rows[0]).toMatchObject({ path: '/repo', current: true });
    expect(rows[1].path).toBeUndefined();
    expect(rows[1].current).toBeUndefined();
  });

  it('drops entries with no value rather than offering unselectable rows', async () => {
    global.fetch = createMockFetch({
      'POST /api/rpc/plugin.run_command': {
        body: { targets: [{ label: 'nameless' }, { value: 'main', label: 'main' }] },
      },
    });
    expect((await getProviderTargets(worktree)).map((t) => t.value)).toEqual(['main']);
  });

  // A provider whose plugin was unloaded, or whose command was renamed, must
  // not take the rest of the menu with it.
  it('answers empty when the command fails', async () => {
    global.fetch = createMockFetch({ 'POST /api/rpc/plugin.run_command': { status: 500 } });
    expect(await getProviderTargets(worktree)).toEqual([]);
  });

  it('asks nothing of a provider that named no command', async () => {
    const called = vi.fn();
    global.fetch = called;
    expect(await getProviderTargets({ ...worktree, targets_command: undefined })).toEqual([]);
    expect(called).not.toHaveBeenCalled();
  });
});

// =============================================================================
// Session lifecycle (pause / resume / end / delete / archive)
// =============================================================================

describe('session lifecycle endpoints', () => {
  it.each([
    {
      name: 'pauseSession calls session.pause',
      fn: pauseSession as (id: string) => Promise<unknown>,
      route: 'POST /api/rpc/session.pause',
    },
  ])('$name', async ({ fn, route }) => {
    const mockFetch = createMockFetch({ [route]: { body: {} } });
    global.fetch = mockFetch;
    await expect(fn('ses-1')).resolves.toBeUndefined();
    expect(mockFetch).toHaveBeenCalledOnce();
    const sent = await mockFetch.sent(0);
    expect(sent.path).toBe(route.split(' ')[1]);
    expect(sent.body).toEqual({ session_id: 'ses-1' });
  });

  it('resumeSession POSTs to /resume', async () => {
    const mockFetch = createMockFetch({ 'POST /api/session/ses-1/resume': { body: {} } });
    global.fetch = mockFetch;
    await expect(resumeSession('ses-1')).resolves.toBeUndefined();
    const sent = await mockFetch.sent(0);
    expect(sent.path).toBe('/api/session/ses-1/resume');
    expect(sent.method).toBe('POST');
  });

  // `endSession`, `archiveSession` and `deleteSession` keep their own REST
  // routes ([[Simplification Plan#Step 19]] item 9): each also releases this
  // web process's own SSE broker entry for the session, which a plain
  // `rpc()` forward cannot reach.
  it('endSession POSTs to /end', async () => {
    const mockFetch = createMockFetch({ 'POST /api/session/ses-1/end': { body: {} } });
    global.fetch = mockFetch;
    await expect(endSession('ses-1')).resolves.toBeUndefined();
    const sent = await mockFetch.sent(0);
    expect(sent.path).toBe('/api/session/ses-1/end');
    expect(sent.method).toBe('POST');
  });

  it('archiveSession POSTs to /archive', async () => {
    const mockFetch = createMockFetch({ 'POST /api/session/ses-1/archive': { body: {} } });
    global.fetch = mockFetch;
    await expect(archiveSession('ses-1')).resolves.toBeUndefined();
    const sent = await mockFetch.sent(0);
    expect(sent.path).toBe('/api/session/ses-1/archive');
    expect(sent.method).toBe('POST');
  });

  it('deleteSession DELETEs the session', async () => {
    const mockFetch = createMockFetch({ 'DELETE /api/session/ses-1': { body: {} } });
    global.fetch = mockFetch;
    await expect(deleteSession('ses-1')).resolves.toBeUndefined();
    const sent = await mockFetch.sent(0);
    expect(sent.path).toBe('/api/session/ses-1');
    expect(sent.method).toBe('DELETE');
  });

  it('pauseSession throws on error', async () => {
    global.fetch = createMockFetch({
      'POST /api/rpc/session.pause': { status: 500 },
    });
    await expect(pauseSession('ses-1')).rejects.toThrow(/RPC `session.pause` failed/);
  });

  it('endSession throws on error', async () => {
    global.fetch = createMockFetch({
      'POST /api/session/ses-1/end': { status: 500 },
    });
    await expect(endSession('ses-1')).rejects.toThrow('Failed to end session: HTTP 500');
  });

  it('archiveSession throws on error', async () => {
    global.fetch = createMockFetch({
      'POST /api/session/ses-1/archive': { status: 500 },
    });
    await expect(archiveSession('ses-1')).rejects.toThrow('Failed to archive session: HTTP 500');
  });

  it('deleteSession throws on error', async () => {
    global.fetch = createMockFetch({
      'DELETE /api/session/ses-1': { status: 403 },
    });
    await expect(deleteSession('ses-1')).rejects.toThrow('Failed to delete session: HTTP 403');
  });
});

// =============================================================================
// Per-session config: precognition
// =============================================================================

describe('precognition endpoints', () => {
  it('getPrecognition returns the flag', async () => {
    global.fetch = createMockFetch({
      'POST /api/rpc/session.knob.get': { body: { knob: 'precognition', value: true } },
    });
    expect(await getPrecognition('ses-1')).toBe(true);
  });

  it('setPrecognition calls session.knob.set with the precognition knob', async () => {
    const mockFetch = createMockFetch({
      'POST /api/rpc/session.knob.set': { body: {} },
    });
    global.fetch = mockFetch;
    await setPrecognition('ses-1', false);
    expect((await mockFetch.sent(0)).body).toEqual({
      session_id: 'ses-1',
      knob: 'precognition',
      value: false,
    });
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
    global.fetch = vi.fn(
      async () =>
        new Response('# Markdown\n\nbody', {
          status: 200,
          headers: { 'Content-Type': 'text/markdown' },
        }),
    ) as typeof fetch;
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
  const richPluginRow = {
    name: 'p1',
    version: '0.1.0',
    source: 'User',
    state: 'Active',
    dir: '/p1',
    tools: 2,
    commands: 1,
    handlers: 0,
    services: 0,
  };

  it('getPlugins fetches /api/plugins and returns rich plugin info', async () => {
    global.fetch = createMockFetch({
      'GET /api/plugins': { body: { plugins: [richPluginRow] } },
    });
    const plugins = await getPlugins();
    expect(plugins).toHaveLength(1);
    expect(plugins[0].name).toBe('p1');
    expect(plugins[0].source).toBe('User');
    expect(plugins[0].state).toBe('Active');
    expect(plugins[0].tools).toBe(2);
  });

  it('reloadPlugin POSTs to /reload and returns the daemon counts', async () => {
    global.fetch = createMockFetch({
      'POST /api/plugins/my-plugin/reload': {
        body: {
          name: 'my-plugin',
          reloaded: true,
          tools: 1,
          commands: 0,
          handlers: 1,
          services: 0,
        },
      },
    });
    const result = await reloadPlugin('my-plugin');
    expect(result.reloaded).toBe(true);
    expect(result.tools).toBe(1);
  });

  it('reloadPlugin URL-encodes the name', async () => {
    const mockFetch = createMockFetch({
      'POST /api/plugins/weird%20name/reload': {
        body: {
          name: 'weird name',
          reloaded: true,
          tools: 0,
          commands: 0,
          handlers: 0,
          services: 0,
        },
      },
    });
    global.fetch = mockFetch;
    await reloadPlugin('weird name');
    expect((await mockFetch.sent(0)).path).toContain('weird%20name');
  });

  it('getPlugins throws on error', async () => {
    global.fetch = createMockFetch({ 'GET /api/plugins': { status: 500 } });
    await expect(getPlugins()).rejects.toThrow('Failed to list plugins');
  });

  it('installPlugin POSTs the request body and returns the result', async () => {
    const mockFetch = createMockFetch({
      'POST /api/plugins': {
        body: {
          name: 'np',
          outcome: { kind: 'cloned', dest: '/tmp/np' },
          manifest: '/tmp/plugins.installed.json',
        },
      },
    });
    global.fetch = mockFetch;
    const result = await installPlugin({ url: 'user/repo', branch: 'main' });
    expect(result.name).toBe('np');
    const body = (await mockFetch.sent(0)).body as Record<string, unknown>;
    expect(body).toEqual({ url: 'user/repo', branch: 'main' });
  });

  it('installPlugin throws on 5xx', async () => {
    global.fetch = createMockFetch({ 'POST /api/plugins': { status: 500 } });
    await expect(installPlugin({ url: 'user/repo' })).rejects.toThrow('Failed to install plugin');
  });

  it.each([
    {
      name: 'removePlugin DELETEs without purge query when purge=false',
      expectPresent: [] as string[],
      expectAbsent: ['purge='],
    },
    {
      name: 'removePlugin appends ?purge=true when purge=true',
      purge: true,
      expectPresent: ['purge=true'],
      expectAbsent: [] as string[],
    },
  ])(
    '$name',
    async ({
      purge,
      expectPresent,
      expectAbsent,
    }: {
      purge?: boolean;
      expectPresent: string[];
      expectAbsent: string[];
    }) => {
      const mockFetch = createMockFetch({
        'DELETE /api/plugins/my-plugin': {
          body: { name: 'my-plugin', manifest: '/tmp/plugins.installed.json', purged_dir: null },
        },
      });
      global.fetch = mockFetch;
      await removePlugin('my-plugin', purge);
      const url = (await mockFetch.sent(0)).url;
      for (const p of expectPresent) expect(url).toContain(p);
      for (const p of expectAbsent) expect(url).not.toContain(p);
    },
  );
});

// =============================================================================
// MCP / Kilns / Notes / Search
// =============================================================================

describe('MCP / kilns / notes / search', () => {
  it('listDir sends root/rel_path/show_ignored and returns the listing envelope', async () => {
    const entries = [
      {
        name: 'web',
        rel_path: 'src/web',
        is_dir: true,
        size: 0,
        modified: 1721270400,
        status: null,
      },
      {
        name: 'api.ts',
        rel_path: 'src/web/api.ts',
        is_dir: false,
        size: 20481,
        modified: 1721270511,
        status: null,
      },
    ];
    // `{ entries, truncated }`, not a bare array: the daemon caps a directory at
    // 1000 entries, and a capped listing has to be distinguishable from a
    // complete one.
    const listing = { entries, truncated: false };
    const mockFetch = createMockFetch({ 'POST /api/rpc/fs.list_dir': { body: listing } });
    global.fetch = mockFetch;
    expect(await listDir('/proj', 'src/web', true)).toEqual(listing);
    const body = (await mockFetch.sent(0)).body;
    expect(body).toEqual({
      root: '/proj',
      rel_path: 'src/web',
      show_ignored: true,
      show_hidden: true,
    });
  });

  it('listDir preserves the truncation flag', async () => {
    // Dropping this on the floor would render a capped folder as if it were
    // the whole thing, which is the failure the cap exists to avoid.
    const listing = { entries: [], truncated: true };
    global.fetch = createMockFetch({ 'POST /api/rpc/fs.list_dir': { body: listing } });
    expect((await listDir('/proj')).truncated).toBe(true);
  });

  it('listDir throws on a non-ok response', async () => {
    global.fetch = createMockFetch({ 'POST /api/rpc/fs.list_dir': { status: 400 } });
    await expect(listDir('/proj')).rejects.toThrow(/RPC `fs.list_dir` failed: HTTP 400/);
  });

  it.each([
    { name: 'listNotes passes kiln + optional pathFilter', kiln: 'default', pathFilter: 'docs/' },
    { name: 'listNotes omits pathFilter when missing', kiln: 'default', pathFilter: undefined },
  ])('$name', async ({ kiln, pathFilter }: { kiln: string; pathFilter?: string }) => {
    const mockFetch = createMockFetch({ 'POST /api/rpc/list_notes': { body: [] } });
    global.fetch = mockFetch;
    await listNotes(kiln, pathFilter);
    const body = (await mockFetch.sent(0)).body;
    expect(body).toEqual(pathFilter ? { kiln, path_filter: pathFilter } : { kiln });
  });

  it('listNotes includes error text on failure', async () => {
    global.fetch = vi.fn(
      async () =>
        new Response('database locked', {
          status: 500,
          headers: { 'Content-Type': 'text/plain' },
        }),
    ) as typeof fetch;
    await expect(listNotes('default')).rejects.toThrow('database locked');
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
    expect((await mockFetch.sent(0)).body).toEqual({ path: '/p' });
  });
});

// =============================================================================
// File operations
// =============================================================================

describe('file endpoints', () => {
  const fileEntry = { name: 'a.md', path: '/a.md', is_dir: false };

  // `GET /api/kiln/files` and `/api/kiln/notes`, which used to answer this
  // shape server-side as `FileEntryRow`, are gone
  // ([[Simplification Plan#Step 19]] item 3): `listFiles`/`listKilnNotes`
  // reshape a `list_notes` row themselves now, so the mock answers that row.
  const noteRow = { name: fileEntry.name, path: fileEntry.path };

  it('listFiles returns the files array', async () => {
    global.fetch = createMockFetch({
      'POST /api/rpc/list_notes': { body: [noteRow] },
    });
    expect(await listFiles('/k')).toEqual([fileEntry]);
  });

  it('listKilnNotes returns the files array', async () => {
    global.fetch = createMockFetch({
      'POST /api/rpc/list_notes': { body: [noteRow] },
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
    expect((await mockFetch.sent(0)).body).toEqual({
      path: '/k/a.md',
      content: 'new content',
    });
  });
});

/**
 * The guarded save is the one write the daemon may refuse, and the one that
 * can be merged instead. Both answers are values a caller branches on.
 */
describe('saveFileIfUnchanged', () => {
  it('sends the base text when it is given one, and reads the merged answer', async () => {
    const mockFetch = createMockFetch({
      'PUT /api/kiln/file': {
        body: { ok: true, content_hash: 'h2', merged: true, content: 'A\nB2\nC\nD\n' },
      },
    });
    global.fetch = mockFetch;

    const answer = await saveFileIfUnchanged('/k/a.md', 'A\nB\nC\nD\n', 'h0', 'A\nB\nC\n');

    expect((await mockFetch.sent(0)).body).toEqual({
      path: '/k/a.md',
      content: 'A\nB\nC\nD\n',
      base_hash: 'h0',
      base_text: 'A\nB\nC\n',
    });
    expect(answer).toEqual({
      ok: true,
      content_hash: 'h2',
      merged: true,
      content: 'A\nB2\nC\nD\n',
    });
  });

  it('sends no base text when it has none', async () => {
    const mockFetch = createMockFetch({
      'PUT /api/kiln/file': { body: { ok: true, content_hash: 'h2' } },
    });
    global.fetch = mockFetch;

    expect(await saveFileIfUnchanged('/k/a.md', 'mine', 'h0')).toEqual({
      ok: true,
      content_hash: 'h2',
    });
    expect((await mockFetch.sent(0)).body).toEqual({
      path: '/k/a.md',
      content: 'mine',
      base_hash: 'h0',
    });
  });

  it('reads a conflict with the texts and the regions it could not settle', async () => {
    global.fetch = createMockFetch({
      'PUT /api/kiln/file': {
        status: 409,
        body: {
          ok: false,
          stale_base: true,
          current_hash: 'h9',
          current_content: 'A\nTHEIRS\n',
          merged_content: 'A\nMINE\n',
          regions: [
            { start_line: 2, end_line: 3, base: 'B\n', ours: 'MINE\n', theirs: 'THEIRS\n' },
          ],
        },
      },
    });

    expect(await saveFileIfUnchanged('/k/a.md', 'A\nMINE\n', 'h0', 'A\nB\n')).toEqual({
      ok: false,
      current_hash: 'h9',
      current_content: 'A\nTHEIRS\n',
      merged_content: 'A\nMINE\n',
      regions: [{ start_line: 2, end_line: 3, base: 'B\n', ours: 'MINE\n', theirs: 'THEIRS\n' }],
    });
  });

  // The refusal a caller that sent no base text gets: a hash and nothing else.
  it('reads a bare refusal as the hash on disk', async () => {
    global.fetch = createMockFetch({
      'PUT /api/kiln/file': { status: 409, body: { ok: false, current_hash: 'h9' } },
    });
    expect(await saveFileIfUnchanged('/k/a.md', 'mine', 'h0')).toEqual({
      ok: false,
      current_hash: 'h9',
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
    expect((await mockFetch.sent(0)).method).toBe('POST');
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
    expect((await mockFetch.sent(0)).method).toBe('DELETE');
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

  it('returns fully unique ids across a fixed-size batch', () => {
    // Date.now() is constant within a tight loop, so uniqueness rests entirely
    // on the random suffix. Pin Math.random to a deterministic, well-separated
    // sequence so we can assert *full* uniqueness (size === count) instead of a
    // probabilistic ">15 of 20" threshold that could flake.
    const count = 50;
    let n = 0;
    const randomSpy = vi.spyOn(Math, 'random').mockImplementation(() => (n++ + 1) / (count + 1));
    try {
      const ids = new Set<string>();
      for (let i = 0; i < count; i++) ids.add(generateMessageId());
      expect(ids.size).toBe(count);
    } finally {
      randomSpy.mockRestore();
    }
  });
});

// =============================================================================
// subscribeToEvents — SSE subscription + reconnect logic.
// We mock EventSource to drive the lifecycle deterministically.
// =============================================================================

describe('subscribeToEvents, subscribeToSurfaceEvents, subscribeToFsEvents, subscribeToSystemEvents', () => {
  // The full mechanics (URL building, reconnect backoff, topic isolation,
  // the shared connection) are tested against the public `sse.ts` API in
  // `lib/query/__tests__/sse.test.ts` and the version handshake in
  // `lib/__tests__/stream-version.test.ts`. This block is the low-level
  // sanity check that each of the four functions still joins the right
  // topic and decodes its own domain's frames — Simplification Plan step 19
  // moved the transport the four share, not what each one reads off it.
  let warnSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    installFakeEventSource();
    warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});
  });

  afterEach(() => {
    resetEventsConnectionForTests();
    warnSpy.mockRestore();
  });

  it("subscribeToEvents joins the session's own topic and decodes its frames", () => {
    const events: unknown[] = [];
    const cleanup = subscribeToEvents('ses-1', (e) => events.push(e));

    expect(FakeEventSource.instances).toHaveLength(1);
    const source = FakeEventSource.instances[0]!;
    expect(source.url).toBe('/api/events?topics=ses-1');

    source.emit('text_delta', { topic: 'ses-1', event: 'text_delta', data: { content: 'hi' } });
    expect(events).toEqual([{ event: 'text_delta', data: { content: 'hi' } }]);

    cleanup();
  });

  it('subscribeToEvents warns on unparseable event data', () => {
    subscribeToEvents('ses-1', () => {});
    const source = FakeEventSource.instances[0]!;
    for (const listener of [...(source.listeners.get('text_delta') ?? [])]) {
      listener({ data: 'not json {{{', lastEventId: '' } as MessageEvent);
    }
    expect(warnSpy).toHaveBeenCalled();
    expect(warnSpy.mock.calls[0]![0]).toContain('Failed to parse SSE event');
  });

  it('encodes a session id with reserved characters into the topics query', () => {
    subscribeToEvents('ses 1/weird', () => {});
    expect(FakeEventSource.instances[0]!.url).toBe('/api/events?topics=ses+1%2Fweird');
  });

  it('subscribeToSurfaceEvents and subscribeToFsEvents join the system topic together', () => {
    const surfaceEvents: unknown[] = [];
    const fsEvents: unknown[] = [];
    const stopSurface = subscribeToSurfaceEvents((e) => surfaceEvents.push(e));
    const stopFs = subscribeToFsEvents((e) => fsEvents.push(e));

    expect(FakeEventSource.instances).toHaveLength(1);
    const source = FakeEventSource.instances[0]!;
    expect(source.url).toBe('/api/events?topics=system');

    source.emit('surface_changed', {
      topic: 'system',
      plugin: 'kanban',
      name: 'board',
      version: 1,
    });
    source.emit('fs_changed', {
      topic: 'system',
      type: 'changed',
      path: '/a.md',
      kind: 'modified',
    });

    expect(surfaceEvents).toEqual([{ plugin: 'kanban', name: 'board', version: 1 }]);
    expect(fsEvents).toEqual([{ type: 'changed', path: '/a.md', kind: 'modified' }]);

    stopSurface();
    stopFs();
  });

  it('subscribeToSystemEvents decodes publication_changed and proposal_changed', () => {
    const events: unknown[] = [];
    const cleanup = subscribeToSystemEvents(
      (e) => events.push(e),
      () => {},
    );

    const source = FakeEventSource.instances[0]!;
    source.emit('publication_changed', { topic: 'system', plugin: 'kanban', key: 'board' });

    expect(events).toEqual([{ event: 'publication_changed', plugin: 'kanban', key: 'board' }]);

    cleanup();
  });

  it('a malformed publication frame is dropped, and the connection keeps reading', () => {
    const events: unknown[] = [];
    const cleanup = subscribeToSystemEvents(
      (e) => events.push(e),
      () => {},
    );
    const source = FakeEventSource.instances[0]!;

    // The shared connection's own dispatcher parses the envelope to read the
    // topic before a domain ever sees the frame, so an unparseable frame is
    // now caught there, not inside `subscribeToSystemEvents`'s own decode.
    for (const listener of [...(source.listeners.get('publication_changed') ?? [])]) {
      listener({ data: 'not json {', lastEventId: '' } as MessageEvent);
    }
    expect(warnSpy.mock.calls[0]![0]).toContain('Failed to parse SSE event');

    source.emit('publication_changed', { topic: 'system', plugin: 'kanban', key: 'board' });
    expect(events).toEqual([{ event: 'publication_changed', plugin: 'kanban', key: 'board' }]);

    cleanup();
    expect(source.closed).toBe(true);
  });
});

// =============================================================================
// fetchRawFile — the one raw-body read (a Blob, not a decoded document).
// =============================================================================

describe('fetchRawFile', () => {
  it('throws the attachment error when the raw fetch fails', async () => {
    global.fetch = vi.fn(async () => ({ ok: false, status: 404 }) as Response);

    await expect(fetchRawFile('notes/a.md')).rejects.toThrow('attachment notes/a.md: 404');
  });

  it('returns the body bytes when the raw fetch succeeds', async () => {
    const blob = new Blob(['bytes']);
    global.fetch = vi.fn(async () => ({ ok: true, blob: async () => blob }) as unknown as Response);

    await expect(fetchRawFile('notes/a.md')).resolves.toBe(blob);
  });
});

// =============================================================================
//
// The server refuses a plugin-route request that names nobody, so a call site
// that stops sending this stops working. It is NOT a credential: any script on
// this origin can set it. See `routes/plugin_caller.rs`.

async function callerOf(mockFetch: ReturnType<typeof createMockFetch>): Promise<string | null> {
  return (await mockFetch.sent(0)).headers.get(PLUGIN_CALLER_HEADER);
}

describe('caller identity', () => {
  it('rides on every request as the app by default', async () => {
    const mockFetch = createMockFetch({ 'GET /api/config': { body: { kiln_path: '/k' } } });
    global.fetch = mockFetch;

    await getConfig();

    expect(await callerOf(mockFetch)).toBe(APP_CALLER);
  });

  it('carries the plugin a block declares, not the app', async () => {
    const mockFetch = createMockFetch({
      'POST /api/rpc/plugin.run_command': { body: { name: 'kanban_move', result: null } },
    });
    global.fetch = mockFetch;

    await runPluginCommand('kanban_move', {}, 'kanban');

    expect(await callerOf(mockFetch)).toBe('kanban');
  });

  it('keeps the caller alongside a body content type rather than replacing it', async () => {
    const mockFetch = createMockFetch({
      'POST /api/rpc/plugin.run_command': { body: { name: 'kanban_move', result: null } },
    });
    global.fetch = mockFetch;

    await runPluginCommand('kanban_move', {}, 'kanban');

    const { headers } = await mockFetch.sent(0);
    expect(headers.get('Content-Type')).toBe('application/json');
    expect(headers.get(PLUGIN_CALLER_HEADER)).toBe('kanban');
  });

  it('reads publications as the plugin when a block asks', async () => {
    const mockFetch = createMockFetch({
      'POST /api/rpc/plugin.publications': { body: { publications: {} } },
    });
    global.fetch = mockFetch;

    await getPluginPublications('kanban:board', 'kanban');

    expect(await callerOf(mockFetch)).toBe('kanban');
  });
});

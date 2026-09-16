import { afterEach, expect, it, vi } from 'vitest';
import * as api from '../api';

afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); vi.useRealTimers(); });

type Case = [string, () => Promise<unknown>, unknown, unknown, unknown?];

it('preserves each settings, scope, and knowledge endpoint contract', async () => {
  const scope = { session_id: 's/x', kilns: ['k'], workspace: null };
  const cases: Case[] = [
    ['/api/interactions/pending', api.listPendingInteractions, { pending: [] }, []],
    ['/api/config', () => api.saveConfig({ theme: 'light' }), { ok: true }, { ok: true }, { values: { theme: 'light' } }],
    ['/api/plugins/options', api.getPluginOptions, { options: { p: {} } }, { p: {} }],
    ['/api/plugins/p%2Fx/option', () => api.getPluginOption('p/x', ['a']), { value: 7 }, 7, { action: 'get', path: ['a'] }],
    ['/api/plugins/p%2Fx/option', () => api.setPluginOption('p/x', ['a'], 7), {}, undefined, { action: 'set', path: ['a'], value: 7 }],
    ['/api/plugins/p%2Fx/option', () => api.executePluginOption('p/x', ['a']), {}, undefined, { action: 'execute', path: ['a'] }],
    ['/api/session/s%2Fx/knobs', () => api.listKnobs('s/x'), { supported: [] }, { supported: [] }],
    ['/api/session/s%2Fx/config/agent-options', () => api.listAgentOptions('s/x'), { options: [] }, { options: [] }],
    ['/api/session/s%2Fx/config/agent-options', () => api.setAgentOption('s/x', 'o', 'v'), {}, undefined, { option_id: 'o', value: 'v' }],
    ['/api/session/s%2Fx/mode', () => api.setSessionMode('s/x', 'plan'), {}, undefined, { mode: 'plan' }],
    ['/api/session/s%2Fx/kilns/connect', () => api.connectSessionKiln('s/x', 'k'), scope, scope, { kiln: 'k' }],
    ['/api/session/s%2Fx/kilns/disconnect', () => api.disconnectSessionKiln('s/x', 'k'), scope, scope, { kiln: 'k' }],
    ['/api/agents', api.listAgents, { agents: [{ name: 'a' }] }, [{ name: 'a' }]],
    ['/api/models', api.listAllModels, { models: ['m'] }, ['m']],
    ['/api/session/s%2Fx/config/context-strategy', () => api.getContextStrategy('s/x'), { context_strategy: null }, null],
    ['/api/session/s%2Fx/config/context-strategy', () => api.setContextStrategy('s/x', 'truncate'), {}, undefined, { context_strategy: 'truncate' }],
    ['/api/commands', api.listSlashCommands, { commands: [{ name: 'help' }] }, [{ name: 'help' }]],
    ['/api/surfaces', api.getSurfaces, { surfaces: [{ id: 'p' }] }, [{ id: 'p' }]],
    ['/api/notes/resolve?kiln=k&name=a+b', () => api.resolveNotePath('k', 'a b'), { path: 'a' }, { path: 'a' }],
    ['/api/backlinks?kiln=k&note=a+b', () => api.getBacklinks('k', 'a b'), { linked: [] }, { linked: [] }],
    ['/api/scm/clone', () => api.scmClone('owner/repo'), { path: '/repo' }, { path: '/repo' }, { url: 'owner/repo' }],
    ['/api/kiln/graph?kiln=a+b', () => api.getKilnGraph('a b'), { nodes: [] }, { nodes: [] }],
    ['/api/kiln/file?path=a+b', () => api.getFileWithHash('a b'), { content: 'text', content_hash: 'h' }, { content: 'text', content_hash: 'h' }],
    ['/api/recents', api.fetchRecents, { recents: [{ abs_path: '/a', name: 'a', opened_at: 1 }] }, [{ absPath: '/a', name: 'a' }]],
    ['/api/canvas?path=a+b', () => api.getCanvas('a b'), { canvas: {} }, { canvas: {} }],
    ['/api/canvas', () => api.saveCanvas('/a', { nodes: [], edges: [] }), {}, undefined, { path: '/a', content: '{"nodes":[],"edges":[]}' }],
  ];
  for (const [path, call, response, expected, body] of cases) {
    const fetch = vi.fn().mockResolvedValue(new Response(JSON.stringify(response)));
    vi.stubGlobal('fetch', fetch);
    expect(await call(), path).toEqual(expected);
    const [url, init] = fetch.mock.calls[0]!;
    expect(url, path).toBe(path);
    const method = path.endsWith('/workspace') || path.endsWith('/context-strategy') && body !== undefined || path === '/api/canvas' ? 'PUT' : body === undefined ? 'GET' : 'POST';
    expect(init.method, path).toBe(method);
    expect(init.body === undefined ? undefined : JSON.parse(init.body), path).toEqual(body);
  }
});

it('maps search result fields and supplies defaults without swallowing errors', async () => {
  const fetch = vi.fn().mockResolvedValue(new Response(JSON.stringify({
    hits: [{ path: '/a', rel_path: 'a', line: 2, text: 'hit', match_start: 1, match_end: 3 }], truncated: true,
  })));
  vi.stubGlobal('fetch', fetch);
  expect(await api.grepSearch('/r', 'q')).toEqual({ truncated: true, hits: [
    { path: '/a', relPath: 'a', line: 2, text: 'hit', matchStart: 1, matchEnd: 3 },
  ] });
  expect(JSON.parse(fetch.mock.calls[0]![1].body)).toEqual({ root: '/r', query: 'q', glob: null, limit: 100, case_insensitive: true });
  fetch.mockResolvedValue(new Response('{"hits":[],"truncated":false}'));
  await api.grepSearch('/r', 'q', { glob: '*.md', limit: 3, caseInsensitive: false });
  expect(JSON.parse(fetch.mock.calls[1]![1].body)).toEqual({ root: '/r', query: 'q', glob: '*.md', limit: 3, case_insensitive: false });
  fetch.mockResolvedValue(new Response('{"results":[{"path":"/a","rel_path":"a","score":0.8}]}'));
  expect(await api.semanticSearch('k', 'q')).toEqual([{ path: '/a', relPath: 'a', score: 0.8 }]);
  expect(JSON.parse(fetch.mock.calls[2]![1].body)).toEqual({ kiln: 'k', query: 'q', limit: 20 });
});

it('distinguishes file mutation success, conflict, authorization, and invalid responses', async () => {
  const calls = [
    { call: () => api.fsMove('/r', 'kiln', 'a', 'b'), path: '/api/fs/move', method: 'POST', body: { root: '/r', kind: 'kiln', from_rel: 'a', to_rel: 'b' }, result: { moved: true } },
    { call: () => api.fsMkdir('/r', 'project', 'a'), path: '/api/fs/mkdir', method: 'POST', body: { root: '/r', kind: 'project', rel_path: 'a' }, result: undefined },
    { call: () => api.fsTrash('/r', 'kiln', 'a'), path: '/api/fs/trash', method: 'POST', body: { root: '/r', kind: 'kiln', rel_path: 'a' }, result: undefined },
    { call: () => api.patchKilnFile('/a', [], 'h'), path: '/api/kiln/file', method: 'PATCH', body: { path: '/a', edits: [], base_hash: 'h' }, result: { ok: true, content_hash: 'next' } },
  ];
  for (const c of calls) {
    const fetch = vi.fn().mockImplementation(() => Promise.resolve(new Response(JSON.stringify(c.result ?? {}))));
    vi.stubGlobal('fetch', fetch);
    expect(await c.call(), c.path).toEqual(c.result);
    expect(fetch.mock.calls[0]![0]).toBe(c.path);
    expect(fetch.mock.calls[0]![1].method).toBe(c.method);
    expect(JSON.parse(fetch.mock.calls[0]![1].body)).toEqual(c.body);
    for (const status of [401, 500]) {
      fetch.mockImplementation(() => Promise.resolve(new Response('{}', { status })));
      await expect(c.call()).rejects.toThrow();
    }
  }
  vi.stubGlobal('fetch', vi.fn().mockImplementation(() => Promise.resolve(new Response('{"error":"held"}', { status: 409 }))));
  expect(await api.patchKilnFile('/a', [])).toEqual({ error: 'held' });
  await expect(api.fsMove('/r', 'kiln', 'a', 'b')).rejects.toThrow('held');
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('not json', { status: 500 })));
  await expect(api.fsMove('/r', 'kiln', 'a', 'b')).rejects.toThrow('move failed: 500');
});

it('keeps optional discovery fallbacks separate from explicit target resolution failures', async () => {
  const fetch = vi.fn();
  vi.stubGlobal('fetch', fetch);
  fetch.mockImplementation(() => Promise.resolve(new Response('{}')));
  expect(await api.listPendingInteractions()).toEqual([]);
  expect(await api.getPluginOptions()).toEqual({});
  expect(await api.listWorkspaceTargets()).toEqual([]);
  await expect(api.resolveWorkspaceTarget('missing:x')).rejects.toThrow('No plugin resolves');
  fetch.mockImplementation(() => Promise.resolve(new Response('{}', { status: 500 })));
  expect(await api.listPendingInteractions()).toEqual([]);
  const publications = { publications: { targets: { p: { axis: 'workspace', targets_command: 'p:list', resolve_command: 'p:resolve' } } } };
  let answer: unknown = { path: '/checkout' };
  fetch.mockImplementation((url: string) => Promise.resolve(new Response(JSON.stringify(
    url === '/api/plugins/publications' ? publications : answer,
  ))));
  expect(await api.resolveWorkspaceTarget('p:branch:topic', '/repo')).toBe('/checkout');
  answer = { targets: [{ value: 'main', path: '/repo' }] };
  expect(await api.listWorkspaceTargets('/repo')).toEqual([expect.objectContaining({ spec: 'p:main', path: '/repo' })]);
  answer = null;
  await expect(api.resolveWorkspaceTarget('p:x')).rejects.toThrow('to no path');
});

it('delivers filesystem and surface events, reconnects, and cancels pending reconnects', () => {
  vi.useFakeTimers();
  vi.spyOn(console, 'warn').mockImplementation(() => {});
  class Source {
    static instances: Source[] = [];
    handlers = new Map<string, (e: MessageEvent) => void>();
    onerror: (() => void) | null = null;
    close = vi.fn();
    constructor(readonly url: string) { Source.instances.push(this); }
    addEventListener(name: string, handler: (e: MessageEvent) => void) { this.handlers.set(name, handler); }
  }
  vi.stubGlobal('EventSource', Source);
  for (const kind of ['fs', 'surface'] as const) {
    const onEvent = vi.fn();
    const cleanup = kind === 'fs' ? api.subscribeToFsEvents(onEvent) : api.subscribeToSurfaceEvents(onEvent);
    let source = Source.instances.at(-1)!;
    expect(source.url).toBe(kind === 'fs' ? '/api/fs/events' : '/api/surfaces/events');
    const names = kind === 'fs' ? ['fs_changed', 'fs_deleted', 'fs_moved'] : ['surface_changed'];
    for (const name of names) {
      source.handlers.get(name)!(new MessageEvent(name, { data: '{"id":"a"}' }));
      source.handlers.get(name)!(new MessageEvent(name, { data: 'invalid' }));
    }
    expect(onEvent).toHaveBeenCalledTimes(names.length);
    expect(onEvent).toHaveBeenCalledWith({ id: 'a' });
    const count = Source.instances.length;
    source.onerror!();
    expect(source.close).toHaveBeenCalledOnce();
    vi.advanceTimersByTime(999);
    expect(Source.instances).toHaveLength(count);
    vi.advanceTimersByTime(1);
    source = Source.instances.at(-1)!;
    expect(Source.instances).toHaveLength(count + 1);
    source.onerror!();
    cleanup();
    source.onerror!();
    vi.advanceTimersByTime(60_000);
    expect(Source.instances).toHaveLength(count + 1);
  }
});

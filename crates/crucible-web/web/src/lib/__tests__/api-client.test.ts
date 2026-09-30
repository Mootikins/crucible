import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { createMockFetch, apiError } from '@/test-utils';
import { getBus } from '@/lib/bus';
import {
  APP_CALLER,
  PLUGIN_CALLER_HEADER,
  callerParam,
  client,
  decode,
  errorSentence,
  expectOk,
  resetAuthThrottleForTests,
  rpc,
  type ApiError,
} from '../api-client';
import { notificationActions, notificationStore } from '@/stores/notificationStore';

/**
 * The client is the whole transport, so what it does on a failure IS the
 * error behaviour of every call in `api.ts` and `diff-api.ts`.
 */

const originalFetch = global.fetch;

const errorToasts = () =>
  notificationStore.notifications.filter((n) => n.type === 'error' && !n.dismissed);

beforeEach(() => {
  resetAuthThrottleForTests();
  notificationActions.clearAll();
});

afterEach(() => {
  global.fetch = originalFetch;
});

describe('the generated client', () => {
  it('asks for the path the caller named, with the method it named', async () => {
    const mockFetch = createMockFetch({ 'GET /api/plugins': { body: { plugins: [] } } });
    global.fetch = mockFetch;

    const answer = decode(await client.GET('/api/plugins'), 'Failed to list plugins');

    expect(answer.plugins).toEqual([]);
    expect(mockFetch.calls('GET /api/plugins')).toBe(1);
  });

  it('puts a path parameter in the path and a query parameter in the query', async () => {
    const mockFetch = createMockFetch({
      'DELETE /api/plugins/a%2Fb': { body: { removed: true } },
    });
    global.fetch = mockFetch;

    await client.DELETE('/api/plugins/{name}', {
      params: { path: { name: 'a/b' }, query: { purge: true }, header: callerParam() },
    });

    const sent = await mockFetch.sent(0);
    expect(sent.path).toBe('/api/plugins/a%2Fb');
    expect(sent.query.get('purge')).toBe('true');
  });

  it('names the app as the caller on every request', async () => {
    const mockFetch = createMockFetch({ 'GET /api/plugins': { body: { plugins: [] } } });
    global.fetch = mockFetch;

    await client.GET('/api/plugins');

    expect((await mockFetch.sent(0)).headers.get(PLUGIN_CALLER_HEADER)).toBe(APP_CALLER);
  });

  it('lets a block name its own plugin instead', async () => {
    const mockFetch = createMockFetch({ 'POST /api/plugins/command': { body: {} } });
    global.fetch = mockFetch;

    await client.POST('/api/plugins/command', {
      body: { name: 'kanban.board', args: {} },
      params: { header: callerParam('kanban') },
    });

    expect((await mockFetch.sent(0)).headers.get(PLUGIN_CALLER_HEADER)).toBe('kanban');
  });
});

describe('a refusal', () => {
  it('becomes an error carrying the daemon sentence, not the envelope', async () => {
    global.fetch = createMockFetch({
      'GET /api/plugins': apiError(422, 'provider ollama is unreachable'),
    });

    const thrown = await client
      .GET('/api/plugins')
      .then((result) => {
        decode(result, 'Failed to list plugins');
        return null;
      })
      .catch((error: ApiError) => error);

    expect(thrown?.message).toBe('Failed to list plugins: provider ollama is unreachable');
    expect(thrown?.message).not.toContain('{');
    expect(thrown?.status).toBe(422);
  });

  it('falls back to the status when the body carries no sentence', async () => {
    global.fetch = createMockFetch({ 'GET /api/plugins': { status: 500 } });

    await expect(
      client.GET('/api/plugins').then((r) => decode(r, 'Failed to list plugins')),
    ).rejects.toThrow('Failed to list plugins: HTTP 500');
  });

  it('raises one toast when the caller asks to notify', async () => {
    global.fetch = createMockFetch({ 'GET /api/plugins': apiError(502, 'daemon is down') });

    await expect(
      client.GET('/api/plugins').then((r) => decode(r, 'Failed to list plugins', { notify: true })),
    ).rejects.toThrow('daemon is down');

    expect(errorToasts()).toHaveLength(1);
    expect(errorToasts()[0].message).toContain('daemon is down');
  });

  it('answers nothing for a write whose reply nobody reads', async () => {
    global.fetch = createMockFetch({ 'POST /api/session/s1/resume': { status: 200 } });

    expect(
      expectOk(
        await client.POST('/api/session/{id}/resume', { params: { path: { id: 's1' } } }),
        'Failed to resume session',
      ),
    ).toBeUndefined();
  });
});

describe('a 401', () => {
  it('asks for the key, and once only inside the throttle window', async () => {
    const prompted = vi.fn();
    const stopListening = getBus().on('authRequired', prompted);
    global.fetch = createMockFetch({ 'GET /api/plugins': { status: 401 } });

    for (let attempt = 0; attempt < 3; attempt += 1) {
      await expect(
        client.GET('/api/plugins').then((r) => decode(r, 'Failed to list plugins')),
      ).rejects.toThrow(/sign in with the API key/);
    }

    expect(prompted).toHaveBeenCalledTimes(1);
    stopListening();
  });

  it('raises no toast beside the prompt, which would say the same twice', async () => {
    global.fetch = createMockFetch({ 'GET /api/plugins': { status: 401 } });

    await expect(
      client.GET('/api/plugins').then((r) => decode(r, 'Failed to list plugins', { notify: true })),
    ).rejects.toThrow();

    expect(errorToasts()).toHaveLength(0);
  });
});

describe('a reply that is not the shape the document declares', () => {
  it('fails the decode rather than answering undefined', async () => {
    global.fetch = createMockFetch({ 'GET /api/plugins': { status: 200 } });

    await expect(
      client.GET('/api/plugins').then((r) => decode(r, 'Failed to list plugins')),
    ).rejects.toThrow(/carried no body/);
  });

  // A path outside the document is a compile error, which is the whole point
  // of the generated client. `bun run typecheck` is the gate; this line is
  // what breaks when the gate stops working.
  it('cannot even name a path the contract does not carry', async () => {
    global.fetch = createMockFetch({});

    // @ts-expect-error `/api/nope` is in no OpenAPI document
    const result = await client.GET('/api/nope');

    expect(result.response.status).toBe(404);
  });
});

describe('the one RPC call', () => {
  // The abort signal a caller passes has to follow the request, not just the
  // options object: `rpc('session.history', ..., { signal })` is how a pane
  // that rebinds mid-fetch cancels the stale read.
  it("forwards the caller's AbortSignal to the request", async () => {
    const mockFetch = createMockFetch({
      'POST /api/rpc/session.history': {
        body: { session_id: 'ses-1', history: [], total_events: 0 },
      },
    });
    global.fetch = mockFetch;
    const controller = new AbortController();
    await rpc('session.history', { session_id: 'ses-1' }, { signal: controller.signal });
    // The client builds the `Request`, so the signal it carries FOLLOWS the
    // caller's rather than being the same object. An abort still reaches it.
    const { signal } = await mockFetch.sent(0);
    expect(signal.aborted).toBe(false);
    controller.abort();
    expect(signal.aborted).toBe(true);
  });
});

describe('the error sentence', () => {
  it('reads the envelope, the bare text and the empty body alike', () => {
    expect(errorSentence({ error: { code: 422, message: 'no such hunk' } })).toBe('no such hunk');
    expect(errorSentence('plain text from a proxy')).toBe('plain text from a proxy');
    expect(errorSentence('')).toBe('');
    expect(errorSentence(undefined)).toBe('');
    expect(errorSentence({ detail: 'other' })).toBe('{"detail":"other"}');
  });
});

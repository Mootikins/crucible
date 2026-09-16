import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { createMockFetch, apiError } from '@/test-utils';
import {
  APP_CALLER,
  PLUGIN_CALLER_HEADER,
  callerParam,
  client,
  decode,
  errorSentence,
  expectOk,
  resetAuthThrottleForTests,
  type ApiError,
} from '../api-client';
import { notificationActions, notificationStore } from '@/stores/notificationStore';

/**
 * The client is the whole transport, so what it does on a failure IS the
 * error behaviour of every call in `api.ts` and `review-api.ts`.
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
    const mockFetch = createMockFetch({ 'GET /api/models': { body: { models: ['m'] } } });
    global.fetch = mockFetch;

    const answer = decode(await client.GET('/api/models'), 'Failed to list models');

    expect(answer.models).toEqual(['m']);
    expect(mockFetch.calls('GET /api/models')).toBe(1);
  });

  it('puts a path parameter in the path and a query parameter in the query', async () => {
    const mockFetch = createMockFetch({
      'GET /api/session/a%2Fb/review/hunks': { body: { session_id: 'a/b', hunks: [] } },
    });
    global.fetch = mockFetch;

    await client.GET('/api/session/{id}/review/hunks', {
      params: { path: { id: 'a/b' }, query: { scope: 'turn' } },
    });

    const sent = await mockFetch.sent(0);
    expect(sent.path).toBe('/api/session/a%2Fb/review/hunks');
    expect(sent.query.get('scope')).toBe('turn');
  });

  it('names the app as the caller on every request', async () => {
    const mockFetch = createMockFetch({ 'GET /api/models': { body: { models: [] } } });
    global.fetch = mockFetch;

    await client.GET('/api/models');

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
      'GET /api/models': apiError(422, 'provider ollama is unreachable'),
    });

    const thrown = await client
      .GET('/api/models')
      .then((result) => {
        decode(result, 'Failed to list models');
        return null;
      })
      .catch((error: ApiError) => error);

    expect(thrown?.message).toBe('Failed to list models: provider ollama is unreachable');
    expect(thrown?.message).not.toContain('{');
    expect(thrown?.status).toBe(422);
  });

  it('falls back to the status when the body carries no sentence', async () => {
    global.fetch = createMockFetch({ 'GET /api/models': { status: 500 } });

    await expect(
      client.GET('/api/models').then((r) => decode(r, 'Failed to list models')),
    ).rejects.toThrow('Failed to list models: HTTP 500');
  });

  it('raises one toast when the caller asks to notify', async () => {
    global.fetch = createMockFetch({ 'GET /api/models': apiError(502, 'daemon is down') });

    await expect(
      client.GET('/api/models').then((r) => decode(r, 'Failed to list models', { notify: true })),
    ).rejects.toThrow('daemon is down');

    expect(errorToasts()).toHaveLength(1);
    expect(errorToasts()[0].message).toContain('daemon is down');
  });

  it('answers nothing for a write whose reply nobody reads', async () => {
    global.fetch = createMockFetch({ 'POST /api/session/s1/pause': { status: 200 } });

    expect(
      expectOk(
        await client.POST('/api/session/{id}/pause', { params: { path: { id: 's1' } } }),
        'Failed to pause session',
      ),
    ).toBeUndefined();
  });
});

describe('a 401', () => {
  it('asks for the key, and once only inside the throttle window', async () => {
    const prompted = vi.fn();
    window.addEventListener('crucible:auth-required', prompted);
    global.fetch = createMockFetch({ 'GET /api/models': { status: 401 } });

    for (let attempt = 0; attempt < 3; attempt += 1) {
      await expect(
        client.GET('/api/models').then((r) => decode(r, 'Failed to list models')),
      ).rejects.toThrow(/sign in with the API key/);
    }

    expect(prompted).toHaveBeenCalledTimes(1);
    window.removeEventListener('crucible:auth-required', prompted);
  });

  it('raises no toast beside the prompt, which would say the same twice', async () => {
    global.fetch = createMockFetch({ 'GET /api/models': { status: 401 } });

    await expect(
      client.GET('/api/models').then((r) => decode(r, 'Failed to list models', { notify: true })),
    ).rejects.toThrow();

    expect(errorToasts()).toHaveLength(0);
  });
});

describe('a reply that is not the shape the document declares', () => {
  it('fails the decode rather than answering undefined', async () => {
    global.fetch = createMockFetch({ 'GET /api/models': { status: 200 } });

    await expect(
      client.GET('/api/models').then((r) => decode(r, 'Failed to list models')),
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

describe('the error sentence', () => {
  it('reads the envelope, the bare text and the empty body alike', () => {
    expect(errorSentence({ error: { code: 422, message: 'no such hunk' } })).toBe('no such hunk');
    expect(errorSentence('plain text from a proxy')).toBe('plain text from a proxy');
    expect(errorSentence('')).toBe('');
    expect(errorSentence(undefined)).toBe('');
    expect(errorSentence({ detail: 'other' })).toBe('{"detail":"other"}');
  });
});

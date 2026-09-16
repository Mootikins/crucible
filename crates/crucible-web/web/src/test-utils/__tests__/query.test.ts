import { describe, it, expect, afterEach, vi } from 'vitest';
import { QueryClient } from '@tanstack/solid-query';
import { getQueryClient, queryClientOptions } from '@/lib/query/client';
import { getBus } from '@/lib/bus';
import { sessionEvents } from '@/lib/query/sse';
import { request } from '@/lib/api';
import { apiError, createMockFetch } from '../mock-fetch';
import { createTestQueryEnv, withQueryClient } from '../query';
import { installFakeEventSource, onlyEventSource } from '../sse';

const originalFetch = global.fetch;

afterEach(() => {
  global.fetch = originalFetch;
});

describe('withQueryClient', () => {
  it('gives the body a client the module answers while the body runs', async () => {
    await withQueryClient(async (client) => {
      expect(client).toBeInstanceOf(QueryClient);
      expect(getQueryClient()).toBe(client);
    });
  });

  it('builds a fresh client for each call, with an empty cache', async () => {
    const first = await withQueryClient(async (client) => {
      client.setQueryData(['kilns'], ['main']);
      return client;
    });

    await withQueryClient(async (client) => {
      expect(client).not.toBe(first);
      expect(client.getQueryData(['kilns'])).toBeUndefined();
    });
  });

  it('keeps the shared defaults, and forgets a cache entry at once', async () => {
    await withQueryClient(async (client) => {
      const defaults = client.getDefaultOptions();
      expect(defaults.queries?.gcTime).toBe(0);
      expect(defaults.queries?.retry).toBe(false);
      expect(defaults.queries?.staleTime).toBe(
        queryClientOptions.defaultOptions?.queries?.staleTime,
      );
    });
  });

  it('puts the module singleton back after the body returns', async () => {
    const singleton = getQueryClient();

    await withQueryClient(async (client) => {
      expect(getQueryClient()).not.toBe(singleton);
      expect(getQueryClient()).toBe(client);
    });

    expect(getQueryClient()).toBe(singleton);
  });

  it('puts the module singleton back after the body throws', async () => {
    const singleton = getQueryClient();

    await expect(
      withQueryClient(async () => {
        throw new Error('the body failed');
      }),
    ).rejects.toThrow('the body failed');

    expect(getQueryClient()).toBe(singleton);
  });

  it('answers what the body answers', async () => {
    expect(await withQueryClient(async () => 'done')).toBe('done');
  });

  it('removes every bus handler the body added', async () => {
    await withQueryClient(async () => {
      getBus().on('openSettings', vi.fn());
      expect(getBus().handlerCount()).toBe(1);
    });

    expect(getBus().handlerCount()).toBe(0);
  });

  it('closes every stream the body opened', async () => {
    installFakeEventSource();

    await withQueryClient(async () => {
      sessionEvents('s1').subscribe(vi.fn());
      expect(onlyEventSource().closed).toBe(false);
    });

    expect(onlyEventSource().closed).toBe(true);
  });
});

describe('createTestQueryEnv', () => {
  it('installs a client and a fetch the app then reads', async () => {
    const env = createTestQueryEnv({
      'GET /api/kilns': () => [{ name: 'main', path: '/kilns/main' }],
    });

    try {
      expect(getQueryClient()).toBe(env.client);
      expect(global.fetch).toBe(env.fetch);
      await expect(request('GET', '/api/kilns')).resolves.toEqual([
        { name: 'main', path: '/kilns/main' },
      ]);
    } finally {
      env.restore();
    }
  });

  it('puts the client and the fetch back on restore', async () => {
    const singleton = getQueryClient();
    const before = global.fetch;

    const env = createTestQueryEnv();
    env.restore();

    expect(getQueryClient()).toBe(singleton);
    expect(global.fetch).toBe(before);
  });
});

describe('createMockFetch', () => {
  it('answers the object form, as its first callers wrote it', async () => {
    global.fetch = createMockFetch({
      'GET /api/config': { body: { kiln_path: '/k' } },
    });

    await expect(request('GET', '/api/config')).resolves.toEqual({ kiln_path: '/k' });
  });

  it('answers a function with the body it returns', async () => {
    global.fetch = createMockFetch({
      'GET /api/kilns': () => [{ name: 'main' }],
    });

    await expect(request('GET', '/api/kilns')).resolves.toEqual([{ name: 'main' }]);
  });

  it('gives the function the request, with its method and its body', async () => {
    const seen: string[] = [];
    global.fetch = createMockFetch({
      'POST /api/session': async (req) => {
        seen.push(req.method, await req.text());
        return { id: 'sess-1' };
      },
    });

    await expect(
      request('POST', '/api/session', {
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ title: 'One' }),
      }),
    ).resolves.toEqual({ id: 'sess-1' });
    expect(seen).toEqual(['POST', '{"title":"One"}']);
  });

  it('answers a Response the function builds itself', async () => {
    global.fetch = createMockFetch({
      'POST /api/session/ses-1/export': () =>
        new Response('# Markdown', { headers: { 'Content-Type': 'text/markdown' } }),
    });

    await expect(
      request('POST', '/api/session/ses-1/export', { parseAs: 'text' }),
    ).resolves.toBe('# Markdown');
  });

  it('counts the calls of one route, and answers zero for a route nobody called', async () => {
    const mockFetch = createMockFetch({ 'GET /api/kilns': () => [] });
    global.fetch = mockFetch;

    expect(mockFetch.calls('GET /api/kilns')).toBe(0);
    await request('GET', '/api/kilns');
    await request('GET', '/api/kilns?refresh=1');
    expect(mockFetch.calls('GET /api/kilns')).toBe(2);
    expect(mockFetch.calls('GET /api/config')).toBe(0);
  });

  it('counts a call it matched no route for', async () => {
    const mockFetch = createMockFetch({});
    global.fetch = mockFetch;

    await expect(request('GET', '/api/config')).rejects.toThrow();

    expect(mockFetch.calls('GET /api/config')).toBe(1);
  });

  it('answers 422 with the envelope the error path unwraps', async () => {
    const refusal = 'root is not a registered project';
    global.fetch = createMockFetch({
      'GET /api/fs/list': apiError(422, refusal),
    });

    await expect(
      request('GET', '/api/fs/list', {
        errorMessage: 'Failed to list folder',
        includeErrorText: true,
      }),
    ).rejects.toThrow(`Failed to list folder: ${refusal}`);
  });

  it('carries the status of a failure to the caller', async () => {
    global.fetch = createMockFetch({ 'GET /api/fs/list': apiError(500, 'the daemon fell over') });

    await expect(request('GET', '/api/fs/list')).rejects.toMatchObject({ status: 500 });
  });

  it('answers a status a function route names, with the error body', async () => {
    global.fetch = createMockFetch({
      'POST /api/session': () => new Response(JSON.stringify({ error: { message: 'no' } }), {
        status: 409,
        headers: { 'Content-Type': 'application/json' },
      }),
    });

    await expect(
      request('POST', '/api/session', { errorMessage: 'Failed', includeErrorText: true }),
    ).rejects.toThrow('Failed: no');
  });
});

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { createRoot } from 'solid-js';
import { apiError } from '@/test-utils/mock-fetch';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import type { Config } from '@/lib/api';
import {
  useConfig,
  useSaveConfig,
  fetchConfigOnce,
  configSnapshot,
  resetConfigForTests,
} from '../config';

/** The storage key `swrLocal('config')` wrote, which the hook keeps. */
const STORAGE_KEY = 'crucible:cache:config';

/** Every field `GET /api/config` declares; the route sends them all. */
const config = (kilnPath: string): Config => ({
  kiln_path: kilnPath,
  remote_shell: false,
  config: {},
  origins: [],
  controls: { options: { type: 'group' }, read_only: [] },
});

const LIVE: Config = config('/kilns/live');
const STORED: Config = config('/kilns/stored');

/** What one save answers when the daemon holds every leaf it was given. */
const SAVED = { ok: true, refused: [], rejected: [] };

let env: TestQueryEnv;
let dispose: (() => void) | null = null;

beforeEach(() => {
  localStorage.removeItem(STORAGE_KEY);
  resetConfigForTests();
});

afterEach(() => {
  dispose?.();
  dispose = null;
  env?.restore();
  localStorage.removeItem(STORAGE_KEY);
  resetConfigForTests();
});

/** Runs the body under one Solid owner, which the test disposes afterwards. */
function inRoot<T>(body: () => T): T {
  return createRoot((disposeRoot) => {
    dispose = disposeRoot;
    return body();
  });
}

describe('useConfig', () => {
  it('fetches once for two callers under one root', async () => {
    env = createTestQueryEnv({ 'GET /api/config': () => LIVE });

    const both = inRoot(() => ({ first: useConfig(), second: useConfig() }));

    await vi.waitFor(() => expect(both.first.data).toEqual(LIVE));
    expect(both.second.data).toEqual(LIVE);
    expect(env.fetch.calls('GET /api/config')).toBe(1);
  });

  it('surfaces a refusal as an error rather than as an absent config', async () => {
    env = createTestQueryEnv({
      'GET /api/config': apiError(500, 'the config store is unreadable'),
    });

    const query = inRoot(() => useConfig());

    await vi.waitFor(() => expect(query.isError).toBe(true));
    expect(query.error?.message).toContain('Failed to get config');
    expect(query.data).toBeUndefined();
  });

  it('paints the stored config before the fetch answers', async () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(STORED));
    let release: (() => void) | undefined;
    const answered = new Promise<void>((resolve) => {
      release = resolve;
    });
    env = createTestQueryEnv({
      'GET /api/config': async () => {
        await answered;
        return LIVE;
      },
    });

    const query = inRoot(() => useConfig());

    // The first read, before the fetch resolves, already has the last answer.
    expect(query.data).toEqual(STORED);

    release?.();
    await vi.waitFor(() => expect(query.data).toEqual(LIVE));
    expect(env.fetch.calls('GET /api/config')).toBe(1);
  });

  it('writes the fetched config back to storage', async () => {
    env = createTestQueryEnv({ 'GET /api/config': () => LIVE });

    const query = inRoot(() => useConfig());

    await vi.waitFor(() => expect(query.data).toEqual(LIVE));
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null')).toEqual(LIVE);
  });
});

describe('useSaveConfig', () => {
  it('sends the values, then invalidates so the next read refetches', async () => {
    let answer = LIVE;
    let sent: unknown = null;
    env = createTestQueryEnv({
      'GET /api/config': () => answer,
      'POST /api/config': async (request) => {
        sent = await request.json();
        answer = config('/kilns/saved');
        return SAVED;
      },
    });

    const both = inRoot(() => ({ query: useConfig(), save: useSaveConfig() }));

    await vi.waitFor(() => expect(both.query.data).toEqual(LIVE));
    expect(env.fetch.calls('GET /api/config')).toBe(1);

    await both.save.mutateAsync({ ui: { theme: 'light' } });

    expect(sent).toEqual({ values: { ui: { theme: 'light' } } });
    await vi.waitFor(() => expect(both.query.data).toEqual(config('/kilns/saved')));
    expect(env.fetch.calls('GET /api/config')).toBe(2);
  });

  it('reports a failed save to the caller and leaves the read alone', async () => {
    env = createTestQueryEnv({
      'GET /api/config': () => LIVE,
      'POST /api/config': apiError(403, 'the config file is read-only'),
    });

    const both = inRoot(() => ({ query: useConfig(), save: useSaveConfig() }));
    await vi.waitFor(() => expect(both.query.data).toEqual(LIVE));

    await expect(both.save.mutateAsync({ ui: { theme: 'light' } })).rejects.toThrow(
      /Failed to save config/,
    );
    expect(env.fetch.calls('GET /api/config')).toBe(1);
  });
});

describe('fetchConfigOnce', () => {
  it('answers the same config the hook reads, from one fetch', async () => {
    env = createTestQueryEnv({ 'GET /api/config': () => LIVE });

    const query = inRoot(() => useConfig());
    await vi.waitFor(() => expect(query.data).toEqual(LIVE));

    await expect(fetchConfigOnce()).resolves.toEqual(LIVE);
    expect(env.fetch.calls('GET /api/config')).toBe(1);
  });
});

describe('configSnapshot', () => {
  it('starts the one fetch and then answers what the cache holds', async () => {
    env = createTestQueryEnv({ 'GET /api/config': () => LIVE });

    expect(configSnapshot()).toBeUndefined();
    await vi.waitFor(() => expect(configSnapshot()).toEqual(LIVE));
    expect(env.fetch.calls('GET /api/config')).toBe(1);
  });

  it('shares the fetch with the hook', async () => {
    env = createTestQueryEnv({ 'GET /api/config': () => LIVE });

    const query = inRoot(() => useConfig());
    configSnapshot();

    await vi.waitFor(() => expect(query.data).toEqual(LIVE));
    await vi.waitFor(() => expect(configSnapshot()).toEqual(LIVE));
    expect(env.fetch.calls('GET /api/config')).toBe(1);
  });

  it('answers nothing after a refusal, and asks no second time', async () => {
    env = createTestQueryEnv({
      'GET /api/config': apiError(401, 'sign in with the API key'),
    });

    configSnapshot();
    await vi.waitFor(() => expect(env.fetch.calls('GET /api/config')).toBe(1));

    for (let i = 0; i < 20; i++) configSnapshot();
    expect(configSnapshot()).toBeUndefined();
    expect(env.fetch.calls('GET /api/config')).toBe(1);
  });
});

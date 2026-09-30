import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { createRoot } from 'solid-js';
import { apiError } from '@/test-utils/mock-fetch';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import type { Project } from '@/lib/types';
import { useKilns, resetKilnsForTests } from '../kilns';
import {
  useProjects,
  useRegisterProject,
  useUnregisterProject,
  useScmClone,
  fetchProjectOnce,
  resetProjectsForTests,
} from '../projects';

/** The storage key `swrLocal('projects')` wrote, which the hook keeps. */
const STORAGE_KEY = 'crucible:cache:projects';
/** The kiln roster's own key, which a registration also refreshes. */
const KILNS_STORAGE_KEY = 'crucible:cache:kilns';

const project = (path: string, name: string): Project => ({
  path,
  name,
  kilns: [],
  last_accessed: '2026-09-15T00:00:00Z',
});

const LIVE = [project('/repos/live', 'live')];
const STORED = [project('/repos/stored', 'stored')];

let env: TestQueryEnv;
let dispose: (() => void) | null = null;

beforeEach(() => {
  localStorage.removeItem(STORAGE_KEY);
  localStorage.removeItem(KILNS_STORAGE_KEY);
  resetProjectsForTests();
  resetKilnsForTests();
});

afterEach(() => {
  dispose?.();
  dispose = null;
  env?.restore();
  localStorage.removeItem(STORAGE_KEY);
  localStorage.removeItem(KILNS_STORAGE_KEY);
  resetProjectsForTests();
  resetKilnsForTests();
});

/** Runs the body under one Solid owner, which the test disposes afterwards. */
function inRoot<T>(body: () => T): T {
  return createRoot((disposeRoot) => {
    dispose = disposeRoot;
    return body();
  });
}

// `project.list`, `project.unregister` and `project.get` reach the browser
// through `POST /api/rpc/{method}` now ([[Simplification Plan#Step 19]]
// item 3); `GET /api/project/list`/`/get` and
// `POST /api/project/unregister` are gone. `POST /api/project/register`
// stays: its `[web] registration_roots` check and its rollback are
// web-owned behavior.

describe('useProjects', () => {
  it('fetches once for two callers under one root', async () => {
    env = createTestQueryEnv({ 'POST /api/rpc/project.list': () => LIVE });

    const both = inRoot(() => ({ first: useProjects(), second: useProjects() }));

    await vi.waitFor(() => expect(both.first.data).toEqual(LIVE));
    expect(both.second.data).toEqual(LIVE);
    expect(env.fetch.calls('POST /api/rpc/project.list')).toBe(1);
  });

  it('surfaces a refusal as an error rather than as an empty roster', async () => {
    env = createTestQueryEnv({
      'POST /api/rpc/project.list': apiError(500, 'the project registry is unreadable'),
    });

    const query = inRoot(() => useProjects());

    await vi.waitFor(() => expect(query.isError).toBe(true));
    expect(query.error?.message).toContain('RPC `project.list` failed');
    expect(query.data).toBeUndefined();
  });

  it('paints the stored roster before the fetch answers', async () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(STORED));
    let release: (() => void) | undefined;
    const answered = new Promise<void>((resolve) => {
      release = resolve;
    });
    env = createTestQueryEnv({
      'POST /api/rpc/project.list': async () => {
        await answered;
        return LIVE;
      },
    });

    const query = inRoot(() => useProjects());

    // The first read, before the fetch resolves, already has the last answer.
    expect(query.data).toEqual(STORED);

    release?.();
    await vi.waitFor(() => expect(query.data).toEqual(LIVE));
    expect(env.fetch.calls('POST /api/rpc/project.list')).toBe(1);
  });

  it('writes the fetched roster back to storage', async () => {
    env = createTestQueryEnv({ 'POST /api/rpc/project.list': () => LIVE });

    const query = inRoot(() => useProjects());

    await vi.waitFor(() => expect(query.data).toEqual(LIVE));
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null')).toEqual(LIVE);
  });
});

describe('useRegisterProject', () => {
  it('sends the path, then invalidates so the next read refetches', async () => {
    let answer = LIVE;
    let sent: unknown = null;
    const added = project('/repos/added', 'added');
    env = createTestQueryEnv({
      'POST /api/rpc/project.list': () => answer,
      'POST /api/project/register': async (request) => {
        sent = await request.json();
        answer = [...LIVE, added];
        return added;
      },
    });

    const both = inRoot(() => ({ query: useProjects(), register: useRegisterProject() }));

    await vi.waitFor(() => expect(both.query.data).toEqual(LIVE));
    expect(env.fetch.calls('POST /api/rpc/project.list')).toBe(1);

    await expect(both.register.mutateAsync('/repos/added')).resolves.toEqual(added);

    expect(sent).toEqual({ path: '/repos/added' });
    await vi.waitFor(() => expect(both.query.data).toEqual([...LIVE, added]));
    expect(env.fetch.calls('POST /api/rpc/project.list')).toBe(2);
  });

  it('asks for the kiln roster again, because a registration can change it', async () => {
    env = createTestQueryEnv({
      'POST /api/rpc/kiln.list': () => [],
      'POST /api/rpc/project.list': () => LIVE,
      'POST /api/project/register': () => project('/repos/added', 'added'),
    });

    const both = inRoot(() => ({ kilns: useKilns(), register: useRegisterProject() }));

    await vi.waitFor(() => expect(both.kilns.data).toEqual([]));
    expect(env.fetch.calls('POST /api/rpc/kiln.list')).toBe(1);

    await both.register.mutateAsync('/repos/added');

    // The kiln read happens in the BACKGROUND: the mutation does not wait for
    // it, so the wait belongs here rather than on the mutation's promise.
    await vi.waitFor(() => expect(env.fetch.calls('POST /api/rpc/kiln.list')).toBe(2));
  });

  it('reports a failed registration to the caller and leaves the roster alone', async () => {
    env = createTestQueryEnv({
      'POST /api/rpc/project.list': () => LIVE,
      'POST /api/project/register': apiError(422, 'that path is not a directory'),
    });

    const both = inRoot(() => ({ query: useProjects(), register: useRegisterProject() }));
    await vi.waitFor(() => expect(both.query.data).toEqual(LIVE));

    await expect(both.register.mutateAsync('/nowhere')).rejects.toThrow(
      /Failed to register project/,
    );
    expect(env.fetch.calls('POST /api/rpc/project.list')).toBe(1);
  });
});

describe('useUnregisterProject', () => {
  it('sends the path, then invalidates so the next read refetches', async () => {
    let answer = LIVE;
    let sent: unknown = null;
    env = createTestQueryEnv({
      'POST /api/rpc/project.list': () => answer,
      'POST /api/rpc/project.unregister': async (request) => {
        sent = await request.json();
        answer = [];
        return null;
      },
    });

    const both = inRoot(() => ({ query: useProjects(), remove: useUnregisterProject() }));

    await vi.waitFor(() => expect(both.query.data).toEqual(LIVE));

    await both.remove.mutateAsync('/repos/live');

    expect(sent).toEqual({ path: '/repos/live' });
    await vi.waitFor(() => expect(both.query.data).toEqual([]));
    expect(env.fetch.calls('POST /api/rpc/project.list')).toBe(2);
  });
});

describe('useScmClone', () => {
  it('sends the url, then invalidates so the next read refetches', async () => {
    const cloned = project('/repos/cloned', 'cloned');
    let answer = LIVE;
    let sent: unknown = null;
    env = createTestQueryEnv({
      'POST /api/rpc/project.list': () => answer,
      'POST /api/scm/clone': async (request) => {
        sent = await request.json();
        answer = [...LIVE, cloned];
        return { path: cloned.path, project: cloned };
      },
    });

    const both = inRoot(() => ({ query: useProjects(), clone: useScmClone() }));

    await vi.waitFor(() => expect(both.query.data).toEqual(LIVE));

    const result = await both.clone.mutateAsync('octocat/Spoon-Knife');

    expect(sent).toEqual({ url: 'octocat/Spoon-Knife' });
    expect(result.path).toBe('/repos/cloned');
    await vi.waitFor(() => expect(both.query.data).toEqual([...LIVE, cloned]));
    expect(env.fetch.calls('POST /api/rpc/project.list')).toBe(2);
  });
});

describe('fetchProjectOnce', () => {
  it('reads one project, and answers the second caller from the cache', async () => {
    const one = project('/repos/one', 'one');
    env = createTestQueryEnv({ 'POST /api/rpc/project.get': () => one });

    await expect(fetchProjectOnce('/repos/one')).resolves.toEqual(one);
    await expect(fetchProjectOnce('/repos/one')).resolves.toEqual(one);
    expect(env.fetch.calls('POST /api/rpc/project.get')).toBe(1);
  });

  // `project.get`'s reply is `Option<Project>`, so a path the daemon does
  // not know is `null`, not a 404 — the route that used to translate the
  // option into a status is gone ([[Simplification Plan#Step 19]] item 3).
  it('answers null for a path the daemon does not know', async () => {
    env = createTestQueryEnv({
      'POST /api/rpc/project.get': () => null,
    });

    await expect(fetchProjectOnce('/repos/missing')).resolves.toBeNull();
  });
});

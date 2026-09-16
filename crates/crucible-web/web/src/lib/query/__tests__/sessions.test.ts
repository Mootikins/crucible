import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { createRoot } from 'solid-js';
import { apiError } from '@/test-utils/mock-fetch';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import type { Session } from '@/lib/types';
import { keys } from '../keys';
import {
  useSessions,
  useSession,
  fetchSessionOnce,
  useCreateSession,
  useDeleteSession,
  useArchiveSession,
  useUnarchiveSession,
  useCancelSession,
  useEndSession,
  useExportSession,
  usePauseSession,
  useResumeSession,
  useSetSessionTitle,
  resetSessionsForTests,
} from '../sessions';

/** The storage key `SessionContext` wrote its last roster under, which the hook keeps. */
const STORAGE_KEY = 'crucible:cache:sessions';

const LIST = 'GET /api/session/list';

function session(id: string, over: Partial<Session> = {}): Session {
  return {
    session_id: id,
    type: 'chat',
    kilns: ['main'],
    workspace: '/repos/app',
    state: 'active',
    title: `Session ${id}`,
    agent_model: 'openai/gpt-4o',
    started_at: '2026-09-15T00:00:00Z',
    last_activity: null,
    event_count: 0,
    archived: false,
    ...over,
  };
}

/**
 * What the daemon's list route answers.
 *
 * The rows go on the wire unchanged: `Session` IS `SessionRow` now, so there
 * is no second spelling for a fixture to convert between.
 */
function listReply(rows: Session[]): { sessions: Session[]; total: number } {
  return { sessions: rows, total: rows.length };
}

/** True when the request asked for the archived rows too. */
function wantsArchived(request: Request): boolean {
  return new URL(request.url).searchParams.get('include_archived') === 'true';
}

let env: TestQueryEnv;
let dispose: (() => void) | null = null;

beforeEach(() => {
  localStorage.removeItem(STORAGE_KEY);
  resetSessionsForTests();
});

afterEach(() => {
  dispose?.();
  dispose = null;
  env?.restore();
  localStorage.removeItem(STORAGE_KEY);
  resetSessionsForTests();
});

/** Runs the body under one Solid owner, which the test disposes afterwards. */
function inRoot<T>(body: () => T): T {
  return createRoot((disposeRoot) => {
    dispose = disposeRoot;
    return body();
  });
}

describe('useSessions', () => {
  it('fetches once for two callers under one root', async () => {
    const rows = [session('s-1')];
    env = createTestQueryEnv({ [LIST]: () => listReply(rows) });

    const both = inRoot(() => ({
      first: useSessions(() => false),
      second: useSessions(() => false),
    }));

    await vi.waitFor(() => expect(both.first.data).toEqual(rows));
    expect(both.second.data).toEqual(rows);
    expect(env.fetch.calls(LIST)).toBe(1);
  });

  it('keeps the archived variant under its own key, and asks for it', async () => {
    const active = session('s-1');
    const archived = session('s-2', { archived: true });
    env = createTestQueryEnv({
      [LIST]: (request) => listReply(wantsArchived(request) ? [active, archived] : [active]),
    });

    const both = inRoot(() => ({
      active: useSessions(() => false),
      all: useSessions(() => true),
    }));

    await vi.waitFor(() => expect(both.active.data).toEqual([active]));
    await vi.waitFor(() => expect(both.all.data).toEqual([active, archived]));
    expect(env.fetch.calls(LIST)).toBe(2);
  });

  it('surfaces a refusal as an error rather than as an empty list', async () => {
    env = createTestQueryEnv({ [LIST]: apiError(422, 'the session store is unreadable') });

    const query = inRoot(() => useSessions(() => false));

    await vi.waitFor(() => expect(query.isError).toBe(true));
    expect(query.error?.message).toContain('Failed to list sessions');
    expect(query.data).toBeUndefined();
  });

  it('paints the stored list before the fetch answers', async () => {
    const stored = [session('s-stored')];
    const live = [session('s-live')];
    localStorage.setItem(STORAGE_KEY, JSON.stringify(stored));
    let release: (() => void) | undefined;
    const answered = new Promise<void>((resolve) => {
      release = resolve;
    });
    env = createTestQueryEnv({
      [LIST]: async () => {
        await answered;
        return listReply(live);
      },
    });

    const query = inRoot(() => useSessions(() => false));

    // The first read, before the fetch resolves, already has the last answer.
    expect(query.data).toEqual(stored);

    release?.();
    await vi.waitFor(() => expect(query.data).toEqual(live));
    expect(env.fetch.calls(LIST)).toBe(1);
  });

  it('writes the fetched list back to storage', async () => {
    const rows = [session('s-1')];
    env = createTestQueryEnv({ [LIST]: () => listReply(rows) });

    const query = inRoot(() => useSessions(() => false));

    await vi.waitFor(() => expect(query.data).toEqual(rows));
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null')).toEqual(rows);
  });
});

describe('useDeleteSession', () => {
  it('removes the row from both variants before the refetch answers', async () => {
    const kept = session('s-1');
    const doomed = session('s-2');
    let rows = [kept, doomed];
    let release: (() => void) | undefined;
    const refetched = new Promise<void>((resolve) => {
      release = resolve;
    });
    let calls = 0;
    env = createTestQueryEnv({
      [LIST]: async () => {
        calls += 1;
        // The two mounts answer at once; every later read waits, so the test
        // can look at the cache while the refetch is still in flight.
        if (calls > 2) await refetched;
        return listReply(rows);
      },
      'DELETE /api/session/s-2': () => {
        rows = [kept];
        return new Response(null, { status: 204 });
      },
    });

    const all = inRoot(() => ({
      active: useSessions(() => false),
      archived: useSessions(() => true),
      remove: useDeleteSession(),
    }));

    await vi.waitFor(() => expect(all.active.data).toEqual([kept, doomed]));
    await vi.waitFor(() => expect(all.archived.data).toEqual([kept, doomed]));
    expect(env.fetch.calls(LIST)).toBe(2);

    const settled = all.remove.mutateAsync('s-2');

    // Both lists lose the row on the write, not on the answer that follows it.
    await vi.waitFor(() => expect(all.active.data).toEqual([kept]));
    expect(all.archived.data).toEqual([kept]);
    await vi.waitFor(() => expect(env.fetch.calls(LIST)).toBe(4));

    release?.();
    await settled;
    expect(all.active.data).toEqual([kept]);
  });

  it('reports a refused delete to the caller and keeps the row', async () => {
    const rows = [session('s-1')];
    env = createTestQueryEnv({
      [LIST]: () => listReply(rows),
      'DELETE /api/session/s-1': apiError(422, 'the session is still running'),
    });

    const both = inRoot(() => ({ list: useSessions(() => false), remove: useDeleteSession() }));
    await vi.waitFor(() => expect(both.list.data).toEqual(rows));

    await expect(both.remove.mutateAsync('s-1')).rejects.toThrow(/Failed to delete session/);
    expect(both.list.data).toEqual(rows);
    expect(env.fetch.calls(LIST)).toBe(1);
  });
});

describe('useArchiveSession', () => {
  it('moves the row from the active list to the archived one', async () => {
    const kept = session('s-1');
    const moving = session('s-2');
    let active = [kept, moving];
    let all = [kept, moving];
    env = createTestQueryEnv({
      [LIST]: (request) => listReply(wantsArchived(request) ? all : active),
      'POST /api/session/s-2/archive': () => {
        active = [kept];
        all = [kept, { ...moving, archived: true }];
        return new Response(null, { status: 204 });
      },
    });

    const parts = inRoot(() => ({
      active: useSessions(() => false),
      archived: useSessions(() => true),
      archive: useArchiveSession(),
    }));

    await vi.waitFor(() => expect(parts.active.data).toEqual([kept, moving]));
    await vi.waitFor(() => expect(parts.archived.data).toEqual([kept, moving]));

    await parts.archive.mutateAsync('s-2');

    expect(parts.active.data).toEqual([kept]);
    expect(parts.archived.data).toEqual([kept, { ...moving, archived: true }]);
  });
});

describe('useUnarchiveSession', () => {
  it('invalidates the list, so one mount and one unarchive are two fetches', async () => {
    const restored = session('s-2', { archived: true });
    let rows: Session[] = [];
    env = createTestQueryEnv({
      [LIST]: () => listReply(rows),
      'POST /api/session/s-2/unarchive': () => {
        rows = [{ ...restored, archived: false }];
        return new Response(null, { status: 204 });
      },
    });

    const both = inRoot(() => ({ list: useSessions(() => false), restore: useUnarchiveSession() }));

    await vi.waitFor(() => expect(both.list.data).toEqual([]));
    expect(env.fetch.calls(LIST)).toBe(1);

    await both.restore.mutateAsync('s-2');

    await vi.waitFor(() => expect(both.list.data).toEqual([{ ...restored, archived: false }]));
    expect(env.fetch.calls(LIST)).toBe(2);
  });
});

describe('useCreateSession', () => {
  it('puts the new session at the head of the list, then asks the daemon again', async () => {
    const existing = session('s-1');
    const created = session('s-new');
    let rows = [existing];
    let sent: unknown = null;
    env = createTestQueryEnv({
      [LIST]: () => listReply(rows),
      'POST /api/session': async (request) => {
        sent = await request.json();
        rows = [created, existing];
        return created;
      },
    });

    const both = inRoot(() => ({ list: useSessions(() => false), create: useCreateSession() }));
    await vi.waitFor(() => expect(both.list.data).toEqual([existing]));

    await expect(both.create.mutateAsync({ kilns: ['main'] })).resolves.toEqual(created);

    expect(sent).toEqual({ kilns: ['main'] });
    expect(both.list.data).toEqual([created, existing]);
    expect(env.fetch.calls(LIST)).toBe(2);
  });
});

describe('the lifecycle mutations', () => {
  it('patch the state of the cached row, and ask for the session again', async () => {
    const row = session('s-1');
    env = createTestQueryEnv({
      [LIST]: () => listReply([row]),
      'POST /api/session/s-1/pause': () => new Response(null, { status: 204 }),
      'POST /api/session/s-1/resume': () => new Response(null, { status: 204 }),
      'POST /api/session/s-1/end': () => new Response(null, { status: 204 }),
    });

    const parts = inRoot(() => ({
      list: useSessions(() => false),
      pause: usePauseSession(),
      resume: useResumeSession(),
      end: useEndSession(),
    }));
    await vi.waitFor(() => expect(parts.list.data).toEqual([row]));

    await parts.pause.mutateAsync('s-1');
    expect(parts.list.data?.[0].state).toBe('paused');

    await parts.resume.mutateAsync('s-1');
    expect(parts.list.data?.[0].state).toBe('active');

    await parts.end.mutateAsync('s-1');
    expect(parts.list.data?.[0].state).toBe('ended');
  });
});

describe('useSetSessionTitle', () => {
  it('patches the title in the list and under the session key', async () => {
    const row = session('s-1', { title: 'old' });
    let sent: unknown = null;
    env = createTestQueryEnv({
      [LIST]: () => listReply([row]),
      'PUT /api/session/s-1/title': async (request) => {
        sent = await request.json();
        return new Response(null, { status: 204 });
      },
    });

    const both = inRoot(() => ({ list: useSessions(() => false), rename: useSetSessionTitle() }));
    await vi.waitFor(() => expect(both.list.data).toEqual([row]));
    env.client.setQueryData(keys.session('s-1'), row);

    await both.rename.mutateAsync({ id: 's-1', title: 'new' });

    expect(sent).toEqual({ title: 'new' });
    expect(both.list.data?.[0].title).toBe('new');
    expect(env.client.getQueryData<Session>(keys.session('s-1'))?.title).toBe('new');
  });
});

describe('useCancelSession', () => {
  it('answers the daemon’s flag and leaves every list alone', async () => {
    const rows = [session('s-1')];
    env = createTestQueryEnv({
      [LIST]: () => listReply(rows),
      'POST /api/session/s-1/cancel': () => ({ cancelled: true }),
    });

    const both = inRoot(() => ({ list: useSessions(() => false), cancel: useCancelSession() }));
    await vi.waitFor(() => expect(both.list.data).toEqual(rows));

    await expect(both.cancel.mutateAsync('s-1')).resolves.toBe(true);
    expect(env.fetch.calls(LIST)).toBe(1);
  });
});

describe('useExportSession', () => {
  it('answers the rendered markdown, which the caller holds itself', async () => {
    env = createTestQueryEnv({
      'POST /api/session/s-1/export': () =>
        new Response('# Session\n\nhello', { headers: { 'Content-Type': 'text/markdown' } }),
    });

    const exporter = inRoot(() => useExportSession());

    await expect(exporter.mutateAsync('s-1')).resolves.toBe('# Session\n\nhello');
  });
});

describe('useSession and fetchSessionOnce', () => {
  it('read one session, and answer the second caller from the cache', async () => {
    const row = session('s-1');
    env = createTestQueryEnv({ 'GET /api/session/s-1': () => row });

    const query = inRoot(() => useSession(() => 's-1'));

    await vi.waitFor(() => expect(query.data).toEqual(row));
    await expect(fetchSessionOnce('s-1')).resolves.toEqual(row);
    expect(env.fetch.calls('GET /api/session/s-1')).toBe(1);
  });

  it('asks for nothing until there is a session to ask about', async () => {
    env = createTestQueryEnv({ 'GET /api/session/s-1': () => session('s-1') });

    const query = inRoot(() => useSession(() => null));

    await Promise.resolve();
    expect(query.data).toBeUndefined();
    expect(env.fetch.calls('GET /api/session/s-1')).toBe(0);
  });

  it('reports a session the daemon no longer holds', async () => {
    env = createTestQueryEnv({
      'GET /api/session/s-gone': apiError(404, 'no such session'),
    });

    await expect(fetchSessionOnce('s-gone')).rejects.toThrow(/Failed to get session/);
  });
});

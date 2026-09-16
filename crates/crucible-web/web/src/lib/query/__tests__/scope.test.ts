import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { createRoot } from 'solid-js';
import { apiError } from '@/test-utils/mock-fetch';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import type { Session } from '@/lib/types';
import { resetSessionsForTests, useSession, useSessions } from '../sessions';
import { useConnectSessionKiln, useDisconnectSessionKiln } from '../scope';

const CONNECT = 'POST /api/session/s-1/kilns/connect';
const DISCONNECT = 'POST /api/session/s-1/kilns/disconnect';
const LIST = 'GET /api/session/list';
const ONE = 'GET /api/session/s-1';

/** The storage key the session roster is seeded from. */
const STORAGE_KEY = 'crucible:cache:sessions';

function session(id: string, over: Partial<Session> = {}): Session {
  return {
    id,
    session_type: 'chat',
    kilns: ['main'],
    workspace: '/repos/app',
    state: 'active',
    title: `Session ${id}`,
    agent_model: null,
    agent_mode: null,
    started_at: '2026-09-15T00:00:00Z',
    last_activity: null,
    event_count: 0,
    archived: false,
    ...over,
  };
}

/** One session as the daemon sends it; `lib/api.ts` maps the field names. */
function wire(row: Session): Record<string, unknown> {
  return {
    session_id: row.id,
    type: row.session_type,
    kilns: row.kilns,
    workspace: row.workspace,
    state: row.state,
    title: row.title,
    agent_model: row.agent_model,
    agent: null,
    started_at: row.started_at,
    last_activity: null,
    event_count: row.event_count,
    archived: row.archived,
  };
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

describe('session scope mutations patch with echo', () => {
  it('folds the attached set the daemon answered into the cached session', async () => {
    // The echo replaces the read that would otherwise follow the write: the
    // route answers the whole scope, so nothing has to ask for it again. The
    // session key is read here because the mutation does not invalidate it —
    // what the assertion sees is the patch and not a refetch.
    env = createTestQueryEnv({
      [ONE]: () => wire(session('s-1')),
      [CONNECT]: () => ({ session_id: 's-1', kilns: ['main', 'extra'], workspace: '/repos/app' }),
    });

    const row = inRoot(() => useSession(() => 's-1'));
    await vi.waitFor(() => expect(row.data?.kilns).toEqual(['main']));
    const connect = inRoot(() => useConnectSessionKiln());

    await connect.mutateAsync({ id: 's-1', kiln: 'extra' });

    expect(row.data?.kilns).toEqual(['main', 'extra']);
    expect(env.fetch.calls(ONE)).toBe(1);
  });

  it('takes the daemon’s set, not the one the click asked for', async () => {
    // The daemon re-checks kiln trust on attach and refuses a mutation
    // mid-turn, so the set it answers can differ from the one that was asked
    // for. An optimistic patch here would show a kiln that is not attached.
    env = createTestQueryEnv({
      [ONE]: () => wire(session('s-1')),
      [CONNECT]: () => ({ session_id: 's-1', kilns: ['main'], workspace: '/repos/app' }),
    });

    const row = inRoot(() => useSession(() => 's-1'));
    await vi.waitFor(() => expect(row.data?.kilns).toEqual(['main']));
    const connect = inRoot(() => useConnectSessionKiln());

    await connect.mutateAsync({ id: 's-1', kiln: 'untrusted' });

    // The refused kiln is nowhere, and the daemon was not asked again.
    expect(row.data?.kilns).toEqual(['main']);
    expect(env.fetch.calls(ONE)).toBe(1);
  });

  it('folds a detach, including the workspace the echo carries', async () => {
    env = createTestQueryEnv({
      [ONE]: () => wire(session('s-1', { kilns: ['main', 'extra'] })),
      [DISCONNECT]: () => ({ session_id: 's-1', kilns: ['main'], workspace: null }),
    });

    const row = inRoot(() => useSession(() => 's-1'));
    await vi.waitFor(() => expect(row.data?.kilns).toEqual(['main', 'extra']));
    const disconnect = inRoot(() => useDisconnectSessionKiln());

    await disconnect.mutateAsync({ id: 's-1', kiln: 'extra' });

    expect(row.data?.kilns).toEqual(['main']);
    expect(row.data?.workspace).toBeNull();
  });

  it('sends the kiln by the name the registry issued', async () => {
    let sent: { kiln: string } | null = null;
    env = createTestQueryEnv({
      [CONNECT]: async (request) => {
        sent = (await request.json()) as { kiln: string };
        return { session_id: 's-1', kilns: ['main', 'extra'], workspace: null };
      },
    });

    const connect = inRoot(() => useConnectSessionKiln());
    await connect.mutateAsync({ id: 's-1', kiln: 'extra' });

    expect(sent).toEqual({ kiln: 'extra' });
  });

  it('leaves the cached set alone when the daemon refuses the attach', async () => {
    env = createTestQueryEnv({
      [ONE]: () => wire(session('s-1')),
      [CONNECT]: apiError(422, 'kiln is not registered'),
    });

    const row = inRoot(() => useSession(() => 's-1'));
    await vi.waitFor(() => expect(row.data?.kilns).toEqual(['main']));
    const connect = inRoot(() => useConnectSessionKiln());

    await expect(connect.mutateAsync({ id: 's-1', kiln: 'nope' })).rejects.toThrow();

    expect(row.data?.kilns).toEqual(['main']);
    expect(env.fetch.calls(ONE)).toBe(1);
  });

  it('asks the daemon for the roster again after the echo lands', async () => {
    // A row in either list carries the kilns this write just changed, and the
    // daemon has the last word on both.
    const rows = [session('s-1')];
    env = createTestQueryEnv({
      [LIST]: () => ({ sessions: rows.map(wire), total: rows.length }),
      [CONNECT]: () => ({ session_id: 's-1', kilns: ['main', 'extra'], workspace: null }),
    });

    const list = inRoot(() => useSessions(() => false));
    await vi.waitFor(() => expect(list.data).toHaveLength(1));
    const connect = inRoot(() => useConnectSessionKiln());

    await connect.mutateAsync({ id: 's-1', kiln: 'extra' });

    await vi.waitFor(() => expect(env.fetch.calls(LIST)).toBe(2));
  });
});

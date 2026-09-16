import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { createRoot, createSignal } from 'solid-js';
import { apiError } from '@/test-utils/mock-fetch';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import type { Session } from '@/lib/types';
import { resetSessionsForTests, useSessions } from '../sessions';
import { useAllModels, useSessionModels, useSwitchModel } from '../models';

const SESSION_MODELS = 'GET /api/session/s-1/models';
const SWITCH = 'POST /api/session/s-1/model';
const ALL_MODELS = 'GET /api/models';
const SESSION_LIST = 'GET /api/session/list';

/** The storage key `swrLocal('models')` wrote, which `useAllModels` keeps. */
const STORAGE_KEY = 'crucible:cache:models';

/** The envelope both model routes answer; `lib/api.ts` unwraps `models`. */
function body(models: string[]): { models: string[] } {
  return { models };
}

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

/** One session as the daemon sends it; `lib/api.ts` maps the field names. */
function wire(row: Session): Record<string, unknown> {
  return {
    session_id: row.session_id,
    type: row.type,
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

describe('useSessionModels', () => {
  it('asks once for every reader of one session', async () => {
    env = createTestQueryEnv({ [SESSION_MODELS]: () => body(['a/one', 'a/two']) });

    const readers = inRoot(() => ({
      composer: useSessionModels(() => 's-1'),
      settings: useSessionModels(() => 's-1'),
    }));

    await vi.waitFor(() => expect(readers.composer.data).toEqual(['a/one', 'a/two']));
    expect(readers.settings.data).toEqual(['a/one', 'a/two']);
    expect(env.fetch.calls(SESSION_MODELS)).toBe(1);
  });

  it('asks nothing while no session is selected', async () => {
    env = createTestQueryEnv({ [SESSION_MODELS]: () => body(['a/one']) });

    const query = inRoot(() => useSessionModels(() => null));

    await vi.waitFor(() => expect(query.fetchStatus).toBe('idle'));
    expect(env.fetch.calls(SESSION_MODELS)).toBe(0);
  });

  it('answers the session the shell points at now, not the one it opened with', async () => {
    // This is the race the `modelsGeneration` counter guarded. The key carries
    // the session id, so a late answer lands on its own key and cannot
    // overwrite the list of the session the user moved to.
    let release: () => void = () => {};
    const answered = new Promise<void>((resolve) => (release = resolve));
    env = createTestQueryEnv({
      [SESSION_MODELS]: async () => {
        await answered;
        return body(['slow/one']);
      },
      'GET /api/session/s-2/models': () => body(['fast/one']),
    });
    const [id, setId] = createSignal<string | null>('s-1');

    const readers = inRoot(() => ({ moving: useSessionModels(id), pinned: useSessionModels(() => 's-1') }));
    setId('s-2');
    await vi.waitFor(() => expect(readers.moving.data).toEqual(['fast/one']));

    release();
    // The first session's answer lands now. It lands on `s-1`'s key, where the
    // pane still bound to `s-1` reads it, and leaves `s-2`'s list alone.
    await vi.waitFor(() => expect(readers.pinned.data).toEqual(['slow/one']));
    expect(readers.moving.data).toEqual(['fast/one']);
  });
});

describe('useSwitchModel', () => {
  it('invalidates session-scoped models', async () => {
    // An agent that accepts a model may offer a different set of them after
    // the switch, so the list the picker holds is stale the moment the daemon
    // answers.
    env = createTestQueryEnv({
      [SESSION_MODELS]: () => body(['a/one', 'a/two']),
      [SWITCH]: () => new Response(null, { status: 204 }),
    });

    const query = inRoot(() => useSessionModels(() => 's-1'));
    await vi.waitFor(() => expect(query.data).toBeDefined());
    const switching = inRoot(() => useSwitchModel());

    await switching.mutateAsync({ id: 's-1', modelId: 'a/two' });

    await vi.waitFor(() => expect(env.fetch.calls(SESSION_MODELS)).toBe(2));
  });

  it('names the new model in the cached rows the rail draws', async () => {
    // The rail reads `agent_model` off the session row, and it must not wait
    // for a list refetch to redraw the chip the user just changed.
    const rows = [session('s-1')];
    env = createTestQueryEnv({
      [SESSION_LIST]: () => ({ sessions: rows.map(wire), total: rows.length }),
      [SWITCH]: () => new Response(null, { status: 204 }),
    });

    const list = inRoot(() => useSessions(() => false));
    await vi.waitFor(() => expect(list.data).toHaveLength(1));
    const switching = inRoot(() => useSwitchModel());

    await switching.mutateAsync({ id: 's-1', modelId: 'a/two' });

    expect(list.data?.[0].agent_model).toBe('a/two');
    // The row is patched, not re-read: the list key is not under the session
    // key the mutation invalidates.
    expect(env.fetch.calls(SESSION_LIST)).toBe(1);
  });

  it('sends the daemon the model the caller picked', async () => {
    let asked: string | null = null;
    env = createTestQueryEnv({
      [SESSION_MODELS]: () => body(['a/one']),
      [SWITCH]: async (request) => {
        asked = ((await request.json()) as { model_id: string }).model_id;
        return new Response(null, { status: 204 });
      },
    });

    const switching = inRoot(() => useSwitchModel());
    await switching.mutateAsync({ id: 's-1', modelId: 'a/two' });

    expect(asked).toBe('a/two');
  });

  it('leaves the cached model alone when the daemon refuses the switch', async () => {
    // A model the daemon rejects must not be named in the rail: the row would
    // claim a model the session does not run.
    const rows = [session('s-1')];
    env = createTestQueryEnv({
      [SESSION_LIST]: () => ({ sessions: rows.map(wire), total: rows.length }),
      [SWITCH]: apiError(422, 'unknown model'),
    });

    const list = inRoot(() => useSessions(() => false));
    await vi.waitFor(() => expect(list.data).toHaveLength(1));
    const switching = inRoot(() => useSwitchModel());

    await expect(switching.mutateAsync({ id: 's-1', modelId: 'a/two' })).rejects.toThrow();

    expect(list.data?.[0].agent_model).toBe('openai/gpt-4o');
  });
});

describe('useAllModels', () => {
  it('asks once for the desktop composer and the phone sheet together', async () => {
    env = createTestQueryEnv({ [ALL_MODELS]: () => body(['a/one', 'b/two']) });

    const both = inRoot(() => ({ composer: useAllModels(), sheet: useAllModels() }));

    await vi.waitFor(() => expect(both.composer.data).toEqual(['a/one', 'b/two']));
    expect(both.sheet.data).toEqual(['a/one', 'b/two']);
    expect(env.fetch.calls(ALL_MODELS)).toBe(1);
  });

  it('paints the stored catalogue before the daemon answers, then corrects it', async () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(['stored/one']));
    let release: () => void = () => {};
    const answered = new Promise<void>((resolve) => (release = resolve));
    env = createTestQueryEnv({
      [ALL_MODELS]: async () => {
        await answered;
        return body(['live/one']);
      },
    });

    const query = inRoot(() => useAllModels());

    await vi.waitFor(() => expect(query.data).toEqual(['stored/one']));
    release();
    await vi.waitFor(() => expect(query.data).toEqual(['live/one']));
    expect(JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null')).toEqual(['live/one']);
  });

  it('reaches the caller with the refusal instead of an empty catalogue', async () => {
    env = createTestQueryEnv({ [ALL_MODELS]: apiError(500, 'provider registry is down') });

    const query = inRoot(() => useAllModels());

    await vi.waitFor(() => expect(query.isError).toBe(true));
    expect(query.error?.message).toContain('provider registry is down');
  });
});

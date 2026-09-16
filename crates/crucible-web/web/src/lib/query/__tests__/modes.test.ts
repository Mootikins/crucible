import { describe, it, expect, afterEach, vi } from 'vitest';
import { createRoot, createSignal } from 'solid-js';
import { apiError } from '@/test-utils/mock-fetch';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { installFakeEventSource, onlyEventSource } from '@/test-utils/sse';
import type { SessionModes } from '@/lib/types';
import { keys } from '../keys';
import { sessionEvents } from '../sse';
import { installSessionEventRoute } from '../routes/session';
import { useSessionModes, useSetSessionMode } from '../modes';

const LIST = 'GET /api/session/s-1/modes';
const SET = 'POST /api/session/s-1/mode';

/** The list the daemon declares for one session, in Lua. */
function modes(current: string, ...ids: string[]): SessionModes {
  return {
    current_mode_id: current,
    modes: ids.map((id) => ({
      id,
      name: id,
      description: null,
      icon: null,
      color: null,
      review_policy: 'none' as const,
    })),
  };
}

let env: TestQueryEnv;
let dispose: (() => void) | null = null;

afterEach(() => {
  dispose?.();
  dispose = null;
  env?.restore();
});

/** Runs the body under one Solid owner, which the test disposes afterwards. */
function inRoot<T>(body: () => T): T {
  return createRoot((disposeRoot) => {
    dispose = disposeRoot;
    return body();
  });
}

describe('useSessionModes', () => {
  it('fetches once for the chat pane and the status chips together', async () => {
    // The gate of this task: `ChatContext` read the list and
    // `SessionStatusChips` read it again, on every mount of the chips.
    env = createTestQueryEnv({ [LIST]: () => modes('ask', 'ask', 'plan') });

    const readers = inRoot(() => ({
      chat: useSessionModes(() => 's-1'),
      chips: useSessionModes(() => 's-1'),
    }));

    await vi.waitFor(() => expect(readers.chat.data).toEqual(modes('ask', 'ask', 'plan')));
    expect(readers.chips.data).toEqual(modes('ask', 'ask', 'plan'));
    expect(env.fetch.calls(LIST)).toBe(1);
  });

  it('asks nothing while the pane shows no session', async () => {
    env = createTestQueryEnv({ [LIST]: () => modes('ask', 'ask') });

    const query = inRoot(() => useSessionModes(() => null));

    await vi.waitFor(() => expect(query.fetchStatus).toBe('idle'));
    expect(env.fetch.calls(LIST)).toBe(0);
  });

  it('follows the session the pane binds to', async () => {
    // The list is per session, and the pane that read it once read it for
    // whichever session it held at the time it was built.
    env = createTestQueryEnv({
      [LIST]: () => modes('ask', 'ask'),
      'GET /api/session/s-2/modes': () => modes('plan', 'plan'),
    });
    const [id, setId] = createSignal<string | null>('s-1');

    const query = inRoot(() => useSessionModes(id));
    await vi.waitFor(() => expect(query.data?.current_mode_id).toBe('ask'));

    setId('s-2');
    await vi.waitFor(() => expect(query.data?.current_mode_id).toBe('plan'));
  });

  it('reads the list again when the stream says the mode changed', async () => {
    // `routes/session.ts` invalidates this key on `mode_changed`. The refetch
    // also carries `current_mode_id`, which the event just moved.
    installFakeEventSource();
    env = createTestQueryEnv({ [LIST]: () => modes('ask', 'ask', 'plan') });
    installSessionEventRoute();

    const query = inRoot(() => useSessionModes(() => 's-1'));
    await vi.waitFor(() => expect(query.data).toBeDefined());

    const stop = sessionEvents('s-1').subscribe(() => {});
    onlyEventSource().emit('mode_changed', { type: 'mode_changed', mode: 'plan' });

    await vi.waitFor(() => expect(env.fetch.calls(LIST)).toBe(2));
    stop();
  });
});

describe('useSetSessionMode', () => {
  it('moves the current mode before the daemon answers, and keeps it', async () => {
    // The control must answer the click. The daemon is held open here, so the
    // assertion falls inside the window the optimistic patch exists for.
    let release: () => void = () => {};
    const answered = new Promise<void>((resolve) => (release = resolve));
    let current = 'ask';
    env = createTestQueryEnv({
      [LIST]: () => modes(current, 'ask', 'plan'),
      [SET]: async (request) => {
        const asked = (await request.json()) as { mode: string };
        await answered;
        current = asked.mode;
        return new Response(null, { status: 204 });
      },
    });
    const held = () => env.client.getQueryData<SessionModes>(keys.sessionModes('s-1'));

    const query = inRoot(() => useSessionModes(() => 's-1'));
    await vi.waitFor(() => expect(query.data).toBeDefined());
    const switched = inRoot(() => useSetSessionMode());

    const writing = switched.mutateAsync({ id: 's-1', mode: 'plan' });
    await vi.waitFor(() => expect(held()?.current_mode_id).toBe('plan'));

    release();
    await writing;
    await vi.waitFor(() => expect(env.fetch.calls(LIST)).toBe(2));
    expect(held()?.current_mode_id).toBe('plan');
  });

  it('puts the mode back when the daemon rejects it', async () => {
    // A mode the daemon refuses must not LOOK set: the chip would report a
    // plan mode nothing enforces.
    env = createTestQueryEnv({
      [LIST]: () => modes('ask', 'ask', 'plan'),
      [SET]: apiError(422, 'unknown mode'),
    });
    const held = () => env.client.getQueryData<SessionModes>(keys.sessionModes('s-1'));

    const query = inRoot(() => useSessionModes(() => 's-1'));
    await vi.waitFor(() => expect(query.data).toBeDefined());
    const switched = inRoot(() => useSetSessionMode());

    await expect(switched.mutateAsync({ id: 's-1', mode: 'plan' })).rejects.toThrow();

    expect(held()?.current_mode_id).toBe('ask');
  });

  it('asks the daemon for the list again once the write settles', async () => {
    env = createTestQueryEnv({
      [LIST]: () => modes('ask', 'ask', 'plan'),
      [SET]: () => new Response(null, { status: 204 }),
    });

    const query = inRoot(() => useSessionModes(() => 's-1'));
    await vi.waitFor(() => expect(query.data).toBeDefined());
    const switched = inRoot(() => useSetSessionMode());

    await switched.mutateAsync({ id: 's-1', mode: 'plan' });

    await vi.waitFor(() => expect(env.fetch.calls(LIST)).toBe(2));
  });
});

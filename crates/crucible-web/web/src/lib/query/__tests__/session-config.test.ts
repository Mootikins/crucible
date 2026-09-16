import { describe, it, expect, afterEach, vi } from 'vitest';
import { createRoot, createSignal } from 'solid-js';
import { apiError } from '@/test-utils/mock-fetch';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import type { AgentConfigOption } from '@/lib/types';
import { keys } from '../keys';
import {
  useAgentOptions,
  useGetContextStrategy,
  useGetPrecognition,
  useSessionKnobs,
  useSessionStatus,
  useSetAgentOption,
  useSetContextStrategy,
  useSetPrecognition,
} from '../session-config';

const KNOBS = 'GET /api/session/s-1/knobs';
const OPTIONS = 'GET /api/session/s-1/config/agent-options';
const SET_OPTION = 'POST /api/session/s-1/config/agent-options';
const PRECOG = 'GET /api/session/s-1/config/precognition';
const SET_PRECOG = 'PUT /api/session/s-1/config/precognition';
const STRATEGY = 'GET /api/session/s-1/config/context-strategy';
const SET_STRATEGY = 'PUT /api/session/s-1/config/context-strategy';
const STATUS = 'GET /api/session/s-1/status';

function option(id: string, current: string | boolean): AgentConfigOption {
  return {
    id,
    name: id,
    description: null,
    category: null,
    kind: typeof current === 'boolean' ? 'toggle' : 'select',
    current,
    choices: [{ value: 'low', name: 'low' }, { value: 'high', name: 'high' }],
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

describe('session config reads', () => {
  it('asks once per setting for every panel that draws it', async () => {
    // The settings panel read all three in one `onMount`, so a second panel —
    // or the same one reopened — paid for the round trip again.
    env = createTestQueryEnv({
      [KNOBS]: () => ({ knobs: [{ id: 'precognition', supported: true }] }),
      [OPTIONS]: () => ({ session_id: 's-1', options: [option('thought_level', 'low')] }),
      [PRECOG]: () => ({ precognition_enabled: true }),
    });

    const readers = inRoot(() => ({
      knobs: useSessionKnobs(() => 's-1'),
      options: useAgentOptions(() => 's-1'),
      precognition: useGetPrecognition(() => 's-1'),
      secondKnobs: useSessionKnobs(() => 's-1'),
    }));

    await vi.waitFor(() => expect(readers.knobs.data?.knobs).toHaveLength(1));
    await vi.waitFor(() => expect(readers.precognition.data).toBe(true));
    expect(readers.options.data?.options).toHaveLength(1);
    expect(readers.secondKnobs.data?.knobs).toHaveLength(1);
    expect(env.fetch.calls(KNOBS)).toBe(1);
  });

  it('asks nothing while no session is selected', async () => {
    env = createTestQueryEnv({ [KNOBS]: () => ({ knobs: [] }) });

    const query = inRoot(() => useSessionKnobs(() => null));

    await vi.waitFor(() => expect(query.fetchStatus).toBe('idle'));
    expect(env.fetch.calls(KNOBS)).toBe(0);
  });

  it('follows the session the panel is pointed at', async () => {
    // The panel read its settings once, in `onMount`, from whichever session
    // was current then. Opening it, switching session and reading it again
    // showed the first session's settings.
    env = createTestQueryEnv({
      [PRECOG]: () => ({ precognition_enabled: true }),
      'GET /api/session/s-2/config/precognition': () => ({ precognition_enabled: false }),
    });
    const [id, setId] = createSignal<string | null>('s-1');

    const query = inRoot(() => useGetPrecognition(id));
    await vi.waitFor(() => expect(query.data).toBe(true));

    setId('s-2');
    await vi.waitFor(() => expect(query.data).toBe(false));
  });

  it('answers an empty option list for a daemon that has no such method', async () => {
    // An older daemon answers 404 here, and an internal session has no agent
    // options anyway. Neither is an error the panel should show.
    env = createTestQueryEnv({ [OPTIONS]: apiError(404, 'unknown method') });

    const query = inRoot(() => useAgentOptions(() => 's-1'));

    await vi.waitFor(() => expect(query.data).toBeDefined());
    expect(query.data?.options).toEqual([]);
    expect(query.isError).toBe(false);
  });
});

describe('session config mutations invalidate scoped cache', () => {
  it('re-reads the agent options after one is set', async () => {
    // The agent is the only authority on what the value became: it may clamp
    // or rename what it is sent, so the list is re-read and never patched.
    let current = 'low';
    env = createTestQueryEnv({
      [OPTIONS]: () => ({ session_id: 's-1', options: [option('thought_level', current)] }),
      [SET_OPTION]: async (request) => {
        current = ((await request.json()) as { value: string }).value;
        return new Response(null, { status: 204 });
      },
    });

    const query = inRoot(() => useAgentOptions(() => 's-1'));
    await vi.waitFor(() => expect(query.data?.options[0].current).toBe('low'));
    const setting = inRoot(() => useSetAgentOption());

    await setting.mutateAsync({ id: 's-1', optionId: 'thought_level', value: 'high' });

    await vi.waitFor(() => expect(query.data?.options[0].current).toBe('high'));
    expect(env.fetch.calls(OPTIONS)).toBe(2);
  });

  it('moves the precognition toggle before the daemon answers', async () => {
    let release: () => void = () => {};
    const answered = new Promise<void>((resolve) => (release = resolve));
    env = createTestQueryEnv({
      [PRECOG]: () => ({ precognition_enabled: true }),
      [SET_PRECOG]: async () => {
        await answered;
        return new Response(null, { status: 204 });
      },
    });
    const held = () => env.client.getQueryData<boolean>(keys.sessionPrecognition('s-1'));

    const query = inRoot(() => useGetPrecognition(() => 's-1'));
    await vi.waitFor(() => expect(query.data).toBe(true));
    const setting = inRoot(() => useSetPrecognition());

    const writing = setting.mutateAsync({ id: 's-1', enabled: false });
    await vi.waitFor(() => expect(held()).toBe(false));

    release();
    await writing;
  });

  it('puts the precognition toggle back when the daemon refuses it', async () => {
    // A setting the daemon rejects must not look applied: the toggle would
    // claim context injection that nothing does.
    //
    // The re-read that follows the refusal is held open, so the assertion
    // falls in the window the revert exists for. Letting the re-read answer
    // would restore the value on its own, and the case would pass with the
    // revert deleted.
    let reads = 0;
    let release: () => void = () => {};
    const reread = new Promise<void>((resolve) => (release = resolve));
    env = createTestQueryEnv({
      [PRECOG]: async () => {
        reads += 1;
        if (reads > 1) await reread;
        return { precognition_enabled: true };
      },
      [SET_PRECOG]: apiError(422, 'precognition is unsupported here'),
    });

    const query = inRoot(() => useGetPrecognition(() => 's-1'));
    await vi.waitFor(() => expect(query.data).toBe(true));
    const setting = inRoot(() => useSetPrecognition());

    const writing = setting.mutateAsync({ id: 's-1', enabled: false });

    // The re-read starts after the revert ran, and it is the held call, so
    // the value asserted here is the one the revert wrote.
    await vi.waitFor(() => expect(reads).toBe(2));
    expect(env.client.getQueryData<boolean>(keys.sessionPrecognition('s-1'))).toBe(true);

    release();
    await expect(writing).rejects.toThrow();
  });

  it('holds the new context strategy, and asks the daemon again', async () => {
    let stored: string | null = 'truncate';
    env = createTestQueryEnv({
      [STRATEGY]: () => ({ context_strategy: stored }),
      [SET_STRATEGY]: async (request) => {
        stored = ((await request.json()) as { context_strategy: string }).context_strategy;
        return new Response(null, { status: 204 });
      },
    });

    const query = inRoot(() => useGetContextStrategy(() => 's-1'));
    await vi.waitFor(() => expect(query.data).toBe('truncate'));
    const setting = inRoot(() => useSetContextStrategy());

    await setting.mutateAsync({ id: 's-1', strategy: 'summarize' });

    await vi.waitFor(() => expect(query.data).toBe('summarize'));
  });
});

describe('useSessionStatus', () => {
  it('answers the slots the daemon published for one session', async () => {
    env = createTestQueryEnv({
      [STATUS]: () => ({ status: [{ key: 'branch', plugin: 'scm', text: 'master', level: 'info' }] }),
    });

    const query = inRoot(() => useSessionStatus(() => 's-1'));

    await vi.waitFor(() => expect(query.data).toHaveLength(1));
    expect(query.data?.[0].text).toBe('master');
  });

  it('answers no slots at all when the daemon refuses the question', async () => {
    // A failed status read is "no chips", never a notification: it fails on
    // every daemon reconnect, and a session with nothing to say is normal.
    env = createTestQueryEnv({ [STATUS]: apiError(500, 'plugin host is down') });

    const query = inRoot(() => useSessionStatus(() => 's-1'));

    await vi.waitFor(() => expect(query.data).toEqual([]));
    expect(query.isError).toBe(false);
  });

  it('drops the previous session’s slots when the shell re-points', async () => {
    // The chips of the session the user left must never linger over the one
    // they opened, not even while its read is in flight.
    let release: () => void = () => {};
    const answered = new Promise<void>((resolve) => (release = resolve));
    env = createTestQueryEnv({
      [STATUS]: () => ({ status: [{ key: 'branch', plugin: 'scm', text: 'master', level: 'info' }] }),
      'GET /api/session/s-2/status': async () => {
        await answered;
        return { status: [] };
      },
    });
    const [id, setId] = createSignal<string | null>('s-1');

    const query = inRoot(() => useSessionStatus(id));
    await vi.waitFor(() => expect(query.data).toHaveLength(1));

    setId('s-2');
    expect(query.data).toBeUndefined();
    release();
  });
});

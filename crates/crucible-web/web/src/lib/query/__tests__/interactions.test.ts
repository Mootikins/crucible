import { describe, it, expect, afterEach, vi } from 'vitest';
import { createRoot } from 'solid-js';
import { apiError } from '@/test-utils/mock-fetch';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { installFakeEventSource, onlyEventSource } from '@/test-utils/sse';
import type { PendingInteractionEntry } from '@/lib/api';
import type { InteractionOf } from '@/lib/types';
import { getBus } from '@/lib/bus';
import { keys } from '../keys';
import { sessionEvents } from '../sse';
import { installSessionEventRoute } from '../routes/session';
import {
  fetchPendingInteractionsOnce,
  usePendingInteractions,
  useRespondToInteraction,
} from '../interactions';

const PENDING = 'GET /api/interactions/pending';
const RESPOND = 'POST /api/interaction/respond';

const perm: InteractionOf<'permission'> = {
  kind: 'permission',
  id: 'r-1',
  action_type: 'bash',
  tokens: ['cargo', 'test'],
  tool_name: 'Bash',
};

/** One entry of the daemon's aggregate. */
function entry(sessionId: string, requestId: string): PendingInteractionEntry {
  return { session_id: sessionId, request_id: requestId, request: { ...perm, id: requestId } };
}

/** The envelope `GET /api/interactions/pending` answers. */
function body(entries: PendingInteractionEntry[]): { pending: PendingInteractionEntry[] } {
  return { pending: entries };
}

let env: TestQueryEnv;
let dispose: (() => void) | null = null;

afterEach(() => {
  dispose?.();
  dispose = null;
  env?.restore();
  vi.useRealTimers();
});

/** Runs the body under one Solid owner, which the test disposes afterwards. */
function inRoot<T>(body: () => T): T {
  return createRoot((disposeRoot) => {
    dispose = disposeRoot;
    return body();
  });
}

describe('usePendingInteractions', () => {
  it('is polled by the store and read by the panes from one request', async () => {
    // The badge, the inbox and every chat pane asked the daemon for this
    // aggregate. The store polled it; the panes each read it again on bind.
    env = createTestQueryEnv({ [PENDING]: () => body([entry('s-1', 'r-1')]) });

    const readers = inRoot(() => ({
      store: usePendingInteractions(),
      inbox: usePendingInteractions(),
    }));

    await vi.waitFor(() => expect(readers.store.data).toHaveLength(1));
    expect(readers.inbox.data).toHaveLength(1);
    expect(env.fetch.calls(PENDING)).toBe(1);
  });

  it('asks again on its own every ten seconds', async () => {
    // The fallback the stream cannot cover: a session with no open pane
    // raises a request on a stream nothing is subscribed to.
    vi.useFakeTimers();
    env = createTestQueryEnv({ [PENDING]: () => body([]) });

    const query = inRoot(() => usePendingInteractions());
    await vi.waitFor(() => expect(query.data).toEqual([]));

    await vi.advanceTimersByTimeAsync(10_000);

    await vi.waitFor(() => expect(env.fetch.calls(PENDING)).toBe(2));
  });

  it('asks again when the stream says a request was raised', async () => {
    // `routes/session.ts` invalidates this key on `interaction_requested`,
    // which is what makes the ten-second interval a fallback and not the
    // mechanism.
    installFakeEventSource();
    env = createTestQueryEnv({ [PENDING]: () => body([]) });
    installSessionEventRoute();

    const query = inRoot(() => usePendingInteractions());
    await vi.waitFor(() => expect(query.data).toEqual([]));

    const stop = sessionEvents('s-1').subscribe(() => {});
    onlyEventSource().emit('interaction_requested', {
      type: 'interaction_requested',
      request: { ...perm, id: 'r-2' },
    });

    await vi.waitFor(() => expect(env.fetch.calls(PENDING)).toBe(2));
    stop();
  });

  it('answers a binding pane from the held list, without a second request', async () => {
    env = createTestQueryEnv({ [PENDING]: () => body([entry('s-1', 'r-1')]) });

    const query = inRoot(() => usePendingInteractions());
    await vi.waitFor(() => expect(query.data).toHaveLength(1));

    const held = await fetchPendingInteractionsOnce();

    expect(held.map((e) => e.request_id)).toEqual(['r-1']);
    expect(env.fetch.calls(PENDING)).toBe(1);
  });
});

describe('useRespondToInteraction', () => {
  it('takes the answered request out of the pending list at once', async () => {
    // The daemon's aggregate lags the answer by up to one poll, so a list
    // that waited for a refetch would keep showing a card that is answered —
    // and the pane that read it would raise the card again on its next bind.
    env = createTestQueryEnv({
      [PENDING]: () => body([entry('s-1', 'r-1'), entry('s-2', 'r-2')]),
      [RESPOND]: () => new Response(null, { status: 204 }),
    });

    const query = inRoot(() => usePendingInteractions());
    await vi.waitFor(() => expect(query.data).toHaveLength(2));
    const respond = inRoot(() => useRespondToInteraction());

    await respond.mutateAsync({
      sessionId: 's-1',
      requestId: 'r-1',
      response: { kind: 'permission', allowed: true },
    });

    expect(query.data?.map((e) => e.request_id)).toEqual(['r-2']);
    // No refetch of its own: the aggregate that would answer it still lists
    // the request this client just answered.
    expect(env.fetch.calls(PENDING)).toBe(1);
  });

  it('tells every pane of that session the request is answered', async () => {
    // The inbox answers on another pane's behalf, and that pane's own card
    // has to go with it. The window CustomEvent that used to say so is gone.
    env = createTestQueryEnv({
      [PENDING]: () => body([entry('s-1', 'r-1')]),
      [RESPOND]: () => new Response(null, { status: 204 }),
    });
    const announced: { sessionId: string; requestId: string }[] = [];
    const off = getBus().on('interactionResolved', (payload) => announced.push(payload));

    const respond = inRoot(() => useRespondToInteraction());
    await respond.mutateAsync({ sessionId: 's-1', requestId: 'r-1', response: { ok: true } });

    expect(announced).toEqual([{ sessionId: 's-1', requestId: 'r-1' }]);
    off();
  });

  it('sends the daemon the session, the request and the answer', async () => {
    let sent: Record<string, unknown> | null = null;
    env = createTestQueryEnv({
      [RESPOND]: async (request) => {
        sent = (await request.json()) as Record<string, unknown>;
        return new Response(null, { status: 204 });
      },
    });

    const respond = inRoot(() => useRespondToInteraction());
    await respond.mutateAsync({
      sessionId: 's-1',
      requestId: 'r-1',
      response: { kind: 'permission', allowed: false },
    });

    expect(sent).toEqual({
      session_id: 's-1',
      request_id: 'r-1',
      response: { kind: 'permission', allowed: false },
    });
  });

  it('puts the request back when the daemon refuses the answer', async () => {
    // A request that was not answered is still waiting on the user; dropping
    // it from the badge would hide an agent that is parked.
    env = createTestQueryEnv({
      [PENDING]: () => body([entry('s-1', 'r-1')]),
      [RESPOND]: apiError(422, 'no such request'),
    });

    const query = inRoot(() => usePendingInteractions());
    await vi.waitFor(() => expect(query.data).toHaveLength(1));
    const respond = inRoot(() => useRespondToInteraction());

    await expect(
      respond.mutateAsync({ sessionId: 's-1', requestId: 'r-1', response: { ok: true } }),
    ).rejects.toThrow();

    expect(env.client.getQueryData<PendingInteractionEntry[]>(keys.pendingInteractions())).toHaveLength(1);
  });
});

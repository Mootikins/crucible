import { describe, it, expect, afterEach, beforeEach, vi } from 'vitest';
import { createRoot } from 'solid-js';
import type { InteractionOf } from '@/lib/types';
import type { PendingInteractionEntry } from '@/lib/api';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';

// No `vi.mock('@/lib/api')`. The remote layer is the shared
// `usePendingInteractions()` list now, so the aggregate answers the ROUTE
// below — which is also what proves the store and the inbox read one request
// between them.
import { attentionStore, attentionActions } from '../attentionStore';

const PENDING = 'GET /api/interactions/pending';

let env: TestQueryEnv;
/** What the aggregate answers next, which each case names up front. */
let pending: PendingInteractionEntry[] = [];

const perm: InteractionOf<'permission'> = {
  kind: 'permission',
  id: 'req-1',
  action_type: 'bash',
  tokens: ['cargo', 'test'],
  tool_name: 'Bash',
};

beforeEach(async () => {
  for (const id of Object.keys(attentionStore.entries)) {
    attentionActions.clear(id);
  }
  pending = [];
  env = createTestQueryEnv({ [PENDING]: () => ({ pending }) });
  // Empty the remote (aggregate) layer too.
  await attentionActions.refresh();
});

afterEach(() => {
  env?.restore();
});

describe('attentionStore', () => {
  it('starts empty with a zero badge', () => {
    createRoot((dispose) => {
      expect(attentionStore.attentionCount()).toBe(0);
      expect(attentionStore.waiting()).toEqual([]);
      dispose();
    });
  });

  it('counts sessions with a pending interaction', () => {
    createRoot((dispose) => {
      attentionActions.report('s1', { pendingInteraction: perm, title: 'scheduler' });
      attentionActions.report('s2', { isStreaming: true, title: 'other' });

      expect(attentionStore.attentionCount()).toBe(1);
      expect(attentionStore.waiting()[0].sessionId).toBe('s1');
      expect(attentionStore.streamingCount()).toBe(1);
      dispose();
    });
  });

  it('drops the badge when the interaction resolves', () => {
    createRoot((dispose) => {
      attentionActions.report('s1', { pendingInteraction: perm });
      expect(attentionStore.attentionCount()).toBe(1);

      attentionActions.report('s1', { pendingInteraction: null });
      expect(attentionStore.attentionCount()).toBe(0);
      dispose();
    });
  });

  it('merges patches without losing earlier state', () => {
    createRoot((dispose) => {
      attentionActions.report('s1', { pendingInteraction: perm, title: 'scheduler' });
      attentionActions.report('s1', { isStreaming: true });

      const entry = attentionStore.get('s1');
      expect(entry?.pendingInteraction).toEqual(perm);
      expect(entry?.isStreaming).toBe(true);
      expect(entry?.title).toBe('scheduler');
      dispose();
    });
  });

  it('clear removes the session entirely', () => {
    createRoot((dispose) => {
      attentionActions.report('s1', { pendingInteraction: perm });
      attentionActions.clear('s1');
      expect(attentionStore.get('s1')).toBeUndefined();
      expect(attentionStore.attentionCount()).toBe(0);
      dispose();
    });
  });
});

describe('attentionStore — daemon aggregate (refresh)', () => {
  it('surfaces polled pending interactions for sessions without a tab', async () => {
    pending = [{ session_id: 's-remote', request_id: 'r1', request: { ...perm, id: 'r1' } }];
    await attentionActions.refresh();

    await createRoot(async (dispose) => {
      expect(attentionStore.attentionCount()).toBe(1);
      expect(attentionStore.get('s-remote')?.pendingInteraction?.id).toBe('r1');
      dispose();
    });
  });

  it('local (open tab) state shadows the polled entry for the same session', async () => {
    pending = [
      { session_id: 's1', request_id: 'r1', request: { ...perm, id: 'r1' } },
      { session_id: 's2', request_id: 'r2', request: { ...perm, id: 'r2' } },
    ];
    await attentionActions.refresh();

    // s1 has an open tab whose reducer already saw the response.
    attentionActions.report('s1', { pendingInteraction: null, title: 't1' });

    await createRoot(async (dispose) => {
      expect(attentionStore.attentionCount()).toBe(1);
      expect(attentionStore.waiting()[0].sessionId).toBe('s2');
      dispose();
    });
  });

  it('resolving from the Inbox never shadows future polled pendings (no local tombstone)', async () => {
    // First pending arrives via poll, answered from the Inbox.
    pending = [{ session_id: 's1', request_id: 'r1', request: { ...perm, id: 'r1' } }];
    await attentionActions.refresh();
    expect(attentionStore.attentionCount()).toBe(1);

    attentionActions.resolveInteraction('s1', 'r1');
    expect(attentionStore.attentionCount()).toBe(0);

    // The SAME session raises a new permission later — it must surface.
    pending = [{ session_id: 's1', request_id: 'r2', request: { ...perm, id: 'r2' } }];
    await attentionActions.refresh();
    expect(attentionStore.attentionCount()).toBe(1);
    expect(attentionStore.get('s1')?.pendingInteraction?.id).toBe('r2');
  });

  it('a later refresh drops resolved entries', async () => {
    pending = [{ session_id: 's9', request_id: 'r9', request: { ...perm, id: 'r9' } }];
    await attentionActions.refresh();
    expect(attentionStore.attentionCount()).toBe(1);

    pending = [];
    await attentionActions.refresh();
    expect(attentionStore.attentionCount()).toBe(0);
  });
});

describe('attentionStore — the shared aggregate', () => {
  it('mirrors the shared list while polling runs, and stops with it', async () => {
    // `startPolling` mounts an observer of `usePendingInteractions()`; the
    // interval, the visibility refetch and the invalidation the chat stream
    // triggers all belong to that query. An invalidation stands for all three
    // here — it is the one the stream sends on `interaction_requested`.
    const stop = attentionActions.startPolling();
    pending = [{ session_id: 's-remote', request_id: 'r1', request: { ...perm, id: 'r1' } }];

    await env.client.invalidateQueries({ queryKey: ['interactions', 'pending'] });

    await vi.waitFor(() => expect(attentionStore.attentionCount()).toBe(1));
    expect(attentionStore.get('s-remote')?.pendingInteraction?.id).toBe('r1');

    stop();
    // The observer left with the root, so nothing refetches for this store.
    pending = [];
    const asked = env.fetch.calls(PENDING);
    await env.client.invalidateQueries({ queryKey: ['interactions', 'pending'] });
    expect(env.fetch.calls(PENDING)).toBe(asked);
  });

  it('builds its derived views under an owner', async () => {
    // Four memos used to be built at module scope, where Solid reports a
    // computation that will never be disposed — on every import of this store.
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    vi.resetModules();

    await import('../attentionStore');

    const said = warn.mock.calls.flat().join(' ');
    expect(said).not.toContain('createRoot');
    warn.mockRestore();
  });
});

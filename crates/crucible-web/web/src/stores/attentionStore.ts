import { createEffect, createMemo, createRoot } from 'solid-js';
import { createStore, produce, reconcile } from 'solid-js/store';
import type { InteractionRequest } from '@/lib/types';
import type { PendingInteractionEntry } from '@/lib/api';
import { refetchPendingInteractions, usePendingInteractions } from '@/lib/query/interactions';

// ── Cross-session attention state ────────────────────────────────────────
// Two sources, one merged view:
//
// - `local`: reported live by each mounted ChatProvider (one per chat tab)
//   as its SSE reducer fires; cleared on dispose. Authoritative for any
//   session with an open tab — it also carries streaming state.
// - `remote`: the daemon's aggregate (`GET /api/interactions/pending`,
//   backed by session.pending_interactions), so sessions WITHOUT an open tab
//   still raise the header badge and appear in the Inbox. The poll is no
//   longer written here: `usePendingInteractions()` owns the request, its
//   ten-second interval and its cache, and every other reader of that list
//   shares them.
//
// Merge rule: a local entry shadows the remote one for the same session —
// the subscribed tab sees interaction events the instant they happen and
// clears them the instant they're answered.

export interface SessionAttention {
  sessionId: string;
  title: string | null;
  pendingInteraction: InteractionRequest | null;
  isStreaming: boolean;
}

const [local, setLocal] = createStore<Record<string, SessionAttention>>({});
const [remote, setRemote] = createStore<Record<string, SessionAttention>>({});

function report(sessionId: string, patch: Partial<Omit<SessionAttention, 'sessionId'>>): void {
  setLocal(
    produce((s) => {
      const existing = s[sessionId] ?? {
        sessionId,
        title: null,
        pendingInteraction: null,
        isStreaming: false,
      };
      s[sessionId] = { ...existing, ...patch };
    })
  );
}

function clear(sessionId: string): void {
  setLocal(
    produce((s) => {
      delete s[sessionId];
    })
  );
  setRemote(
    produce((s) => {
      delete s[sessionId];
    })
  );
}

/** Mark one interaction answered (Inbox respond path). Updates whichever
 * layer holds it — never *creates* a local entry: a local tombstone with
 * `pendingInteraction: null` would permanently shadow every future polled
 * pending for that session (local shadows remote by design). */
function resolveInteraction(sessionId: string, requestId: string): void {
  if (local[sessionId]?.pendingInteraction?.id === requestId) {
    setLocal(sessionId, 'pendingInteraction', null);
  }
  if (remote[sessionId]?.pendingInteraction?.id === requestId) {
    setRemote(
      produce((s) => {
        delete s[sessionId];
      })
    );
  }
}

/** Folds one answer of the aggregate into the remote layer. */
function applyPending(pending: PendingInteractionEntry[]): void {
  const next: Record<string, SessionAttention> = {};
  for (const entry of pending) {
    next[entry.session_id] = {
      sessionId: entry.session_id,
      title: null,
      pendingInteraction: entry.request,
      isStreaming: false,
    };
  }
  setRemote(reconcile(next));
}

/**
 * Re-reads the daemon's aggregate now, through the key every reader shares.
 *
 * The answer also reaches the inbox and the chat panes, because they read the
 * same key rather than a copy of this store.
 */
async function refresh(): Promise<void> {
  applyPending(await refetchPendingInteractions());
}

/**
 * Mirrors the shared aggregate into the remote layer. Returns a stop fn.
 *
 * The app calls it once, at start. What it starts is an OBSERVER of
 * `usePendingInteractions()`, not a timer: the interval, the refetch on a tab
 * becoming visible again and the invalidation the chat stream triggers all
 * belong to that query now. The store keeps only the merge rule.
 *
 * The root is what gives the effect an owner, and disposing it takes the
 * observer off the query — so the interval stops with it.
 */
function startPolling(): () => void {
  return createRoot((dispose) => {
    const pending = usePendingInteractions();
    createEffect(() => {
      const entries = pending.data;
      if (entries) applyPending(entries);
    });
    return dispose;
  });
}

/**
 * The derived views, under one root.
 *
 * A memo built at module scope has no owner, which Solid reports as a
 * computation that will never be disposed — a warning on every import of this
 * store. The root is deliberately never disposed: these four live as long as
 * the module does, which is what the warning was asking to be made explicit.
 */
const { waiting, attentionCount, streamingCount } = createRoot(() => {
  /** Merged view: local (open tabs) shadows remote (the shared aggregate). */
  const merged = createMemo<Record<string, SessionAttention>>(() => ({
    ...remote,
    ...local,
  }));

  /** Sessions currently waiting on a human response. */
  const waiting = createMemo(() =>
    Object.values(merged()).filter((e) => e.pendingInteraction !== null)
  );

  /** Header/inbox badge count. */
  const attentionCount = createMemo(() => waiting().length);

  const streamingCount = createMemo(
    () => Object.values(merged()).filter((e) => e.isStreaming).length
  );

  return { waiting, attentionCount, streamingCount };
});

function get(sessionId: string): SessionAttention | undefined {
  return local[sessionId] ?? remote[sessionId];
}

export const attentionStore = {
  /** Local (open-tab) entries only — tests and debugging. */
  entries: local,
  waiting,
  attentionCount,
  streamingCount,
  get,
} as const;

export const attentionActions = {
  report,
  clear,
  resolveInteraction,
  refresh,
  startPolling,
} as const;

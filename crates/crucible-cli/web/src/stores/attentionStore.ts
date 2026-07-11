import { createMemo } from 'solid-js';
import { createStore, produce, reconcile } from 'solid-js/store';
import type { InteractionRequest } from '@/lib/types';
import { listPendingInteractions } from '@/lib/api';

// ── Cross-session attention state ────────────────────────────────────────
// Two sources, one merged view:
//
// - `local`: reported live by each mounted ChatProvider (one per chat tab)
//   as its SSE reducer fires; cleared on dispose. Authoritative for any
//   session with an open tab — it also carries streaming state.
// - `remote`: the daemon's aggregate (`GET /api/interactions/pending`,
//   backed by session.pending_interactions), polled so sessions WITHOUT an
//   open tab still raise the header badge and appear in the Inbox.
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

/** Re-fetch the daemon's pending-interaction aggregate. */
async function refresh(): Promise<void> {
  const pending = await listPendingInteractions();
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

const POLL_INTERVAL_MS = 10_000;

/** Poll the daemon aggregate while the page is visible. Returns a stop fn. */
function startPolling(): () => void {
  void refresh();
  const tick = () => {
    if (!document.hidden) void refresh();
  };
  const interval = setInterval(tick, POLL_INTERVAL_MS);
  const onVisible = () => {
    if (!document.hidden) void refresh();
  };
  document.addEventListener('visibilitychange', onVisible);
  return () => {
    clearInterval(interval);
    document.removeEventListener('visibilitychange', onVisible);
  };
}

/** Merged view: local (open tabs) shadows remote (daemon poll). */
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

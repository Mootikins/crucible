import { createMemo } from 'solid-js';
import { createStore, produce } from 'solid-js/store';
import type { InteractionRequest } from '@/lib/types';

// ── Cross-session attention state ────────────────────────────────────────
// Permission/Ask/Popup requests and streaming state live per-session inside
// each ChatProvider (one per chat tab). The Inbox and the header badge need
// the aggregate: "which sessions are waiting on me right now?" Each
// ChatProvider reports into this module store as its SSE reducer fires and
// clears its entry on dispose.
//
// Honest limitation (recorded in Web User Stories): only sessions with an
// open chat tab stream events to the browser, so only those can raise
// attention here. Daemon-side aggregation is the long-run fix.

export interface SessionAttention {
  sessionId: string;
  title: string | null;
  pendingInteraction: InteractionRequest | null;
  isStreaming: boolean;
}

const [entries, setEntries] = createStore<Record<string, SessionAttention>>({});

function report(sessionId: string, patch: Partial<Omit<SessionAttention, 'sessionId'>>): void {
  setEntries(
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
  setEntries(
    produce((s) => {
      delete s[sessionId];
    })
  );
}

/** Sessions currently waiting on a human response, oldest-reported first. */
const waiting = createMemo(() =>
  Object.values(entries).filter((e) => e.pendingInteraction !== null)
);

/** Header/inbox badge count. */
const attentionCount = createMemo(() => waiting().length);

const streamingCount = createMemo(
  () => Object.values(entries).filter((e) => e.isStreaming).length
);

function get(sessionId: string): SessionAttention | undefined {
  return entries[sessionId];
}

export const attentionStore = {
  entries,
  waiting,
  attentionCount,
  streamingCount,
  get,
} as const;

export const attentionActions = {
  report,
  clear,
} as const;

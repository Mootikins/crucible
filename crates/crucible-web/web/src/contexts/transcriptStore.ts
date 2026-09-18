import { createStore } from 'solid-js/store';
import type {
  ChatMode,
  ChatEvent,
  ConnectionStatus,
  InteractionRequest,
  Message,
  SubagentEvent,
} from '@/lib/types';
import { advanceSessionCursor, sessionEvents } from '@/lib/query/sse';
import { attentionActions } from '@/stores/attentionStore';
import { tabHost } from '@/lib/tab-host';
import { createChatEventReducer } from './chatEventReducer';

/**
 * The live transcript of every session the shell shows, keyed by session id.
 *
 * The transcript belongs to the SESSION, not to whichever pane mounted first:
 * two panes of one session fold ONE stream into ONE store (a pane subscribing
 * with a store of its own would apply every event twice), and a pane that
 * adopts a session another pane already shows reads the transcript that
 * session already built. This is the only sanctioned copy of query data
 * outside the cache — the document `useSessionHistory` holds is what the
 * daemon persisted; this holds what the panes are drawing.
 *
 * Panes hold their session open (`retainTranscript`/`releaseTranscript`);
 * the last pane out frees the state, the same refcount the SSE root keeps
 * on the source itself.
 */

/** Everything a pane draws of one session, and the streaming state around it. */
export interface TranscriptState {
  messages: Message[];
  subagentEvents: SubagentEvent[];
  currentStreamingMessageId: string | null;
  isLoading: boolean;
  isStreaming: boolean;
  error: string | null;
  connectionStatus: ConnectionStatus;
  pendingInteraction: InteractionRequest | null;
  chatMode: ChatMode;
  sessionTitle: string | null;
  /** Messages typed while a turn streamed, awaiting dispatch. The optimistic
   * entries render at the end of the streaming block; the queue turns each
   * into its own turn when the stream goes idle, in join order. */
  queuedTurns: QueuedTurn[];
}

/** One queued prompt and the optimistic entry that renders it. */
export interface QueuedTurn {
  /** The temp id of the queued user message (its transcript entry). */
  tempId: string;
  content: string;
}

function blankTranscript(): TranscriptState {
  return {
    messages: [],
    subagentEvents: [],
    currentStreamingMessageId: null,
    isLoading: false,
    isStreaming: false,
    error: null,
    connectionStatus: 'connected',
    pendingInteraction: null,
    chatMode: 'ask',
    sessionTitle: null,
    queuedTurns: [],
  };
}

const [transcripts, setTranscripts] = createStore<Record<string, TranscriptState>>({});

/** One refcount per session a pane is bound to. */
const refcounts = new Map<string, number>();
/** The unsubscribe of the one stream subscription per session. */
const unsubscribers = new Map<string, () => void>();
/**
 * The last seq APPLIED to the session's transcript. A streamed or replayed
 * event at or below it has already been folded — by this stream, or by the
 * history hydration that recorded it — and applying it again would draw the
 * turn twice.
 */
const lastAppliedSeq = new Map<string, number>();
/**
 * One-shot event identities already folded WITHOUT a seq — synthetic events
 * (`stream_gap`) and client-minted ones (`connection`), for which the seq
 * comparison has nothing to compare.
 */
const appliedEvents = new Map<string, Set<string>>();
/** Waiters for the session's first stream open, and whether it happened. */
const openWaiters = new Map<string, { opened: boolean; waiters: (() => void)[] }>();

/**
 * The event types whose reducer case ADDS an entry rather than updating one,
 * so a replay of the same event would draw it twice. `tool_result_delta`
 * repeats its call id per frame and `token` carries no id at all — neither is
 * one-shot, and neither is listed.
 */
const ONE_SHOT_EVENTS: Record<string, true> = {
  tool_call: true,
  message_complete: true,
  segment_complete: true,
  subagent_spawned: true,
  subagent_completed: true,
  subagent_failed: true,
  delegation_spawned: true,
  delegation_completed: true,
  delegation_failed: true,
  interaction_requested: true,
};

function oneShotKey(event: ChatEvent): string | null {
  return ONE_SHOT_EVENTS[event.type] !== undefined && 'id' in event
    ? `${event.type}:${event.id}`
    : null;
}

function stateOf(sessionId: string): TranscriptState {
  return transcripts[sessionId] ?? blankTranscript();
}

/** Creates the session's state if no pane showed it yet. No stream, no fold. */
function ensureState(sessionId: string): void {
  if (transcripts[sessionId]) return;
  setTranscripts(sessionId, blankTranscript());
  appliedEvents.set(sessionId, new Set());
  openWaiters.set(sessionId, { opened: false, waiters: [] as (() => void)[] });
}

/** Builds the session's one reducer and opens its one stream subscription. */
function ensureStream(sessionId: string): void {
  if (unsubscribers.has(sessionId)) return;

  const patch = (part: Partial<TranscriptState>) => setTranscripts(sessionId, part);
  const title = () => stateOf(sessionId).sessionTitle;

  // The same mirrors the pane kept: the Inbox badge covers every session
  // with an open tab, not just the focused one.
  const setIsStreaming = (value: boolean) => {
    patch({ isStreaming: value });
    attentionActions.report(sessionId, { isStreaming: value, title: title() });
  };
  const setPendingInteraction = (request: InteractionRequest | null) => {
    patch({ pendingInteraction: request });
    attentionActions.report(sessionId, { pendingInteraction: request, title: title() });
  };

  const reducer = createChatEventReducer({
    messages: () => stateOf(sessionId).messages,
    currentStreamingMessageId: () => stateOf(sessionId).currentStreamingMessageId,
    setCurrentStreamingMessageId: (id) => patch({ currentStreamingMessageId: id }),
    addMessage: (message) =>
      setTranscripts(sessionId, 'messages', (prev) => [...prev, message]),
    insertMessageAfter: (index, message) =>
      setTranscripts(sessionId, 'messages', (prev) => {
        const next = [...prev];
        next.splice(index + 1, 0, message);
        return next;
      }),
    updateMessage: (id, updates) =>
      setTranscripts(sessionId, 'messages', (prev) => {
        const index = prev.findIndex((m) => m.id === id);
        if (index === -1) return prev;
        const next = [...prev];
        next[index] = { ...next[index], ...updates };
        return next;
      }),
    appendToMessage: (id, content) =>
      setTranscripts(sessionId, 'messages', (prev) => {
        const index = prev.findIndex((m) => m.id === id);
        if (index === -1) return prev;
        const next = [...prev];
        next[index] = { ...next[index], content: next[index].content + content };
        return next;
      }),
    addToolMessage: (tool) => {
      const toolMessage: Message = {
        id: `tool-${tool.callId ?? tool.id}`,
        role: 'tool',
        content: '',
        timestamp: Date.now(),
        toolCall: tool,
      };
      setTranscripts(sessionId, 'messages', (prev) => {
        const streamingId = stateOf(sessionId).currentStreamingMessageId;
        const index = streamingId ? prev.findIndex((m) => m.id === streamingId) : -1;
        if (index !== -1 && prev[index].content === '') {
          const next = [...prev];
          next.splice(index, 0, toolMessage);
          return next;
        }
        return [...prev, toolMessage];
      });
    },
    updateToolMessage: (callId, updater) =>
      setTranscripts(
        sessionId,
        'messages',
        (m) => m.role === 'tool' && m.toolCall?.callId === callId,
        'toolCall',
        (tool) => updater(tool as NonNullable<Message['toolCall']>),
      ),
    setSubagentEvents: (value) =>
      setTranscripts(
        sessionId,
        'subagentEvents',
        typeof value === 'function' ? (value as (prev: SubagentEvent[]) => SubagentEvent[])(stateOf(sessionId).subagentEvents) : value,
      ),
    setChatMode: (mode) => patch({ chatMode: mode }),
    setPendingInteraction,
    setError: (value) => patch({ error: value }),
    setConnectionStatus: (value) => patch({ connectionStatus: value }),
    setIsLoading: (value) => patch({ isLoading: value }),
    setIsStreaming,
    onTitleChanged: (newTitle) => {
      patch({ sessionTitle: newTitle });
      const host = tabHost();
      const tab = host.find((t) => t.metadata?.sessionId === sessionId);
      if (tab) host.update(tab.id, { title: newTitle });
      attentionActions.report(sessionId, { title: newTitle });
      // The session list (Home resume, Inbox) learns the new name from the
      // stream's own route, which emits `sessionTitleChanged` on the bus once
      // per event. Announcing it here as well would announce it once per open
      // pane, and the panes of one session all carry the same title.
    },
  });

  const unsubscribe = sessionEvents(sessionId).subscribe(
    (event) => {
      const seq = event.seq;
      if (seq !== undefined) {
        // The seq comparison replaces the identity set for stamped events:
        // it covers every event kind (not only the one-shots) and every
        // replay boundary, because the cursor and the seq share one scale.
        if (seq <= (lastAppliedSeq.get(sessionId) ?? 0)) return;
        reducer(event);
        // Advance only AFTER the apply. A receipt that advanced the watermark
        // and then never applied would be skipped by the next replay too —
        // lost twice (T3 Code's rule). A reducer that throws propagates out
        // of this handler before these lines run, leaving the watermark
        // where the last successful apply left it.
        lastAppliedSeq.set(sessionId, seq);
        advanceSessionCursor(sessionId, seq);
        return;
      }
      // No seq: synthetic or client-minted. Identity dedup, so a replayed
      // one-shot cannot double-draw.
      const key = oneShotKey(event);
      if (key) {
        const seen = appliedEvents.get(sessionId);
        if (seen) {
          if (seen.has(key)) return;
          seen.add(key);
        }
      }
      reducer(event);
    },
    () => {
      const open = openWaiters.get(sessionId);
      if (!open || open.opened) return;
      open.opened = true;
      for (const resolve of open.waiters.splice(0)) resolve();
    },
  );
  unsubscribers.set(sessionId, unsubscribe);
}

/** A pane bound to this session holds its transcript open. Idempotent. */
export function retainTranscript(sessionId: string): void {
  refcounts.set(sessionId, (refcounts.get(sessionId) ?? 0) + 1);
  ensureState(sessionId);
  ensureStream(sessionId);
}

/**
 * A pane left this session; the last one out frees the state and stream.
 *
 * The seq watermark and the resume cursor OUTLIVE the last pane on purpose:
 * the daemon's log still holds everything past them, so a pane that rebinds
 * reopens the stream from the same position instead of losing the events no
 * one was watching.
 */
export function releaseTranscript(sessionId: string): void {
  const held = (refcounts.get(sessionId) ?? 1) - 1;
  if (held > 0) {
    refcounts.set(sessionId, held);
    return;
  }
  refcounts.delete(sessionId);
  unsubscribers.get(sessionId)?.();
  unsubscribers.delete(sessionId);
  appliedEvents.delete(sessionId);
  openWaiters.delete(sessionId);
  forgetSession(sessionId);
}

/** The session's state, created on first sight. A reactive read by key. */
export function transcriptOf(sessionId: string): TranscriptState {
  ensureState(sessionId);
  return transcripts[sessionId];
}

/**
 * Records the max seq a history hydration folded, after the fold finished.
 *
 * One seam: the hydration is the OTHER apply path (it reconstructed the
 * transcript the log holds), so it moves the same watermark the stream's
 * applies move — and the resume cursor with it, because a stream reopened
 * after a hydration must replay only what the hydration did not cover.
 * Monotonic: a hydration that lands behind the stream (a slow read racing a
 * live turn) never drags the watermark back.
 */
export function recordTranscriptHydration(sessionId: string, maxSeq: number | undefined): void {
  if (maxSeq === undefined) return;
  if (maxSeq > (lastAppliedSeq.get(sessionId) ?? 0)) {
    lastAppliedSeq.set(sessionId, maxSeq);
    advanceSessionCursor(sessionId, maxSeq);
  }
}

/** Resolves when the session's stream first opened (at once, if it already has). */
export function transcriptOpened(sessionId: string): Promise<void> {
  const open = openWaiters.get(sessionId) ?? { opened: false, waiters: [] };
  if (open.opened) return Promise.resolve();
  // The executor form because the project's ES2022 lib predates
  // Promise.withResolvers; the resolver is stored, not nested.
  return new Promise<void>((resolve) => {
    open.waiters.push(resolve);
  });
}

/** Re-issues the session's stream after a lost connection. */
export function retryTranscriptStream(sessionId: string): void {
  sessionEvents(sessionId).reconnect();
}

/** Writes a pane's own live state onto the session (optimistic sends, gates). */
export function patchTranscript(sessionId: string, part: Partial<TranscriptState>): void {
  ensureState(sessionId);
  setTranscripts(sessionId, part);
}

/** Sets the session's streaming flag and reports it to the attention store. */
export function setTranscriptStreaming(sessionId: string, value: boolean): void {
  ensureState(sessionId);
  setTranscripts(sessionId, 'isStreaming', value);
  attentionActions.report(sessionId, { isStreaming: value, title: stateOf(sessionId).sessionTitle });
}

/** Sets the session's pending request and reports it to the attention store. */
export function setTranscriptPendingInteraction(
  sessionId: string,
  request: InteractionRequest | null,
): void {
  ensureState(sessionId);
  setTranscripts(sessionId, 'pendingInteraction', request);
  attentionActions.report(sessionId, {
    pendingInteraction: request,
    title: stateOf(sessionId).sessionTitle,
  });
}

/** Applies one mutation to the session's message list (fold, send, cancel). */
export function updateTranscriptMessages(
  sessionId: string,
  apply: (prev: Message[]) => Message[],
): void {
  ensureState(sessionId);
  setTranscripts(sessionId, 'messages', (prev) => apply(prev));
}

/**
 * Parks a mid-turn prompt: the optimistic entry renders at the end of the
 * streaming block, the queue entry waits for the stream to go idle.
 *
 * `existingTempId` adopts the optimistic entry an ALREADY-FAILED dispatch
 * left behind (a send the daemon refused as concurrent), so a requeue never
 * renders the prompt twice. Synchronous, so the pane that parks the entry is
 * the only pane that saw it — the flusher's shift below cannot double-park.
 */
export function queueTurn(sessionId: string, content: string, existingTempId?: string): void {
  ensureState(sessionId);
  const tempId = existingTempId ?? `msg_${Date.now()}_${Math.random().toString(36).substring(2, 9)}`;
  setTranscripts(sessionId, 'queuedTurns', (prev) => [...prev, { tempId, content }]);
  if (existingTempId === undefined) {
    setTranscripts(sessionId, 'messages', (prev) => [
      ...prev,
      { id: tempId, role: 'user', content, timestamp: Date.now(), queued: true },
    ]);
  } else {
    setTranscripts(
      sessionId,
      'messages',
      (m) => m.id === existingTempId,
      'queued',
      true,
    );
  }
}

/**
 * Takes the oldest queued turn, or nothing.
 *
 * The shift IS the claim: two panes of one session both run a flusher, and
 * only the one whose shift returned an entry may dispatch it — the second
 * reads the now-empty queue. Never dispatch off a peek.
 */
export function shiftQueuedTurn(sessionId: string): QueuedTurn | undefined {
  const queue = stateOf(sessionId).queuedTurns;
  if (queue.length === 0) return undefined;
  const [head] = queue;
  setTranscripts(sessionId, 'queuedTurns', (prev) => prev.slice(1));
  return head;
}

/** The session's messages as they stand, read outside a computation. */
export function transcriptMessages(sessionId: string): Message[] {
  return stateOf(sessionId).messages;
}

/** Wipes one session's transcript (Ctrl+K): every pane of that session. */
export function clearTranscript(sessionId: string): void {
  if (!transcripts[sessionId]) return;
  setTranscripts(sessionId, {
    ...blankTranscript(),
    // The title is the session's, not the transcript's; a cleared view keeps it.
    sessionTitle: transcripts[sessionId].sessionTitle,
  });
  appliedEvents.set(sessionId, new Set());
}

/** The test seam: forgets every session, so one case cannot answer the next.
 *  The seq watermark goes with them; `resetSseForTests` retires the cursors
 *  the streams derived from it. */
export function resetTranscriptsForTests(): void {
  for (const unsubscribe of unsubscribers.values()) unsubscribe();
  unsubscribers.clear();
  refcounts.clear();
  appliedEvents.clear();
  lastAppliedSeq.clear();
  openWaiters.clear();
  for (const sessionId of Object.keys(transcripts)) {
    forgetSession(sessionId);
  }
}

/** Removes one session's state. The typed setter takes no `undefined`, which
 *  is the only way solid's store deletes a key. */
function forgetSession(sessionId: string): void {
  setTranscripts(sessionId, undefined as unknown as TranscriptState);
}

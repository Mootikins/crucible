import { createSignal, untrack, type Accessor, type Setter } from 'solid-js';
import { createStore } from 'solid-js/store';
import type {
  ChatMode,
  ConnectionStatus,
  InteractionRequest,
  Message,
  SequencedChatEvent,
} from '@/lib/types';
import { advanceSessionCursor, sessionCursor, sessionEvents } from '@/lib/query/sse';
import { refetchSessionHistory } from '@/lib/query/history';
import { attentionActions } from '@/stores/attentionStore';
import { tabHost } from '@/lib/tab-host';
import {
  applyTranscriptOp,
  itemToMessage,
  type Transcript,
  type TranscriptItem,
  type TranscriptOp,
} from '@/lib/transcript';
import { generateMessageId } from '@/lib/turn';
import { createChatEventReducer } from './chatEventReducer';
import type { CommentRef } from '@/lib/diffset';

/**
 * The transcript of every session the shell shows, keyed by session id.
 *
 * The daemon folds the events of a session, and this store holds the result:
 * the snapshot that the history route serves, with the ops of each
 * `transcript` frame applied in order. The store folds no event itself.
 *
 * The transcript belongs to the SESSION, not to one pane: two panes of one
 * session read one store, fed by one stream subscription. Panes hold their
 * session open (`retainTranscript`/`releaseTranscript`); the last pane out
 * frees the state.
 *
 * Beside the daemon's transcript, the store holds what only this browser
 * knows: the optimistic entry of a message that the daemon did not echo yet,
 * the notice of a failed send, and the state around the stream.
 */

/** A message that this browser sent or queued, before the daemon echoed it. */
interface OptimisticTurn {
  /** The client-minted id of the entry. */
  id: string;
  content: string;
  timestamp: number;
  /** Typed while a turn ran, and not sent yet. */
  queued: boolean;
  /** The turn id that the send answered. The daemon's user turn with this id
   * replaces the entry. */
  messageId?: string;
  /** How many user turns the transcript held when the entry was made. Before
   * the send answers, the first new user turn with the same text replaces
   * the entry. */
  userTurnsBefore: number;
}

/** A row that only this browser draws, after the transcript item `after`. */
interface LocalEntry {
  message: Message;
  /** The id of the transcript item before the row, or null for the start. */
  after: string | null;
}

/** The state of one session that every pane of it reads. */
export interface TranscriptState {
  optimistic: OptimisticTurn[];
  local: LocalEntry[];
  isLoading: boolean;
  isStreaming: boolean;
  error: string | null;
  connectionStatus: ConnectionStatus;
  pendingInteraction: InteractionRequest | null;
  chatMode: ChatMode;
  sessionTitle: string | null;
  /** Messages typed while a turn streamed, awaiting dispatch, in join order. */
  queuedTurns: QueuedTurn[];
}

/** One queued prompt and the optimistic entry that renders it. */
export interface QueuedTurn {
  /** The id of the optimistic entry. */
  tempId: string;
  content: string;
  /** The stored review comments that the turn attaches. */
  comments?: CommentRef[];
}

/** The ops of one `transcript` frame. */
interface TranscriptFrame {
  seq: number | null;
  ops: TranscriptOp[];
}

/**
 * How the transcript of one session keeps up with the daemon.
 *
 * `log` holds the frames since the last snapshot. A new snapshot can be older
 * than the live stream: the daemon stores no text delta. The frames above the
 * snapshot's `as_of_seq` then apply again on top of it.
 */
interface SyncState {
  /** A snapshot read is in flight. New frames wait in `pending`. */
  resyncing: boolean;
  /** An op did not fit, and a new snapshot did not fix it. The store skips
   * the ops that do not fit, and reads a snapshot again when the turn ends. */
  stale: boolean;
  pending: TranscriptFrame[];
  log: TranscriptFrame[];
  /** When the read of the snapshot in the store finished, in ms. */
  fetchedAt: number;
  /** When the stream subscription of the session started, in ms. */
  subscribedAt: number;
  /** How many times the stream opened. */
  opens: number;
  /** The stream started at a resume cursor, so it replays stored events. */
  resumed: boolean;
  /** The seq of the first event that this subscription received. */
  firstLiveSeq: number | null;
  /** The `as_of_seq` of the last snapshot that the store took. */
  seedSeq: number | null;
  /** The first live seq was compared with the snapshot. */
  gapChecked: boolean;
}

/** The log keeps at most this many frames. An older frame is dropped. */
const MAX_LOGGED_FRAMES = 5000;

function blankTranscript(): TranscriptState {
  return {
    optimistic: [],
    local: [],
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

/**
 * The daemon's transcript of each session, outside the store: an applied op
 * replaces one item object, and the view maps each item object once.
 */
const daemonTranscripts = new Map<
  string,
  [Accessor<Transcript | null>, Setter<Transcript | null>]
>();
const syncStates = new Map<string, SyncState>();

/** One refcount per session a pane is bound to. */
const refcounts = new Map<string, number>();
/** The unsubscribe of the one stream subscription per session. */
const unsubscribers = new Map<string, () => void>();
/**
 * The last seq that the reducer read. A streamed or replayed event at or
 * below it was read already.
 */
const lastAppliedSeq = new Map<string, number>();
/** Waiters for the session's first stream open, and whether it happened. */
const openWaiters = new Map<string, { opened: boolean; waiters: (() => void)[] }>();

function stateOf(sessionId: string): TranscriptState {
  return transcripts[sessionId] ?? blankTranscript();
}

function signalOf(sessionId: string): [Accessor<Transcript | null>, Setter<Transcript | null>] {
  let signal = daemonTranscripts.get(sessionId);
  if (!signal) {
    signal = createSignal<Transcript | null>(null);
    daemonTranscripts.set(sessionId, signal);
  }
  return signal;
}

function syncOf(sessionId: string): SyncState {
  let sync = syncStates.get(sessionId);
  if (!sync) {
    sync = {
      resyncing: false,
      stale: false,
      pending: [],
      log: [],
      fetchedAt: 0,
      subscribedAt: 0,
      opens: 0,
      resumed: false,
      firstLiveSeq: null,
      seedSeq: null,
      gapChecked: false,
    };
    syncStates.set(sessionId, sync);
  }
  return sync;
}

/** Creates the session's state if no pane showed it yet. */
function ensureState(sessionId: string): void {
  if (transcripts[sessionId]) return;
  setTranscripts(sessionId, blankTranscript());
  openWaiters.set(sessionId, { opened: false, waiters: [] as (() => void)[] });
}

/** Removes each optimistic entry that the daemon's transcript now holds. */
function pruneOptimistic(sessionId: string, transcript: Transcript): void {
  const shown = unconfirmedOptimistic(stateOf(sessionId).optimistic, transcript.items);
  if (shown.length === stateOf(sessionId).optimistic.length) return;
  const keep = new Set(shown.map((entry) => entry.id));
  setTranscripts(sessionId, 'optimistic', (prev) => prev.filter((entry) => keep.has(entry.id)));
}

/**
 * Applies the ops of one frame. An op that does not fit is skipped, and the
 * first such op starts a snapshot read.
 */
function applyFrame(sessionId: string, frame: TranscriptFrame, replay = false): boolean {
  const [read, write] = signalOf(sessionId);
  const current = read();
  if (!current) return true;
  if (frame.seq !== null && frame.seq <= current.as_of_seq) return true;
  let next = current;
  let fitted = true;
  for (const op of frame.ops) {
    const applied = applyTranscriptOp(next, op);
    if (applied) next = applied;
    else fitted = false;
  }
  if (frame.seq !== null) next = { ...next, as_of_seq: Math.max(next.as_of_seq, frame.seq) };
  write(next);
  if (!replay) {
    const sync = syncOf(sessionId);
    sync.log.push(frame);
    if (sync.log.length > MAX_LOGGED_FRAMES) sync.log.shift();
  }
  pruneOptimistic(sessionId, next);
  return fitted;
}

/** Receives one `transcript` frame from the stream. */
function receiveFrame(sessionId: string, frame: TranscriptFrame): void {
  const sync = syncOf(sessionId);
  if (sync.resyncing || !signalOf(sessionId)[0]()) {
    sync.pending.push(frame);
    return;
  }
  if (!applyFrame(sessionId, frame) && !sync.stale) {
    sync.stale = true;
    resyncTranscript(sessionId);
  }
}

/**
 * Puts a snapshot of the daemon's transcript in the store.
 *
 * A snapshot older than the transcript the store holds is ignored, unless
 * the store asked for it (`force`). After the snapshot, the frames of the
 * log and the frames that waited apply again when they are newer.
 * `fetchedAt` is when the read of the snapshot finished.
 */
export function seedTranscript(
  sessionId: string,
  snapshot: Transcript,
  fetchedAt: number,
  force = false,
): void {
  // A caller in an effect must not subscribe to what the seed reads and writes.
  untrack(() => seed(sessionId, snapshot, fetchedAt, force));
}

function seed(sessionId: string, snapshot: Transcript, fetchedAt: number, force: boolean): void {
  ensureState(sessionId);
  const sync = syncOf(sessionId);
  const [read, write] = signalOf(sessionId);
  const current = read();
  if (!force && !sync.resyncing && current && snapshot.as_of_seq < current.as_of_seq) return;
  write(snapshot);
  sync.resyncing = false;
  sync.fetchedAt = fetchedAt;
  sync.seedSeq = snapshot.as_of_seq;
  if (force) sync.gapChecked = true;
  const frames = [...sync.log, ...sync.pending];
  sync.pending = [];
  sync.log = frames.filter((frame) => frame.seq === null || frame.seq > snapshot.as_of_seq);
  let fitted = true;
  for (const frame of sync.log) {
    if (!applyFrame(sessionId, frame, true)) fitted = false;
  }
  sync.stale = !fitted;
  pruneOptimistic(sessionId, read() ?? snapshot);
  // The snapshot covers every stored event up to its seq, so a reopened
  // stream replays only what follows it.
  if (snapshot.as_of_seq > (lastAppliedSeq.get(sessionId) ?? 0)) {
    lastAppliedSeq.set(sessionId, snapshot.as_of_seq);
    advanceSessionCursor(sessionId, snapshot.as_of_seq);
  }
  checkLiveGap(sessionId);
}

/**
 * Compares the first live event of a fresh stream with the snapshot.
 *
 * The daemon stamps each event of a session with the next seq, and a fresh
 * stream (no resume cursor) forwards each event after it subscribed. The
 * snapshot holds each event up to its `as_of_seq`. When the first live event
 * is above `as_of_seq + 1`, the events between happened after the snapshot
 * and before the subscription, and their ops are lost. The store then reads
 * a new snapshot. Otherwise no read is necessary.
 */
function checkLiveGap(sessionId: string): void {
  const sync = syncOf(sessionId);
  if (sync.resumed || sync.gapChecked) return;
  if (sync.firstLiveSeq === null || sync.seedSeq === null) return;
  sync.gapChecked = true;
  if (sync.firstLiveSeq > sync.seedSeq + 1) resyncTranscript(sessionId);
}

/**
 * Reads a new snapshot and puts it in the store. New frames wait until it
 * arrives. A failed read lets the waiting frames apply to what the store has.
 */
function resyncTranscript(sessionId: string): void {
  const sync = syncOf(sessionId);
  if (sync.resyncing) return;
  sync.resyncing = true;
  void refetchSessionHistory(sessionId)
    .then((document) => {
      if (!transcripts[sessionId]) return;
      seedTranscript(sessionId, document.transcript ?? { as_of_seq: 0, items: [] }, Date.now(), true);
    })
    .catch(() => {
      sync.resyncing = false;
      for (const frame of sync.pending.splice(0)) applyFrame(sessionId, frame);
    });
}

/** Builds the session's one reducer and opens its one stream subscription. */
function ensureStream(sessionId: string): void {
  if (unsubscribers.has(sessionId)) return;

  const patch = (part: Partial<TranscriptState>) => setTranscripts(sessionId, part);
  const sync = syncOf(sessionId);
  sync.subscribedAt = Date.now();
  // The stream states this cursor when it connects, below.
  sync.resumed = sessionCursor(sessionId) !== undefined;

  const reducer = createChatEventReducer({
    setChatMode: (mode) => patch({ chatMode: mode }),
    setPendingInteraction: (request) => setTranscriptPendingInteraction(sessionId, request),
    setError: (value) => patch({ error: value }),
    addErrorNotice: (message) => addLocalRow(sessionId, { role: 'system', content: message }),
    setConnectionStatus: (value) => patch({ connectionStatus: value }),
    setIsLoading: (value) => patch({ isLoading: value }),
    isStreaming: () => stateOf(sessionId).isStreaming,
    setIsStreaming: (value) => setTranscriptStreaming(sessionId, value),
    onTitleChanged: (newTitle) => {
      patch({ sessionTitle: newTitle });
      const host = tabHost();
      const tab = host.find((t) => t.metadata?.sessionId === sessionId);
      if (tab) host.update(tab.id, { title: newTitle });
      attentionActions.report(sessionId, { title: newTitle });
      // The session list (Home resume, Inbox) learns the new name from the
      // stream's own route, which emits `sessionTitleChanged` on the bus once
      // per event.
    },
  });

  const unsubscribe = sessionEvents(sessionId).subscribe(
    (event: SequencedChatEvent) => {
      if (event.seq != null && sync.firstLiveSeq === null) {
        sync.firstLiveSeq = event.seq;
        checkLiveGap(sessionId);
      }
      if (event.type === 'transcript') {
        receiveFrame(sessionId, { seq: event.seq ?? null, ops: event.ops });
        return;
      }
      // A replayed event of the stored log carries no ops, and a gap lost
      // some. Either way the store reads the snapshot again. A turn that
      // ended while an op did not fit is in the stored log now.
      if (
        (event.type === 'connection' && event.status === 'connected' && needsSnapshotOnOpen(sessionId)) ||
        (event.type === 'session_event' && event.event === 'stream_gap') ||
        (event.type === 'turn_finished' && syncOf(sessionId).stale)
      ) {
        resyncTranscript(sessionId);
      }
      const seq = event.seq;
      if (seq != null) {
        if (seq <= (lastAppliedSeq.get(sessionId) ?? 0)) return;
        reducer(event);
        // Advance only AFTER the apply. A receipt that advanced the watermark
        // and then never applied would be skipped by the next replay too.
        lastAppliedSeq.set(sessionId, seq);
        advanceSessionCursor(sessionId, seq);
        return;
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

/**
 * Whether an open of the stream makes the store read a new snapshot.
 *
 * A reopen does: the stream replays the events it missed without their ops.
 * The first open of a stream that resumes at a cursor does when the snapshot
 * in the store is older than the subscription (a cached read). The first
 * open of a fresh stream does not: `checkLiveGap` finds a lost event.
 */
function needsSnapshotOnOpen(sessionId: string): boolean {
  const sync = syncOf(sessionId);
  sync.opens += 1;
  if (sync.opens > 1) return true;
  if (!sync.resumed) return false;
  return signalOf(sessionId)[0]() !== null && sync.fetchedAt < sync.subscribedAt;
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
 * a pane that rebinds reopens the stream from the same position.
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
  openWaiters.delete(sessionId);
  forgetSession(sessionId);
}

/** The session's state, created on first sight. A reactive read by key. */
export function transcriptOf(sessionId: string): TranscriptState {
  ensureState(sessionId);
  return transcripts[sessionId];
}

/** The daemon's transcript of the session, or null before the first snapshot. */
export function daemonTranscriptOf(sessionId: string): Transcript | null {
  return signalOf(sessionId)[0]();
}

/** The optimistic entries that the transcript does not hold yet, in order. */
function unconfirmedOptimistic(
  optimistic: readonly OptimisticTurn[],
  items: readonly TranscriptItem[],
): OptimisticTurn[] {
  const userTurns = items.filter(
    (item): item is Extract<TranscriptItem, { type: 'user_turn' }> =>
      item.type === 'user_turn' && item.origin == null,
  );
  const ids = new Set(items.map((item) => item.id));
  const claimed = new Set<string>();
  const shown: OptimisticTurn[] = [];
  for (const entry of optimistic) {
    if (entry.messageId) {
      claimed.add(entry.messageId);
      if (!ids.has(entry.messageId)) shown.push(entry);
      continue;
    }
    const echo = userTurns
      .slice(entry.userTurnsBefore)
      .find((turn) => turn.content === entry.content && !claimed.has(turn.id));
    if (echo) claimed.add(echo.id);
    else shown.push(entry);
  }
  return shown;
}

/** The view model of each item, made once for each item object. */
const itemViews = new WeakMap<TranscriptItem, Message | null>();

function viewOf(item: TranscriptItem): Message | null {
  if (!itemViews.has(item)) itemViews.set(item, itemToMessage(item));
  return itemViews.get(item) ?? null;
}

/**
 * The rows that a pane draws: the daemon's transcript, the local rows after
 * the items they follow, then the optimistic entries.
 */
export function renderTranscript(
  transcript: Transcript | null,
  state: TranscriptState,
): Message[] {
  const items = transcript?.items ?? [];
  const localAfter = new Map<string | null, Message[]>();
  const ids = new Set(items.map((item) => item.id));
  for (const entry of state.local) {
    // A row whose item is gone (a snapshot without it) goes to the end.
    const key = entry.after === null || ids.has(entry.after) ? entry.after : '\u0000end';
    localAfter.set(key, [...(localAfter.get(key) ?? []), entry.message]);
  }
  const rows: Message[] = [...(localAfter.get(null) ?? [])];
  for (const item of items) {
    const view = viewOf(item);
    if (view) rows.push(view);
    rows.push(...(localAfter.get(item.id) ?? []));
  }
  rows.push(...(localAfter.get('\u0000end') ?? []));
  for (const entry of unconfirmedOptimistic(state.optimistic, items)) {
    rows.push({
      id: entry.id,
      role: 'user',
      content: entry.content,
      timestamp: entry.timestamp,
      ...(entry.queued ? { queued: true } : {}),
    });
  }
  return rows;
}

/** Resolves when the session's stream first opened (at once, if it already has). */
export function transcriptOpened(sessionId: string): Promise<void> {
  const open = openWaiters.get(sessionId) ?? { opened: false, waiters: [] };
  if (open.opened) return Promise.resolve();
  return new Promise<void>((resolve) => {
    open.waiters.push(resolve);
  });
}

/** Re-issues the session's stream after a lost connection. */
export function retryTranscriptStream(sessionId: string): void {
  sessionEvents(sessionId).reconnect();
}

/** Writes a pane's own live state onto the session (gates, errors, mode). */
export function patchTranscript(sessionId: string, part: Partial<TranscriptState>): void {
  ensureState(sessionId);
  setTranscripts(sessionId, part);
}

/** Sets the session's streaming flag and reports it to the attention store. */
export function setTranscriptStreaming(sessionId: string, value: boolean): void {
  ensureState(sessionId);
  setTranscripts(sessionId, 'isStreaming', value);
  attentionActions.report(sessionId, {
    isStreaming: value,
    title: stateOf(sessionId).sessionTitle,
  });
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

function userTurnCount(sessionId: string): number {
  return (daemonTranscriptOf(sessionId)?.items ?? []).filter(
    (item) => item.type === 'user_turn' && item.origin == null,
  ).length;
}

/**
 * Shows a message that this browser sends now or queues. The answer is the
 * id of the entry.
 */
export function addOptimisticTurn(sessionId: string, content: string, queued = false): string {
  ensureState(sessionId);
  const id = generateMessageId();
  setTranscripts(sessionId, 'optimistic', (prev) => [
    ...prev,
    { id, content, timestamp: Date.now(), queued, userTurnsBefore: userTurnCount(sessionId) },
  ]);
  return id;
}

/** The send of the entry answered the turn id. The daemon's item replaces it. */
export function confirmOptimisticTurn(sessionId: string, id: string, messageId: string): void {
  // The last pane of the session can leave while the send is in flight.
  if (!transcripts[sessionId]) return;
  setTranscripts(sessionId, 'optimistic', (entry) => entry.id === id, 'messageId', messageId);
  const transcript = daemonTranscriptOf(sessionId);
  if (transcript) pruneOptimistic(sessionId, transcript);
}

/** Removes the entry: the send opened no turn. */
export function dropOptimisticTurn(sessionId: string, id: string): void {
  if (!transcripts[sessionId]) return;
  setTranscripts(sessionId, 'optimistic', (prev) => prev.filter((entry) => entry.id !== id));
}

/** Adds a row that only this browser draws, after the last transcript item. */
export function addLocalRow(sessionId: string, message: Omit<Message, 'id' | 'timestamp'>): void {
  if (!transcripts[sessionId]) return;
  const items = daemonTranscriptOf(sessionId)?.items ?? [];
  const after = items.length > 0 ? items[items.length - 1].id : null;
  setTranscripts(sessionId, 'local', (prev) => [
    ...prev,
    { message: { id: generateMessageId(), timestamp: Date.now(), ...message }, after },
  ]);
}

/**
 * The send of the entry failed. The text stays on screen as a local row,
 * next to the notice of the failure.
 */
export function failOptimisticTurn(sessionId: string, id: string, notice: string): void {
  if (!transcripts[sessionId]) return;
  const entry = stateOf(sessionId).optimistic.find((e) => e.id === id);
  dropOptimisticTurn(sessionId, id);
  if (entry) addLocalRow(sessionId, { role: 'user', content: entry.content });
  addLocalRow(sessionId, { role: 'system', content: notice });
}

/**
 * Parks a mid-turn prompt: the optimistic entry renders at the end of the
 * transcript, and the queue entry waits for the stream to go idle.
 *
 * `existingTempId` adopts the entry of a dispatch that the daemon refused as
 * concurrent, so a requeue never renders the prompt twice.
 */
export function queueTurn(
  sessionId: string,
  content: string,
  existingTempId?: string,
  comments?: CommentRef[],
): void {
  ensureState(sessionId);
  const tempId = existingTempId ?? addOptimisticTurn(sessionId, content, true);
  if (existingTempId !== undefined) setOptimisticQueued(sessionId, existingTempId, true);
  setTranscripts(sessionId, 'queuedTurns', (prev) => [
    ...prev,
    { tempId, content, ...(comments?.length ? { comments } : {}) },
  ]);
}

/** Marks the entry as queued, or as sent. */
export function setOptimisticQueued(sessionId: string, id: string, queued: boolean): void {
  if (!transcripts[sessionId]) return;
  setTranscripts(sessionId, 'optimistic', (entry) => entry.id === id, 'queued', queued);
}

/**
 * Takes the oldest queued turn, or nothing.
 *
 * The shift IS the claim: two panes of one session both run a flusher, and
 * only the one whose shift returned an entry may dispatch it.
 */
export function shiftQueuedTurn(sessionId: string): QueuedTurn | undefined {
  const queue = stateOf(sessionId).queuedTurns;
  if (queue.length === 0) return undefined;
  const [head] = queue;
  setTranscripts(sessionId, 'queuedTurns', (prev) => prev.slice(1));
  return head;
}

/** The test seam: forgets every session, so one case cannot answer the next. */
export function resetTranscriptsForTests(): void {
  for (const unsubscribe of unsubscribers.values()) unsubscribe();
  unsubscribers.clear();
  refcounts.clear();
  lastAppliedSeq.clear();
  openWaiters.clear();
  for (const sessionId of Object.keys(transcripts)) {
    forgetSession(sessionId);
  }
  daemonTranscripts.clear();
  syncStates.clear();
}

/** Removes one session's state. The typed setter takes no `undefined`, which
 *  is the only way solid's store deletes a key. */
function forgetSession(sessionId: string): void {
  setTranscripts(sessionId, undefined as unknown as TranscriptState);
  daemonTranscripts.delete(sessionId);
  syncStates.delete(sessionId);
}

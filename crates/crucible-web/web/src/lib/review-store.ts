/**
 * Session-scoped review state: the composed diff, its comments, and whether
 * the agent is currently parked waiting on it.
 *
 * Global rather than context-bound because its three consumers do not share a
 * provider. `ChangesPanel` renders in the RIGHT edge region, outside the
 * per-chat-tab `ChatProvider`; `FileViewerPanel` renders in the CENTER region,
 * outside it too; `ToolCard` renders inside it. A context would reach one of
 * the three. This follows `pendingDiffStore`'s shape for the same reason.
 *
 * Keyed by session id, not "the current session": several chat tabs can be
 * open at once and a delegated child ledger is addressed by the CHILD's
 * session id, so a single-slot store would cross their queues.
 *
 * Consumers call `useReviewSession(() => id)`. One binding serves every
 * consumer of a session, so the panel, the editor and every ToolCard on screen
 * share ONE fetch. The stream under it is `sessionEvents(id)`, the shared root
 * of `lib/query/sse.ts`, which the chat pane reads too: a session with a
 * transcript and a changes panel holds ONE `EventSource`, not two.
 */
import { createEffect, createSignal, on, onCleanup, type Accessor } from 'solid-js';
import { createStore, produce } from 'solid-js/store';
import { createSingletonRoot } from '@solid-primitives/rootless';
import { sessionEvents } from './query/sse';
import type { ChatEvent } from './types';
import type {
  BulkOutcome,
  DegradedRoot,
  IntegritySkip,
  NewComment,
  ReviewHunksResponse,
} from './review-api';
import {
  addReviewCommentOnce,
  invalidateReview,
  rebaseReviewOnce,
  resolveReviewCommentOnce,
  setHunkStateOnce,
  setHunkStatesOnce,
  undoRejectOnce,
  useReviewHunks,
} from './query/review';
import {
  hunkPath,
  isExternal,
  type ComposedHunk,
  type ReviewComment,
  type ReviewScope,
  type ReviewState,
} from './review-types';

/** What the daemon's `review_gate` event says about a held tool call. */
interface ReviewGate {
  blocked: boolean;
  tool: string;
  path: string | null;
}

export interface ReviewSessionState {
  /**
   * Which hunks `hunks` holds: the session's, or the current turn's.
   *
   * Per session and shared by every consumer of the slot, because the slot
   * IS the listing: the panel, the gutter and the transcript read one array,
   * and a second array per scope would be a second fetch and a second event
   * stream for the same session. Under `turn` a consumer that reasons from
   * absence (the transcript's "superseded") must stay silent.
   */
  scope: ReviewScope;
  hunks: ComposedHunk[];
  comments: ReviewComment[];
  /**
   * Roots the daemon can no longer account for, and the unscoped losses that
   * name no root at all.
   *
   * Kept because a degraded root contributes ZERO hunks while the gate holds
   * every write under it. Dropping them left the panel drawing "No changes in
   * this session yet" for exactly the state in which nothing can proceed, with
   * no reason shown and no release offered.
   */
  degraded: DegradedRoot[];
  skips: IntegritySkip[];
  /** A list has succeeded at least once. Distinguishes "no changes" from
   * "we do not know yet" — the ToolCard's superseded rule needs that. */
  loaded: boolean;
  loading: boolean;
  error: string | null;
  gate: ReviewGate | null;
}

const EMPTY: ReviewSessionState = {
  scope: 'session',
  hunks: [],
  comments: [],
  degraded: [],
  skips: [],
  loaded: false,
  loading: false,
  error: null,
  gate: null,
};

/**
 * Coalescing window for refreshes. The daemon fires several events per turn
 * that each imply "the composed diff may have moved"; re-listing on each would
 * be one round trip per tool call.
 */
export const REVIEW_REFRESH_DEBOUNCE_MS = 150;

const [sessions, setSessions] = createStore<Record<string, ReviewSessionState>>({});

/** The pending coalesced refresh of each bound session. */
const timers = new Map<string, ReturnType<typeof setTimeout>>();

function ensureSlot(id: string): void {
  if (!sessions[id]) setSessions(id, { ...EMPTY });
}

// =============================================================================
// Reads
// =============================================================================

export const reviewStore = {
  /** Reactive state for a session. Never undefined — an unbound session reads
   * as empty-and-unloaded, so consumers need no null branch. */
  session(id: string | undefined | null): ReviewSessionState {
    return (id && sessions[id]) || EMPTY;
  },

  /** The scope a session lists under. An unbound session is the session's. */
  scope(id: string | undefined | null): ReviewScope {
    return reviewStore.session(id).scope;
  },

  /** Hunks touching an absolute path, in composed-diff order. */
  hunksForPath(id: string | undefined | null, absPath: string): ComposedHunk[] {
    return reviewStore.session(id).hunks.filter((h) => hunkPath(h) === absPath);
  },

  /**
   * Every live hunk on an absolute path, across every session under review.
   *
   * The editor is asked for a PATH, not a session — a buffer in the center
   * region has no idea which chat tab's agent touched it, and with several
   * sessions sharing a workspace the honest answer is "all of them". The
   * session id rides along so an accept/reject from the gutter still names the
   * ledger that owns the hunk.
   */
  hunksForOpenPath(absPath: string): { sessionId: string; hunk: ComposedHunk }[] {
    const out: { sessionId: string; hunk: ComposedHunk }[] = [];
    for (const id of Object.keys(sessions)) {
      for (const hunk of sessions[id].hunks) {
        if (hunkPath(hunk) === absPath) out.push({ sessionId: id, hunk });
      }
    }
    return out;
  },

  /** Hunks a tool call contributed to that are STILL LIVE in the composed diff. */
  hunksForToolCall(id: string | undefined | null, callId: string): ComposedHunk[] {
    return reviewStore.session(id).hunks.filter((h) => h.tool_call_ids.includes(callId));
  },

  /**
   * The queue depth: unreviewed AND attributable.
   *
   * External hunks are excluded because they never block the agent — counting
   * the user's own concurrent edits as work owed would make the badge argue
   * for reviewing yourself.
   */
  unreviewedCount(id: string | undefined | null): number {
    return reviewStore.session(id).hunks.filter((h) => h.state === 'unreviewed' && !isExternal(h))
      .length;
  },

  /**
   * Whether the daemon's answer for this session is missing something, and
   * therefore whether an empty queue means "nothing to review" or "nothing can
   * be reviewed".
   *
   * `informational` skips are excluded: a lost comment costs no safety
   * property and is not worth a banner over.
   */
  isDegraded(id: string | undefined | null): boolean {
    const state = reviewStore.session(id);
    return state.degraded.length > 0 || state.skips.some((s) => s.record.kind !== 'informational');
  },
};

// =============================================================================
// Attribution labels
// =============================================================================

/**
 * `tool_call_id` → the tool's name, contributed by whichever `ToolCard`
 * rendered that call.
 *
 * A composed hunk carries only ids. The name lives on the transcript, inside a
 * per-tab `ChatProvider` the gutter and the panel cannot reach, so the
 * transcript pushes it here instead.
 *
 * Deliberately NOT "turn 7 · Edit": `Interval.node_id` — the turn coordinate —
 * does not cross the wire on `ComposedHunk`, and a number inferred from the
 * transcript's ordering would be a guess presented as attribution. The label
 * says only what is known.
 */
const [toolNames, setToolNames] = createStore<Record<string, string>>({});

/** Short, stable stand-in for a call whose card is not on screen. */
function shortCallId(callId: string): string {
  return callId.length > 10 ? `${callId.slice(0, 8)}…` : callId;
}

export function indexToolCall(callId: string, name: string): void {
  if (toolNames[callId] !== name) setToolNames(callId, name);
}

export function toolCallLabel(callId: string): string {
  return toolNames[callId] ?? shortCallId(callId);
}

// =============================================================================
// Cross-surface navigation
// =============================================================================

/**
 * A hunk the user asked to see in the open buffer. `FileViewerPanel` consumes
 * and clears it.
 *
 * Not tab metadata: `Pane` reads a tab's metadata untracked and only re-renders
 * a panel when the ACTIVE TAB ID changes, so mutating metadata on an
 * already-open file scrolls nothing.
 */
const [pendingReveal, setPendingReveal] = createSignal<{
  path: string;
  line: number;
} | null>(null);
export { pendingReveal };

/** The tool call the gutter chip pointed at. Set, then scrolled to by id. */
const [revealedToolCall, setRevealedToolCall] = createSignal<string | null>(null);
export { revealedToolCall };

// =============================================================================
// Writes / lifecycle
// =============================================================================

/**
 * Gate state carried by a listing, which is how it survives a reload.
 *
 * `review_gate` is an event, and events are dropped rather than replayed
 * across a reconnect — so a tab opened (or refreshed) while a turn is already
 * parked would show no "waiting on review" chip and the agent would read as
 * hung. The listing carries the daemon's live block, so a reload restores it.
 *
 * Three cases, and the third is why this is not a one-liner. An object means
 * blocked. An explicit `null` means nothing is parked, and must CLEAR a stale
 * chip. A missing key means the daemon does not report gate state at all, and
 * must leave whatever the event stream established alone — overwriting it with
 * `null` would erase a live block on every refresh.
 */
function gateFromList(data: ReviewHunksResponse, current: ReviewGate | null): ReviewGate | null {
  if (!('gate' in data)) return current;
  return data.gate ? { blocked: true, tool: data.gate.tool, path: data.gate.path } : null;
}

/**
 * Folds one answer from the daemon into a session's slot.
 *
 * The listing itself is held in `lib/query/review.ts`, under the session; this
 * is the slot the three surfaces read, which carries the optimistic marks the
 * listing corrects.
 */
function adoptListing(id: string, data: ReviewHunksResponse): void {
  // The session may have been released while the request was in flight;
  // writing then would resurrect a slot nobody reads.
  if (!sessions[id]) return;
  // The daemon echoes the scope it answered. A listing for a scope the user
  // has since left is not this slot's diff any more; the switch issued its own
  // refetch, and that one lands with the right word.
  if (data.scope && data.scope !== sessions[id].scope) return;
  setSessions(id, (s) => ({
    ...s,
    hunks: data.hunks,
    comments: data.comments,
    // Defaulted rather than left alone: a daemon that reports neither key has
    // nothing degraded to report, and carrying a stale banner forward would
    // outlive the failure it described.
    degraded: data.degraded ?? [],
    skips: data.integrity?.skips ?? [],
    gate: gateFromList(data, s.gate),
    loaded: true,
    loading: false,
    error: null,
  }));
}

/** Folds one refusal into the slot. */
function adoptFailure(id: string, error: Error): void {
  if (!sessions[id]) return;
  // `loaded` is deliberately NOT set: a failed list means we still do not know
  // what the composed diff holds, and the superseded rule must stay silent
  // rather than claim every edit was thrown away.
  setSessions(id, (s) => ({ ...s, loading: false, error: error.message }));
}

export const reviewActions = {
  /**
   * Ask the daemon for this session's hunks and comments again.
   *
   * The listing is a cache entry now, so this INVALIDATES it rather than
   * fetching: the bound session is observing that entry, so the invalidation
   * is what fetches, and two of these inside one round trip are one request.
   */
  async refresh(id: string): Promise<void> {
    ensureSlot(id);
    if (sessions[id]) setSessions(id, 'loading', true);
    await invalidateReview(id);
  },

  async setState(id: string, hunkId: string, state: ReviewState): Promise<void> {
    // Optimistic: the round trip is a disk write plus a git diff, and a
    // checkmark that lags a second reads as a dropped click. A refresh follows
    // either way, so a rejected optimistic state corrects itself.
    setSessions(
      id,
      produce((s) => {
        const h = s.hunks.find((x) => x.id === hunkId);
        if (h) h.state = state;
      }),
    );
    await setHunkStateOnce(id, hunkId, state);
  },

  /**
   * List under another scope. The daemon decides what the turn holds; this
   * only re-asks with the other word. The same scope again is not a round
   * trip.
   */
  async setScope(id: string, scope: ReviewScope): Promise<void> {
    ensureSlot(id);
    if (sessions[id].scope === scope) return;
    setSessions(id, 'scope', scope);
    await reviewActions.refresh(id);
  },

  /** Reject == revert on disk == tell the agent. One operation, one name. */
  reject(id: string, hunkId: string): Promise<void> {
    return reviewActions.setState(id, hunkId, 'rejected');
  },

  /**
   * One decision over several hunks: ONE daemon call, the ids in the order
   * given.
   *
   * Optimistic like `setState`, for the same reason, and corrected by the
   * same refresh. The outcome is returned rather than swallowed: `failed`
   * names the hunks the daemon refused, and only the caller can show them.
   */
  async setStates(id: string, hunkIds: string[], state: ReviewState): Promise<BulkOutcome> {
    const named = new Set(hunkIds);
    setSessions(
      id,
      produce((s) => {
        for (const h of s.hunks) if (named.has(h.id)) h.state = state;
      }),
    );
    return setHunkStatesOnce(id, hunkIds, state);
  },

  /** A bulk reject: every id reverted on disk, one note to the agent. */
  rejectMany(id: string, hunkIds: string[]): Promise<BulkOutcome> {
    return reviewActions.setStates(id, hunkIds, 'rejected');
  },

  /**
   * Take back the most recent reject, single or bulk, as one action.
   *
   * Not optimistic: the browser does not know which batch is on top of the
   * daemon's stack, so there is nothing honest to mark before the answer.
   * A `failed` list means the batch is still on the stack for the next try.
   */
  async undoReject(id: string): Promise<BulkOutcome> {
    return undoRejectOnce(id);
  },

  /**
   * Accept the worktree as the new base, releasing a block reviewing cannot.
   *
   * Deliberately NOT optimistic and NOT silent about failure: it throws so the
   * caller can surface why. A rebase that half-succeeds leaves some roots
   * degraded, and the refresh below is what shows which.
   */
  async rebase(id: string): Promise<void> {
    await rebaseReviewOnce(id);
  },

  async comment(id: string, comment: NewComment): Promise<void> {
    await addReviewCommentOnce(id, comment);
  },

  async resolveComment(id: string, commentId: string): Promise<void> {
    await resolveReviewCommentOnce(id, commentId);
  },

  /** Ask the open editor for this path to scroll to a hunk. */
  reveal(path: string, line: number): void {
    setPendingReveal({ path, line });
  },
  clearReveal(): void {
    setPendingReveal(null);
  },

  /**
   * Scroll the chat transcript to the ToolCard that made a change.
   *
   * Done by DOM id rather than through the chat store: the gutter chip lives
   * in the center region and the transcript lives inside a per-tab
   * `ChatProvider` this code cannot reach. `ToolCard` stamps
   * `data-tool-call-id`, which is the whole contract.
   */
  revealToolCall(callId: string): void {
    setRevealedToolCall(callId);
    const el = document.querySelector(`[data-tool-call-id="${CSS.escape(callId)}"]`);
    el?.scrollIntoView({ block: 'center' });
  },
};

// =============================================================================
// Subscription
// =============================================================================

/** Coalesces a burst of "the diff may have moved" events into one listing. */
function scheduleRefresh(id: string): void {
  const pending = timers.get(id);
  if (pending) clearTimeout(pending);
  timers.set(
    id,
    setTimeout(() => {
      timers.delete(id);
      void reviewActions.refresh(id);
    }, REVIEW_REFRESH_DEBOUNCE_MS),
  );
}

/**
 * Folds one chat event of a session into its review slot.
 *
 * The stream carries every event of the session, and the transcript reads the
 * same frames for its own fold. These four are the ones that move the composed
 * diff; the rest belong to the pane.
 */
function foldEvent(id: string, event: ChatEvent): void {
  switch (event.type) {
    case 'session_event': {
      const data = (event.data ?? {}) as Record<string, unknown>;
      if (event.event === 'review_gate') {
        setSessions(id, 'gate', {
          blocked: data.blocked === true,
          tool: typeof data.tool === 'string' ? data.tool : '',
          path: typeof data.path === 'string' ? data.path : null,
        });
        return;
      }
      // `review_gate` and `review_changed` both make the listing wrong, and
      // the ROUTE of this stream (`lib/query/routes/session.ts`) invalidates
      // it for every reader at once. Re-listing here as well would be a second
      // request for one event. The gate above is different: it is state only
      // this slot holds, and no cache entry carries it.
      return;
    }
    // No event announces "the agent just created hunks" — `review_changed`
    // only fires for review ACTIONS. Without these two the queue would stay
    // empty until the user clicked something, which is precisely backwards.
    case 'tool_result':
    case 'message_complete':
      scheduleRefresh(id);
      return;
    case 'connection':
      // Events are dropped, not replayed, across a reconnect.
      if (event.status === 'connected') scheduleRefresh(id);
      return;
  }
}

/** The one binding of a session, and the way to drop it whatever the count. */
interface SessionBinding {
  /**
   * Counts this consumer in, and builds the binding for the first of them.
   * It registers the count-out with the caller's reactive owner, so a consumer
   * that goes away needs no bookkeeping of its own.
   */
  enter: () => void;
  /** Drops the binding whatever the count says. Only the test seam runs it. */
  dispose: () => void;
}

const bindings = new Map<string, SessionBinding>();

/**
 * The binding of a session: its slot, its first listing and its place on the
 * shared stream.
 *
 * `createSingletonRoot` counts the consumers. The store held its own `refs`
 * count before, over its own `EventSource`; both are gone. The count that
 * closes the stream now lives in `lib/query/sse.ts`, where the chat pane is
 * counted beside the panel, and the count here decides one thing the stream
 * cannot: when the SLOT goes, since a slot per session ever visited is a leak.
 *
 * The root is detached (`createSingletonRoot(factory, null)`). Without that it
 * would belong to the owner of whichever consumer asked first, and that
 * component going away would delete the slot under every other consumer.
 */
function sessionBinding(id: string): SessionBinding {
  const existing = bindings.get(id);
  if (existing) return existing;

  const binding: SessionBinding = { enter: () => {}, dispose: () => {} };
  binding.enter = createSingletonRoot((dispose) => {
    binding.dispose = dispose;
    ensureSlot(id);
    const unsubscribe = sessionEvents(id).subscribe((event) => foldEvent(id, event));

    // The listing itself. It is observed HERE, inside the binding's own root,
    // rather than in any of the three surfaces: they do not share an owner,
    // and the one that mounts first would then own the fetch for the other
    // two. An observer is also what makes the stream's invalidation fetch —
    // an entry nobody is reading is marked wrong and left alone.
    const listing = useReviewHunks(
      () => id,
      () => sessions[id]?.scope ?? 'session',
    );

    createEffect(() => {
      if (listing.data) adoptListing(id, listing.data);
    });
    createEffect(() => {
      if (listing.error) adoptFailure(id, listing.error);
    });
    createEffect(() => {
      if (sessions[id]) setSessions(id, 'loading', listing.isFetching);
    });

    onCleanup(() => {
      // Only when this binding is still the one on file: the test seam may
      // have dropped it already and a later consumer built its successor.
      if (bindings.get(id) === binding) bindings.delete(id);
      unsubscribe();
      const pending = timers.get(id);
      if (pending) clearTimeout(pending);
      timers.delete(id);
      // Drop the key rather than leaving an empty record behind — a store path
      // set to undefined keeps the key, so every session visited would leak a
      // slot.
      setSessions(
        produce((state) => {
          delete state[id];
        }),
      );
    });
  }, null);

  bindings.set(id, binding);
  return binding;
}

/**
 * Bind a component's lifetime to a session's review state.
 *
 * Enters the binding of the id it is given and leaves the previous one, so
 * following the active session across tabs never leaks a subscription.
 */
export function useReviewSession(sessionId: Accessor<string | undefined | null>): void {
  createEffect(
    on(sessionId, (id) => {
      // `enter` registers its own cleanup on this effect, which runs before
      // the next id and when the component goes.
      if (id) sessionBinding(id).enter();
    }),
  );
}

/** Test seam: drop every binding and slot. */
export function __resetReviewStore(): void {
  for (const binding of [...bindings.values()]) binding.dispose();
  bindings.clear();
  for (const pending of timers.values()) clearTimeout(pending);
  timers.clear();
  setSessions(produce((s) => Object.keys(s).forEach((k) => delete s[k])));
  setToolNames(produce((s) => Object.keys(s).forEach((k) => delete s[k])));
  setPendingReveal(null);
  setRevealedToolCall(null);
}

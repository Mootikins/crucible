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
 * Consumers call `useReviewSession(() => id)`. The subscription is refcounted,
 * so the panel, the editor, and every ToolCard on screen share ONE fetch and
 * ONE EventSource per session.
 */
import { createEffect, createSignal, on, onCleanup, type Accessor } from 'solid-js';
import { createStore, produce } from 'solid-js/store';
import { subscribeToEvents } from './api';
import {
  addReviewComment,
  listReviewHunks,
  rebaseReview,
  resolveReviewComment,
  setHunkState,
  type DegradedRoot,
  type IntegritySkip,
  type NewComment,
  type ReviewHunksResponse,
} from './review-api';
import {
  hunkPath,
  isExternal,
  type ComposedHunk,
  type ReviewComment,
  type ReviewState,
} from './review-types';

/** What the daemon's `review_gate` event says about a held tool call. */
export interface ReviewGate {
  blocked: boolean;
  tool: string;
  path: string | null;
}

export interface ReviewSessionState {
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

interface Subscription {
  refs: number;
  close: () => void;
  timer: ReturnType<typeof setTimeout> | null;
}
const subs = new Map<string, Subscription>();

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

export const reviewActions = {
  /** Replace a session's hunks and comments from the daemon. */
  async refresh(id: string): Promise<void> {
    ensureSlot(id);
    setSessions(id, 'loading', true);
    try {
      const data = await listReviewHunks(id);
      // The session may have been released while the request was in flight;
      // writing then would resurrect a slot nobody reads.
      if (!sessions[id]) return;
      setSessions(id, (s) => ({
        ...s,
        hunks: data.hunks,
        comments: data.comments,
        // Defaulted rather than left alone: a daemon that reports neither key
        // has nothing degraded to report, and carrying a stale banner forward
        // would outlive the failure it described.
        degraded: data.degraded ?? [],
        skips: data.integrity?.skips ?? [],
        gate: gateFromList(data, s.gate),
        loaded: true,
        loading: false,
        error: null,
      }));
    } catch (e) {
      if (!sessions[id]) return;
      // `loaded` is deliberately NOT set: a failed list means we still do not
      // know what the composed diff holds, and the superseded rule must stay
      // silent rather than claim every edit was thrown away.
      setSessions(id, (s) => ({
        ...s,
        loading: false,
        error: (e as Error).message,
      }));
    }
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
    try {
      await setHunkState(id, hunkId, state);
    } finally {
      await reviewActions.refresh(id);
    }
  },

  /** Reject == revert on disk == tell the agent. One operation, one name. */
  reject(id: string, hunkId: string): Promise<void> {
    return reviewActions.setState(id, hunkId, 'rejected');
  },

  /**
   * Accept the worktree as the new base, releasing a block reviewing cannot.
   *
   * Deliberately NOT optimistic and NOT silent about failure: it throws so the
   * caller can surface why. A rebase that half-succeeds leaves some roots
   * degraded, and the refresh below is what shows which.
   */
  async rebase(id: string): Promise<void> {
    try {
      await rebaseReview(id);
    } finally {
      await reviewActions.refresh(id);
    }
  },

  async comment(id: string, comment: NewComment): Promise<void> {
    await addReviewComment(id, comment);
    await reviewActions.refresh(id);
  },

  async resolveComment(id: string, commentId: string): Promise<void> {
    await resolveReviewComment(id, commentId);
    await reviewActions.refresh(id);
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

function scheduleRefresh(id: string): void {
  const sub = subs.get(id);
  if (!sub) return;
  if (sub.timer) clearTimeout(sub.timer);
  sub.timer = setTimeout(() => {
    sub.timer = null;
    void reviewActions.refresh(id);
  }, REVIEW_REFRESH_DEBOUNCE_MS);
}

function retain(id: string): () => void {
  const existing = subs.get(id);
  if (existing) {
    existing.refs += 1;
    return () => release(id);
  }

  ensureSlot(id);
  const close = subscribeToEvents(id, (event) => {
    switch (event.type) {
      case 'session_event': {
        const data = (event.data ?? {}) as Record<string, unknown>;
        if (event.event_type === 'review_gate') {
          setSessions(id, 'gate', {
            blocked: data.blocked === true,
            tool: typeof data.tool === 'string' ? data.tool : '',
            path: typeof data.path === 'string' ? data.path : null,
          });
          // A block means there is something to review RIGHT NOW; do not make
          // the user wait out the debounce to find out what.
          void reviewActions.refresh(id);
          return;
        }
        if (event.event_type === 'review_changed') scheduleRefresh(id);
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
  });

  subs.set(id, { refs: 1, close, timer: null });
  void reviewActions.refresh(id);
  return () => release(id);
}

function release(id: string): void {
  const sub = subs.get(id);
  if (!sub) return;
  sub.refs -= 1;
  if (sub.refs > 0) return;
  if (sub.timer) clearTimeout(sub.timer);
  sub.close();
  subs.delete(id);
  // Drop the key rather than leaving an empty record behind — a store path set
  // to undefined keeps the key, so every session visited would leak a slot.
  setSessions(
    produce((s) => {
      delete s[id];
    }),
  );
}

/**
 * Bind a component's lifetime to a session's review state.
 *
 * Retains on the id it is given and releases the previous one, so following
 * the active session across tabs never leaks a subscription.
 */
export function useReviewSession(sessionId: Accessor<string | undefined | null>): void {
  createEffect(
    on(sessionId, (id) => {
      if (!id) return;
      const dispose = retain(id);
      onCleanup(dispose);
    }),
  );
}

/** Test seam: drop every subscription and slot. */
export function __resetReviewStore(): void {
  for (const [id, sub] of subs) {
    if (sub.timer) clearTimeout(sub.timer);
    sub.close();
    subs.delete(id);
  }
  setSessions(produce((s) => Object.keys(s).forEach((k) => delete s[k])));
  setToolNames(produce((s) => Object.keys(s).forEach((k) => delete s[k])));
  setPendingReveal(null);
  setRevealedToolCall(null);
}

/**
 * Session-scoped review state: the files of the session record and its
 * comments.
 *
 * Global rather than context-bound because its consumer, `ChangesPanel`,
 * renders in the RIGHT edge region, outside the per-chat-tab `ChatProvider`.
 *
 * Keyed by session id, not "the current session": several chat tabs can be
 * open at once and a delegated child record is addressed by the CHILD's
 * session id, so a single-slot store would cross their records.
 *
 * Consumers call `useReviewSession(() => id)`. One binding serves every
 * consumer of a session, so every surface on screen shares ONE fetch. The
 * stream under it is `sessionEvents(id)`, the shared root of
 * `lib/query/sse.ts`, which the chat pane reads too: a session with a
 * transcript and a changes panel holds ONE `EventSource`, not two.
 *
 * The listing is the session record diffset (`GET /api/diff?session=`) and
 * its comments (`GET /api/diff/comments?session=`). The daemon no longer
 * lists hunks, and no hunk has a decision.
 */
import { createEffect, createSignal, on, onCleanup, type Accessor } from 'solid-js';
import { createStore, produce } from 'solid-js/store';
import { createSingletonRoot } from '@solid-primitives/rootless';
import { sessionEvents } from './query/sse';
import type { ChatEvent } from './types';
import type { DiffFileEntry } from './diffset';
import {
  invalidateReview,
  resolveReviewCommentOnce,
  useSessionRecord,
  type SessionRecordListing,
} from './query/review';
import type { ReviewComment } from './review-types';

export interface ReviewSessionState {
  /** The files that differ between the session base and the disk. */
  files: DiffFileEntry[];
  comments: ReviewComment[];
  /** A list has succeeded at least once. Distinguishes "no changes" from
   * "we do not know yet". */
  loaded: boolean;
  loading: boolean;
  error: string | null;
}

const EMPTY: ReviewSessionState = {
  files: [],
  comments: [],
  loaded: false,
  loading: false,
  error: null,
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
};

// =============================================================================
// Cross-surface navigation
// =============================================================================

/**
 * A line the user asked to see in the open buffer. `FileViewerPanel` consumes
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

// =============================================================================
// Writes / lifecycle
// =============================================================================

/**
 * Folds one answer from the daemon into a session's slot.
 *
 * The listing itself is held in `lib/query/review.ts`, under the session; this
 * is the slot the surfaces read.
 */
function adoptListing(id: string, data: SessionRecordListing): void {
  // The session may have been released while the request was in flight;
  // writing then would resurrect a slot nobody reads.
  if (!sessions[id]) return;
  setSessions(id, (s) => ({
    ...s,
    files: data.files,
    comments: data.comments,
    loaded: true,
    loading: false,
    error: null,
  }));
}

/** Folds one refusal into the slot. */
function adoptFailure(id: string, error: Error): void {
  if (!sessions[id]) return;
  // `loaded` is deliberately NOT set: a failed list means we still do not know
  // what the session record holds, and "no changes" would be a false claim.
  setSessions(id, (s) => ({ ...s, loading: false, error: error.message }));
}

export const reviewActions = {
  /**
   * Ask the daemon for this session's record and comments again.
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

  async resolveComment(id: string, commentId: string): Promise<void> {
    await resolveReviewCommentOnce(id, commentId);
  },

  /** Ask the open editor for this path to scroll to a line. */
  reveal(path: string, line: number): void {
    setPendingReveal({ path, line });
  },
  clearReveal(): void {
    setPendingReveal(null);
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
 * same frames for its own fold. These four are the ones that move the session
 * record; the rest belong to the pane.
 */
function foldEvent(id: string, event: ChatEvent): void {
  switch (event.type) {
    case 'session_event': {
      // `review_changed` makes the listing wrong, and the ROUTE of this
      // stream (`lib/query/routes/session.ts`) invalidates it for every reader
      // at once. Re-listing here as well would be a second request for one
      // event.
      return;
    }
    // No event announces "the agent just changed a file" — `review_changed`
    // only fires for review ACTIONS. Without these two the record would stay
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
    // rather than in any surface: surfaces do not share an owner, and the one
    // that mounts first would then own the fetch for the others. An observer is also what makes the stream's invalidation fetch —
    // an entry nobody is reading is marked wrong and left alone.
    const listing = useSessionRecord(() => id);

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
  setPendingReveal(null);
}

/**
 * The route of the chat stream: one event, one cache write.
 *
 * Two folds read the same stream, and they are not the same fold.
 * `contexts/chatEventReducer.ts` keeps the transcript ONE pane draws: the
 * streaming bubble, the tool rows, the thinking block. This module keeps what
 * EVERY pane reads: the cached history, the session list, the mode list, the
 * pending interactions and the review. A token belongs to the first and never
 * to the second, which is why most of the 22 event types write nothing here.
 *
 * The plan's Part D "Event to key mapping" table is the contract. Three rows
 * of it are read wider than they are written, and each says so at its case:
 * `error`, `mode_changed` and `review_changed`.
 *
 * The route runs once per event per session, inside the shared root of
 * `lib/query/sse.ts`, so two panes on one session invalidate one key once.
 */
import type { QueryClient } from '@tanstack/solid-query';
import type { ChatEvent } from '@/lib/types';
import type { SessionHistoryResponse } from '@/lib/types';
import { keys } from '../keys';
import { setSessionEventRoute, type SessionRouteContext } from '../sse';

/**
 * How long the route waits before it re-lists a review.
 *
 * The daemon fires several events per turn that each mean "the composed diff
 * may have moved" — one per tool call, and one per review action. Re-listing
 * on each is one round trip per edit for a diff that settles once.
 */
export const REVIEW_INVALIDATE_DEBOUNCE_MS = 150;

/** The pending coalesced re-list of each session, by id. */
const reviewTimers = new Map<string, ReturnType<typeof setTimeout>>();

/** Coalesces a burst of "the diff moved" events into one listing. */
function scheduleReviewInvalidation(client: QueryClient, sessionId: string): void {
  const pending = reviewTimers.get(sessionId);
  if (pending) clearTimeout(pending);
  reviewTimers.set(
    sessionId,
    setTimeout(() => {
      reviewTimers.delete(sessionId);
      void client.invalidateQueries({ queryKey: keys.review(sessionId) });
    }, REVIEW_INVALIDATE_DEBOUNCE_MS),
  );
}

/** Test seam: drop every pending re-list, so one case cannot reach the next. */
export function resetReviewInvalidationForTests(): void {
  for (const pending of reviewTimers.values()) clearTimeout(pending);
  reviewTimers.clear();
}

/** One persisted event of the history document, as the daemon records it. */
type HistoryEvent = SessionHistoryResponse['history'][number];

/** The payload of `session_event`, which carries no type of its own. */
type SessionEventData = { message_id?: string; content?: string } | null;

/** One recorded event's payload. `data` is `unknown` on the wire, so a read
 * of it narrows here rather than trusting a field. */
const payloadOf = (event: HistoryEvent): { message_id?: string } =>
  (event.data ?? {}) as { message_id?: string };

/**
 * Adds the echoed user message to the cached history.
 *
 * The daemon echoes the turn over the stream with the id the send answered, so
 * a pane that is reading the history sees its own message without a refetch.
 * It is a patch and not an invalidation because the turn is still running: a
 * refetch here would race the tokens that follow it.
 */
function appendUserMessage(client: QueryClient, sessionId: string, data: SessionEventData): void {
  const messageId = data?.message_id;
  const content = data?.content;
  if (!messageId || content === undefined) return;

  client.setQueryData<SessionHistoryResponse>(keys.sessionHistory(sessionId), (held) => {
    // Nothing read the history, so there is nothing to keep current. Minting a
    // document from one event would answer the next reader a transcript of one
    // message and call it whole.
    if (!held) return held;
    if (held.history.some((event) => payloadOf(event).message_id === messageId)) return held;

    const echoed: HistoryEvent = {
      type: 'event',
      session_id: sessionId,
      event: 'user_message',
      data: { message_id: messageId, content },
      timestamp: new Date().toISOString(),
    };
    return {
      ...held,
      history: [...held.history, echoed],
      total_events: held.total_events + 1,
    };
  });
}

/** Routes the events the daemon forwards under one `session_event` type. */
function routeSessionSubEvent(
  event: Extract<ChatEvent, { type: 'session_event' }>,
  { client, sessionId }: SessionRouteContext,
): void {
  switch (event.event) {
    case 'user_message':
      appendUserMessage(client, sessionId, event.data as SessionEventData);
      break;

    // The table debounces these two, and the debounce is load-bearing. An
    // invalidation does NOT fold concurrent refetches of one key into one
    // request: it CANCELS the one in flight and starts another, so a turn that
    // fires five of these is five listings of a diff that settles once. The
    // timer is per session and deletes itself, and a listing it starts after
    // the last reader has gone reaches a key nobody holds.
    case 'review_gate':
    case 'review_changed':
      scheduleReviewInvalidation(client, sessionId);
      break;

    // `stream_gap` tells the user their transcript has a hole, which the pane
    // reducer surfaces. No cached key is wrong because of it.
    default:
      break;
  }
}

/** Turns one chat event into the cache writes it owes every pane. */
function routeSessionEvent(event: ChatEvent, context: SessionRouteContext): void {
  const { client, bus, sessionId } = context;

  switch (event.type) {
    // A turn ended, whichever way. The daemon persisted it, and the cached
    // document predates it.
    //
    // The table qualifies the error row with "on critical error". The route
    // cannot tell a critical error from a recoverable one: `code` is the
    // daemon's own string and no list of it exists here. It invalidates for
    // every error instead, which costs one refetch of a document the failed
    // turn changed anyway.
    case 'message_complete':
    case 'error':
      void client.invalidateQueries({ queryKey: keys.sessionHistory(sessionId) });
      break;

    case 'interaction_requested':
      void client.invalidateQueries({ queryKey: keys.pendingInteractions() });
      break;

    // The table qualifies this row with "on unknown mode", which is a question
    // about one pane's list, not about the cache. The refetch also carries
    // `current_mode_id`, which the event just moved, so an unconditional
    // invalidation keeps a cached list right in both ways a mode change can
    // make it wrong.
    case 'mode_changed':
      void client.invalidateQueries({ queryKey: keys.sessionModes(sessionId) });
      break;

    // Both lists, because the archived flag is part of the key and a rename
    // reaches the row under either flag.
    case 'title_changed':
      // Named one at a time: the flag sits in an OBJECT inside the key, so a
      // prefix match on `['sessions']` would not reach either variant.
      void client.invalidateQueries({ queryKey: keys.sessions(false) });
      void client.invalidateQueries({ queryKey: keys.sessions(true) });
      bus.emit('sessionTitleChanged', { sessionId, title: event.title });
      break;

    case 'session_event':
      routeSessionSubEvent(event, context);
      break;

    // The rest are the stream itself (`token`, `thinking`, the tool events,
    // `segment_complete`), the transport (`connection`) or per-pane state (the
    // subagent and delegation events, `context_usage`, `precognition_result`).
    // One pane's reducer owns each of them.
    default:
      break;
  }
}

/**
 * Names this module the route of the chat stream.
 *
 * The app calls it once, at start (`src/index.tsx`), because a route installed
 * by the first consumer to import a module would leave the cache stale for
 * whichever pane opened before that import ran. A test calls it too, after
 * `resetSseForTests` forgets it.
 */
export function installSessionEventRoute(): void {
  setSessionEventRoute(routeSessionEvent);
}

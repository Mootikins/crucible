/**
 * The route of the chat stream: one event, one cache write.
 *
 * The transcript is not a cache key. `contexts/transcriptStore.ts` applies
 * the ops of the `transcript` frames, and reads the history document again
 * when it needs a new snapshot. This module keeps the other things EVERY pane
 * reads: the session list, the mode list, the pending interactions and the
 * review. Most event types therefore write nothing here.
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
import { keys } from '../keys';
import { diffsetKey } from '@/lib/diffset';
import { setEventRoute, type SessionRouteContext } from '../sse';

/**
 * How long the route waits before it re-lists a session record.
 *
 * The daemon fires several events per turn that each mean "the record may
 * have moved" — one per tool call, and one per review action. Re-listing on
 * each is one round trip per edit for a diff that settles once.
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
      // The files, their texts and the comments sit under this one key.
      const key = diffsetKey({ kind: 'session_record', session: sessionId });
      void client.invalidateQueries({ queryKey: keys.diffset(key) });
    }, REVIEW_INVALIDATE_DEBOUNCE_MS),
  );
}

/** Test seam: drop every pending re-list, so one case cannot reach the next. */
export function resetReviewInvalidationForTests(): void {
  for (const pending of reviewTimers.values()) clearTimeout(pending);
  reviewTimers.clear();
}

/** Routes the events the daemon forwards under one `session_event` type. */
function routeSessionSubEvent(
  event: Extract<ChatEvent, { type: 'session_event' }>,
  { client, bus, sessionId }: SessionRouteContext,
): void {
  switch (event.event) {
    // A prompt ended: a client answered it, or it ended with no answer (a
    // cancelled turn). The pane drops its card, and the Inbox refetches.
    case 'interaction_completed': {
      const requestId = (event.data as { request_id?: unknown } | null)?.request_id;
      if (typeof requestId === 'string') bus.emit('interactionResolved', { sessionId, requestId });
      void client.invalidateQueries({ queryKey: keys.pendingInteractions() });
      break;
    }

    case 'plugin_turn_limit_changed':
      void client.invalidateQueries({ queryKey: keys.sessionPluginTurnLimit(sessionId) });
      break;
    case 'plugin_approval_changed':
      void client.invalidateQueries({ queryKey: keys.sessionPluginApprovals(sessionId) });
      break;

    // The table debounces this one, and the debounce is load-bearing. An
    // invalidation does NOT fold concurrent refetches of one key into one
    // request: it CANCELS the one in flight and starts another, so a turn that
    // fires five of these is five listings of a diff that settles once. The
    // timer is per session and deletes itself, and a listing it starts after
    // the last reader has gone reaches a key nobody holds.
    case 'review_changed':
      scheduleReviewInvalidation(client, sessionId);
      break;
    case 'status_items_changed':
      void client.invalidateQueries({ queryKey: keys.sessionStatus(sessionId) });
      break;

    // `stream_gap` makes the transcript store read the snapshot again. No
    // cached key is wrong because of it.
    default:
      break;
  }
}

/** Turns one chat event into the cache writes it owes every pane. */
function routeSessionEvent(event: ChatEvent, context: SessionRouteContext): void {
  const { client, bus, sessionId } = context;

  switch (event.type) {
    // A turn ended. Its edits can move the session record.
    case 'message_complete':
      scheduleReviewInvalidation(client, sessionId);
      break;

    // No event says "the agent changed a file": `review_changed` fires for
    // review actions only. A tool result can move the session record, and a
    // reconnect can hide the events that did.
    case 'tool_result':
      scheduleReviewInvalidation(client, sessionId);
      break;
    case 'connection':
      if (event.status === 'connected') scheduleReviewInvalidation(client, sessionId);
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

    case 'commands_changed':
      void client.invalidateQueries({ queryKey: keys.slashCommands(sessionId) });
      break;

    case 'session_event':
      routeSessionSubEvent(event, context);
      break;

    // The rest are the transcript (the `transcript` frame, and the events
    // whose ops it carries) or the session state that
    // `contexts/chatEventReducer.ts` keeps.
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
  setEventRoute('session', routeSessionEvent);
}

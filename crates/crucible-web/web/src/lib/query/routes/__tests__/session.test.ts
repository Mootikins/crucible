import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import type { QueryKey } from '@tanstack/solid-query';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { FakeEventSource, installFakeEventSource, onlyEventSource } from '@/test-utils/sse';
import { getBus } from '@/lib/bus';
import type { SessionHistoryResponse } from '@/lib/types';
import { keys } from '../../keys';
import { sessionEvents } from '../../sse';
import {
  installSessionEventRoute,
  resetReviewInvalidationForTests,
  REVIEW_INVALIDATE_DEBOUNCE_MS,
} from '../session';

const SESSION = 's1';

let env: TestQueryEnv;
let invalidated: QueryKey[];
let stop: (() => void) | null = null;

/**
 * Opens the session stream and answers the source the route reads.
 *
 * The route runs inside the stream, not beside it, so every case drives it the
 * way the daemon does: one frame on the wire, `{event, data}` — the SSE
 * `event:` name and the payload's own tag are always the same string.
 */
function openStream() {
  stop = sessionEvents(SESSION).subscribe(() => {});
  const source = onlyEventSource();
  // Production frames carry a `topic` field (Simplification Plan step 19);
  // this stream only ever joins its own session's topic, so the tests below
  // can still write the plain payload each frame carried before the shared
  // connection existed.
  return {
    emit: (type: string, data: Record<string, unknown>, options?: { lastEventId?: string }) =>
      source.emit(type, { topic: SESSION, ...data }, options),
  };
}

beforeEach(() => {
  installFakeEventSource();
  env = createTestQueryEnv();
  installSessionEventRoute();
  invalidated = [];
  vi.spyOn(env.client, 'invalidateQueries').mockImplementation((filters) => {
    invalidated.push((filters?.queryKey ?? []) as QueryKey);
    return Promise.resolve();
  });
});

afterEach(() => {
  stop?.();
  stop = null;
  resetReviewInvalidationForTests();
  vi.restoreAllMocks();
  env.restore();
});

/** One history document, as `session.history` answers it. */
function history(events: SessionHistoryResponse['history']): SessionHistoryResponse {
  return {
    session_id: SESSION,
    type: 'chat',
    state: 'active',
    kilns: [],
    history: events,
    total_events: events.length,
    transcript: { as_of_seq: 0, items: [] },
  };
}

describe('the session event route', () => {
  it('refreshes both the selected model and its menu when an ACP model changes', () => {
    const source = openStream();
    source.emit('model_switched', { event: 'model_switched', data: { model_id: 'agent-model', provider: 'acp' } });
    expect(invalidated).toContainEqual(keys.session(SESSION));
    expect(invalidated).toContainEqual(keys.sessions(false));
    expect(invalidated).toContainEqual(keys.sessions(true));
  });

  it('refreshes the status list when Lua publishes a replacement', () => {
    const source = openStream();
    source.emit('status_items_changed', { event: 'status_items_changed', data: { status: [] } });
    expect(invalidated).toEqual([keys.sessionStatus(SESSION)]);
  });
  it('leaves the cache alone for a token', () => {
    const source = openStream();

    source.emit('text_delta', { event: 'text_delta', data: { content: 'hi' } });

    expect(invalidated).toEqual([]);
  });

  // The transcript store keeps the transcript current from the ops, so the
  // end of a turn invalidates no history.
  it('reads the review again when a turn completes, and leaves the history alone', async () => {
    const source = openStream();

    source.emit('message_complete', {
      event: 'message_complete',
      data: { message_id: 'msg-1', full_response: 'done' },
    });
    await new Promise((resolve) => setTimeout(resolve, REVIEW_INVALIDATE_DEBOUNCE_MS + 30));

    expect(invalidated).toEqual([keys.diffset(`session-${SESSION}`)]);
  });

  it('writes nothing for a transcript frame, which the transcript store applies', () => {
    env.client.setQueryData(keys.sessionHistory(SESSION), history([]));
    const source = openStream();

    source.emit('transcript', { type: 'transcript', seq: 1, ops: [] });

    expect(invalidated).toEqual([]);
    expect(env.client.getQueryData<SessionHistoryResponse>(keys.sessionHistory(SESSION))).toEqual(history([]));
  });

  it('invalidates both session lists and tells the bus about a new title', () => {
    const titles: { sessionId: string; title: string }[] = [];
    getBus().on('sessionTitleChanged', (payload) => titles.push(payload));
    const source = openStream();

    source.emit('title_changed', { event: 'title_changed', data: { title: 'Renamed' } });

    expect(invalidated).toEqual([keys.sessions(false), keys.sessions(true)]);
    expect(titles).toEqual([{ sessionId: SESSION, title: 'Renamed' }]);
  });

  it('invalidates the mode list when the daemon changes mode', () => {
    const source = openStream();

    source.emit('mode_changed', { event: 'mode_changed', data: { mode: 'review' } });

    expect(invalidated).toEqual([keys.sessionModes(SESSION)]);
  });

  it('invalidates the pending interactions when the agent asks', () => {
    const source = openStream();

    source.emit('interaction_requested', {
      event: 'interaction_requested',
      data: { request_id: 'req-1', request: { kind: 'ask', question: 'which?' } },
    });

    expect(invalidated).toEqual([keys.pendingInteractions()]);
  });

  it('drops a prompt that the daemon ended, in the pane and in the Inbox', () => {
    const resolved: { sessionId: string; requestId: string }[] = [];
    getBus().on('interactionResolved', (payload) => resolved.push(payload));
    const source = openStream();

    source.emit('interaction_completed', {
      event: 'interaction_completed',
      data: { request_id: 'perm-1', response: { kind: 'cancelled' } },
    });

    expect(resolved).toEqual([{ sessionId: SESSION, requestId: 'perm-1' }]);
    expect(invalidated).toEqual([keys.pendingInteractions()]);
  });

  // The loop limit and the TUI change the approval too, so the web
  // control reads it again.
  it('invalidates the plugin approvals when the daemon changes one', () => {
    openStream().emit('plugin_approval_changed', {
      event: 'plugin_approval_changed',
      data: { plugin: 'goal', approval: 'ask' },
    });
    expect(invalidated).toEqual([keys.sessionPluginApprovals(SESSION)]);
  });

  it('writes no echoed user message into the history', () => {
    env.client.setQueryData(keys.sessionHistory(SESSION), history([]));
    openStream().emit('user_message', {
      event: 'user_message',
      data: { message_id: 'msg-1', content: 'hello' },
    });

    const held = env.client.getQueryData<SessionHistoryResponse>(keys.sessionHistory(SESSION));
    expect(held?.history).toEqual([]);
    expect(invalidated).toEqual([]);
  });

  /**
   * The event means "the composed diff may have moved", and a turn fires
   * several of them. They coalesce
   * into ONE listing, because an invalidation does not fold concurrent
   * refetches of a key into one request: it cancels the one in flight and
   * starts another, so a burst is a listing per edit of a diff that settles
   * once.
   */
  it('coalesces a burst of change events into one review listing', async () => {
    const source = openStream();

    source.emit('review_changed', { event: 'review_changed', data: {} });
    source.emit('review_changed', { event: 'review_changed', data: {} });

    expect(invalidated).toEqual([]);
    await new Promise((resolve) => setTimeout(resolve, REVIEW_INVALIDATE_DEBOUNCE_MS + 30));

    expect(invalidated).toEqual([keys.diffset(`session-${SESSION}`)]);
  });

  // The negative: a session whose stream said nothing about the review keeps
  // the listing it has.
  it('leaves the review of a session that said nothing alone', async () => {
    const source = openStream();

    source.emit('text_delta', { event: 'text_delta', data: { content: 'x' } });
    await new Promise((resolve) => setTimeout(resolve, REVIEW_INVALIDATE_DEBOUNCE_MS + 30));

    expect(invalidated).toEqual([]);
  });

  it('writes nothing for a dropped-event warning, which the transcript store answers', () => {
    const source = openStream();

    source.emit('stream_gap', {
      event: 'stream_gap',
      data: { dropped: 3 },
    });

    expect(invalidated).toEqual([]);
  });

  it('names the session of the stream the event arrived on', () => {
    // Both sessions' topics now travel one shared connection
    // (Simplification Plan step 19), so one frame at a time, tagged with its
    // own topic, is what tells the two apart — not two different sources.
    const titles: { sessionId: string; title: string }[] = [];
    getBus().on('sessionTitleChanged', (payload) => titles.push(payload));
    const stopOther = sessionEvents('s2').subscribe(() => {});
    stop = sessionEvents('s1').subscribe(() => {});
    // Joining `s1` while `s2` is already open rebuilds the ONE shared
    // connection to carry both, closing the source `s2` alone opened.
    const source = FakeEventSource.instances.at(-1)!;

    expect(source.url).toBe('/api/events?topics=s2%2Cs1');
    source.emit('title_changed', { topic: 's1', event: 'title_changed', data: { title: 'One' } });
    source.emit('title_changed', { topic: 's2', event: 'title_changed', data: { title: 'Two' } });

    expect(titles).toEqual([
      { sessionId: 's1', title: 'One' },
      { sessionId: 's2', title: 'Two' },
    ]);
    stopOther();
  });
});

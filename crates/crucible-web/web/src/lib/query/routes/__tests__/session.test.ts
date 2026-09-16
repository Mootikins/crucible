import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import type { QueryKey } from '@tanstack/solid-query';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { FakeEventSource, installFakeEventSource, onlyEventSource } from '@/test-utils/sse';
import { getBus } from '@/lib/bus';
import type { SessionHistoryResponse } from '@/lib/api';
import { keys } from '../../keys';
import { sessionEvents } from '../../sse';
import { installSessionEventRoute } from '../session';

const SESSION = 's1';

let env: TestQueryEnv;
let invalidated: QueryKey[];
let stop: (() => void) | null = null;

/**
 * Opens the session stream and answers the source the route reads.
 *
 * The route runs inside the stream, not beside it, so every case drives it the
 * way the daemon does: one frame on the wire.
 */
function openStream() {
  stop = sessionEvents(SESSION).subscribe(() => {});
  return onlyEventSource();
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
  vi.restoreAllMocks();
  env.restore();
});

/** One history document, as `GET /api/session/{id}/history` answers it. */
function history(events: SessionHistoryResponse['history']): SessionHistoryResponse {
  return { session_id: SESSION, history: events, total_events: events.length };
}

describe('the session event route', () => {
  it('leaves the cache alone for a token, which only the pane reducer folds', () => {
    const source = openStream();

    source.emit('token', { type: 'token', content: 'hi' });

    expect(invalidated).toEqual([]);
  });

  it('invalidates the history when a turn completes', () => {
    const source = openStream();

    source.emit('message_complete', { type: 'message_complete', id: 'msg-1', content: 'done' });

    expect(invalidated).toEqual([keys.sessionHistory(SESSION)]);
  });

  it('invalidates the history when the turn fails', () => {
    const source = openStream();

    source.emit('error', { type: 'error', code: 'provider', message: 'no' });

    expect(invalidated).toEqual([keys.sessionHistory(SESSION)]);
  });

  it('invalidates both session lists and tells the bus about a new title', () => {
    const titles: { sessionId: string; title: string }[] = [];
    getBus().on('sessionTitleChanged', (payload) => titles.push(payload));
    const source = openStream();

    source.emit('title_changed', { type: 'title_changed', title: 'Renamed' });

    expect(invalidated).toEqual([keys.sessions(false), keys.sessions(true)]);
    expect(titles).toEqual([{ sessionId: SESSION, title: 'Renamed' }]);
  });

  it('invalidates the mode list when the daemon changes mode', () => {
    const source = openStream();

    source.emit('mode_changed', { type: 'mode_changed', mode: 'review' });

    expect(invalidated).toEqual([keys.sessionModes(SESSION)]);
  });

  it('invalidates the pending interactions when the agent asks', () => {
    const source = openStream();

    source.emit('interaction_requested', {
      type: 'interaction_requested',
      id: 'req-1',
      kind: 'ask',
      question: 'which?',
    });

    expect(invalidated).toEqual([keys.pendingInteractions()]);
  });

  it('appends the echoed user message to the history it already holds', () => {
    env.client.setQueryData(keys.sessionHistory(SESSION), history([]));
    const source = openStream();

    source.emit('session_event', {
      type: 'session_event',
      event: 'user_message',
      data: { message_id: 'msg-1', content: 'hello' },
    });

    const held = env.client.getQueryData<SessionHistoryResponse>(keys.sessionHistory(SESSION));
    expect(held?.history).toHaveLength(1);
    expect(held?.history[0]).toMatchObject({
      session_id: SESSION,
      event: 'user_message',
      data: { message_id: 'msg-1', content: 'hello' },
    });
    expect(held?.total_events).toBe(1);
    expect(invalidated).toEqual([]);
  });

  it('adds the echoed user message once, whatever the number of echoes', () => {
    env.client.setQueryData(keys.sessionHistory(SESSION), history([]));
    const source = openStream();
    const frame = {
      type: 'session_event',
      event: 'user_message',
      data: { message_id: 'msg-1', content: 'hello' },
    };

    source.emit('session_event', frame);
    source.emit('session_event', frame);

    const held = env.client.getQueryData<SessionHistoryResponse>(keys.sessionHistory(SESSION));
    expect(held?.history).toHaveLength(1);
  });

  it('mints no history entry when nothing read the history yet', () => {
    const source = openStream();

    source.emit('session_event', {
      type: 'session_event',
      event: 'user_message',
      data: { message_id: 'msg-1', content: 'hello' },
    });

    expect(env.client.getQueryData(keys.sessionHistory(SESSION))).toBeUndefined();
  });

  it('invalidates the review of a gate and of a change', () => {
    const source = openStream();

    source.emit('session_event', { type: 'session_event', event: 'review_gate', data: {} });
    source.emit('session_event', { type: 'session_event', event: 'review_changed', data: {} });

    expect(invalidated).toEqual([keys.review(SESSION), keys.review(SESSION)]);
  });

  it('writes nothing for a dropped-event warning, which the pane surfaces', () => {
    const source = openStream();

    source.emit('session_event', {
      type: 'session_event',
      event: 'stream_gap',
      data: { dropped: 3 },
    });

    expect(invalidated).toEqual([]);
  });

  it('names the session of the stream the event arrived on', () => {
    const titles: { sessionId: string; title: string }[] = [];
    getBus().on('sessionTitleChanged', (payload) => titles.push(payload));
    const stopOther = sessionEvents('s2').subscribe(() => {});
    stop = sessionEvents('s1').subscribe(() => {});
    const [first, second] = FakeEventSource.instances;

    expect(second!.url).toBe('/api/chat/events/s1');
    second!.emit('title_changed', { type: 'title_changed', title: 'One' });
    first!.emit('title_changed', { type: 'title_changed', title: 'Two' });

    expect(titles).toEqual([
      { sessionId: 's1', title: 'One' },
      { sessionId: 's2', title: 'Two' },
    ]);
    stopOther();
  });
});

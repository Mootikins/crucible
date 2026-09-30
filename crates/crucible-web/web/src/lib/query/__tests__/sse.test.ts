import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { QueryClient } from '@tanstack/solid-query';
import {
  sessionEvents,
  surfaceEvents,
  fsEvents,
  setEventRoute,
  systemEvents,
  resetSseForTests,
  advanceSessionCursor,
  sessionCursor,
} from '../sse';
import { getQueryClient, setQueryClientForTests } from '../client';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { notificationActions, notificationStore } from '@/stores/notificationStore';
import { getBus } from '../../bus';
import {
  FakeEventSource,
  installFakeEventSource,
  onlyEventSource as onlySource,
} from '@/test-utils/sse';

/**
 * Runs the microtasks that are in the queue now. The stream closes one
 * microtask after its last subscriber leaves.
 */
async function settle(): Promise<void> {
  await Promise.resolve();
}

/** The most recently opened source — the one still live, after a rebuild
 * replaced an earlier one. */
function latestSource(): FakeEventSource {
  const source = FakeEventSource.instances.at(-1);
  if (!source) throw new Error('no EventSource was opened');
  return source;
}

beforeEach(() => {
  installFakeEventSource();
  setQueryClientForTests(new QueryClient());
});

afterEach(() => {
  resetSseForTests();
  setQueryClientForTests(null);
});

describe('sessionEvents', () => {
  it('builds one EventSource for the session, whatever the number of subscribers', () => {
    const first = vi.fn();
    const second = vi.fn();

    sessionEvents('s1').subscribe(first);
    sessionEvents('s1').subscribe(second);

    expect(FakeEventSource.instances).toHaveLength(1);
    expect(onlySource().url).toBe('/api/events?topics=s1');
  });

  it('reads the notifications of the session once, when it attaches, oldest first', async () => {
    const env: TestQueryEnv = createTestQueryEnv({
      'POST /api/rpc/session.list_notifications': () => ({
        notifications: [
          { id: 'n2', kind: 'toast', message: 'newer' },
          { id: 'n1', kind: 'warning', message: 'older' },
        ],
      }),
    });
    try {
      sessionEvents('s1').subscribe(vi.fn());
      sessionEvents('s1').subscribe(vi.fn());
      onlySource().open();

      await vi.waitFor(() =>
        // The store is one for the whole file, so only these two are read.
        expect(
          notificationStore.notifications
            .filter((n) => n.message === 'older' || n.message === 'newer')
            .map((n) => [n.type, n.message]),
        ).toEqual([
          ['warning', 'older'],
          ['info', 'newer'],
        ]),
      );
      expect(env.fetch.calls('POST /api/rpc/session.list_notifications')).toBe(1);
    } finally {
      env.restore();
    }
  });

  it('a notice read on attach closes in the daemon for its session', async () => {
    const env: TestQueryEnv = createTestQueryEnv({
      'POST /api/rpc/session.list_notifications': () => ({
        notifications: [{ id: 'n-attach', kind: 'warning', message: 'read on attach' }],
      }),
      'POST /api/rpc/session.dismiss_notification': () => ({ success: true }),
    });
    try {
      sessionEvents('s1').subscribe(vi.fn());
      onlySource().open();
      const entry = await vi.waitFor(() => {
        const found = notificationStore.notifications.find(
          (n) => !n.dismissed && n.message === 'read on attach',
        );
        expect(found).toBeDefined();
        return found!;
      });

      notificationActions.dismiss(entry.id);

      await vi.waitFor(() =>
        expect(env.fetch.calls('POST /api/rpc/session.dismiss_notification')).toBe(1),
      );
    } finally {
      env.restore();
    }
  });

  it('gives every message to every handler', () => {
    const first = vi.fn();
    const second = vi.fn();
    sessionEvents('s1').subscribe(first);
    sessionEvents('s1').subscribe(second);

    onlySource().emit('text_delta', { topic: 's1', event: 'text_delta', data: { content: 'hi' } });

    expect(first).toHaveBeenCalledWith({ event: 'text_delta', data: { content: 'hi' } });
    expect(second).toHaveBeenCalledWith({ event: 'text_delta', data: { content: 'hi' } });
  });

  it('holds the stream open until the last subscriber leaves', async () => {
    const stopFirst = sessionEvents('s1').subscribe(vi.fn());
    const stopSecond = sessionEvents('s1').subscribe(vi.fn());
    const source = onlySource();

    stopFirst();
    await settle();
    expect(source.closed).toBe(false);

    stopSecond();
    await settle();
    expect(source.closed).toBe(true);
  });

  it('keeps the source when the last subscriber leaves and a new one joins in the same task', async () => {
    // A split of a pane unmounts the chat and mounts it again in one batch.
    // The stream must not close and open again with `?after=` for that.
    const stop = sessionEvents('s1').subscribe(vi.fn());
    onlySource().open();

    stop();
    const joined = vi.fn();
    sessionEvents('s1').subscribe(vi.fn(), joined);
    await settle();

    expect(FakeEventSource.instances).toHaveLength(1);
    expect(onlySource().closed).toBe(false);
    expect(joined).toHaveBeenCalledTimes(1);
  });

  it('keeps the two sessions apart, on one shared connection', () => {
    // Two different session ids are two different topics, so the second
    // subscribe rebuilds the ONE connection to carry both — it does not open
    // a second `EventSource`.
    const first = vi.fn();
    const second = vi.fn();
    sessionEvents('s1').subscribe(first);
    sessionEvents('s2').subscribe(second);

    expect(latestSource().url).toBe('/api/events?topics=s1%2Cs2');

    latestSource().emit('text_delta', { topic: 's1', event: 'text_delta', data: { content: 'one' } });

    expect(first).toHaveBeenCalledTimes(1);
    expect(second).not.toHaveBeenCalled();
  });

  it('opens a new stream after the old one closed', async () => {
    const stop = sessionEvents('s1').subscribe(vi.fn());
    stop();
    await settle();

    sessionEvents('s1').subscribe(vi.fn());

    expect(FakeEventSource.instances).toHaveLength(2);
    expect(FakeEventSource.instances[0]!.closed).toBe(true);
    expect(FakeEventSource.instances[1]!.closed).toBe(false);
  });

  it('stops one handler alone, and leaves the other one live', () => {
    const first = vi.fn();
    const second = vi.fn();
    const stopFirst = sessionEvents('s1').subscribe(first);
    sessionEvents('s1').subscribe(second);

    stopFirst();
    onlySource().emit('text_delta', { topic: 's1', event: 'text_delta', data: { content: 'hi' } });

    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);
  });

  it('answers the last event through the root accessor', () => {
    const stream = sessionEvents('s1');
    stream.subscribe(vi.fn());

    expect(stream.latest()).toBeUndefined();
    onlySource().emit('text_delta', { topic: 's1', event: 'text_delta', data: { content: 'hi' } });

    expect(stream.latest()).toEqual({ event: 'text_delta', data: { content: 'hi' } });
  });

  it('tells the first subscriber and a later one that the stream is open', () => {
    const first = vi.fn();
    sessionEvents('s1').subscribe(vi.fn(), first);
    expect(first).not.toHaveBeenCalled();

    onlySource().open();
    expect(first).toHaveBeenCalledTimes(1);

    const later = vi.fn();
    sessionEvents('s1').subscribe(vi.fn(), later);
    expect(later).toHaveBeenCalledTimes(1);
  });

  it('gives each event to the route, with the session id and the two stores', () => {
    const route = vi.fn();
    setEventRoute('session', route);
    sessionEvents('s1').subscribe(vi.fn());

    onlySource().emit('text_delta', { topic: 's1', event: 'text_delta', data: { content: 'hi' } });

    expect(route).toHaveBeenCalledWith(
      { event: 'text_delta', data: { content: 'hi' } },
      { client: getQueryClient(), bus: getBus(), sessionId: 's1' },
    );
  });

  it('keeps the handlers running when the route throws', () => {
    setEventRoute('session', () => {
      throw new Error('route is broken');
    });
    const handler = vi.fn();
    sessionEvents('s1').subscribe(handler);

    onlySource().emit('text_delta', { topic: 's1', event: 'text_delta', data: { content: 'hi' } });

    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('holds no route after the reset', () => {
    const route = vi.fn();
    setEventRoute('session', route);
    resetSseForTests();

    sessionEvents('s1').subscribe(vi.fn());
    onlySource().emit('text_delta', { topic: 's1', event: 'text_delta', data: { content: 'hi' } });

    expect(route).not.toHaveBeenCalled();
  });
});

describe('the session resume cursor', () => {
  it('is absent before anything was applied, and the stream opens bare', () => {
    expect(sessionCursor('s1')).toBeUndefined();
    sessionEvents('s1').subscribe(vi.fn());

    expect(onlySource().url).toBe('/api/events?topics=s1');
  });

  it('states the applied seq as this topic\'s ?after= pair on every reopen', () => {
    advanceSessionCursor('s1', 7);
    sessionEvents('s1').subscribe(vi.fn());

    expect(onlySource().url).toBe('/api/events?topics=s1&after=s1%3A7');

    // A manual reconnect re-reads the cursor at connect time, so a watermark
    // that moved while the stream was down travels on the new source.
    advanceSessionCursor('s1', 9);
    sessionEvents('s1').reconnect();

    expect(FakeEventSource.instances[1]!.url).toBe('/api/events?topics=s1&after=s1%3A9');
  });

  it('advances monotonically and never walks back', () => {
    advanceSessionCursor('s1', 5);
    advanceSessionCursor('s1', 3);

    expect(sessionCursor('s1')).toBe(5);
  });

  it('carries the seq a frame stamped (topic:seq), and nothing when it did not', () => {
    const seen: Array<{ seq?: number | null; event?: string; type?: string }> = [];
    sessionEvents('s1').subscribe((event) => seen.push(event));

    onlySource().emit(
      'text_delta',
      { topic: 's1', event: 'text_delta', data: { content: 'a' } },
      { lastEventId: 's1:12' },
    );
    onlySource().emit('text_delta', { topic: 's1', event: 'text_delta', data: { content: 'b' } });
    onlySource().emit(
      'message_complete',
      {
        topic: 's1',
        event: 'message_complete',
        data: { message_id: 'm1', full_response: 'c' },
      },
      { lastEventId: 's1:13' },
    );

    expect(seen.map((event) => ['event' in event ? event.event : event.type, event.seq])).toEqual([
      ['text_delta', 12],
      ['text_delta', undefined],
      ['message_complete', 13],
    ]);
  });
});

describe('the shared connection', () => {
  it('carries the surface, filesystem and system domains on one EventSource', () => {
    // All three read the daemon's `system` topic, so joining the second and
    // third domain must not open a second connection.
    surfaceEvents().subscribe(vi.fn());
    fsEvents().subscribe(vi.fn());
    systemEvents().subscribe(vi.fn());

    expect(FakeEventSource.instances).toHaveLength(1);
    expect(onlySource().url).toBe('/api/events?topics=system');
  });

  it('rebuilds the connection when a new topic joins, and drops it from the url when the topic leaves', async () => {
    sessionEvents('s1').subscribe(vi.fn());
    expect(latestSource().url).toBe('/api/events?topics=s1');

    const stopSurface = surfaceEvents().subscribe(vi.fn());
    expect(FakeEventSource.instances).toHaveLength(2);
    expect(latestSource().url).toBe('/api/events?topics=s1%2Csystem');
    expect(FakeEventSource.instances[0]!.closed).toBe(true);

    stopSurface();
    await settle();
    expect(FakeEventSource.instances).toHaveLength(3);
    expect(latestSource().url).toBe('/api/events?topics=s1');
  });

  it('routes a frame only to the domain whose event names it matches', () => {
    const surfaceHandler = vi.fn();
    const fsHandler = vi.fn();
    const systemHandler = vi.fn();
    surfaceEvents().subscribe(surfaceHandler);
    fsEvents().subscribe(fsHandler);
    systemEvents().subscribe(systemHandler);

    onlySource().emit('surface_changed', {
      topic: 'system',
      plugin: 'board',
      name: 'tasks',
      version: 2,
    });

    expect(surfaceHandler).toHaveBeenCalledTimes(1);
    expect(fsHandler).not.toHaveBeenCalled();
    expect(systemHandler).not.toHaveBeenCalled();
  });

  it('a frame of a topic nobody joined reaches nobody', () => {
    const handler = vi.fn();
    sessionEvents('s1').subscribe(handler);

    // The frontend never asks for `s2`; a frame naming it (impossible through
    // the real route, since this connection never named it as a topic) is
    // dropped rather than misrouted to `s1`'s handler.
    onlySource().emit('text_delta', { topic: 's2', event: 'text_delta', data: { content: 'not mine' } });

    expect(handler).not.toHaveBeenCalled();
  });
});

describe('surfaceEvents', () => {
  it('builds one EventSource for every subscriber', () => {
    const first = vi.fn();
    const second = vi.fn();
    surfaceEvents().subscribe(first);
    surfaceEvents().subscribe(second);

    expect(onlySource().url).toBe('/api/events?topics=system');

    onlySource().emit('surface_changed', {
      topic: 'system',
      plugin: 'board',
      name: 'tasks',
      version: 2,
      withdrawn: true,
    });
    expect(first).toHaveBeenCalledWith({
      plugin: 'board',
      name: 'tasks',
      version: 2,
      withdrawn: true,
    });
    expect(second).toHaveBeenCalledWith({
      plugin: 'board',
      name: 'tasks',
      version: 2,
      withdrawn: true,
    });
  });

  it('closes the stream when the last subscriber leaves', async () => {
    const stopFirst = surfaceEvents().subscribe(vi.fn());
    const stopSecond = surfaceEvents().subscribe(vi.fn());
    const source = onlySource();

    stopFirst();
    await settle();
    expect(source.closed).toBe(false);
    stopSecond();
    await settle();
    expect(source.closed).toBe(true);
  });

  it('gives each event to the route', () => {
    const route = vi.fn();
    setEventRoute('surface', route);
    surfaceEvents().subscribe(vi.fn());

    onlySource().emit('surface_changed', {
      topic: 'system',
      plugin: 'board',
      name: 'tasks',
      version: 2,
      withdrawn: true,
    });

    expect(route).toHaveBeenCalledWith(
      { plugin: 'board', name: 'tasks', version: 2, withdrawn: true },
      { client: getQueryClient(), bus: getBus() },
    );
  });
});

describe('fsEvents', () => {
  it('builds one EventSource for every subscriber', () => {
    const first = vi.fn();
    const second = vi.fn();
    fsEvents().subscribe(first);
    fsEvents().subscribe(second);

    expect(onlySource().url).toBe('/api/events?topics=system');

    onlySource().emit('fs_changed', { topic: 'system', type: 'changed', path: '/k/a.md', kind: 'modified' });
    expect(first).toHaveBeenCalledWith({ type: 'changed', path: '/k/a.md', kind: 'modified' });
    expect(second).toHaveBeenCalledWith({ type: 'changed', path: '/k/a.md', kind: 'modified' });
  });

  it('closes the stream when the last subscriber leaves', async () => {
    const stop = fsEvents().subscribe(vi.fn());
    const source = onlySource();

    stop();
    await settle();

    expect(source.closed).toBe(true);
  });

  it('gives each event to the route', () => {
    const route = vi.fn();
    setEventRoute('fs', route);
    fsEvents().subscribe(vi.fn());

    onlySource().emit('fs_deleted', { topic: 'system', type: 'deleted', path: '/k/a.md' });

    expect(route).toHaveBeenCalledWith(
      { type: 'deleted', path: '/k/a.md' },
      { client: getQueryClient(), bus: getBus() },
    );
  });
});

describe('systemEvents', () => {
  it('reads the system route and carries both events', () => {
    const handler = vi.fn();
    const route = vi.fn();
    setEventRoute('system', route);
    systemEvents().subscribe(handler);

    expect(onlySource().url).toBe('/api/events?topics=system');

    onlySource().emit('proposal_changed', { topic: 'system', id: 'p-1' });
    onlySource().emit('publication_changed', { topic: 'system', plugin: 'board', key: 'rows' });

    expect(handler.mock.calls).toEqual([
      [{ event: 'proposal_changed', id: 'p-1' }],
      [{ event: 'publication_changed', plugin: 'board', key: 'rows' }],
    ]);
    expect(route).toHaveBeenCalledWith(
      { event: 'proposal_changed', id: 'p-1' },
      { client: getQueryClient(), bus: getBus() },
    );
  });

  it('drops a proposal frame without an id', () => {
    const handler = vi.fn();
    systemEvents().subscribe(handler);

    onlySource().emit('proposal_changed', { topic: 'system', other: 1 });

    expect(handler).not.toHaveBeenCalled();
  });

  it('builds one EventSource for every subscriber, and names the plugin and the key', () => {
    const first = vi.fn();
    const second = vi.fn();
    systemEvents().subscribe(first);
    systemEvents().subscribe(second);

    expect(onlySource().url).toBe('/api/events?topics=system');

    onlySource().emit('publication_changed', { topic: 'system', plugin: 'board', key: 'rows' });
    expect(first).toHaveBeenCalledWith({ event: 'publication_changed', plugin: 'board', key: 'rows' });
    expect(second).toHaveBeenCalledWith({ event: 'publication_changed', plugin: 'board', key: 'rows' });
  });

  it('closes the stream when the last subscriber leaves', async () => {
    const stopFirst = systemEvents().subscribe(vi.fn());
    const stopSecond = systemEvents().subscribe(vi.fn());
    const source = onlySource();

    stopFirst();
    await settle();
    expect(source.closed).toBe(false);
    stopSecond();
    await settle();
    expect(source.closed).toBe(true);
  });

  it('gives each event to the route', () => {
    const route = vi.fn();
    setEventRoute('system', route);
    systemEvents().subscribe(vi.fn());

    onlySource().emit('publication_changed', { topic: 'system', plugin: 'board', key: 'rows' });

    expect(route).toHaveBeenCalledWith(
      { event: 'publication_changed', plugin: 'board', key: 'rows' },
      { client: getQueryClient(), bus: getBus() },
    );
  });

  it('drops a frame it cannot parse, and keeps the stream', () => {
    const handler = vi.fn();
    systemEvents().subscribe(handler);

    for (const listener of [...(onlySource().listeners.get('publication_changed') ?? [])]) {
      listener({ data: 'not json' } as MessageEvent);
    }

    expect(handler).not.toHaveBeenCalled();
    expect(onlySource().closed).toBe(false);
  });

  it('reports the system stream_gap through the reconcile hook, not as a frame', () => {
    const route = vi.fn();
    const handler = vi.fn();
    setEventRoute('system', route);
    systemEvents().subscribe(handler);

    onlySource().emit('stream_gap', { topic: 'system', dropped: 3 });

    // The gap does not reach the handler or the route as an ordinary frame —
    // `reconcile` (wired to `hooks.onGap` in `api.ts`) is the only thing it
    // triggers, mirroring the old `/api/events/system` stream's own split.
    expect(handler).not.toHaveBeenCalled();
    expect(route).not.toHaveBeenCalled();
  });
});

describe('resetSseForTests', () => {
  it('closes every live stream', () => {
    sessionEvents('s1').subscribe(vi.fn());
    surfaceEvents().subscribe(vi.fn());
    fsEvents().subscribe(vi.fn());
    systemEvents().subscribe(vi.fn());

    resetSseForTests();

    expect(FakeEventSource.instances.every((s) => s.closed)).toBe(true);
  });
});

describe('reconnect', () => {
  it('rebuilds the ONE shared connection, keeping every subscriber of every joined topic', () => {
    const first = vi.fn();
    const second = vi.fn();
    const stream = sessionEvents('s1');
    stream.subscribe(first);
    stream.subscribe(second);
    const original = onlySource();

    stream.reconnect();

    expect(FakeEventSource.instances).toHaveLength(2);
    expect(original.closed).toBe(true);
    const replacement = latestSource();
    expect(replacement.url).toBe('/api/events?topics=s1');
    expect(replacement.closed).toBe(false);

    replacement.emit('text_delta', { topic: 's1', event: 'text_delta', data: { content: 'after' } });
    expect(first).toHaveBeenCalledWith({ event: 'text_delta', data: { content: 'after' } });
    expect(second).toHaveBeenCalledWith({ event: 'text_delta', data: { content: 'after' } });
  });

  it('reconnecting one stream also rebuilds a concurrently joined topic, on the same new source', () => {
    // Both streams read the ONE shared connection, so reconnecting the
    // session stream carries `system` along too — there is only one
    // transport to rebuild.
    sessionEvents('s1').subscribe(vi.fn());
    const systemHandler = vi.fn();
    systemEvents().subscribe(systemHandler);
    expect(latestSource().url).toBe('/api/events?topics=s1%2Csystem');

    sessionEvents('s1').reconnect();

    const replacement = latestSource();
    expect(replacement.url).toBe('/api/events?topics=s1%2Csystem');
    replacement.emit('publication_changed', { topic: 'system', plugin: 'p', key: 'k' });
    expect(systemHandler).toHaveBeenCalledWith({ event: 'publication_changed', plugin: 'p', key: 'k' });
  });

  it('keeps the route on the new source', () => {
    const route = vi.fn();
    setEventRoute('session', route);
    const stream = sessionEvents('s1');
    stream.subscribe(vi.fn());

    stream.reconnect();
    latestSource().emit('text_delta', { topic: 's1', event: 'text_delta', data: { content: 'after' } });

    expect(route).toHaveBeenCalledTimes(1);
  });

  it('opens nothing when no subscriber is there', () => {
    sessionEvents('s1').reconnect();

    expect(FakeEventSource.instances).toHaveLength(0);
  });

  it('closes the last source when the last subscriber leaves after it', async () => {
    const stream = sessionEvents('s1');
    const stop = stream.subscribe(vi.fn());
    stream.reconnect();

    stop();
    await settle();

    expect(FakeEventSource.instances.every((s) => s.closed)).toBe(true);
  });

  it('opens a new plugin source too', () => {
    const handler = vi.fn();
    const stream = systemEvents();
    stream.subscribe(handler);
    const original = onlySource();

    stream.reconnect();

    expect(FakeEventSource.instances).toHaveLength(2);
    expect(original.closed).toBe(true);
    latestSource().emit('publication_changed', { topic: 'system', plugin: 'board', key: 'rows' });
    expect(handler).toHaveBeenCalledWith({ event: 'publication_changed', plugin: 'board', key: 'rows' });
  });

  it('makes a joiner wait for the next open', () => {
    const stream = sessionEvents('s1');
    stream.subscribe(vi.fn(), vi.fn());
    onlySource().open();

    stream.reconnect();
    const later = vi.fn();
    stream.subscribe(vi.fn(), later);
    expect(later).not.toHaveBeenCalled();

    latestSource().open();
    expect(later).toHaveBeenCalledTimes(1);
  });
});

describe('the open state of the chat stream', () => {
  it('makes a joiner wait through a drop and the backoff behind it', () => {
    // `subscribeToEvents` owns the backoff and reports the transport through
    // the `connection` event it builds itself. The test drives that real path:
    // the source fails, the retry timer runs, and the new source opens.
    vi.useFakeTimers();
    try {
      const stream = sessionEvents('s1');
      stream.subscribe(vi.fn());
      FakeEventSource.instances[0]!.open();

      FakeEventSource.instances[0]!.onerror?.(new Event('error'));
      const later = vi.fn();
      stream.subscribe(vi.fn(), later);
      expect(later).not.toHaveBeenCalled();

      vi.advanceTimersByTime(1000);
      expect(FakeEventSource.instances).toHaveLength(2);
      expect(later).not.toHaveBeenCalled();

      FakeEventSource.instances[1]!.open();
      expect(later).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });
});

describe('a handler that throws', () => {
  it('leaves the handlers behind it with their event', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const broken = vi.fn(() => {
      throw new Error('handler is broken');
    });
    const good = vi.fn();
    sessionEvents('s1').subscribe(broken);
    sessionEvents('s1').subscribe(good);

    onlySource().emit('text_delta', { topic: 's1', event: 'text_delta', data: { content: 'hi' } });

    expect(broken).toHaveBeenCalledTimes(1);
    expect(good).toHaveBeenCalledTimes(1);
    expect(warn).toHaveBeenCalled();
    warn.mockRestore();
  });

  it('leaves the stream open for the next event', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const broken = vi.fn(() => {
      throw new Error('handler is broken');
    });
    sessionEvents('s1').subscribe(broken);

    onlySource().emit('text_delta', { topic: 's1', event: 'text_delta', data: { content: 'one' } });
    onlySource().emit('text_delta', { topic: 's1', event: 'text_delta', data: { content: 'two' } });

    expect(broken).toHaveBeenCalledTimes(2);
    expect(onlySource().closed).toBe(false);
    warn.mockRestore();
  });
});

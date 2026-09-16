import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { QueryClient } from '@tanstack/solid-query';
import {
  sessionEvents,
  surfaceEvents,
  fsEvents,
  pluginEvents,
  setSessionEventRoute,
  setSurfaceEventRoute,
  setFsEventRoute,
  setPluginEventRoute,
  resetSseForTests,
} from '../sse';
import { getQueryClient, setQueryClientForTests } from '../client';
import { getBus } from '../../bus';
import {
  FakeEventSource,
  installFakeEventSource,
  onlyEventSource as onlySource,
} from '@/test-utils/sse';

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
    expect(onlySource().url).toBe('/api/chat/events/s1');
  });

  it('gives every message to every handler', () => {
    const first = vi.fn();
    const second = vi.fn();
    sessionEvents('s1').subscribe(first);
    sessionEvents('s1').subscribe(second);

    onlySource().emit('token', { type: 'token', content: 'hi' });

    expect(first).toHaveBeenCalledWith({ type: 'token', content: 'hi' });
    expect(second).toHaveBeenCalledWith({ type: 'token', content: 'hi' });
  });

  it('holds the stream open until the last subscriber leaves', () => {
    const stopFirst = sessionEvents('s1').subscribe(vi.fn());
    const stopSecond = sessionEvents('s1').subscribe(vi.fn());
    const source = onlySource();

    stopFirst();
    expect(source.closed).toBe(false);

    stopSecond();
    expect(source.closed).toBe(true);
  });

  it('builds a second EventSource for a second session', () => {
    sessionEvents('s1').subscribe(vi.fn());
    sessionEvents('s2').subscribe(vi.fn());

    expect(FakeEventSource.instances.map((s) => s.url)).toEqual([
      '/api/chat/events/s1',
      '/api/chat/events/s2',
    ]);
  });

  it('keeps the two sessions apart', () => {
    const first = vi.fn();
    const second = vi.fn();
    sessionEvents('s1').subscribe(first);
    sessionEvents('s2').subscribe(second);

    FakeEventSource.instances[0]!.emit('token', { type: 'token', content: 'one' });

    expect(first).toHaveBeenCalledTimes(1);
    expect(second).not.toHaveBeenCalled();
  });

  it('opens a new stream after the old one closed', () => {
    const stop = sessionEvents('s1').subscribe(vi.fn());
    stop();

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
    onlySource().emit('token', { type: 'token', content: 'hi' });

    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);
  });

  it('answers the last event through the root accessor', () => {
    const stream = sessionEvents('s1');
    stream.subscribe(vi.fn());

    expect(stream.latest()).toBeUndefined();
    onlySource().emit('token', { type: 'token', content: 'hi' });

    expect(stream.latest()).toEqual({ type: 'token', content: 'hi' });
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
    setSessionEventRoute(route);
    sessionEvents('s1').subscribe(vi.fn());

    onlySource().emit('token', { type: 'token', content: 'hi' });

    expect(route).toHaveBeenCalledWith(
      { type: 'token', content: 'hi' },
      { client: getQueryClient(), bus: getBus(), sessionId: 's1' },
    );
  });

  it('keeps the handlers running when the route throws', () => {
    setSessionEventRoute(() => {
      throw new Error('route is broken');
    });
    const handler = vi.fn();
    sessionEvents('s1').subscribe(handler);

    onlySource().emit('token', { type: 'token', content: 'hi' });

    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('holds no route after the reset', () => {
    const route = vi.fn();
    setSessionEventRoute(route);
    resetSseForTests();

    sessionEvents('s1').subscribe(vi.fn());
    onlySource().emit('token', { type: 'token', content: 'hi' });

    expect(route).not.toHaveBeenCalled();
  });
});

describe('surfaceEvents', () => {
  it('builds one EventSource for every subscriber', () => {
    const first = vi.fn();
    const second = vi.fn();
    surfaceEvents().subscribe(first);
    surfaceEvents().subscribe(second);

    expect(onlySource().url).toBe('/api/surfaces/events');

    onlySource().emit('surface_changed', { plugin: 'board', name: 'tasks', version: 2, withdrawn: true });
    expect(first).toHaveBeenCalledWith({ plugin: 'board', name: 'tasks', version: 2, withdrawn: true });
    expect(second).toHaveBeenCalledWith({ plugin: 'board', name: 'tasks', version: 2, withdrawn: true });
  });

  it('closes the stream when the last subscriber leaves', () => {
    const stopFirst = surfaceEvents().subscribe(vi.fn());
    const stopSecond = surfaceEvents().subscribe(vi.fn());
    const source = onlySource();

    stopFirst();
    expect(source.closed).toBe(false);
    stopSecond();
    expect(source.closed).toBe(true);
  });

  it('gives each event to the route', () => {
    const route = vi.fn();
    setSurfaceEventRoute(route);
    surfaceEvents().subscribe(vi.fn());

    onlySource().emit('surface_changed', { plugin: 'board', name: 'tasks', version: 2, withdrawn: true });

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

    expect(onlySource().url).toBe('/api/fs/events');

    onlySource().emit('fs_changed', { type: 'changed', path: '/k/a.md', kind: 'modified' });
    expect(first).toHaveBeenCalledWith({ type: 'changed', path: '/k/a.md', kind: 'modified' });
    expect(second).toHaveBeenCalledWith({ type: 'changed', path: '/k/a.md', kind: 'modified' });
  });

  it('closes the stream when the last subscriber leaves', () => {
    const stop = fsEvents().subscribe(vi.fn());
    const source = onlySource();

    stop();

    expect(source.closed).toBe(true);
  });

  it('gives each event to the route', () => {
    const route = vi.fn();
    setFsEventRoute(route);
    fsEvents().subscribe(vi.fn());

    onlySource().emit('fs_deleted', { type: 'deleted', path: '/k/a.md' });

    expect(route).toHaveBeenCalledWith(
      { type: 'deleted', path: '/k/a.md' },
      { client: getQueryClient(), bus: getBus() },
    );
  });
});

describe('pluginEvents', () => {
  it('builds one EventSource for every subscriber, and names the plugin and the key', () => {
    const first = vi.fn();
    const second = vi.fn();
    pluginEvents().subscribe(first);
    pluginEvents().subscribe(second);

    expect(onlySource().url).toBe('/api/plugins/events');

    onlySource().emit('publication_changed', { plugin: 'board', key: 'rows' });
    expect(first).toHaveBeenCalledWith('board', 'rows');
    expect(second).toHaveBeenCalledWith('board', 'rows');
  });

  it('closes the stream when the last subscriber leaves', () => {
    const stopFirst = pluginEvents().subscribe(vi.fn());
    const stopSecond = pluginEvents().subscribe(vi.fn());
    const source = onlySource();

    stopFirst();
    expect(source.closed).toBe(false);
    stopSecond();
    expect(source.closed).toBe(true);
  });

  it('gives each event to the route', () => {
    const route = vi.fn();
    setPluginEventRoute(route);
    pluginEvents().subscribe(vi.fn());

    onlySource().emit('publication_changed', { plugin: 'board', key: 'rows' });

    expect(route).toHaveBeenCalledWith(
      { plugin: 'board', key: 'rows' },
      { client: getQueryClient(), bus: getBus() },
    );
  });

  it('drops a frame it cannot parse, and keeps the stream', () => {
    const handler = vi.fn();
    pluginEvents().subscribe(handler);

    for (const listener of [...(onlySource().listeners.get('publication_changed') ?? [])]) {
      listener({ data: 'not json' } as MessageEvent);
    }

    expect(handler).not.toHaveBeenCalled();
    expect(onlySource().closed).toBe(false);
  });
});

describe('resetSseForTests', () => {
  it('closes every live stream', () => {
    sessionEvents('s1').subscribe(vi.fn());
    surfaceEvents().subscribe(vi.fn());
    fsEvents().subscribe(vi.fn());
    pluginEvents().subscribe(vi.fn());
    expect(FakeEventSource.instances).toHaveLength(4);

    resetSseForTests();

    expect(FakeEventSource.instances.every((s) => s.closed)).toBe(true);
  });
});

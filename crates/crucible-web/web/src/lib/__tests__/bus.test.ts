import { describe, it, expect, afterEach, expectTypeOf, vi } from 'vitest';
import { createRoot } from 'solid-js';
import { createBus, getBus, resetBusForTests, type BusEvents } from '../bus';

afterEach(() => {
  // The singleton keeps no listener from one test into the next one.
  resetBusForTests();
});

describe('createBus', () => {
  it('gives a payload to the handler of the event it emits', () => {
    const bus = createBus();
    const handler = vi.fn();
    bus.on('newSession', handler);

    bus.emit('newSession', { workspace: 'main' });

    expect(handler).toHaveBeenCalledTimes(1);
    expect(handler).toHaveBeenCalledWith({ workspace: 'main' });
  });

  it('gives the payload to every handler of that event', () => {
    const bus = createBus();
    const first = vi.fn();
    const second = vi.fn();
    bus.on('openFile', first);
    bus.on('openFile', second);

    bus.emit('openFile', { path: '/notes/one.md', name: 'one.md' });

    expect(first).toHaveBeenCalledWith({ path: '/notes/one.md', name: 'one.md' });
    expect(second).toHaveBeenCalledWith({ path: '/notes/one.md', name: 'one.md' });
  });

  it('keeps an event away from the handler of another event', () => {
    const bus = createBus();
    const handler = vi.fn();
    bus.on('switchModel', handler);

    bus.emit('clearChat', {});

    expect(handler).not.toHaveBeenCalled();
  });

  it('stops the handler when the caller runs the answer of on()', () => {
    const bus = createBus();
    const handler = vi.fn();
    const unsubscribe = bus.on('toggleHiddenFiles', handler);

    unsubscribe();
    bus.emit('toggleHiddenFiles', {});

    expect(handler).not.toHaveBeenCalled();
  });

  it('stops the handler when the caller runs off()', () => {
    const bus = createBus();
    const handler = vi.fn();
    bus.on('interactionResolved', handler);

    bus.emit('interactionResolved', { sessionId: 'sess-1', requestId: 'req-1' });
    bus.off('interactionResolved', handler);
    bus.emit('interactionResolved', { sessionId: 'sess-1', requestId: 'req-2' });

    expect(handler).toHaveBeenCalledTimes(1);
    expect(handler).toHaveBeenCalledWith({ sessionId: 'sess-1', requestId: 'req-1' });
  });

  it('keeps one handler when off() removes the other one', () => {
    const bus = createBus();
    const kept = vi.fn();
    const removed = vi.fn();
    bus.on('openSettings', kept);
    bus.on('openSettings', removed);

    bus.off('openSettings', removed);
    bus.emit('openSettings', {});

    expect(kept).toHaveBeenCalledTimes(1);
    expect(removed).not.toHaveBeenCalled();
  });

  it('accepts off() for a handler it does not hold', () => {
    const bus = createBus();
    expect(() => bus.off('authOk', vi.fn())).not.toThrow();
  });

  it('keeps the handlers of one bus away from another bus', () => {
    const first = createBus();
    const second = createBus();
    const handler = vi.fn();
    first.on('focusSearch', handler);

    second.emit('focusSearch', {});

    expect(handler).not.toHaveBeenCalled();
  });

  it('stops the handler when the Solid owner of the caller disposes', () => {
    const bus = createBus();
    const handler = vi.fn();
    const dispose = createRoot((disposeRoot) => {
      bus.on('exportSession', handler);
      return disposeRoot;
    });

    bus.emit('exportSession', {});
    dispose();
    bus.emit('exportSession', {});

    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('holds no handler after the Solid owner of the caller disposes', () => {
    const bus = createBus();
    const dispose = createRoot((disposeRoot) => {
      bus.on('exportSession', vi.fn());
      return disposeRoot;
    });
    expect(bus.handlerCount()).toBe(1);

    dispose();

    // The bookkeeping of the bus must not grow with each mount and unmount.
    expect(bus.handlerCount()).toBe(0);
  });

  it('holds no handler after the caller runs the answer of on()', () => {
    const bus = createBus();
    const unsubscribe = bus.on('focusSearch', vi.fn());
    expect(bus.handlerCount()).toBe(1);

    unsubscribe();

    expect(bus.handlerCount()).toBe(0);
  });

  it('holds no handler after the caller runs off()', () => {
    const bus = createBus();
    const handler = vi.fn();
    bus.on('focusSearch', handler);

    bus.off('focusSearch', handler);

    expect(bus.handlerCount()).toBe(0);
  });

  it('accepts two removals of one handler', () => {
    const bus = createBus();
    const handler = vi.fn();
    const unsubscribe = bus.on('authOk', handler);

    unsubscribe();
    expect(() => bus.off('authOk', handler)).not.toThrow();
    expect(() => unsubscribe()).not.toThrow();
    expect(bus.handlerCount()).toBe(0);
  });

  it('adds one handler once, for two calls with the same handler', () => {
    const bus = createBus();
    const handler = vi.fn();
    const first = bus.on('authRequired', handler);
    const second = bus.on('authRequired', handler);

    bus.emit('authRequired', {});

    expect(handler).toHaveBeenCalledTimes(1);
    expect(bus.handlerCount()).toBe(1);
    expect(second).toBe(first);
  });

  it('accepts an event that no handler answers', () => {
    const bus = createBus();
    expect(() => bus.emit('focusSessionSearch', {})).not.toThrow();
  });

  it('gives the payload to the later handler when an earlier one throws', () => {
    const bus = createBus();
    const reported = vi.spyOn(console, 'error').mockImplementation(() => {});
    const later = vi.fn();
    bus.on('clearChat', () => {
      throw new Error('the handler failed');
    });
    bus.on('clearChat', later);

    expect(() => bus.emit('clearChat', {})).not.toThrow();

    expect(later).toHaveBeenCalledTimes(1);
    expect(reported).toHaveBeenCalled();
    reported.mockRestore();
  });

  it('removes every handler of every event on clear()', () => {
    const bus = createBus();
    const handler = vi.fn();
    bus.on('openSettings', handler);
    bus.on('switchModel', vi.fn());
    expect(bus.handlerCount()).toBe(2);

    bus.clear();
    bus.emit('openSettings', {});

    expect(bus.handlerCount()).toBe(0);
    expect(handler).not.toHaveBeenCalled();
  });
});

describe('getBus', () => {
  it('answers one bus for the whole module', () => {
    const bus = getBus();
    expect(getBus()).toBe(bus);
  });

  it('carries an event from one caller of getBus() to another one', () => {
    const handler = vi.fn();
    getBus().on('sessionTitleChanged', handler);

    getBus().emit('sessionTitleChanged', { sessionId: 'sess-1', title: 'One' });

    expect(handler).toHaveBeenCalledWith({ sessionId: 'sess-1', title: 'One' });
  });
});

describe('resetBusForTests', () => {
  it('removes every handler from the singleton', () => {
    const handler = vi.fn();
    getBus().on('openSession', handler);

    resetBusForTests();
    getBus().emit('openSession', { sessionId: 'sess-1', title: 'One' });

    expect(handler).not.toHaveBeenCalled();
  });

  it('keeps the same bus after the reset', () => {
    const bus = getBus();
    resetBusForTests();
    expect(getBus()).toBe(bus);
  });

  it('leaves the singleton with no handler', () => {
    getBus().on('openSession', vi.fn());

    resetBusForTests();

    expect(getBus().handlerCount()).toBe(0);
  });
});

describe('the types of the bus', () => {
  it('names the payload of each event', () => {
    expectTypeOf<BusEvents['newSession']>().toEqualTypeOf<{ workspace?: string }>();
    expectTypeOf<BusEvents['openFile']>().toEqualTypeOf<{ path: string; name?: string }>();
    expectTypeOf<BusEvents['interactionResolved']>().toEqualTypeOf<{
      sessionId: string;
      requestId: string;
    }>();
    expectTypeOf<BusEvents['openCommandPalette']>().toEqualTypeOf<{
      mode?: 'commands' | 'notes';
    }>();
    expectTypeOf<BusEvents['clearChat']>().toEqualTypeOf<Record<string, never>>();
  });

  it('gives the handler the payload type of its event', () => {
    const bus = createBus();
    bus.on('openSession', (payload) => {
      expectTypeOf(payload).toEqualTypeOf<{ sessionId: string; title: string }>();
    });
  });

  it('refuses a payload that does not belong to the event', () => {
    const bus = createBus();
    // @ts-expect-error `openFile` demands a `path`, not a `sessionId`.
    bus.emit('openFile', { sessionId: 'sess-1' });
    // @ts-expect-error The bus names no `crucible:new-session` event.
    bus.on('crucible:new-session', () => {});
    // @ts-expect-error `interactionResolved` demands a `requestId` too.
    bus.emit('interactionResolved', { sessionId: 'sess-1' });
  });
});

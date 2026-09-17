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
    bus.on('sessionTitleChanged', handler);

    bus.emit('sessionTitleChanged', { sessionId: 'sess-1', title: 'One' });

    expect(handler).toHaveBeenCalledTimes(1);
    expect(handler).toHaveBeenCalledWith({ sessionId: 'sess-1', title: 'One' });
  });

  it('gives the payload to every handler of that event', () => {
    const bus = createBus();
    const first = vi.fn();
    const second = vi.fn();
    bus.on('interactionResolved', first);
    bus.on('interactionResolved', second);

    bus.emit('interactionResolved', { sessionId: 'sess-1', requestId: 'req-1' });

    expect(first).toHaveBeenCalledWith({ sessionId: 'sess-1', requestId: 'req-1' });
    expect(second).toHaveBeenCalledWith({ sessionId: 'sess-1', requestId: 'req-1' });
  });

  it('keeps an event away from the handler of another event', () => {
    const bus = createBus();
    const handler = vi.fn();
    bus.on('authOk', handler);

    bus.emit('authRequired', {});

    expect(handler).not.toHaveBeenCalled();
  });

  it('stops the handler when the caller runs the answer of on()', () => {
    const bus = createBus();
    const handler = vi.fn();
    const unsubscribe = bus.on('authOk', handler);

    unsubscribe();
    bus.emit('authOk', {});

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
    bus.on('authRequired', kept);
    bus.on('authRequired', removed);

    bus.off('authRequired', removed);
    bus.emit('authRequired', {});

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
    first.on('authOk', handler);

    second.emit('authOk', {});

    expect(handler).not.toHaveBeenCalled();
  });

  it('stops the handler when the Solid owner of the caller disposes', () => {
    const bus = createBus();
    const handler = vi.fn();
    const dispose = createRoot((disposeRoot) => {
      bus.on('authOk', handler);
      return disposeRoot;
    });

    bus.emit('authOk', {});
    dispose();
    bus.emit('authOk', {});

    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('holds no handler after the Solid owner of the caller disposes', () => {
    const bus = createBus();
    const dispose = createRoot((disposeRoot) => {
      bus.on('authOk', vi.fn());
      return disposeRoot;
    });
    expect(bus.handlerCount()).toBe(1);

    dispose();

    // The bookkeeping of the bus must not grow with each mount and unmount.
    expect(bus.handlerCount()).toBe(0);
  });

  it('holds no handler after the caller runs the answer of on()', () => {
    const bus = createBus();
    const unsubscribe = bus.on('authOk', vi.fn());
    expect(bus.handlerCount()).toBe(1);

    unsubscribe();

    expect(bus.handlerCount()).toBe(0);
  });

  it('holds no handler after the caller runs off()', () => {
    const bus = createBus();
    const handler = vi.fn();
    bus.on('authOk', handler);

    bus.off('authOk', handler);

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
    expect(() => bus.emit('authRequired', {})).not.toThrow();
  });

  it('gives the payload to the later handler when an earlier one throws', () => {
    const bus = createBus();
    const reported = vi.spyOn(console, 'error').mockImplementation(() => {});
    const later = vi.fn();
    bus.on('authOk', () => {
      throw new Error('the handler failed');
    });
    bus.on('authOk', later);

    expect(() => bus.emit('authOk', {})).not.toThrow();

    expect(later).toHaveBeenCalledTimes(1);
    expect(reported).toHaveBeenCalled();
    reported.mockRestore();
  });

  it('removes every handler of every event on clear()', () => {
    const bus = createBus();
    const handler = vi.fn();
    bus.on('authOk', handler);
    bus.on('authRequired', vi.fn());
    expect(bus.handlerCount()).toBe(2);

    bus.clear();
    bus.emit('authOk', {});

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
    getBus().on('sessionTitleChanged', handler);

    resetBusForTests();
    getBus().emit('sessionTitleChanged', { sessionId: 'sess-1', title: 'One' });

    expect(handler).not.toHaveBeenCalled();
  });

  it('keeps the same bus after the reset', () => {
    const bus = getBus();
    resetBusForTests();
    expect(getBus()).toBe(bus);
  });

  it('leaves the singleton with no handler', () => {
    getBus().on('sessionTitleChanged', vi.fn());

    resetBusForTests();

    expect(getBus().handlerCount()).toBe(0);
  });
});

describe('the types of the bus', () => {
  it('names the payload of each event', () => {
    expectTypeOf<BusEvents['interactionResolved']>().toEqualTypeOf<{
      sessionId: string;
      requestId: string;
    }>();
    expectTypeOf<BusEvents['sessionTitleChanged']>().toEqualTypeOf<{
      sessionId: string;
      title: string;
    }>();
    expectTypeOf<BusEvents['newSession']>().toEqualTypeOf<{ workspace?: string }>();
    expectTypeOf<BusEvents['openSession']>().toEqualTypeOf<{
      sessionId: string;
      title: string;
    }>();
    expectTypeOf<BusEvents['authOk']>().toEqualTypeOf<Record<string, never>>();
    expectTypeOf<BusEvents['authRequired']>().toEqualTypeOf<Record<string, never>>();
  });

  it('delivers a session-opening payload to its handler', () => {
    const bus = createBus();
    const newPayloads: BusEvents['newSession'][] = [];
    const openPayloads: BusEvents['openSession'][] = [];
    bus.on('newSession', (payload) => newPayloads.push(payload));
    bus.on('openSession', (payload) => openPayloads.push(payload));

    bus.emit('newSession', { workspace: '/home/me/atlas' });
    bus.emit('newSession', {});
    bus.emit('openSession', { sessionId: 's1', title: 'One' });

    expect(newPayloads).toEqual([{ workspace: '/home/me/atlas' }, {}]);
    expect(openPayloads).toEqual([{ sessionId: 's1', title: 'One' }]);
  });

  it('delivers a file/settings payload to its handler', () => {
    const bus = createBus();
    const files: BusEvents['openFile'][] = [];
    const palettes: BusEvents['openCommandPalette'][] = [];
    let settings = 0;
    bus.on('openFile', (payload) => files.push(payload));
    bus.on('openCommandPalette', (payload) => palettes.push(payload));
    bus.on('openSettings', () => { settings += 1; });

    bus.emit('openFile', { path: '/kiln/notes/linker.md', name: 'linker' });
    bus.emit('openFile', { path: '/kiln/notes/plain.md' });
    bus.emit('openCommandPalette', { mode: 'notes' });
    bus.emit('openCommandPalette', {});
    bus.emit('openSettings', {});
    expect(files).toEqual([
      { path: '/kiln/notes/linker.md', name: 'linker' },
      { path: '/kiln/notes/plain.md' },
    ]);
    expect(palettes).toEqual([{ mode: 'notes' }, {}]);
    expect(settings).toBe(1);
  });


  it('delivers a chat-input payload to its handler', () => {
    const bus = createBus();
    const seen: string[] = [];
    bus.on('clearChat', () => seen.push('clearChat'));
    bus.on('switchModel', () => seen.push('switchModel'));
    bus.on('toggleHiddenFiles', () => seen.push('toggleHiddenFiles'));

    bus.emit('clearChat', {});
    bus.emit('switchModel', {});
    bus.emit('toggleHiddenFiles', {});

    expect(seen).toEqual(['clearChat', 'switchModel', 'toggleHiddenFiles']);
  });

  it('delivers a focus payload to its handler', () => {
    const bus = createBus();
    const seen: string[] = [];
    bus.on('focusSearch', () => seen.push('focusSearch'));
    bus.on('focusSessionSearch', () => seen.push('focusSessionSearch'));

    bus.emit('focusSearch', {});
    bus.emit('focusSessionSearch', {});

    expect(seen).toEqual(['focusSearch', 'focusSessionSearch']);
  });

  it('gives the handler the payload type of its event', () => {
    const bus = createBus();
    bus.on('sessionTitleChanged', (payload) => {
      expectTypeOf(payload).toEqualTypeOf<{ sessionId: string; title: string }>();
    });
  });

  it('refuses a payload that does not belong to the event', () => {
    const bus = createBus();
    // @ts-expect-error `sessionTitleChanged` demands a `title`, not a `path`.
    bus.emit('sessionTitleChanged', { sessionId: 'sess-1', path: '/a.md' });
    // @ts-expect-error The bus names no `crucible:new-session` event.
    bus.on('crucible:new-session', () => {});
    // @ts-expect-error `openFile` demands a `path`, not a `name` alone.
    bus.emit('openFile', { name: 'no path' });
    // @ts-expect-error `interactionResolved` demands a `requestId` too.
    bus.emit('interactionResolved', { sessionId: 'sess-1' });
  });
});

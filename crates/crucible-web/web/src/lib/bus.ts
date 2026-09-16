import { createRoot, getOwner, onCleanup } from 'solid-js';
import { createEmitter, type Emitter } from '@solid-primitives/event-bus';

/**
 * The payload of each typed event.
 *
 * Four events, and they are the DATA events: `authOk` and `authRequired` from
 * the API client, `interactionResolved` from the interactions cache, and
 * `sessionTitleChanged` from the session stream. Each one reports that
 * something on the server changed, and each one is emitted by a module that no
 * component owns.
 *
 * The other eleven `crucible:*` window CustomEvents of the Part B2 table stay
 * on `window`. They are UI COMMANDS — open the palette, open a file, clear the
 * chat — and the browser specs drive them through `page.evaluate`, which
 * reaches `window.dispatchEvent` and cannot reach a module singleton. They move
 * here when a bridge exists that gives a spec the bus.
 *
 * `Record<string, never>` marks an event that carries no data. The window
 * CustomEvent carried no `detail` either, so a caller must write `{}`.
 */
export type BusEvents = {
  authOk: Record<string, never>;
  authRequired: Record<string, never>;
  interactionResolved: { sessionId: string; requestId: string };
  sessionTitleChanged: { sessionId: string; title: string };
};

/** A handler of one event. The type of its payload comes from `BusEvents`. */
type BusHandler<K extends keyof BusEvents> = (payload: BusEvents[K]) => void;

/** The typed replacement of `window.dispatchEvent` and `window.addEventListener`. */
export type Bus = {
  /**
   * Adds a handler for one event. The answer removes that handler again. A
   * caller inside a Solid owner also loses the handler when the owner
   * disposes, so a component needs no `onCleanup` of its own.
   *
   * A second call with the same event and the same handler adds nothing and
   * answers the first removal, the way `addEventListener` refuses a duplicate.
   */
  on<K extends keyof BusEvents>(event: K, handler: BusHandler<K>): () => void;
  /**
   * Gives the payload to every handler of that event, in the order they
   * arrived. An event with no handler is not an error. A handler that throws
   * does not stop the handlers after it; the bus writes that error to the
   * console, as the browser did for a `window` listener.
   */
  emit<K extends keyof BusEvents>(event: K, payload: BusEvents[K]): void;
  /** Removes a handler that `on` added. A handler it does not hold is not an error. */
  off<K extends keyof BusEvents>(event: K, handler: BusHandler<K>): void;
  /** Removes every handler of every event. The test seam calls it. */
  clear(): void;
  /**
   * Counts the handlers the bus holds, over every event. Only a test reads it,
   * to prove that a removal prunes the bookkeeping and leaves nothing behind.
   */
  handlerCount(): number;
};

/** The removal function of each handler, per event. `off` and `clear` read it. */
type Registry = Map<keyof BusEvents, Map<BusHandler<never>, () => void>>;

/**
 * Builds one bus. The app shares the singleton below; a test that must not
 * touch the singleton builds its own.
 *
 * `createEmitter` registers a cleanup at the moment it runs, so a `createRoot`
 * gives it an owner. Without that owner Solid writes a warning to the console
 * for a bus the app never disposes.
 *
 * `createEmitter` answers `{ on, emit, clear }` and no `off`, so this function
 * keeps the removal function of each handler. The removal is the one owner of
 * the bookkeeping: it leaves the emitter AND prunes the map, and `on` gives it
 * to `onCleanup` itself. The emitter binds a removal of its own to the same
 * owner, but that one knows only the emitter. To let it run alone would empty
 * the emitter and keep this map full for the life of the app.
 */
export function createBus(): Bus {
  const emitter: Emitter<BusEvents> = createRoot(() => createEmitter<BusEvents>());
  const registry: Registry = new Map();

  function on<K extends keyof BusEvents>(event: K, handler: BusHandler<K>): () => void {
    const held = registry.get(event)?.get(handler as BusHandler<never>);
    if (held) return held;

    // The emitter calls the handlers of one event in a loop. A handler that
    // throws into that loop hides every handler after it, so each one runs
    // behind this guard.
    const guarded = (payload: BusEvents[K]) => {
      try {
        handler(payload);
      } catch (error) {
        console.error(`[bus] the handler of "${String(event)}" threw`, error);
      }
    };

    const unsubscribe = emitter.on(event, guarded);
    let done = false;
    const remove = () => {
      if (done) return;
      done = true;
      unsubscribe();
      const handlers = registry.get(event);
      handlers?.delete(handler as BusHandler<never>);
      if (handlers?.size === 0) registry.delete(event);
    };

    let handlers = registry.get(event);
    if (!handlers) registry.set(event, (handlers = new Map()));
    handlers.set(handler as BusHandler<never>, remove);

    if (getOwner()) onCleanup(remove);
    return remove;
  }

  return {
    on,
    emit<K extends keyof BusEvents>(event: K, payload: BusEvents[K]): void {
      emitter.emit(event, payload);
    },
    off<K extends keyof BusEvents>(event: K, handler: BusHandler<K>): void {
      registry.get(event)?.get(handler as BusHandler<never>)?.();
    },
    clear(): void {
      emitter.clear();
      registry.clear();
    },
    handlerCount(): number {
      let count = 0;
      for (const handlers of registry.values()) count += handlers.size;
      return count;
    },
  };
}

/**
 * The module singleton. Every emitter and every listener of the four events
 * above reads this one bus, the way they all read one `window` before.
 */
const moduleBus = createBus();

/** Answers the one bus the app shares. */
export function getBus(): Bus {
  return moduleBus;
}

/**
 * The test seam. It removes every handler the singleton holds, so a handler of
 * one test cannot answer an event of the next test. Production code never
 * calls it.
 */
export function resetBusForTests(): void {
  moduleBus.clear();
}

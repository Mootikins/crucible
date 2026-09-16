import { createRoot } from 'solid-js';
import { createEmitter, type Emitter } from '@solid-primitives/event-bus';

/**
 * The payload of each typed event. The 15 events replace the 15 `crucible:*`
 * window CustomEvents of the Part B2 table; the name of each one is the
 * camelCase form of the window event it replaces.
 *
 * `Record<string, never>` marks an event that carries no data. The window
 * CustomEvent carried no `detail` either, so a caller must write `{}`.
 */
export type BusEvents = {
  authOk: Record<string, never>;
  authRequired: Record<string, never>;
  clearChat: Record<string, never>;
  exportSession: Record<string, never>;
  focusSearch: Record<string, never>;
  focusSessionSearch: Record<string, never>;
  interactionResolved: { sessionId: string; requestId: string };
  newSession: { workspace?: string };
  openCommandPalette: { mode?: 'commands' | 'notes' };
  openFile: { path: string; name?: string };
  openSession: { sessionId: string; title: string };
  openSettings: Record<string, never>;
  sessionTitleChanged: { sessionId: string; title: string };
  switchModel: Record<string, never>;
  toggleHiddenFiles: Record<string, never>;
};

/** A handler of one event. The type of its payload comes from `BusEvents`. */
export type BusHandler<K extends keyof BusEvents> = (payload: BusEvents[K]) => void;

/** The typed replacement of `window.dispatchEvent` and `window.addEventListener`. */
export type Bus = {
  /**
   * Adds a handler for one event. The answer removes that handler again. A
   * caller inside a Solid owner also loses the handler when the owner
   * disposes, so a component needs no `onCleanup` of its own.
   */
  on<K extends keyof BusEvents>(event: K, handler: BusHandler<K>): () => void;
  /** Gives the payload to every handler of that event, in the order they arrived. */
  emit<K extends keyof BusEvents>(event: K, payload: BusEvents[K]): void;
  /** Removes a handler that `on` added. A handler it does not hold is not an error. */
  off<K extends keyof BusEvents>(event: K, handler: BusHandler<K>): void;
  /** Removes every handler of every event. The test seam calls it. */
  clear(): void;
};

/** The unsubscribe function `on` answered, per event and per handler. */
type Unsubscribers = Map<keyof BusEvents, Map<BusHandler<never>, () => void>>;

/**
 * Builds one bus. The app shares the singleton below; a test that must not
 * touch the singleton builds its own.
 *
 * `createEmitter` registers a cleanup at the moment it runs, so a `createRoot`
 * gives it an owner. Without that owner Solid writes a warning to the console
 * for a bus the app never disposes.
 *
 * `createEmitter` answers `{ on, emit, clear }` and no `off`, so this function
 * keeps the unsubscribe function of each handler and `off` runs it.
 */
export function createBus(): Bus {
  const emitter: Emitter<BusEvents> = createRoot(() => createEmitter<BusEvents>());
  const unsubscribers: Unsubscribers = new Map();

  function forget<K extends keyof BusEvents>(event: K, handler: BusHandler<K>): void {
    const perHandler = unsubscribers.get(event);
    if (!perHandler) return;
    perHandler.delete(handler as BusHandler<never>);
    if (perHandler.size === 0) unsubscribers.delete(event);
  }

  return {
    on<K extends keyof BusEvents>(event: K, handler: BusHandler<K>): () => void {
      // `createEmitter` binds the unsubscribe to the owner of this caller.
      const unsubscribe = emitter.on(event, handler);
      const remove = () => {
        unsubscribe();
        forget(event, handler);
      };
      let perHandler = unsubscribers.get(event);
      if (!perHandler) unsubscribers.set(event, (perHandler = new Map()));
      perHandler.set(handler as BusHandler<never>, remove);
      return remove;
    },
    emit<K extends keyof BusEvents>(event: K, payload: BusEvents[K]): void {
      emitter.emit(event, payload);
    },
    off<K extends keyof BusEvents>(event: K, handler: BusHandler<K>): void {
      unsubscribers.get(event)?.get(handler as BusHandler<never>)?.();
    },
    clear(): void {
      emitter.clear();
      unsubscribers.clear();
    },
  };
}

/**
 * The module singleton. Every dispatch site and every listener site of the
 * Part B2 table reads this one bus, the way they all read one `window` before.
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

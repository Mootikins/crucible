/**
 * One shared root per server-sent-event stream.
 *
 * Four streams reach the browser: a session's chat events, surface changes,
 * filesystem changes, and system events (publications and proposals).
 * Before this module each consumer opened its own `EventSource`, so two
 * panes on one session held two streams and the review store carried a
 * hand-written refcount to stop a third. Here every consumer of a stream
 * shares one root: the first `subscribe` joins it, the last unsubscribe
 * leaves it one microtask later, and a `subscribe` after that leave joins
 * a fresh one.
 *
 * Simplification Plan step 19 moved what a root's `connect` actually opens:
 * the four streams' topics (a session's own id, or `system` for the other
 * three) now travel one physical `EventSource`, `GET /api/events`, owned by
 * `lib/api.ts`. This module still keeps one root per stream (so `surfaceEvents`,
 * `fsEvents` and `systemEvents` do not fight over which one reads a `system`
 * topic frame), it just no longer owns a transport of its own —
 * `api.ts`'s `joinEventsTopic` does, and `reconnectEventsConnection` is the
 * one knob every root's manual `reconnect()` turns.
 *
 * The root also carries ONE hook point per stream, the "route". A route turns
 * an event into a cache write (`setQueryData`, `invalidateQueries`) or a bus
 * message. This module deliberately holds no route of its own: the tasks of
 * Part D install them, so the translation of an event to a query key lives
 * beside the hook that owns that key, not here.
 *
 * Refcount by subscriber, not by reactive owner. The plan names
 * `createSingletonRoot`, whose count is the number of reactive owners that
 * read it. That is the wrong unit here: every consumer holds an unsubscribe
 * function it runs itself, and `ChatContext` runs it to rebind the same pane
 * to another session, with no owner going away to be counted out. Such a count
 * would therefore never fall to zero and no source would ever close, so this
 * module counts subscribers and owns one detached `createRoot` per stream
 * instead.
 */
import { createRoot, createSignal, onCleanup, type Accessor } from 'solid-js';
import type { QueryClient } from '@tanstack/solid-query';
import {
  reconnectEventsConnection,
  resetEventsConnectionForTests,
  subscribeToEvents,
  subscribeToFsEvents,
  subscribeToSurfaceEvents,
  subscribeToSystemEvents,
  type SurfaceChangedEvent,
  type SystemEvent,
} from '@/lib/api';
import type { ChatEvent, FsEvent, SequencedChatEvent } from '@/lib/types';
import { getBus, type Bus } from '@/lib/bus';
import { sessionNotifications } from '@/lib/query/daemon-notification';
import { getQueryClient } from './client';

// =============================================================================
// The shape a consumer sees
// =============================================================================

/** What one stream gives every consumer of it. */
export interface SseStream<E> {
  /**
   * Adds a handler, and opens the stream when it is the first one. The answer
   * removes that handler again. When it was the last, the stream closes one
   * microtask later, unless a new subscriber joins before then.
   *
   * `onOpen` fires once, the first time this subscriber sees an open stream.
   * A consumer that joins a stream that is open already gets it at once. A
   * consumer that joins while the stream is down waits for the next open, so
   * an open callback never announces a source that cannot carry events.
   */
  subscribe(handler: (event: E) => void, onOpen?: () => void): () => void;
  /**
   * Closes the source and opens a new one, keeping every subscriber and the
   * route. It is the manual retry: the backoff of a stream that dropped can
   * stand at 30 seconds, and a user who asks to reconnect must not wait it
   * out. The new source starts the backoff again from the first step.
   */
  reconnect(): void;
  /** The event that arrived last, or undefined before the first one. */
  latest: Accessor<E | undefined>;
}

// =============================================================================
// The hook point Part D fills
// =============================================================================

/** The two stores a route writes to. It takes them, so a test can give its own. */
export interface SseRouteContext {
  client: QueryClient;
  bus: Bus;
}

/** The context of the chat stream also names the session the events belong to. */
export interface SessionRouteContext extends SseRouteContext {
  sessionId: string;
}

/** The event and the route context of each stream, by the name of the stream. */
interface StreamRouteTypes {
  session: { event: ChatEvent; context: SessionRouteContext };
  surface: { event: SurfaceChangedEvent; context: SseRouteContext };
  fs: { event: FsEvent; context: SseRouteContext };
  system: { event: SystemEvent; context: SseRouteContext };
}

/** The name of one of the four streams. */
export type StreamName = keyof StreamRouteTypes;

/** Turns one event of the stream `S` into cache writes or bus messages. */
export type EventRoute<S extends StreamName> = (
  event: StreamRouteTypes[S]['event'],
  context: StreamRouteTypes[S]['context'],
) => void;

/**
 * Recovers what the stream could have missed. It runs at each open and at
 * each gap, before the open reaches a subscriber.
 */
export type Reconcile = (context: SseRouteContext) => void;

/** The two hooks of one stream. */
interface StreamHooks<S extends StreamName> {
  route: EventRoute<S> | null;
  reconcile: Reconcile | null;
}

type HookTable = { [S in StreamName]: StreamHooks<S> };

function emptyHooks(): HookTable {
  return {
    session: { route: null, reconcile: null },
    surface: { route: null, reconcile: null },
    fs: { route: null, reconcile: null },
    system: { route: null, reconcile: null },
  };
}

let hooks: HookTable = emptyHooks();

/**
 * Names the route and the reconcile of one stream. `null` removes the one
 * that is there.
 */
export function setEventRoute<S extends StreamName>(
  stream: S,
  route: EventRoute<S> | null,
  reconcile: Reconcile | null = null,
): void {
  (hooks as { [K in StreamName]: StreamHooks<K> })[stream] = { route, reconcile } as HookTable[S];
}

/** The two stores as they are now. A route reads the injected client in a test. */
function routeContext(): SseRouteContext {
  return { client: getQueryClient(), bus: getBus() };
}

/**
 * Runs one route. A route that throws must not stop the handlers behind it:
 * a broken translation of one event type would otherwise end the chat stream
 * of a running turn.
 */
function runRoute<S extends StreamName>(
  name: string,
  stream: S,
  event: StreamRouteTypes[S]['event'],
  context: StreamRouteTypes[S]['context'],
): void {
  const route = (hooks as { [K in StreamName]: StreamHooks<K> })[stream].route;
  if (!route) return;
  try {
    route(event, context);
  } catch (error) {
    console.warn(`SSE route failed (${name}):`, error);
  }
}

// =============================================================================
// The root
// =============================================================================

/** Opens the source, and answers the function that closes it. */
type Connect<E> = (onEvent: (event: E) => void, onOpen: () => void, onGap: () => void, onDisconnect: () => void) => () => void;

/** What one stream needs to run. */
interface StreamSpec<E> {
  /** Names the stream in a warning. */
  name: string;
  connect: Connect<E>;
  route: (event: E) => void;
  reconcile?: () => void;
  /**
   * Reads the transport state out of an event: true for open, false for down,
   * undefined for an event that says nothing about it. The chat stream carries
   * such an event (`connection`); the three others do not, and leave it out.
   */
  openState?: (event: E) => boolean | undefined;
  /**
   * Forces a hard reconnect of the underlying transport, in place of the
   * default close-then-`connect()` dance. The four streams share ONE
   * connection now (`GET /api/events`, Simplification Plan step 19): a manual
   * reconnect of any one of them must rebuild that shared connection for
   * every topic it carries, not close and reopen only this stream's own
   * topic subscription.
   */
  forceReconnect?: () => void;
}

/** One handler, the open callback that arrived with it, and whether it ran. */
interface Subscriber<E> {
  handler: (event: E) => void;
  onOpen?: () => void;
  openDelivered: boolean;
}

/** Closes every live root. The reset seam runs them. */
const liveRoots = new Set<() => void>();

function createStream<E>(spec: StreamSpec<E>, dispose: () => void): SseStream<E> {
  const subscribers = new Set<Subscriber<E>>();
  const [latest, setLatest] = createSignal<E | undefined>(undefined);
  let close: (() => void) | null = null;
  let open = false;
  let disposed = false;

  onCleanup(() => {
    disposed = true;
    close?.();
    close = null;
    open = false;
  });

  function deliver(event: E): void {
    setLatest(() => event);
    const state = spec.openState?.(event);
    if (state === true) announceOpen();
    else if (state === false) open = false;
    spec.route(event);
    // A copy, because a handler may unsubscribe itself while the loop runs.
    for (const subscriber of [...subscribers]) {
      // One handler that throws must not cost the handlers behind it their
      // event: three panes share this loop, and two of them are innocent.
      try {
        subscriber.handler(event);
      } catch (error) {
        console.warn(`SSE handler failed (${spec.name}):`, error);
      }
    }
  }

  /** Runs one open callback, and runs it once whichever path reaches it. */
  function deliverOpen(subscriber: Subscriber<E>): void {
    if (!subscriber.onOpen || subscriber.openDelivered) return;
    subscriber.openDelivered = true;
    try {
      subscriber.onOpen();
    } catch (error) {
      console.warn(`SSE open callback failed (${spec.name}):`, error);
    }
  }

  /**
   * Closes the stream one microtask later, if no subscriber came back.
   *
   * A split of a pane unmounts its chat and mounts it again in one
   * synchronous batch. The only subscriber leaves and comes back in that
   * batch. A close at once would drop the source, and the new subscriber
   * would open a second request with `?after=`. The wait lets the stream
   * outlive that batch; `createSingletonRoot` uses the same microtask.
   */
  function closeWhenStillUnused(): void {
    queueMicrotask(() => {
      if (!disposed && subscribers.size === 0) dispose();
    });
  }

  function reconcile(): void {
    try {
      spec.reconcile?.();
    } catch (error) {
      console.warn(`SSE reconciliation failed (${spec.name}):`, error);
    }
  }

  function announceOpen(): void {
    reconcile();
    open = true;
    for (const subscriber of [...subscribers]) deliverOpen(subscriber);
  }

  return {
    latest,

    subscribe(handler, onOpen) {
      const subscriber: Subscriber<E> = { handler, onOpen, openDelivered: false };
      subscribers.add(subscriber);
      // The connect may open at once, which announces to this subscriber too;
      // `deliverOpen` then does nothing on the line below.
      if (!close) close = spec.connect(deliver, announceOpen, reconcile, () => { open = false; });
      if (open) deliverOpen(subscriber);
      let live = true;
      return () => {
        if (!live) return;
        live = false;
        subscribers.delete(subscriber);
        if (subscribers.size === 0) closeWhenStillUnused();
      };
    },

    reconnect() {
      // Nobody subscribes, so there is no source to reissue. The next
      // `subscribe` opens one.
      if (!close) return;
      if (spec.forceReconnect) {
        // The topic join itself (`close`) stays open; only the shared
        // transport underneath it rebuilds, which is every topic's manual
        // retry at once, and starts the backoff again at its first step.
        open = false;
        spec.forceReconnect();
        return;
      }
      close();
      open = false;
      // A new connect, not a reopen of the old one: the backoff of each
      // stream lives in the closure `connect` builds, so a new closure starts
      // the wait again at its first step.
      close = spec.connect(deliver, announceOpen, reconcile, () => { open = false; });
    },
  };
}

/**
 * Answers the one stream of a key, and builds it when there is none.
 *
 * The root is detached (`createRoot(fn, null)`). Without that it would belong
 * to the owner of whichever consumer asked for the stream first, and that
 * component going away would close the source under every other consumer.
 */
function rootFor<E>(
  roots: Map<string, SseStream<E>>,
  key: string,
  spec: StreamSpec<E>,
): SseStream<E> {
  const existing = roots.get(key);
  if (existing) return existing;

  let disposeRoot: () => void = () => {};
  const stream = createRoot<SseStream<E>>((dispose) => {
    disposeRoot = dispose;
    onCleanup(() => {
      roots.delete(key);
      liveRoots.delete(dispose);
    });
    return createStream(spec, dispose);
  }, null);

  roots.set(key, stream);
  liveRoots.add(disposeRoot);
  return stream;
}

// =============================================================================
// The four streams
// =============================================================================

/** The open roots of each stream, by key. */
const roots = {
  session: new Map<string, SseStream<SequencedChatEvent>>(),
  surface: new Map<string, SseStream<SurfaceChangedEvent>>(),
  fs: new Map<string, SseStream<FsEvent>>(),
  system: new Map<string, SseStream<SystemEvent>>(),
} as const satisfies Record<StreamName, Map<string, unknown>>;

/** Runs the reconcile of one stream with the stores as they are now. */
function reconcileStream(stream: StreamName): void {
  hooks[stream].reconcile?.(routeContext());
}

/** The key of a stream the whole app shares, which has no id to key on. */
const GLOBAL = 'global';

// =============================================================================
// The resume cursor
// =============================================================================

/**
 * The last seq APPLIED for a session, per session.
 *
 * This is the number a reopened stream states as `?after=` and the server
 * replays past. It advances ONLY after an event (or a history fold) has been
 * applied to the session's store — never on receipt, because a frame received
 * and then dropped before its apply would otherwise be skipped by the replay
 * as well and lost twice (T3 Code's rule).
 */
const sessionCursors = new Map<string, number>();

/** The session's resume cursor, or undefined before anything was applied. */
export function sessionCursor(sessionId: string): number | undefined {
  return sessionCursors.get(sessionId);
}

/** Advances the cursor, monotonically. A lower seq never moves it back. */
export function advanceSessionCursor(sessionId: string, seq: number): void {
  const current = sessionCursors.get(sessionId);
  if (current === undefined || seq > current) {
    sessionCursors.set(sessionId, seq);
  }
}

/**
 * The chat events of one session — the session's own topic of the shared
 * connection (`GET /api/events`, Simplification Plan step 19).
 *
 * One join per session id: two panes on one session share it, and a pane on
 * another session joins its own. All the topics a running app has joined
 * travel one physical `EventSource`; joining or leaving one rebuilds it.
 */
export function sessionEvents(sessionId: string): SseStream<SequencedChatEvent> {
  const name = `chat events ${sessionId}`;
  return rootFor(roots.session, sessionId, {
    name,
    // The open reaches the root only through the `connection` event and
    // `openState` below, so one open is announced once.
    connect: (onEvent) => {
      // The stream owns the snapshot of its notifications. The notifications
      // read the `connection` and `stream_gap` events themselves, so the
      // snapshot comes after the subscription opens, and a reopen or a gap
      // reads it again.
      const notifications = sessionNotifications(sessionId);
      const close = subscribeToEvents(sessionId, (event) => {
        notifications.event(event);
        onEvent(event);
      }, () => sessionCursor(sessionId));
      return () => {
        close();
        notifications.dispose();
      };
    },
    reconcile: () => reconcileStream('session'),
    route: (event) => runRoute(name, 'session', event, { ...routeContext(), sessionId }),
    // `subscribeToEvents` reports the transport through this event: it sends
    // `reconnecting` from its error handler and `connected` from its open
    // handler. Without reading it the stream would look open through a drop.
    openState: (event) =>
      'type' in event && event.type === 'connection' ? event.status === 'connected' : undefined,
    forceReconnect: reconnectEventsConnection,
  });
}

/** The surface changes of every plugin — the `system` topic of the shared
 * connection. */
export function surfaceEvents(): SseStream<SurfaceChangedEvent> {
  return rootFor(roots.surface, GLOBAL, {
    name: 'surface events',
    connect: subscribeToSurfaceEvents,
    reconcile: () => reconcileStream('surface'),
    route: (event) => runRoute('surface events', 'surface', event, routeContext()),
    forceReconnect: reconnectEventsConnection,
  });
}

/** The filesystem changes of every watched root — the `system` topic of the
 * shared connection. */
export function fsEvents(): SseStream<FsEvent> {
  return rootFor(roots.fs, GLOBAL, {
    name: 'fs events',
    connect: subscribeToFsEvents,
    reconcile: () => reconcileStream('fs'),
    route: (event) => runRoute('fs events', 'fs', event, routeContext()),
    forceReconnect: reconnectEventsConnection,
  });
}

/**
 * The daemon's system session: plugin publications and proposal changes, the
 * `system` topic of the shared connection. A proposal belongs to no user
 * session, so only this stream carries it.
 */
export function systemEvents(): SseStream<SystemEvent> {
  return rootFor(roots.system, GLOBAL, {
    name: 'system events',
    connect: subscribeToSystemEvents,
    reconcile: () => reconcileStream('system'),
    route: (event) => runRoute('system events', 'system', event, routeContext()),
    forceReconnect: reconnectEventsConnection,
  });
}

// =============================================================================
// The test seam
// =============================================================================

/**
 * Closes every stream and forgets every route.
 *
 * A test calls it between cases, so a source of one case cannot answer the
 * next one. Production code never calls it.
 */
export function resetSseForTests(): void {
  for (const dispose of [...liveRoots]) dispose();
  liveRoots.clear();
  for (const perKey of Object.values(roots)) perKey.clear();
  sessionCursors.clear();
  hooks = emptyHooks();
  // Disposing every root above already leaves each of its topics the normal
  // way; this also forgets one joined directly (a test that called
  // `subscribeToEvents` and friends without going through a root).
  resetEventsConnectionForTests();
}

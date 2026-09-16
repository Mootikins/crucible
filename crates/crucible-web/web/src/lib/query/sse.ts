/**
 * One shared root per server-sent-event stream.
 *
 * Four streams reach the browser: a session's chat events, surface changes,
 * filesystem changes, and plugin publications. Before this module each
 * consumer opened its own `EventSource`, so two panes on one session held two
 * streams and `review-store.ts` carried a hand-written refcount to stop a
 * third. Here every consumer of a stream shares one source: the first
 * `subscribe` opens it, the last unsubscribe closes it, and a later
 * `subscribe` opens a fresh one.
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
  subscribeToEvents,
  subscribeToFsEvents,
  subscribeToSurfaceEvents,
  type SurfaceChangedEvent,
} from '@/lib/api';
import type { ChatEvent, FsEvent } from '@/lib/types';
import { getBus, type Bus } from '@/lib/bus';
import { getQueryClient } from './client';

// =============================================================================
// The shape a consumer sees
// =============================================================================

/** What one stream gives every consumer of it. */
export interface SseStream<E> {
  /**
   * Adds a handler, and opens the stream when it is the first one. The answer
   * removes that handler again, and closes the stream when it was the last.
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

/** What `pluginEvents` gives, where an event names a plugin and a key. */
export interface PluginEventStream {
  subscribe(handler: (plugin: string, key: string) => void, onOpen?: () => void): () => void;
  reconnect(): void;
  latest: Accessor<PluginPublicationEvent | undefined>;
}

/** One plugin published a new value for one key (`publication_changed`). */
export interface PluginPublicationEvent {
  plugin: string;
  key: string;
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

export type SessionEventRoute = (event: ChatEvent, context: SessionRouteContext) => void;
export type SurfaceEventRoute = (event: SurfaceChangedEvent, context: SseRouteContext) => void;
export type FsEventRoute = (event: FsEvent, context: SseRouteContext) => void;
export type PluginEventRoute = (event: PluginPublicationEvent, context: SseRouteContext) => void;

let sessionRoute: SessionEventRoute | null = null;
let surfaceRoute: SurfaceEventRoute | null = null;
let fsRoute: FsEventRoute | null = null;
let pluginRoute: PluginEventRoute | null = null;

/** Names the route of the chat stream. `null` removes the one that is there. */
export function setSessionEventRoute(route: SessionEventRoute | null): void {
  sessionRoute = route;
}

/** Names the route of the surface stream. */
export function setSurfaceEventRoute(route: SurfaceEventRoute | null): void {
  surfaceRoute = route;
}

/** Names the route of the filesystem stream. */
export function setFsEventRoute(route: FsEventRoute | null): void {
  fsRoute = route;
}

/** Names the route of the plugin stream. */
export function setPluginEventRoute(route: PluginEventRoute | null): void {
  pluginRoute = route;
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
function runRoute<E, C>(
  name: string,
  route: ((event: E, context: C) => void) | null,
  event: E,
  context: C,
): void {
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
type Connect<E> = (onEvent: (event: E) => void, onOpen: () => void) => () => void;

/** What one stream needs to run. */
interface StreamSpec<E> {
  /** Names the stream in a warning. */
  name: string;
  connect: Connect<E>;
  route: (event: E) => void;
  /**
   * Reads the transport state out of an event: true for open, false for down,
   * undefined for an event that says nothing about it. The chat stream carries
   * such an event (`connection`); the three others do not, and leave it out.
   */
  openState?: (event: E) => boolean | undefined;
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

  onCleanup(() => {
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

  function announceOpen(): void {
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
      if (!close) close = spec.connect(deliver, announceOpen);
      if (open) deliverOpen(subscriber);
      let live = true;
      return () => {
        if (!live) return;
        live = false;
        subscribers.delete(subscriber);
        if (subscribers.size === 0) dispose();
      };
    },

    reconnect() {
      // Nobody subscribes, so there is no source to reissue. The next
      // `subscribe` opens one.
      if (!close) return;
      close();
      open = false;
      // A new connect, not a reopen of the old one: the backoff of each
      // stream lives in the closure `connect` builds, so a new closure starts
      // the wait again at its first step.
      close = spec.connect(deliver, announceOpen);
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

const sessionRoots = new Map<string, SseStream<ChatEvent>>();
const surfaceRoots = new Map<string, SseStream<SurfaceChangedEvent>>();
const fsRoots = new Map<string, SseStream<FsEvent>>();
const pluginRoots = new Map<string, SseStream<PluginPublicationEvent>>();

/** The key of a stream the whole app shares, which has no id to key on. */
const GLOBAL = 'global';

/**
 * The chat events of one session (`GET /api/chat/events/{id}`).
 *
 * One source per session id: two panes on one session share it, and a pane on
 * another session opens its own.
 */
export function sessionEvents(sessionId: string): SseStream<ChatEvent> {
  return rootFor(sessionRoots, sessionId, {
    name: `chat events ${sessionId}`,
    connect: (onEvent, onOpen) => subscribeToEvents(sessionId, onEvent, onOpen),
    route: (event) =>
      runRoute(`chat events ${sessionId}`, sessionRoute, event, {
        ...routeContext(),
        sessionId,
      }),
    // `subscribeToEvents` reports the transport through this event: it sends
    // `reconnecting` from its error handler and `connected` from its open
    // handler. Without reading it the stream would look open through a drop.
    openState: (event) => (event.type === 'connection' ? event.status === 'connected' : undefined),
  });
}

/** The surface changes of every plugin (`GET /api/surfaces/events`). */
export function surfaceEvents(): SseStream<SurfaceChangedEvent> {
  return rootFor(surfaceRoots, GLOBAL, {
    name: 'surface events',
    connect: (onEvent) => subscribeToSurfaceEvents(onEvent),
    route: (event) => runRoute('surface events', surfaceRoute, event, routeContext()),
  });
}

/** The filesystem changes of every watched root (`GET /api/fs/events`). */
export function fsEvents(): SseStream<FsEvent> {
  return rootFor(fsRoots, GLOBAL, {
    name: 'fs events',
    connect: (onEvent) => subscribeToFsEvents(onEvent),
    route: (event) => runRoute('fs events', fsRoute, event, routeContext()),
  });
}

/** The URL of the plugin stream. `api.ts` has no function for it yet. */
const PLUGIN_EVENTS_URL = '/api/plugins/events';

/**
 * Opens the plugin stream.
 *
 * It does not reconnect on an error, which is what the stream does today: the
 * three other streams back off and retry inside `api.ts`, and Task D4 moves
 * this one's consumer here without changing that. A consumer that must get
 * back on calls `reconnect()`.
 */
function connectPluginEvents(
  onEvent: (event: PluginPublicationEvent) => void,
  onOpen: () => void,
): () => void {
  const source = new EventSource(PLUGIN_EVENTS_URL);
  source.addEventListener('publication_changed', (e: MessageEvent) => {
    try {
      const { plugin, key } = JSON.parse(e.data) as PluginPublicationEvent;
      onEvent({ plugin, key });
    } catch {
      // A malformed frame is not worth tearing the stream down for; the next
      // one will arrive, and a stale block is better than a dead one.
      console.warn('Failed to parse plugin SSE event:', e.data);
    }
  });
  source.onopen = () => onOpen();
  return () => source.close();
}

/** The publications of every plugin (`GET /api/plugins/events`). */
export function pluginEvents(): PluginEventStream {
  const stream = rootFor(pluginRoots, GLOBAL, {
    name: 'plugin events',
    connect: connectPluginEvents,
    route: (event) => runRoute('plugin events', pluginRoute, event, routeContext()),
  });
  return {
    latest: stream.latest,
    reconnect: () => stream.reconnect(),
    subscribe(handler, onOpen) {
      return stream.subscribe((event) => handler(event.plugin, event.key), onOpen);
    },
  };
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
  sessionRoots.clear();
  surfaceRoots.clear();
  fsRoots.clear();
  pluginRoots.clear();
  sessionRoute = null;
  surfaceRoute = null;
  fsRoute = null;
  pluginRoute = null;
}

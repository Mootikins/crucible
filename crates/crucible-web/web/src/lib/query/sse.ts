/**
 * One singleton root per server-sent-event stream.
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
 * Refcount by subscriber, not by reactive owner. `createSingletonRoot` counts
 * the owners that read it, which is the wrong unit for two of the consumers:
 * `review-store.ts` subscribes from module scope, where there is no owner, and
 * every consumer already holds an unsubscribe function it must be able to run
 * on its own. So the root here is entered once, under one detached owner, and
 * the subscriber count decides when the source closes.
 */
import {
  createRoot,
  createSignal,
  getOwner,
  onCleanup,
  runWithOwner,
  type Accessor,
  type Owner,
} from 'solid-js';
import { createSingletonRoot } from '@solid-primitives/rootless';
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
   * `onOpen` fires once, when the stream is open. A consumer that joins an
   * open stream gets it at once, because the source it shares opened before
   * it arrived and will not announce that again.
   */
  subscribe(handler: (event: E) => void, onOpen?: () => void): () => void;
  /** The event that arrived last, or undefined before the first one. */
  latest: Accessor<E | undefined>;
}

/** What `pluginEvents` gives, where an event names a plugin and a key. */
export interface PluginEventStream {
  subscribe(handler: (plugin: string, key: string) => void, onOpen?: () => void): () => void;
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
function runRoute<E, C>(route: ((event: E, context: C) => void) | null, event: E, context: C): void {
  if (!route) return;
  try {
    route(event, context);
  } catch (error) {
    console.warn('SSE route failed:', error);
  }
}

// =============================================================================
// The root
// =============================================================================

/** Opens the source, and answers the function that closes it. */
type Connect<E> = (onEvent: (event: E) => void, onOpen: () => void) => () => void;

/** One handler, the open callback that arrived with it, and whether it ran. */
interface Subscriber<E> {
  handler: (event: E) => void;
  onOpen?: () => void;
  openDelivered: boolean;
}

/**
 * The owner of every root. It is never disposed, so the cleanup that
 * `createSingletonRoot` registers on it never runs and the subscriber count
 * below is the only thing that closes a source.
 */
let detachedOwner: Owner | null = null;

function sseOwner(): Owner {
  if (!detachedOwner) createRoot(() => (detachedOwner = getOwner()));
  return detachedOwner as Owner;
}

/** Closes every live root. The reset seam runs them. */
const liveRoots = new Set<() => void>();

function createStream<E>(
  connect: Connect<E>,
  route: (event: E) => void,
  dispose: () => void,
): SseStream<E> {
  const subscribers = new Set<Subscriber<E>>();
  const [latest, setLatest] = createSignal<E | undefined>(undefined);
  let close: (() => void) | null = null;
  let opened = false;

  onCleanup(() => {
    close?.();
    close = null;
  });

  function deliver(event: E): void {
    setLatest(() => event);
    route(event);
    // A copy, because a handler may unsubscribe itself while the loop runs.
    for (const subscriber of [...subscribers]) subscriber.handler(event);
  }

  /** Runs one open callback, and runs it once whichever path reaches it. */
  function deliverOpen(subscriber: Subscriber<E>): void {
    if (!subscriber.onOpen || subscriber.openDelivered) return;
    subscriber.openDelivered = true;
    subscriber.onOpen();
  }

  function announceOpen(): void {
    opened = true;
    for (const subscriber of [...subscribers]) deliverOpen(subscriber);
  }

  return {
    latest,
    subscribe(handler, onOpen) {
      const subscriber: Subscriber<E> = { handler, onOpen, openDelivered: false };
      subscribers.add(subscriber);
      // The connect may open at once, which announces to this subscriber too;
      // `deliverOpen` then does nothing on the line below.
      if (!close) close = connect(deliver, announceOpen);
      if (opened) deliverOpen(subscriber);
      let live = true;
      return () => {
        if (!live) return;
        live = false;
        subscribers.delete(subscriber);
        if (subscribers.size === 0) dispose();
      };
    },
  };
}

/**
 * Answers the one stream of a key, and builds it when there is none.
 *
 * The stream object is held, not the accessor `createSingletonRoot` answers,
 * so the accessor runs once per source and registers one cleanup on the
 * detached owner per source, rather than one per call.
 */
function rootFor<E>(
  roots: Map<string, SseStream<E>>,
  key: string,
  connect: Connect<E>,
  route: (event: E) => void,
): SseStream<E> {
  const existing = roots.get(key);
  if (existing) return existing;

  let disposeRoot: () => void = () => {};
  const accessor = createSingletonRoot<SseStream<E>>((dispose) => {
    disposeRoot = dispose;
    onCleanup(() => {
      roots.delete(key);
      liveRoots.delete(dispose);
    });
    return createStream(connect, route, dispose);
  }, null);

  const stream = runWithOwner(sseOwner(), accessor) as SseStream<E>;
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
  return rootFor(
    sessionRoots,
    sessionId,
    (onEvent, onOpen) => subscribeToEvents(sessionId, onEvent, onOpen),
    (event) => runRoute(sessionRoute, event, { ...routeContext(), sessionId }),
  );
}

/** The surface changes of every plugin (`GET /api/surfaces/events`). */
export function surfaceEvents(): SseStream<SurfaceChangedEvent> {
  return rootFor(
    surfaceRoots,
    GLOBAL,
    (onEvent) => subscribeToSurfaceEvents(onEvent),
    (event) => runRoute(surfaceRoute, event, routeContext()),
  );
}

/** The filesystem changes of every watched root (`GET /api/fs/events`). */
export function fsEvents(): SseStream<FsEvent> {
  return rootFor(
    fsRoots,
    GLOBAL,
    (onEvent) => subscribeToFsEvents(onEvent),
    (event) => runRoute(fsRoute, event, routeContext()),
  );
}

/** The URL of the plugin stream. `api.ts` has no function for it yet. */
const PLUGIN_EVENTS_URL = '/api/plugins/events';

/**
 * Opens the plugin stream.
 *
 * It reconnects on no error, which is what the stream did before this module:
 * the three other streams back off and retry inside `api.ts`, and Task D4
 * moves this one's consumer here without changing that.
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
  const stream = rootFor(pluginRoots, GLOBAL, connectPluginEvents, (event) =>
    runRoute(pluginRoute, event, routeContext()),
  );
  return {
    latest: stream.latest,
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

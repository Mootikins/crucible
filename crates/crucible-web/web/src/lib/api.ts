import type { components } from './api-schema';
import type { SchemaResumeWarning } from './api-schema';
import type { RpcMethods } from './rpc-methods';
import type { SessionCommand } from './slash-commands';
import {
  APP_CALLER,
  callerParam,
  client,
  decode,
  expectOk,
  rpc,
  type ApiError,
} from './api-client';
import { getBus } from './bus';
import type { CanvasDoc, CanvasResponse } from './canvas-types';
import type { CommentRef } from './diffset';
import { rawFileUrl } from './paths';
import { assertStreamVersion } from './stream-version';
import type {
  BaseRequest,
  BaseResult,
  CreateEntryParams,
  ReorderGroupsParams,
  SetPropertyParams,
  WriteOutcome,
} from './query/bases';
import type {
  AnchoredEdit,
  AppConfigNode,
  ChatEvent,
  CreateSessionParams,
  MergeRegion,
  PendingInteractionEntry,
  PluginCommand,
  PluginOptionNode,
  PluginPublications,
  Project,
  ProviderInfo,
  ProviderTarget,
  SemanticHit,
  Session,
  SessionEventName,
  SessionSearchResponse,
  SessionModes,
  TargetProvider,
  FileEntry,
  NoteEntry,
  BacklinksResponse,
  SequencedChatEvent,
  FsListing,
  FsEvent,
} from './types';

/**
 * Wire shapes below are aliases into the generated contract
 * (`api-schema.d.ts`, written from `openapi.json`, written from the axum
 * router). A shape the document cannot describe — a plugin's own vocabulary,
 * or a value the browser assembles — keeps its hand-written form and says so.
 */
type Schemas = components['schemas'];

export type { SessionCommand };

/**
 * What `GET /api/config` answers.
 *
 * Two of its fields are open objects on the wire, so the generated type says
 * only "an object" for them. `config` is the daemon's effective config and
 * `controls` is the control tree Lua declares; both are the daemon's
 * vocabulary, and a fixed shape in Rust would make this layer a second owner
 * of them. The narrowing below is the browser's READING of those two objects,
 * not a second contract — everything else comes from the document, including
 * `origins`, which the route does declare.
 */
export type Config = Omit<Schemas['ConfigResponse'], 'config' | 'controls'> & {
  config: Record<string, unknown>;
  controls: AppConfigControls;
};

/**
 * What the app config offers a settings UI: the controls, and the leaves that
 * take none.
 *
 * Hand-written, because `ConfigResponse.controls` is `serde_json::Value`.
 * `options` is the SAME node shape a plugin's tree uses, which is the point —
 * one renderer draws both. `read_only` is the app config's own half: a leaf
 * with no control still shows, with the reason it has none.
 */
interface AppConfigControls {
  options: AppConfigNode;
  read_only: { path: string; reason: string }[];
}

/** What one save did: the leaves that landed, and the leaves that could not.
 * `ok` is false when anything was refused or withheld; `refused` names the
 * file and line of the higher layer that holds each leaf. */
export type ConfigSaveResult = Schemas['ConfigSaveReply'];

/** Settings trees, keyed by the plugin that declared them. */
export type PluginOptions = Record<string, PluginOptionNode>;

/**
 * One item of a session's status list: the daemon's `StatusDisplayItem`, the
 * same type that the `status_items_changed` event carries.
 *
 * Rendered generically — `id`, `plugin` and `text` stay plain strings rather
 * than unions on purpose. The moment the frontend enumerates them, a new
 * plugin needs a frontend change to be visible at all, which is the thing this
 * channel exists to avoid.
 */
export type StatusDisplayItem = Schemas['StatusDisplayItem'];

/**
 * The engine method that opens the menu of plugin approvals: the daemon's
 * `crucible_core::types::PLUGIN_APPROVAL_ACTION`. Only the engine's
 * plugin-turn items carry it.
 */
export const PLUGIN_APPROVAL_ACTION = 'plugin_approval';

// =============================================================================
// API auth (browser: HttpOnly session cookie; programmatic: Bearer header)
// =============================================================================
//
// The server enforces auth on /api/* for non-loopback clients when an API key
// is configured (~/.config/crucible/api_key). The browser signs in once via
// POST /api/auth/login, which mints an HttpOnly session cookie that rides on
// every request — including SSE, where EventSource cannot set headers. Keys
// deliberately never travel in URLs (the old `?token=` bootstrap and
// `?access_token=` SSE fallback leaked via history, server logs, and
// referrers) and are never stored where page JS can read them.

// One-time hygiene: purge the key the pre-cookie flow kept in localStorage.
try {
  localStorage.removeItem('crucible_api_token');
} catch {
  // non-browser context (tests) or storage disabled
}

/**
 * Exchange the API key for the HttpOnly session cookie. Returns whether the
 * server accepted the key; on success the caller should reload so every
 * context refetches with credentials.
 */
export async function login(key: string): Promise<boolean> {
  try {
    const res = await fetch('/api/auth/login', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ key }),
    });
    // The success counterpart to `authRequired`. Modules that gave up on a 401
    // need a signal to re-ask; without one, anything that cached a failure
    // stayed broken for the life of the page even after signing in (the
    // terminal's availability check did exactly that).
    if (res.ok) getBus().emit('authOk', {});
    return res.ok;
  } catch {
    return false;
  }
}

/**
 * The caller header and the app's own name live on the client, which puts the
 * header on every request. They are re-exported because the plugin blocks and
 * the tests read them from this module.
 */
export { APP_CALLER, PLUGIN_CALLER_HEADER } from './api-client';

/**
 * What this file reads out of a payload the document describes only as "an
 * object" or as "any JSON".
 *
 * These are the fields a route forwards without parsing: the daemon's
 * effective config and its control tree, an interaction's kind-tagged body,
 * the pane layout blob. No contract can narrow them, because the vocabulary
 * belongs to the daemon or to a plugin and grows without this file — a fixed
 * shape in Rust would make this layer a second owner of it.
 *
 * It is a CLAIM about an opaque payload, not a second contract, and it is
 * named so that a reader can count the claims. Everything else in this file
 * takes its type from the document.
 */
function openJson<T>(value: unknown): T {
  return value as T;
}

// =============================================================================
// Chat Endpoints
// =============================================================================

/** What the daemon did with a sent message: a turn, or a command that ran without one. */
export type SendOutcome = Schemas['SendOutcome'];

/**
 * Send a chat message to a session.
 * Returns what the daemon did with it. A turn does NOT stream here —
 * subscribe to events separately via `subscribeToEvents`.
 *
 * Refuses a blank message with no attached comment before it ever reaches
 * the daemon: `session.send_message` (`POST /api/rpc/{method}` now — see
 * [[Simplification Plan#Step 19]]) has no such check, since a plain forward
 * gives the daemon no place to make this decision once for every caller.
 */
export async function sendChatMessage(
  sessionId: string,
  content: string,
  comments?: CommentRef[],
): Promise<SendOutcome> {
  if (content.trim().length === 0 && !comments?.length) {
    throw new Error('Failed to send message: Message cannot be empty');
  }
  return rpc(
    'session.send_message',
    // The references only. The daemon builds the context of each comment.
    { session_id: sessionId, content, ...(comments?.length ? { comments } : {}) },
    { notify: true },
  );
}

/**
 * The SSE `event:` names `subscribeToEvents` installs a listener for.
 *
 * `satisfies` binds the tuple to `SessionEventName` — the generated union of
 * every wire name `SessionEventPayload` declares — so a name that is not a
 * variant fails to compile. `MissingSseEventType` below closes the other
 * direction: a variant added in Rust (in any of the eight payload groups)
 * and not listed here makes `_SSE_EVENT_TYPES_ARE_COMPLETE` unassignable.
 * There is no curated subset any more, and no `default:` anywhere that could
 * swallow a name silently — every wire name the daemon can send is either
 * handled here and in `chatEventReducer`, or named as deliberately unhandled.
 *
 * `connection` is absent on purpose: the client mints it, the daemon never
 * sends it, so there is no server event to listen for. `transcript` is
 * added by hand: it is `TranscriptFrame`'s own frame name, not one of
 * `SessionEventPayload`'s.
 */
export const SSE_EVENT_TYPES = [
  'acp_resume_fallback',
  'background_job_completed',
  'base:changed',
  'bash_job_completed',
  'bash_job_failed',
  'bash_job_spawned',
  'classification_required',
  'commands_changed',
  'context_cleared',
  'context_injected',
  'context_limit_resolved',
  'context_strategy_changed',
  'delegation_completed',
  'delegation_failed',
  'delegation_spawned',
  'file_changed',
  'file_deleted',
  'file_moved',
  'interaction_completed',
  'interaction_requested',
  'kiln_notes_indexed',
  'mcp_servers_ready',
  'message_complete',
  'mode_changed',
  'model_switched',
  'note:created',
  'note:deleted',
  'note:modified',
  'note:renamed',
  'notification_added',
  'notification_dismissed',
  'plugin_approval_changed',
  'plugin_turn_limit_changed',
  'plugins_discovered',
  'post_llm_call',
  'precognition_complete',
  'precognition_toggled',
  'process_complete',
  'proposal_changed',
  'providers_listed',
  'publication_changed',
  'replay_complete',
  'review_changed',
  'scope_changed',
  'segment_complete',
  'session:created',
  'session:ended',
  'session_initialized',
  'session_undo',
  'status_items_changed',
  'stream_gap',
  'surface_changed',
  'system_prompt_changed',
  'text_delta',
  'thinking',
  'title_changed',
  'tool_call',
  'tool_call_update',
  'tool_result',
  'turn_finished',
  'ui_style_changed',
  'user_message',
  'webhook:received',
  'workflow.assessed',
  'workflow.cancelled',
  'workflow.completed',
  'workflow.failed',
  'workflow.gate_approved',
  'workflow.gate_reached',
  'workflow.step_completed',
  'workflow.step_started',
  'workspace_indexed',
  'transcript',
] as const satisfies readonly (SessionEventName | 'transcript')[];

/** Every daemon event name the tuple above forgot. Empty, or the build stops. */
type MissingSseEventType = Exclude<SessionEventName, (typeof SSE_EVENT_TYPES)[number]>;
const _SSE_EVENT_TYPES_ARE_COMPLETE: [MissingSseEventType] extends [never] ? true : never = true;
void _SSE_EVENT_TYPES_ARE_COMPLETE;

/**
 * A stream carried a payload this build cannot read.
 *
 * Named, and thrown, rather than asserted past. Each of the three streams
 * used to write `JSON.parse(e.data) as <the type it hoped for>`, so a daemon
 * one rename ahead reached a reducer as a record of `undefined` fields and
 * the console said nothing. Modelled on `EventDecodeError` in
 * `crucible-core/src/protocol/session_events/mod.rs`.
 */
class EventDecodeError extends Error {
  constructor(stream: string, event: string, reason: string) {
    super(`the ${stream} stream sent a ${event} event this build cannot read: ${reason}`);
    this.name = 'EventDecodeError';
  }
}

/**
 * One SSE payload, checked before it is believed.
 *
 * `carries` is as much of the document as a browser can enforce at run time:
 * the field that tells the variants apart, or the fields every variant
 * declares. The one assertion below is guarded by it and stands for all three
 * streams, where there used to be three unguarded ones.
 */
function decodeEvent<T>(
  stream: string,
  event: string,
  raw: string,
  carries: (payload: object) => boolean,
): T {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    throw new EventDecodeError(stream, event, 'the payload is not JSON');
  }
  if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
    throw new EventDecodeError(stream, event, 'the payload is not an object');
  }
  if (!carries(parsed)) {
    throw new EventDecodeError(stream, event, 'the payload is not a shape the document declares');
  }
  return parsed as T;
}

/**
 * Installs the fail-closed version gate on one stream source.
 *
 * The server's first `stream_version` frame names the protocol it speaks (the
 * mirror of the `X-Crucible-Stream-Version` header, which `EventSource`
 * cannot read). A version this build does not understand closes the stream
 * for good — a reconnect would only re-meet the same protocol — and nothing
 * the stream carries is delivered. An absent handshake is the legacy
 * protocol and is allowed.
 */
function guardStreamVersion(stream: string, source: EventSource, shutDown: () => void): void {
  source.addEventListener('stream_version', (e: MessageEvent) => {
    try {
      assertStreamVersion(stream, e.data);
    } catch (error) {
      // Surface the refusal, do not guess: the error is the user-visible
      // statement, the close is the fail-closed half.
      console.error(error);
      shutDown();
      source.close();
    }
  });
}

/** Every `type` a chat event may carry, as a set the decode can ask. */
const CHAT_EVENT_TAGS = new Set<string>(SSE_EVENT_TYPES);

/**
 * A chat payload tagged with a name the document declares.
 *
 * A `SessionEventPayload` frame carries its tag under `event` (adjacent
 * tagging: `{event, data}`); the `transcript` frame carries `type` instead,
 * because it is not one of `SessionEventPayload`'s own variants. Both tags
 * share one name set, since `SSE_EVENT_TYPES` includes `transcript` by hand.
 */
function isChatEvent(payload: object): boolean {
  const tag =
    'event' in payload && typeof payload.event === 'string'
      ? payload.event
      : 'type' in payload && typeof payload.type === 'string'
        ? payload.type
        : undefined;
  return tag !== undefined && CHAT_EVENT_TAGS.has(tag);
}

/**
 * The event names of the two side-channel domains carried on the `system`
 * topic, by domain.
 *
 * The Rust test `every_side_channel_event_name_has_a_frontend_listener`
 * (`crucible-web/src/routes/chat.rs`) compares this table with the event
 * names that the routes compile, in both directions.
 */
const SIDE_CHANNEL_EVENTS = {
  surface: ['surface_changed'],
  system: ['publication_changed', 'proposal_changed'],
} as const;

/** The first retry of a dropped stream waits this long. */
const RECONNECT_BASE_MS = 1000;
/** Each further retry doubles the wait, up to this cap. */
const RECONNECT_CAP_MS = 30_000;

/** What a reconnecting source tells its owner about the transport. */
interface ReconnectingSourceHooks {
  /** Runs at each open: the first open and each reopen. */
  onOpen?: () => void;
  /** Runs when the transport drops. A retry is then on its timer. */
  onDisconnect?: () => void;
  /** Runs when the version gate refuses the protocol. No retry follows. */
  onRefused?: () => void;
}

/**
 * Opens one `EventSource` that the client itself reopens after each error.
 *
 * The browser's own retry is not enough. It retries a network drop, but a
 * non-2xx answer or a wrong content type puts the source in the CLOSED
 * state, and the browser never tries it again. Thus each error closes the
 * source here and opens a new one after an exponential backoff. An open sets
 * the backoff back to its first step.
 *
 * `url` is read at each (re)connect, so the chat stream can state its
 * current resume cursor. The answer closes the source and stops the retries.
 */
function openReconnectingSource(
  url: () => string,
  streamName: string,
  listeners: Readonly<Record<string, (event: MessageEvent) => void>>,
  hooks: ReconnectingSourceHooks = {},
): () => void {
  let source: EventSource | null = null;
  let attempts = 0;
  let retry: ReturnType<typeof setTimeout> | null = null;
  let closed = false;

  function stop(): void {
    closed = true;
    if (retry) clearTimeout(retry);
    retry = null;
    source?.close();
    source = null;
  }

  function connect(): void {
    retry = null;
    if (closed) return;
    const current = new EventSource(url());
    source = current;
    guardStreamVersion(streamName, current, () => {
      stop();
      hooks.onRefused?.();
    });
    for (const [name, listener] of Object.entries(listeners)) {
      current.addEventListener(name, listener);
    }
    current.onopen = () => {
      attempts = 0;
      hooks.onOpen?.();
    };
    current.onerror = () => {
      // An error of a source that this function replaced or closed is old news.
      if (closed || source !== current) return;
      current.close();
      source = null;
      attempts++;
      const delay = Math.min(RECONNECT_BASE_MS * 2 ** (attempts - 1), RECONNECT_CAP_MS);
      console.warn(
        `The ${streamName} stream disconnected. Reconnect in ${delay}ms (attempt ${attempts}).`,
      );
      hooks.onDisconnect?.();
      retry = setTimeout(connect, delay);
    };
  }

  connect();
  return stop;
}

// =============================================================================
// The one event stream (`GET /api/events`, Simplification Plan step 19)
// =============================================================================
//
// Four streams used to reach the browser, each its own `EventSource`: the
// chat events of a session, the filesystem watcher, the surface changes and
// the daemon's system session (publications and proposals). They now share
// ONE connection, `GET /api/events?topics=<a>,<b>,...`, and every frame's
// JSON body carries a `topic` field naming which one it belongs to — a
// session id, or `system` for the other three. `joinEventsTopic` is the one
// place that owns the shared connection; `subscribeToEvents`,
// `subscribeToFsEvents`, `subscribeToSurfaceEvents` and
// `subscribeToSystemEvents` below keep their old names and signatures (`sse.ts`
// calls them by name) but now join a topic of it instead of opening a source
// of their own.

/** The topic every publication, proposal, filesystem and surface event
 * travels on — the same literal the route reserves on the wire. */
const SYSTEM_TOPIC = 'system';

/** What a joiner of one topic wants to hear. */
interface EventsTopicHandlers {
  /** One frame of this topic, other than a `stream_gap` of the `system`
   * topic (that one is `onGap` instead, matching the old per-route split). */
  onFrame: (name: string, raw: string, lastEventId: string) => void;
  /** The shared connection opened, or already was open when this joined. */
  onOpen?: () => void;
  /** The `system` topic's own `stream_gap`, decoded and validated. */
  onGap?: () => void;
  /** The shared connection dropped, or refused the protocol. */
  onDisconnect?: () => void;
}

/** One joiner's handlers, so `dispatchFrame` can fan out to every joiner of
 * a topic — the `system` topic has three (fs, surface, system events). */
const topicSubscribers = new Map<string, Set<EventsTopicHandlers>>();
/** The resume cursor of a joined topic, read again at every (re)connect. A
 * topic with none (the `system` topic, always) states no `after` pair. */
const topicCursors = new Map<string, () => number | undefined>();
/** Closes the shared connection. `null` when no topic is joined. */
let closeEventsConnection: (() => void) | null = null;
let eventsConnectionOpen = false;

/** Every SSE `event:` name any of the four domains listens for. */
function allStreamEventNames(): readonly string[] {
  return [
    ...SSE_EVENT_TYPES,
    ...SIDE_CHANNEL_EVENTS.surface,
    ...SIDE_CHANNEL_EVENTS.system,
    ...FS_SSE_EVENT_TYPES,
  ];
}

/** `topics=<a>,<b>,...&after=<topic>:<seq>,...`, read fresh at each (re)connect. */
function eventsConnectionUrl(): string {
  const topics = [...topicSubscribers.keys()];
  const after = topics
    .map((topic) => {
      const seq = topicCursors.get(topic)?.();
      return seq === undefined ? null : `${topic}:${seq}`;
    })
    .filter((pair): pair is string => pair !== null);
  const params = new URLSearchParams({ topics: topics.join(',') });
  if (after.length > 0) params.set('after', after.join(','));
  return `/api/events?${params.toString()}`;
}

/** Routes one frame to every joiner of the topic its body names. */
function dispatchEventsFrame(name: string, raw: string, lastEventId: string): void {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    console.warn(`Failed to parse SSE event (${name}):`, raw);
    return;
  }
  const named =
    typeof parsed === 'object' &&
    parsed !== null &&
    'topic' in parsed &&
    typeof parsed.topic === 'string'
      ? parsed.topic
      : undefined;
  // Every real frame the server sends names its topic (see
  // `routes/events.rs::with_topic`); a frame with none can only be a
  // hand-built test frame. When exactly one topic is joined there is no
  // ambiguity to resolve, so it is read as that topic's own frame — the
  // single-stream behavior every domain had before it shared this
  // connection. Two or more joined topics make the frame unroutable, and it
  // is dropped rather than guessed at.
  const topic =
    named ?? (topicSubscribers.size === 1 ? [...topicSubscribers.keys()][0] : undefined);
  if (topic === undefined) return;
  const subscribers = topicSubscribers.get(topic);
  if (!subscribers) return;

  // The `system` topic's gap is its own hook (`reconcile`, in `sse.ts`); a
  // session topic's gap is an ordinary frame its own reducer reads, exactly
  // as the four streams split it before they shared one connection.
  if (name === 'stream_gap' && topic === SYSTEM_TOPIC) {
    try {
      decodeEvent<{ dropped: number }>(
        topic,
        'stream_gap',
        raw,
        (payload) =>
          'dropped' in payload && typeof payload.dropped === 'number' && payload.dropped >= 0,
      );
    } catch {
      console.warn('Failed to parse stream gap:', raw);
      return;
    }
    for (const subscriber of subscribers) subscriber.onGap?.();
    return;
  }

  // The topic is the envelope's own field, not part of the payload shape a
  // domain decodes: `ChatEvent`, `FsEvent`, `SurfaceChangedEvent` and
  // `SystemEvent` are the same shapes they were before a topic existed on
  // the wire (Simplification Plan step 19 grew only the envelope). Strip it
  // before a subscriber's own `decodeEvent` reads the frame again.
  const { topic: _topic, ...withoutTopic } = parsed as Record<string, unknown>;
  const raw2 = JSON.stringify(withoutTopic);
  for (const subscriber of subscribers) subscriber.onFrame(name, raw2, lastEventId);
}

/** Tears the shared connection down and, if a topic is still joined, opens a
 * fresh one — the backoff restarts, as a manual reconnect always did. */
function rebuildEventsConnection(): void {
  closeEventsConnection?.();
  closeEventsConnection = null;
  eventsConnectionOpen = false;
  if (topicSubscribers.size === 0) return;

  const listeners: Record<string, (event: MessageEvent) => void> = {};
  for (const name of allStreamEventNames()) {
    listeners[name] = (e: MessageEvent) => dispatchEventsFrame(name, e.data, e.lastEventId);
  }
  closeEventsConnection = openReconnectingSource(eventsConnectionUrl, 'events', listeners, {
    onOpen: () => {
      eventsConnectionOpen = true;
      for (const subscribers of topicSubscribers.values()) {
        for (const subscriber of subscribers) subscriber.onOpen?.();
      }
    },
    onDisconnect: () => {
      eventsConnectionOpen = false;
      for (const subscribers of topicSubscribers.values()) {
        for (const subscriber of subscribers) subscriber.onDisconnect?.();
      }
    },
    onRefused: () => {
      eventsConnectionOpen = false;
      for (const subscribers of topicSubscribers.values()) {
        for (const subscriber of subscribers) subscriber.onDisconnect?.();
      }
    },
  });
}

/**
 * Joins the shared connection to receive the frames of `topic`. The first
 * joiner of a topic not already carried rebuilds the connection with it
 * added; the answer leaves, and the last joiner of a topic rebuilds the
 * connection without it.
 *
 * `cursor`, when given, is this topic's own resume cursor (a session's
 * applied seq); the `system` topic keeps no log and names none.
 */
function joinEventsTopic(
  topic: string,
  handlers: EventsTopicHandlers,
  cursor?: () => number | undefined,
): () => void {
  let subscribers = topicSubscribers.get(topic);
  const isNewTopic = !subscribers;
  if (!subscribers) {
    subscribers = new Set();
    topicSubscribers.set(topic, subscribers);
  }
  subscribers.add(handlers);
  if (cursor) topicCursors.set(topic, cursor);

  if (isNewTopic) {
    rebuildEventsConnection();
  } else if (eventsConnectionOpen) {
    handlers.onOpen?.();
  }

  let live = true;
  return () => {
    if (!live) return;
    live = false;
    subscribers!.delete(handlers);
    if (subscribers!.size === 0) {
      topicSubscribers.delete(topic);
      topicCursors.delete(topic);
      rebuildEventsConnection();
    }
  };
}

/**
 * Forces a hard reconnect of the shared connection — every joined topic's
 * manual retry, because there is one transport under all of them now.
 * A no-op when nothing is joined.
 */
export function reconnectEventsConnection(): void {
  rebuildEventsConnection();
}

/**
 * Forgets every joined topic and closes the shared connection.
 *
 * Test-only. The connection is a module-level singleton, so a test that calls
 * `subscribeToEvents`/`subscribeToFsEvents`/`subscribeToSurfaceEvents`/
 * `subscribeToSystemEvents` directly (not through `lib/query/sse.ts`'s
 * roots, whose own `resetSseForTests` leaves every topic through the normal
 * path) must call this between cases, or a topic a case forgot to leave
 * still looks joined to the next one and no fresh connection opens for it.
 */
export function resetEventsConnectionForTests(): void {
  closeEventsConnection?.();
  closeEventsConnection = null;
  eventsConnectionOpen = false;
  topicSubscribers.clear();
  topicCursors.clear();
}

/**
 * Subscribe to SSE events for a session.
 * Returns a cleanup function that leaves the shared connection's `session_id`
 * topic.
 *
 * Call this BEFORE sending a message so no events are missed.
 * Automatically reconnects on disconnect with exponential backoff. Each
 * (re)connect states the caller's cursor — the last seq it APPLIED — as this
 * topic's `after=` pair, and the server replays the persisted events past it.
 *
 * The transport state travels as a client-minted `connection` event:
 * `connected` at each open, `reconnecting` at each drop. The server
 * subscribes the daemon session before it returns the stream headers, so
 * `connected` means that no event will be dropped.
 */
export function subscribeToEvents(
  sessionId: string,
  onEvent: (event: SequencedChatEvent) => void,
  /**
   * Reads the resume cursor: the last seq APPLIED for this session. Polled at
   * every (re)connect, because a reconnect must name the position the store
   * has actually reached — not the last frame received, which an apply that
   * never ran would have lost.
   */
  cursor?: () => number | undefined,
): () => void {
  return joinEventsTopic(
    sessionId,
    {
      onFrame: (eventType, raw, lastEventId) => {
        try {
          const event = decodeEvent<ChatEvent>('chat', eventType, raw, isChatEvent);
          // The seq the route stamped as the frame's `id:` — absent when the
          // frame carried none, and then the event travels without one.
          const seq = readSeq(lastEventId);
          onEvent(seq === undefined ? event : { ...event, seq });
        } catch {
          console.warn(`Failed to parse SSE event (${eventType}):`, raw);
        }
      },
      onOpen: () => onEvent({ type: 'connection', status: 'connected' }),
      // Transient transport status — NOT a daemon 'error' (that path
      // overwrites the streaming message and nulls the streaming id,
      // permanently losing the in-flight turn on a routine idle reconnect).
      onDisconnect: () =>
        onEvent({ type: 'connection', status: 'reconnecting', message: 'Reconnecting…' }),
    },
    cursor,
  );
}

/** The seq off a frame's `id:` field, or undefined when the frame sent none. */
function readSeq(lastEventId: string): number | undefined {
  // The id is `topic:seq`; only the part after the last colon is the seq.
  const seq = Number.parseInt(lastEventId.slice(lastEventId.lastIndexOf(':') + 1), 10);
  return Number.isInteger(seq) && seq > 0 ? seq : undefined;
}

/**
 * Respond to an interaction request from the agent.
 */

/**
 * Aggregate pending interactions across all sessions (Inbox poll).
 * Returns [] on any failure so callers degrade gracefully against daemons
 * that predate the endpoint.
 */
export async function listPendingInteractions(): Promise<PendingInteractionEntry[]> {
  try {
    const resp = decode(
      await client.GET('/api/interactions/pending'),
      'Failed to list pending interactions',
    );
    return openJson<PendingInteractionEntry[]>(resp.pending ?? []);
  } catch {
    return [];
  }
}

export async function respondToInteraction(
  sessionId: string,
  requestId: string,
  response: unknown,
): Promise<void> {
  expectOk(
    await client.POST('/api/interaction/respond', {
      body: { session_id: sessionId, request_id: requestId, response },
    }),
    'Failed to respond',
  );
}

// =============================================================================
// Config Endpoints
// =============================================================================

/** Get server configuration including the configured kiln path. */
export async function getConfig(): Promise<Config> {
  return openJson<Config>(decode(await client.GET('/api/config'), 'Failed to get config'));
}

/**
 * Save values as the user's durable preference.
 *
 * A refusal is part of the answer, not an error: a leaf the user's `init.lua`
 * holds comes back in `refused` with the file and the line that holds it, and
 * the leaves beside it still saved.
 */
export async function saveConfig(values: Record<string, unknown>): Promise<ConfigSaveResult> {
  return decode(await client.POST('/api/config', { body: { values } }), 'Failed to save config');
}

/**
 * Every command loaded plugins declared.
 *
 * The enumeration that has to exist before a primitive can be offered as a
 * button: the daemon has always known these, and nothing carried them to a
 * browser, so a caller could invoke a command it had no way to discover.
 */
export async function getPluginCommands(): Promise<PluginCommand[]> {
  const body = decode(await client.GET('/api/plugins/commands'), 'Failed to list plugin commands');
  return body.commands ?? [];
}

/**
 * What plugins published, keyed by contribution kind then plugin.
 *
 * Pass `key` to narrow daemon-side. A caller drawing one key should ask for
 * that key: without it the response carries every plugin's data, which is more
 * than the caller needs and — once third-party block code can run — more than
 * it should receive.
 */
export async function getPluginPublications(
  key?: string,
  caller: string = APP_CALLER,
): Promise<PluginPublications> {
  const body = await rpc('plugin.publications', { key: key ?? null }, { caller });
  return body.publications ?? {};
}

/**
 * Settings trees every plugin declared, rendered for this frontend.
 *
 * Re-read rather than cached: a tree's function-valued fields (`values`,
 * `disabled`) describe the box as it is *now* — which runtimes are installed,
 * what another setting was just changed to — so a stale tree is a wrong one.
 */
export async function getPluginOptions(): Promise<PluginOptions> {
  const body = decode(await client.GET('/api/plugins/options'), 'Failed to get plugin settings');
  return openJson<PluginOptions>(body.options ?? {});
}

/** Read one option's current value. */
export async function getPluginOption(plugin: string, path: string[]): Promise<unknown> {
  const body = decode(
    await client.POST('/api/plugins/{name}/option', {
      params: { path: { name: plugin }, header: callerParam() },
      body: { action: 'get', path },
    }),
    'Failed to read plugin setting',
  );
  // One route answers three actions: `get` carries the value, `set` and
  // `execute` carry `ok`. The document declares both arms, so the read has to
  // say which one it wants.
  return 'value' in body ? body.value : undefined;
}

/** Write one option. The plugin's own setter decides what that means. */
export async function setPluginOption(
  plugin: string,
  path: string[],
  value: unknown,
): Promise<void> {
  expectOk(
    await client.POST('/api/plugins/{name}/option', {
      params: { path: { name: plugin }, header: callerParam() },
      body: { action: 'set', path, value },
    }),
    'Failed to change plugin setting',
  );
}

/** Press a `type = "execute"` node. */
export async function executePluginOption(plugin: string, path: string[]): Promise<void> {
  expectOk(
    await client.POST('/api/plugins/{name}/option', {
      params: { path: { name: plugin }, header: callerParam() },
      body: { action: 'execute', path },
    }),
    'Plugin action failed',
  );
}

/**
 * Invoke a plugin command and hand back what it returned.
 *
 * Untyped by design — the caller knows the shape it asked for, and a schema
 * here would be one only today's plugins could satisfy.
 */
export async function runPluginCommand(
  name: string,
  args: unknown = {},
  caller: string = APP_CALLER,
): Promise<unknown> {
  // `plugin.run_command`'s reply is `PluginRunCommandReply` (`{name,
  // result}`), the same envelope `POST /api/plugins/command` answered.
  return rpc('plugin.run_command', { name, args }, { caller });
}

/** Providers on one axis, sorted by label so the menu is stable. */
export async function getTargetProviders(axis: TargetProvider['axis']): Promise<TargetProvider[]> {
  const answers = (await getPluginPublications()).targets ?? {};
  const providers: TargetProvider[] = [];
  for (const [plugin, value] of Object.entries(answers)) {
    const decl = value as Partial<TargetProvider> | null;
    // Publications are opaque JSON from a plugin. A malformed one is skipped
    // rather than allowed to throw: a throw is swallowed by swrLocal and the
    // whole control silently never appears, so one bad plugin would hide every
    // good one's targets.
    if (decl?.axis !== axis) continue;
    providers.push({
      plugin,
      axis,
      label: typeof decl.label === 'string' && decl.label ? decl.label : plugin,
      targets_command: typeof decl.targets_command === 'string' ? decl.targets_command : undefined,
      resolve_command: typeof decl.resolve_command === 'string' ? decl.resolve_command : undefined,
    });
  }
  return providers.sort((a, b) => a.label.localeCompare(b.label));
}

/**
 * Ask one provider what it currently offers.
 *
 * `workspace` is the project the user has selected — the repo a worktree would
 * be cut from. Answers `[]` rather than throwing for the same reason
 * `getTargetProviders` skips a malformed declaration: one provider that is
 * unreachable (its plugin unloaded, its command renamed) must not take the
 * menu down with it.
 */
export async function getProviderTargets(
  provider: TargetProvider,
  workspace?: string,
): Promise<ProviderTarget[]> {
  if (!provider.targets_command) return [];
  try {
    const result = await runPluginCommand(provider.targets_command, { workspace });
    // A plugin command answers opaque JSON, which no document narrows: the
    // provider names the command and the plugin decides what comes back.
    const list = Array.isArray(result) ? result : (result as { targets?: unknown } | null)?.targets;
    if (!Array.isArray(list)) return [];
    return list.flatMap((item) => {
      const target = item as Partial<ProviderTarget> | null;
      if (!target || typeof target.value !== 'string') return [];
      return [
        {
          value: target.value,
          label: typeof target.label === 'string' ? target.label : target.value,
          hint: typeof target.hint === 'string' ? target.hint : undefined,
          disabled: target.disabled === true,
          spec: `${provider.plugin}:${target.value}`,
          path: typeof target.path === 'string' ? target.path : undefined,
          current: target.current === true ? true : undefined,
          default: target.default === true ? true : undefined,
        },
      ];
    });
  } catch {
    return [];
  }
}

/**
 * Every workspace target on the box, across providers, already flattened.
 *
 * For the consumers that want the *data* rather than a menu — the session tree
 * labelling a checkout with its branch, the files-pane picker jumping to one.
 * They used to call `scm.branches` and parse git's answer themselves, so the
 * daemon and the plugin each held a copy of what a branch is.
 */
export async function listWorkspaceTargets(workspace?: string): Promise<ProviderTarget[]> {
  const providers = await getTargetProviders('workspace');
  const answers = await Promise.all(providers.map((p) => getProviderTargets(p, workspace)));
  return answers.flat();
}

/**
 * Materialise a workspace target now, outside session creation, and answer
 * with its path.
 *
 * The same `resolve_command` the daemon calls before `session.create` — for
 * the files-pane picker, which switches the browsable root without starting a
 * session. Providers are idempotent, so asking for a checkout that already
 * exists returns it rather than failing.
 *
 * Throws, unlike the enumerating calls: a target the user explicitly picked
 * and which could not be resolved has to say so, not silently do nothing.
 */
export async function resolveWorkspaceTarget(spec: string, workspace?: string): Promise<string> {
  const [plugin, ...rest] = spec.split(':');
  const provider = (await getTargetProviders('workspace')).find((p) => p.plugin === plugin);
  if (!provider?.resolve_command) {
    throw new Error(`No plugin resolves workspace targets named '${plugin}'`);
  }
  const answer = await runPluginCommand(provider.resolve_command, {
    target: rest.join(':'),
    workspace,
  });
  // Opaque plugin JSON again: the resolve command is the plugin's own, so the
  // reading is checked here rather than declared anywhere.
  const path = (answer as { path?: unknown } | null)?.path;
  if (typeof path !== 'string' || !path) {
    throw new Error(`Plugin '${plugin}' resolved '${spec}' to no path`);
  }
  return path;
}

// =============================================================================
// Session Endpoints
// =============================================================================

export async function createSession(params: CreateSessionParams): Promise<Session> {
  // The daemon's reason rides the error; `SessionContext.createSession`
  // is the one that toasts it, so no `notify` here or it shows twice.
  return decode(await client.POST('/api/session', { body: params }), 'Failed to create session');
}

/** List sessions with optional filters. */
/**
 * The list a reply was supposed to carry.
 *
 * A shape the client did not expect — a reverse proxy's error page served as
 * 200, a daemon one field-rename ahead, a truncated body — used to reach the
 * user as `Cannot read properties of undefined (reading 'map')`. That is a
 * stack trace wearing a toast: it names nothing anyone can act on, and it
 * hides which call failed. Each caller already declares a human `errorMessage`
 * for an HTTP failure; a wrong shape deserves the same sentence.
 */
function expectList<T>(value: T[] | undefined, field: string, what: string): T[] {
  if (Array.isArray(value)) return value;
  throw new Error(`${what}: the server's reply carried no "${field}" list`);
}

export async function listSessions(filters?: {
  kiln?: string;
  workspace?: string;
  type?: string;
  state?: string;
  includeArchived?: boolean;
}): Promise<Session[]> {
  const data = await rpc('session.list', {
    kilns: filters?.kiln ? [filters.kiln] : undefined,
    workspace: filters?.workspace,
    type: filters?.type,
    state: filters?.state,
    include_archived: filters?.includeArchived ? true : undefined,
  });
  return expectList(data.sessions, 'sessions', 'Failed to list sessions');
}

/**
 * Search sessions by title/content.
 *
 * Answers MATCHED LINES, not sessions. This call used to declare an array of
 * sessions and map it through `mapSession`, so every field it read was
 * `undefined` and a hit rendered as an untitled row with no date.
 *
 * `kilns` is the scope, and the scope rule is kiln-set *overlap* — a result
 * needs to share at least one kiln with it. Pass the caller's whole set, not
 * one member: a member stands only for the sessions that share that member. An
 * unscoped search matches nothing and says so in `note`.
 */
export async function searchSessions(
  query: string,
  kilns?: string | string[],
  limit?: number,
): Promise<SessionSearchResponse> {
  const scope = (typeof kilns === 'string' ? [kilns] : (kilns ?? [])).filter(Boolean);
  const data = await rpc('session.search', { query, kilns: scope, limit });
  return {
    ...data,
    matches: expectList(data.matches, 'matches', 'Failed to search sessions'),
  };
}

// =============================================================================
// Content Search (ripgrep) — search_grep
// =============================================================================

/** What `search_grep` answers, read straight off the document. */
export type GrepResponse = Schemas['GrepSearchResponse'];

/**
 * Ripgrep content search over an absolute `root` (must be inside a registered
 * kiln or project — the daemon rejects anything else). `glob` filters by name
 * (e.g. `*.md` for notes); omit to search all files. Respects .gitignore.
 *
 * Answers the document's own hit shape (`rel_path`/`match_start`/`match_end`)
 * with no camelCase remap: a remap once hid a field rename from `tsc`, and
 * `SearchPanel` now reads the wire's own names.
 */
export async function grepSearch(
  root: string,
  query: string,
  opts?: { glob?: string; limit?: number; caseInsensitive?: boolean },
): Promise<GrepResponse> {
  return rpc('search_grep', {
    root,
    query,
    glob: opts?.glob ?? null,
    limit: opts?.limit ?? 100,
    case_insensitive: opts?.caseInsensitive ?? true,
  });
}

// =============================================================================
// Semantic Search (vector) — POST /api/search/semantic
// =============================================================================

/**
 * Semantic (vector) search over a kiln's processed notes: the daemon embeds
 * `query` with the kiln's embedding provider, then ranks notes by vector
 * similarity. Returns [] if the kiln has no embeddings or no provider is
 * configured. Unlike grep, this matches meaning, not literal text.
 */
export async function semanticSearch(
  kiln: string,
  query: string,
  limit = 20,
): Promise<SemanticHit[]> {
  const data = decode(
    await client.POST('/api/search/semantic', { body: { kiln, query, limit } }),
    'Semantic search failed',
  );
  // `openapi-fetch`'s inferred response type widens `BlockRef.cited` from the
  // document's `[number, number][]` to `number[][]` somewhere in its own
  // generic pipeline; `components['schemas']` keeps the tuple. Both describe
  // the SAME bytes, so the cast is safe — a real field rename still fails
  // `tsc` on `SemanticHit`'s other fields, which this narrowing does not
  // touch.
  return expectList(data.results, 'results', 'Failed to search notes') as SemanticHit[];
}

/** Pause a session. */
export async function pauseSession(id: string): Promise<void> {
  await rpc('session.pause', { session_id: id });
}

/**
 * Resume a session (also auto-subscribes to events on the backend).
 *
 * Answers the warnings the daemon computed for this resume — what a stored
 * revival did not bring back (see `ResumeWarning` in `crucible-core`). Empty
 * for a resume that stayed in memory, which lost nothing.
 */
export async function resumeSession(id: string): Promise<SchemaResumeWarning[]> {
  const data = decode(
    await client.POST('/api/session/{id}/resume', { params: { path: { id } } }),
    'Failed to resume session',
  );
  // Only the restored (cold-resume) shape carries `warnings` at all; the
  // live shape has nothing torn down to report.
  return 'warnings' in data ? (data.warnings ?? []) : [];
}

/**
 * End a session.
 *
 * Kept as its own route, not `rpc('session.end', ...)`: the route also
 * releases the web process's own SSE broker entry for the session
 * (`ReconnectingDaemon::close_event_streams`), which is local web-process
 * state a plain RPC forward has no way to reach.
 */
export async function endSession(id: string): Promise<void> {
  expectOk(
    await client.POST('/api/session/{id}/end', { params: { path: { id } } }),
    'Failed to end session',
  );
}

/**
 * Delete a session permanently.
 *
 * Kept as its own route for the same reason as {@link endSession}: it
 * releases the web process's own SSE broker entry too.
 */
export async function deleteSession(id: string): Promise<void> {
  expectOk(
    await client.DELETE('/api/session/{id}', { params: { path: { id } } }),
    'Failed to delete session',
  );
}

/**
 * Archive a session (hide from default listing).
 *
 * Kept as its own route for the same reason as {@link endSession}: it
 * releases the web process's own SSE broker entry too.
 */
export async function archiveSession(id: string): Promise<void> {
  expectOk(
    await client.POST('/api/session/{id}/archive', { params: { path: { id } } }),
    'Failed to archive session',
  );
}

/** List available models for a session. */
export async function listModels(sessionId: string): Promise<string[]> {
  return (await rpc('session.list_models', { session_id: sessionId }, { notify: true })).models;
}

export type PluginApproval = components['schemas']['PluginApproval'];

/** List the modes a session may enter, and the one it is in. */
export async function listModes(sessionId: string): Promise<SessionModes> {
  return rpc('session.list_modes', { session_id: sessionId }, { notify: true });
}

/** Switch the model for a session. */
export async function switchModel(sessionId: string, modelId: string): Promise<void> {
  await setKnob(sessionId, { knob: 'model', value: modelId });
}

/** Set the session mode (normal/plan/auto). Confirmation echoes back as a
 * mode_changed SSE event. */
export async function setSessionMode(sessionId: string, mode: string): Promise<void> {
  await setKnob(sessionId, { knob: 'mode', value: mode });
}

/** List available LLM providers and their models. */
export async function listProviders(): Promise<ProviderInfo[]> {
  const data = await rpc('providers.list', {});
  return expectList(data.providers, 'providers', 'Failed to list providers');
}

// =============================================================================
// Session knobs
//
// One route pair for every knob — model, mode, context strategy,
// precognition, plugin turn limit. `KnobValue` names its own knob (the
// `knob` tag), so `setKnob` cannot send a value shaped for the wrong knob.
// =============================================================================

export type KnobValue = components['schemas']['KnobValue'];

/** Write one session knob. */
export async function setKnob(sessionId: string, value: KnobValue): Promise<void> {
  await rpc('session.knob.set', { session_id: sessionId, ...value });
}

/** Read one session knob, in the same shape `setKnob` writes. */
export async function getKnob(sessionId: string, knob: KnobValue['knob']): Promise<KnobValue> {
  return rpc('session.knob.get', { session_id: sessionId, knob });
}

/** Get the precognition state for a session. */
export async function getPrecognition(sessionId: string): Promise<boolean> {
  const value = await getKnob(sessionId, 'precognition');
  return value.knob === 'precognition' ? value.value : true;
}

/** Set the precognition state for a session. */
export async function setPrecognition(sessionId: string, enabled: boolean): Promise<void> {
  await setKnob(sessionId, { knob: 'precognition', value: enabled });
}

/** Get the context-assembly strategy. */
export async function getContextStrategy(
  sessionId: string,
): Promise<Schemas['ContextStrategy'] | null> {
  const value = await getKnob(sessionId, 'context_strategy');
  return value.knob === 'context_strategy' ? value.value : null;
}

/**
 * Set the context-assembly strategy.
 *
 * `strategy`'s type is the document's own closed set (`ContextStrategy`), so
 * a caller cannot build a request naming a spelling the daemon does not
 * know — `tsc` refuses it here, before a round trip earns a 422.
 */
export async function setContextStrategy(
  sessionId: string,
  strategy: Schemas['ContextStrategy'],
): Promise<void> {
  await setKnob(sessionId, { knob: 'context_strategy', value: strategy });
}

// =============================================================================
// Session Export
// =============================================================================

/** Export a session to markdown. Returns the raw markdown string. */
export async function exportSession(sessionId: string): Promise<string> {
  return decode(
    await client.POST('/api/session/{id}/export', {
      params: { path: { id: sessionId } },
      parseAs: 'text',
    }),
    'Failed to export session',
  );
}

// =============================================================================
// Slash Command Execution
// =============================================================================

export type CommandResult = Schemas['CommandResponse'];

/** Execute a built-in slash command in a session. */
export async function executeCommand(sessionId: string, command: string): Promise<CommandResult> {
  return decode(
    await client.POST('/api/session/{id}/command', {
      params: { path: { id: sessionId } },
      body: { command },
    }),
    'Failed to execute command',
  );
}

// =============================================================================
// Plugin Endpoints
// =============================================================================

/**
 * A surface changed: identity and version, never the rows.
 *
 * `withdrawn` says the surface is gone: drop it, and do not refetch. The daemon
 * sets it when it drops the entry and omits the field otherwise, so absent
 * means present. This is the one change that is actionable on its own — every
 * other event withholds the rows so the browser has to ask, and a withdrawal
 * has nothing left to ask for.
 */
export type SurfaceChangedEvent = Schemas['SurfaceChangedEvent'];

/**
 * Subscribe to surface changes: the `system` topic of the shared connection
 * (`GET /api/events`). Returns a cleanup function that leaves the topic.
 */
export function subscribeToSurfaceEvents(
  onEvent: (event: SurfaceChangedEvent) => void,
  onOpen: () => void = () => {},
  onGap: () => void = () => {},
  onDisconnect: () => void = () => {},
): () => void {
  return joinEventsTopic(SYSTEM_TOPIC, {
    onFrame: (name, raw) => {
      if (name !== 'surface_changed') return;
      try {
        // The payload carries no tag of its own — the stream has one event
        // name — so the check is the fields the document requires.
        onEvent(
          decodeEvent<SurfaceChangedEvent>(
            'surface',
            'surface_changed',
            raw,
            (payload) =>
              'name' in payload &&
              typeof payload.name === 'string' &&
              'plugin' in payload &&
              typeof payload.plugin === 'string' &&
              'version' in payload &&
              typeof payload.version === 'number',
          ),
        );
      } catch {
        console.warn('Failed to parse surface SSE event:', raw);
      }
    },
    onOpen,
    onGap,
    onDisconnect,
  });
}

/**
 * Rich plugin metadata from `GET /api/plugins`.
 *
 * `version` is null when the plugin has no `spec.luau`: the version is declared
 * in its fragment. `last_error` says why a plugin is not Active, and is null
 * for a healthy one.
 */
export type PluginInfo = Schemas['PluginInfo'];

/** Plugin reload response (counts of reloaded capabilities). */
export type PluginReloadResult = Schemas['PluginReloadReply'];

/** List discovered plugins with rich metadata. */
export async function getPlugins(): Promise<PluginInfo[]> {
  return decode(await client.GET('/api/plugins'), 'Failed to list plugins').plugins;
}

/** Reload a plugin by name. Returns the daemon's capability counts. */
export async function reloadPlugin(name: string): Promise<PluginReloadResult> {
  return decode(
    await client.POST('/api/plugins/{name}/reload', {
      params: { path: { name }, header: callerParam() },
    }),
    'Failed to reload plugin',
  );
}

export type InstallPluginParams = Schemas['PluginInstallRequest'];

/**
 * What an install did.
 *
 * `manifest` is the path of the file the install wrote — the field is NOT
 * `plugins_toml`, which is what this file used to call it. `loaded` says
 * whether the plugin actually activated on the running daemon: "installed"
 * must not read as success while the plugin sits broken.
 */
export type InstallPluginResult = Schemas['PluginInstallReply'];

/**
 * Install a plugin by URL. Synchronous — can take 10+ seconds for a
 * fresh clone over a slow network. Caller should show a spinner.
 */
export async function installPlugin(params: InstallPluginParams): Promise<InstallPluginResult> {
  return decode(
    await client.POST('/api/plugins', {
      params: { header: callerParam() },
      body: params,
    }),
    'Failed to install plugin',
  );
}

/**
 * What a remove did.
 *
 * `purge_error` is set when the manifest removal succeeded but deleting the
 * directory failed. `kept_dir` is set when the plugin was removed without a
 * purge: the directory remains and loads again on the next daemon restart or
 * plugin install.
 */
export type RemovePluginResult = Schemas['PluginRemoveReply'];

/** Remove a plugin by name. If `purge`, the cloned directory is also deleted. */
export async function removePlugin(name: string, purge = false): Promise<RemovePluginResult> {
  return decode(
    await client.DELETE('/api/plugins/{name}', {
      params: { path: { name }, query: { purge: purge || undefined }, header: callerParam() },
    }),
    'Failed to remove plugin',
  );
}

// =============================================================================
// Search Endpoints
// =============================================================================

export async function listNotes(kiln: string, pathFilter?: string): Promise<NoteEntry[]> {
  return rpc('list_notes', { kiln, path_filter: pathFilter });
}

/**
 * Resolve a wikilink target to a file by walking the kiln.
 *
 * Independent of the note index — following a link is a path question, and
 * answering it from the index means an unprocessed kiln resolves nothing and
 * silently falls back to the default kiln, opening a same-named note from the
 * wrong vault.
 */
export async function resolveNotePath(
  kiln: string,
  name: string,
): Promise<{ path: string; absolutePath: string; title?: string }> {
  const answer = decode(
    await client.GET('/api/notes/resolve', { params: { query: { kiln, name } } }),
    'Failed to resolve note',
  );
  // The wire says `null` for a note with no title; this answer has always said
  // absent, and its callers read it that way.
  return {
    path: answer.path,
    absolutePath: answer.absolutePath,
    ...(answer.title === null || answer.title === undefined ? {} : { title: answer.title }),
  };
}

/**
 * Linked + unlinked mentions for a note. `note` accepts a note name or
 * kiln-relative path (fuzzy-resolved server-side).
 */
export async function getBacklinks(kiln: string, note: string): Promise<BacklinksResponse> {
  return decode(
    await client.GET('/api/backlinks', { params: { query: { kiln, note } } }),
    'Failed to get backlinks',
  );
}

// =============================================================================
// Project Endpoints
// =============================================================================

/** Register a project. */
export async function registerProject(path: string): Promise<Project> {
  return decode(
    await client.POST('/api/project/register', { body: { path } }),
    'Failed to register project',
  );
}

// =============================================================================
// SCM Endpoints (branch/worktree browsing)
// =============================================================================

export type ScmCloneResponse = Schemas['ScmCloneResponse'];

/** Clone a remote repo into `[workspace] root_dir` and register it as a
 * project. Slow (network clone) — no client-side timeout beyond fetch's. */
export async function scmClone(url: string): Promise<ScmCloneResponse> {
  return decode(
    await client.POST('/api/scm/clone', { body: { url } }),
    'Failed to clone repository',
  );
}

/**
 * `list_notes`, narrowed to `{name, path, is_dir}`.
 *
 * `GET /api/kiln/files` and `GET /api/kiln/notes` used to do this reshape
 * server-side under one name, `FileEntryRow` — one route body for two
 * routes, so the two answered the same projection. They are gone
 * ([[Simplification Plan#Step 19]] item 3): `listFiles` and `listKilnNotes`
 * below are that one reshape now, so the two still cannot drift apart.
 */
function toFileEntries(notes: RpcMethods['list_notes']['result']): FileEntry[] {
  return notes.map((n) => ({ name: n.name, path: n.path, is_dir: false }));
}

/** List files in a kiln directory. */
export async function listFiles(path: string): Promise<FileEntry[]> {
  return toFileEntries(await rpc('list_notes', { kiln: path }));
}

/** List kiln notes. */
export async function listKilnNotes(kilnPath: string): Promise<FileEntry[]> {
  return toFileEntries(await rpc('list_notes', { kiln: kilnPath }));
}

/** Get file content by path. */
export async function getFileContent(path: string): Promise<string> {
  return decode(
    await client.GET('/api/kiln/file', { params: { query: { path } } }),
    'Failed to get file content',
  ).content;
}

/** Save file content by path. */
/**
 * A file's text AND the hash of the bytes just read.
 *
 * The hash is what an offline write anchors on: `PATCH`/`PUT` compare it to
 * the bytes on disk, so a note that changed meanwhile is a conflict rather
 * than an overwrite. `getFileContent` stays the plain-text call every editor
 * surface already uses.
 */
export async function getFileWithHash(
  path: string,
): Promise<{ content: string; content_hash: string }> {
  return decode(
    await client.GET('/api/kiln/file', { params: { query: { path } } }),
    'Failed to read file',
  );
}

export async function saveFileContent(path: string, content: string): Promise<void> {
  expectOk(await client.PUT('/api/kiln/file', { body: { path, content } }), 'Failed to save file');
}

/**
 * What a guarded save answers.
 *
 * `merged: true` means the caller's base was stale and the route merged its
 * text with the disk: `content` is what was written, and the caller holds it
 * nowhere else. A refusal carries the hash on disk now; when the caller sent a
 * base text it also carries both texts and every region the merge could not
 * settle, because a second round trip to fetch them would race the same way.
 */
export type GuardedSave =
  | { ok: true; content_hash: string; merged?: false }
  | { ok: true; content_hash: string; merged: true; content: string }
  | {
      ok: false;
      current_hash: string;
      current_content?: string;
      merged_content?: string;
      regions?: MergeRegion[];
    };

/**
 * Save a whole file, refusing if it moved on since `baseHash` was read.
 *
 * The compare happens inside the daemon's write, which is the only place it
 * can be right: a browser that reads, compares and then PUTs leaves a window
 * between the read and the write, and runs on the machine with the stale view
 * of the disk.
 *
 * `baseText` is the note as it was at `baseHash`. It asks to be MERGED rather
 * than refused: the caller is the only party that holds the text its edit was
 * made from, so without it the route can only refuse, and the edit costs the
 * user the whole note. A merge that leaves a region writes nothing.
 *
 * A 409 is a VALUE, not a throw — it carries the hash on disk now, which is
 * what a caller needs to decide between re-reading and resolving.
 */
export async function saveFileIfUnchanged(
  path: string,
  content: string,
  baseHash: string,
  baseText?: string,
): Promise<GuardedSave> {
  // `base_text` stays absent when the caller has none: the body serialiser
  // drops an `undefined` property, and the daemon's own defaults only apply to
  // a field that is ABSENT.
  const result = await client.PUT('/api/kiln/file', {
    body: { path, content, base_hash: baseHash, base_text: baseText },
  });
  if (result.response.status === 409 && result.error) {
    const refused = result.error;
    return {
      ok: false,
      current_hash: refused.current_hash,
      ...('current_content' in refused ? { current_content: refused.current_content } : {}),
      ...('merged_content' in refused ? { merged_content: refused.merged_content } : {}),
      ...('regions' in refused ? { regions: refused.regions } : {}),
    };
  }
  // The route answers with the hash of what it wrote, so nothing here needs a
  // hash function and nothing needs a second read to learn it.
  const body = decode(result, `Failed to save ${path}`);
  if (body.merged) {
    return { ok: true, content_hash: body.content_hash, merged: true, content: body.content ?? '' };
  }
  return { ok: true, content_hash: body.content_hash };
}

/**
 * What a refused patch answers.
 *
 * `failed` names why each edit could not be applied, and which arm carries
 * `matches` or `other`. It is empty when the base alone refused the batch: no
 * anchor was read.
 * `stale_base` says the file moved on since `base_hash`, which makes the daemon
 * refuse before it reads an anchor, even one that still applies.
 */
export type PatchRefused = Extract<Schemas['FileWriteConflict'], { failed: unknown }> & {
  /** The route's `ok` is a `bool` field it only ever sets false, so the union
   * above cannot discriminate on it. Pinning it here is what lets a caller
   * write `if (!answer.ok)`. */
  ok: false;
};

/**
 * Change a note's LINES, or change nothing.
 *
 * The whole batch is matched against the file as it is on disk and applied
 * all-or-none, so two clients editing different parts of one note do not
 * overwrite each other the way `saveFileContent` does — that sends the whole
 * body and the daemon writes it blind.
 *
 * A refusal is a value, not a throw: it names the edit and why, because a
 * caller that can only say "failed" makes the user re-read the file.
 */
export async function patchKilnFile(
  path: string,
  edits: AnchoredEdit[],
  baseHash?: string,
): Promise<{ ok: true; content_hash: string } | PatchRefused> {
  const result = await client.PATCH('/api/kiln/file', {
    body: { path, edits, base_hash: baseHash },
  });
  // Only the anchored-edit arm of the conflict is a value here. A 409 in one
  // of the other two shapes is a refusal this caller cannot act on, so it
  // throws with the daemon's reason like any other failure.
  if (result.response.status === 409 && result.error && 'failed' in result.error) {
    return { ...result.error, ok: false };
  }
  const body = decode(result, `Failed to edit ${path}`);
  return { ok: true, content_hash: body.content_hash };
}

// =============================================================================
// Mock API (for standalone development without backend)
// =============================================================================
// const delay = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));
//
// export async function sendChatMessageMock(
//   message: string,
//   onChunk: (chunk: string) => void
// ): Promise<void> {
//   await delay(300);
//   const response = getMockResponse(message);
//   for (const char of response) {
//     await delay(15);
//     onChunk(char);
//   }
// }
//
// function getMockResponse(message: string): string {
//   const lower = message.toLowerCase();
//   if (lower.includes('hello') || lower.includes('hi')) {
//     return "Hello! I'm a mock assistant running entirely in your browser.";
//   }
//   if (lower.includes('test')) {
//     return "This is a test response. The chat is working correctly!";
//   }
//   return `You said: "${message}"\n\nThis is a mock response.`;
// }

// =============================================================================
// Layout Persistence Endpoints
// =============================================================================

import type { SerializedLayout, StoredLayout } from '@/windowing';
import type { TabContentType } from '@/types/windowTypes';

export async function saveLayout(layout: SerializedLayout<TabContentType>): Promise<void> {
  try {
    expectOk(await client.POST('/api/layout', { body: layout }), 'Failed to save layout');
  } catch (err) {
    console.warn(err instanceof Error ? err.message : 'Failed to save layout');
  }
}

/** The stored layout, at whatever version the server holds. */
export async function loadLayout(): Promise<StoredLayout<TabContentType> | null> {
  try {
    return openJson<StoredLayout<TabContentType>>(
      decode(await client.GET('/api/layout'), 'Failed to load layout'),
    );
  } catch (err) {
    if ((err as ApiError).status === 404) {
      return null;
    }
    console.warn(err instanceof Error ? err.message : 'Failed to load layout');
    return null;
  }
}

export async function resetLayout(): Promise<void> {
  try {
    expectOk(await client.DELETE('/api/layout'), 'Failed to reset layout');
  } catch (err) {
    console.warn(err instanceof Error ? err.message : 'Failed to reset layout');
  }
}

// =============================================================================
// Recently Opened Files (server-side, stored next to the layout blob)
// =============================================================================

/** Server-persisted recents, newest first. */
export async function fetchRecents(): Promise<{ absPath: string; name: string }[]> {
  const raw = decode(await client.GET('/api/recents'), 'Failed to load recents');
  return raw.recents.map((r) => ({ absPath: r.abs_path, name: r.name }));
}

/** Record a file open (fire-and-forget from the caller's perspective). */
export async function recordRecent(absPath: string, name: string): Promise<void> {
  expectOk(
    await client.POST('/api/recents', { body: { abs_path: absPath, name } }),
    'Failed to record recent file',
  );
}

// =============================================================================
// File-System Explorer Endpoints (Phase 1 web file tree)
// =============================================================================

/**
 * List one directory level inside a registered project (daemon `fs.list_dir`,
 * read-only). Kilns never use this path — their tree is built client-side from
 * `listNotes`. `relPath` is project-root-relative POSIX (`''` = the root).
 *
 * The tree shows ALL files (gitignored included — `show_ignored` is always
 * sent true); dotfiles stay behind the explicit `showHidden` toggle and
 * `.git` never lists (daemon policy).
 *
 * Goes through `rpc()` ([[Simplification Plan#Step 19]] item 3), so a
 * refusal reads the daemon's own sentence rather than a bare status; the
 * one error message names the method (`` RPC `fs.list_dir` failed ``), not
 * the root that was asked for. The params are the row's own
 * (`root` / `rel_path` / `show_ignored` / `show_hidden`), so a rename in
 * Rust fails the build here.
 *
 * `notify: false` rejects without a toast. The `@` completer uses it, because
 * a refused workspace root is not an error that the user can correct there.
 */
export async function listDir(
  root: string,
  relPath = '',
  showHidden = false,
  { notify = true }: { notify?: boolean } = {},
): Promise<FsListing> {
  return rpc(
    'fs.list_dir',
    { root, rel_path: relPath, show_ignored: true, show_hidden: showHidden },
    { notify },
  );
}

/**
 * Outcome of a move: kiln `.md` moves carry the wikilink-rewrite report.
 *
 * `rewritten_sources` names the sources whose inbound links were rewritten;
 * `skipped` names the inbound links intentionally left untouched (ambiguous or
 * stale).
 */
export type FsMoveOutcome = Schemas['FsMoveReply'];

/**
 * Move/rename a file or directory within one root (daemon `fs.move` — the
 * file-tree drag-and-drop backend). `kind` selects the daemon-side allowlist:
 * registered projects or already-open kilns. Overwrites are rejected
 * daemon-side; surface the error message to the user, don't retry. Kiln
 * `.md` moves route through the wikilink-aware rename daemon-side, so links
 * keep resolving; the outcome reports what was rewritten or skipped.
 */
export async function fsMove(
  root: string,
  kind: Schemas['FsRootKind'],
  fromRel: string,
  toRel: string,
): Promise<FsMoveOutcome> {
  return rpc('fs.move', { root, kind, from_rel: fromRel, to_rel: toRel });
}

/** Create a folder (and missing parents) inside one root. */
export async function fsMkdir(
  root: string,
  kind: Schemas['FsRootKind'],
  relPath: string,
): Promise<void> {
  await rpc('fs.mkdir', { root, kind, rel_path: relPath });
}

/**
 * Move a file or directory to the root's `.crucible/trash/` (recoverable by
 * hand; the trash dir is excluded from indexing/watching). Kiln notes leave
 * the link index immediately so backlinks re-resolve.
 */
export async function fsTrash(
  root: string,
  kind: Schemas['FsRootKind'],
  relPath: string,
): Promise<void> {
  await rpc('fs.trash', { root, kind, rel_path: relPath });
}

/**
 * SSE event names the filesystem domain of the `system` topic emits. Kept in
 * lockstep with the Rust `FsEvent::event_name()` (web/fs_events.rs). Each
 * event's `data` parses
 * to the `FsEvent` discriminated union.
 */
const FS_SSE_EVENT_TYPES = ['fs_changed', 'fs_deleted', 'fs_moved'] as const;

/**
 * The `type` each of those payloads carries, which is NOT the event name: the
 * stream says `fs_changed` and the body says `changed`.
 *
 * `satisfies` binds the tuple to the generated union, and `MissingFsEventTag`
 * closes the other direction, the way `SSE_EVENT_TYPES` does for chat.
 */
const FS_EVENT_TAGS = ['changed', 'deleted', 'moved'] as const satisfies readonly FsEvent['type'][];

/** Every daemon tag the tuple above forgot. Empty, or the build stops. */
type MissingFsEventTag = Exclude<FsEvent['type'], (typeof FS_EVENT_TAGS)[number]>;
const _FS_EVENT_TAGS_ARE_COMPLETE: [MissingFsEventTag] extends [never] ? true : never = true;
void _FS_EVENT_TAGS_ARE_COMPLETE;

const FS_EVENT_TAG_SET = new Set<string>(FS_EVENT_TAGS);

/**
 * Subscribe to live filesystem-change events: the `system` topic of the
 * shared connection (`GET /api/events`). In Phase 1 only watched kiln
 * directories emit these. Returns a cleanup function that leaves the topic.
 */
export function subscribeToFsEvents(
  onEvent: (event: FsEvent) => void,
  onOpen: () => void = () => {},
  onGap: () => void = () => {},
  onDisconnect: () => void = () => {},
): () => void {
  return joinEventsTopic(SYSTEM_TOPIC, {
    onFrame: (name, raw) => {
      if (!(FS_SSE_EVENT_TYPES as readonly string[]).includes(name)) return;
      try {
        onEvent(
          decodeEvent<FsEvent>(
            'file-system',
            name,
            raw,
            (payload) =>
              'type' in payload &&
              typeof payload.type === 'string' &&
              FS_EVENT_TAG_SET.has(payload.type),
          ),
        );
      } catch {
        console.warn(`Failed to parse FS SSE event (${name}):`, raw);
      }
    },
    onOpen,
    onGap,
    onDisconnect,
  });
}

/**
 * One event of the system stream. The `event` field copies the SSE event
 * name, so a consumer can tell the two shapes apart.
 *
 * Hand-written for the shape, not for the fields: the document's own
 * `SystemEvent` is `PublicationChangedEvent | ProposalChangedEvent` with NO
 * `event` field — the daemon's enum is `#[serde(untagged)]`, so the two
 * shapes on the wire differ only in the SSE frame's `event:` name, which
 * `subscribeToSystemEvents` reads separately from the JSON body. This type
 * puts that name back onto the parsed body, which the document cannot
 * describe; each variant's OTHER fields (`plugin`/`key`, `id`) are read off
 * the document's own shapes below, so a daemon rename still fails `tsc`.
 */
export type SystemEvent =
  | ({ event: 'publication_changed' } & Schemas['PublicationChangedEvent'])
  | ({ event: 'proposal_changed' } & Schemas['ProposalChangedEvent']);

/**
 * Subscribe to the daemon's system session: the `system` topic of the shared
 * connection (`GET /api/events`).
 *
 * The stream carries `publication_changed` and `proposal_changed`. The
 * connection reopens itself after each error with a backoff. Cleanup leaves
 * the topic; `reconnectEventsConnection` reconnects the shared connection by
 * hand.
 */
export function subscribeToSystemEvents(
  onEvent: (event: SystemEvent) => void,
  onOpen: () => void,
  onGap: () => void = () => {},
  onDisconnect: () => void = () => {},
): () => void {
  return joinEventsTopic(SYSTEM_TOPIC, {
    onFrame: (name, raw) => {
      if (name === 'publication_changed') {
        try {
          const payload = decodeEvent<{ plugin: string; key: string }>(
            'system',
            'publication_changed',
            raw,
            (p) =>
              'plugin' in p &&
              typeof p.plugin === 'string' &&
              'key' in p &&
              typeof p.key === 'string',
          );
          onEvent({ event: 'publication_changed', plugin: payload.plugin, key: payload.key });
        } catch {
          console.warn('Failed to parse system SSE event:', raw);
        }
      } else if (name === 'proposal_changed') {
        try {
          const payload = decodeEvent<{ id: string }>(
            'system',
            'proposal_changed',
            raw,
            (p) => 'id' in p && typeof p.id === 'string',
          );
          onEvent({ event: 'proposal_changed', id: payload.id });
        } catch {
          console.warn('Failed to parse system SSE event:', raw);
        }
      }
    },
    onOpen,
    onGap,
    onDisconnect,
  });
}

// ===========================================================================
// Canvas
// ===========================================================================

/**
 * Read a `.canvas` document.
 *
 * References that fail kiln containment come back redacted, with the node ids
 * and reasons in `rejected`. The offending paths are deliberately not returned,
 * so a quarantined node can be explained but never fetched.
 */
export async function getCanvas(path: string): Promise<CanvasResponse> {
  return openJson<CanvasResponse>(
    decode(
      await client.GET('/api/canvas', { params: { query: { path } } }),
      'Failed to load canvas',
    ),
  );
}

/** Write a `.canvas` document. Refused server-side if any reference escapes the kiln. */
export async function saveCanvas(path: string, canvas: CanvasDoc): Promise<void> {
  expectOk(
    await client.PUT('/api/canvas', { body: { path, content: JSON.stringify(canvas) } }),
    'Failed to save canvas',
  );
}

/**
 * One file's raw bytes (`GET /api/file/raw`).
 *
 * The URL itself lives in `lib/paths.ts` for the DOM surfaces that fetch it
 * themselves (a canvas media node, an inline image). The offline mirror needs
 * the BYTES, so the read happens here on the raw `fetch` — the answer is a
 * body, not a document the generated client could decode, the same trade
 * `login` makes.
 */
export async function fetchRawFile(path: string): Promise<Blob> {
  const response = await fetch(rawFileUrl(path));
  if (!response.ok) throw new Error(`attachment ${path}: ${response.status}`);
  return await response.blob();
}

/** Bases expressions are evaluated by the daemon. */
export async function queryBase(request: BaseRequest): Promise<BaseResult> {
  return decode(
    await client.GET('/api/bases/query', {
      params: {
        query: { kiln: request.kiln, ...request.source, view: request.view, this: request.this },
      },
    }),
    'Could not query base',
  );
}
export async function writeBaseProperty(request: SetPropertyParams): Promise<WriteOutcome> {
  return decode(await client.PUT('/api/bases/property', { body: request }), 'Base write refused');
}
export async function createBaseEntry(request: CreateEntryParams): Promise<WriteOutcome> {
  return decode(await client.POST('/api/bases/entries', { body: request }), 'Base write refused');
}
export async function reorderBaseGroups(request: ReorderGroupsParams): Promise<WriteOutcome> {
  return decode(
    await client.PUT('/api/bases/group-order', { body: request }),
    'Could not reorder groups',
  );
}

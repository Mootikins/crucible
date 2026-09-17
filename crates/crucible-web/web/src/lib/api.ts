import type { components } from './api-schema';
import { APP_CALLER, callerParam, client, decode, expectOk, type ApiError } from './api-client';
import { getBus } from './bus';
import type { CanvasDoc, CanvasResponse } from './canvas-types';
import { rawFileUrl } from './paths';
import type {
  AgentProfileEntry,
  AnchoredEdit,
  AppConfigNode,
  ChatEvent,
  CreateSessionParams,
  GrepHit,
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
  SessionHistoryResponse,
  SessionScope,
  SessionSearchResponse,
  SessionKnobSupport,
  SessionModes,
  SkillSummary,
  Surface,
  TargetProvider,
  FileEntry,
  NoteEntry,
  BacklinksResponse,
  KilnListEntry,
  FsListing,
  FsEvent,
  AgentConfigOptions,
} from './types';

/**
 * Wire shapes below are aliases into the generated contract
 * (`api-schema.d.ts`, written from `openapi.json`, written from the axum
 * router). A shape the document cannot describe — a plugin's own vocabulary,
 * or a value the browser assembles — keeps its hand-written form and says so.
 */
type Schemas = components['schemas'];

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
 * One keyed status slot a plugin published for a session.
 *
 * Rendered generically — `key`, `plugin` and `level` stay plain strings rather
 * than unions on purpose. The moment the frontend enumerates them, a new
 * plugin needs a frontend change to be visible at all, which is the thing this
 * channel exists to avoid.
 */
export type SessionStatusSlot = Schemas['SessionStatusSlot'];

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

/**
 * Send a chat message to a session.
 * Returns the assigned message_id. Does NOT stream events —
 * subscribe to events separately via `subscribeToEvents`.
 */
export async function sendChatMessage(
  sessionId: string,
  content: string,
): Promise<string> {
  return decode(
    await client.POST('/api/chat/send', {
      body: { session_id: sessionId, content },
    }),
    'Failed to send message',
    { notify: true },
  ).message_id;
}

/**
 * The SSE `event:` names `subscribeToEvents` installs a listener for.
 *
 * `satisfies` binds the tuple to the generated `ChatEvent` union, so a name
 * that is not a variant fails to compile. `MissingSseEventType` below closes
 * the other direction: a variant added in Rust and not listed here makes
 * `_SSE_EVENT_TYPES_ARE_COMPLETE` unassignable, which is the check the old
 * "append it here" comment asked a human to perform.
 *
 * `connection` is absent on purpose: the client mints it, the daemon never
 * sends it, so there is no server event to listen for.
 */
export const SSE_EVENT_TYPES = [
  'token',
  'tool_call',
  'tool_result',
  'tool_result_delta',
  'tool_result_complete',
  'tool_result_error',
  'thinking',
  'segment_complete',
  'message_complete',
  'error',
  'interaction_requested',
  'session_event',
  'subagent_spawned',
  'subagent_completed',
  'subagent_failed',
  'delegation_spawned',
  'delegation_completed',
  'delegation_failed',
  'context_usage',
  'precognition_result',
  'mode_changed',
  'title_changed',
] as const satisfies readonly Schemas['ChatEvent']['type'][];

/** Every daemon event name the tuple above forgot. Empty, or the build stops. */
type MissingSseEventType = Exclude<
  Schemas['ChatEvent']['type'],
  (typeof SSE_EVENT_TYPES)[number]
>;
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

/** Every `type` a chat event may carry, as a set the decode can ask. */
const CHAT_EVENT_TAGS = new Set<string>(SSE_EVENT_TYPES);

/** A chat payload tagged with a variant the document declares. */
function isChatEvent(payload: object): boolean {
  return 'type' in payload && typeof payload.type === 'string' && CHAT_EVENT_TAGS.has(payload.type);
}

/**
 * Subscribe to SSE events for a session.
 * Returns a cleanup function that closes the EventSource.
 *
 * Call this BEFORE sending a message so no events are missed.
 * Automatically reconnects on disconnect with exponential backoff.
 */
export function subscribeToEvents(
  sessionId: string,
  onEvent: (event: ChatEvent) => void,
  /**
   * Fires once, when the stream is first open. The server subscribes the
   * daemon session before returning stream headers, so "open" means events
   * will not be dropped — senders that must not lose the first tokens
   * (lazy-created sessions auto-sending their first message) wait for this.
   */
  onOpen?: () => void,
): () => void {
  // EventSource cannot set headers; the HttpOnly session cookie (set by
  // login()) authenticates the stream for non-localhost clients.
  const url = `/api/chat/events/${encodeURIComponent(sessionId)}`;
  let source: EventSource | null = null;
  let reconnectAttempts = 0;
  let reconnectTimeout: ReturnType<typeof setTimeout> | null = null;
  let closed = false;
  let opened = false;

  function connect() {
    if (closed) return;

    source = new EventSource(url);

    for (const eventType of SSE_EVENT_TYPES) {
      source.addEventListener(eventType, (e: MessageEvent) => {
        reconnectAttempts = 0;
        try {
          onEvent(decodeEvent<ChatEvent>('chat', eventType, e.data, isChatEvent));
        } catch {
          console.warn(`Failed to parse SSE event (${eventType}):`, e.data);
        }
      });
    }

    source.onerror = () => {
      if (closed) return;
      
      source?.close();
      source = null;
      
      reconnectAttempts++;
      const delay = Math.min(1000 * Math.pow(2, reconnectAttempts - 1), 30000);
      
      console.warn(`SSE disconnected, reconnecting in ${delay}ms (attempt ${reconnectAttempts})`);
      // Transient transport status — NOT a daemon 'error' (that path overwrites
      // the streaming message and nulls the streaming id, permanently losing
      // the in-flight turn on a routine idle reconnect).
      onEvent({ type: 'connection', status: 'reconnecting', message: 'Reconnecting…' });

      reconnectTimeout = setTimeout(connect, delay);
    };

    source.onopen = () => {
      reconnectAttempts = 0;
      if (!opened) {
        opened = true;
        onOpen?.();
      }
      onEvent({ type: 'connection', status: 'connected' });
    };
  }

  connect();

  return () => {
    closed = true;
    if (reconnectTimeout) {
      clearTimeout(reconnectTimeout);
    }
    source?.close();
  };
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
  return decode(
    await client.POST('/api/config', { body: { values } }),
    'Failed to save config',
  );
}



/**
 * Every command loaded plugins declared.
 *
 * The enumeration that has to exist before a primitive can be offered as a
 * button: the daemon has always known these, and nothing carried them to a
 * browser, so a caller could invoke a command it had no way to discover.
 */
export async function getPluginCommands(): Promise<PluginCommand[]> {
  const body = decode(
    await client.GET('/api/plugins/commands'),
    'Failed to list plugin commands',
  );
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
  const body = decode(
    await client.GET('/api/plugins/publications', {
      params: { query: { key }, header: callerParam(caller) },
    }),
    'Failed to get plugin publications',
  );
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
  const result = await client.POST('/api/plugins/command', {
    params: { header: callerParam(caller) },
    body: { name, args },
  });
  expectOk(result, `Plugin command '${name}' failed`);
  // Read whole rather than decoded: a command may answer nothing, and what it
  // does answer is its own vocabulary. The caller knows the shape it asked for.
  return result.data;
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
      targets_command:
        typeof decl.targets_command === 'string' ? decl.targets_command : undefined,
      resolve_command:
        typeof decl.resolve_command === 'string' ? decl.resolve_command : undefined,
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
    const list = Array.isArray(result)
      ? result
      : (result as { targets?: unknown } | null)?.targets;
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
  const data = decode(
    await client.GET('/api/session/list', {
      params: {
        query: {
          kiln: filters?.kiln,
          workspace: filters?.workspace,
          type: filters?.type,
          state: filters?.state,
          include_archived: filters?.includeArchived ? true : undefined,
        },
      },
    }),
    'Failed to list sessions',
  );
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
  const scope = (typeof kilns === 'string' ? [kilns] : kilns ?? []).filter(Boolean);
  const data = decode(
    await client.GET('/api/sessions/search', {
      params: { query: { q: query, kiln: scope, limit } },
    }),
    'Failed to search sessions',
  );
  return {
    ...data,
    matches: expectList(data.matches, 'matches', 'Failed to search sessions'),
  };
}

export async function getSession(id: string): Promise<Session> {
  return decode(
    await client.GET('/api/session/{id}', { params: { path: { id } } }),
    'Failed to get session',
  );
}

// =============================================================================
// Content Search (ripgrep) — POST /api/search/grep
// =============================================================================

export interface GrepResponse {
  hits: GrepHit[];
  truncated: boolean;
}


/**
 * Ripgrep content search over an absolute `root` (must be inside a registered
 * kiln or project — the daemon rejects anything else). `glob` filters by name
 * (e.g. `*.md` for notes); omit to search all files. Respects .gitignore.
 */
export async function grepSearch(
  root: string,
  query: string,
  opts?: { glob?: string; limit?: number; caseInsensitive?: boolean },
): Promise<GrepResponse> {
  const data = decode(
    await client.POST('/api/search/grep', {
      body: {
        root,
        query,
        glob: opts?.glob ?? null,
        limit: opts?.limit ?? 100,
        case_insensitive: opts?.caseInsensitive ?? true,
      },
    }),
    'Search failed',
  );
  return {
    truncated: data.truncated,
    hits: data.hits.map((h) => ({
      path: h.path,
      relPath: h.rel_path,
      line: h.line,
      text: h.text,
      matchStart: h.match_start,
      matchEnd: h.match_end,
    })),
  };
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
  return expectList(data.results, 'results', 'Failed to search notes').map((r) => ({
    path: r.path,
    relPath: r.rel_path,
    score: r.score,
  }));
}

/** Pause a session. */
export async function pauseSession(id: string): Promise<void> {
  expectOk(
    await client.POST('/api/session/{id}/pause', { params: { path: { id } } }),
    'Failed to pause session',
  );
}

/** Resume a session (also auto-subscribes to events on the backend). */
export async function resumeSession(id: string): Promise<void> {
  expectOk(
    await client.POST('/api/session/{id}/resume', { params: { path: { id } } }),
    'Failed to resume session',
  );
}

/** End a session. */
export async function endSession(id: string): Promise<void> {
  expectOk(
    await client.POST('/api/session/{id}/end', { params: { path: { id } } }),
    'Failed to end session',
  );
}

/** Delete a session permanently. */
export async function deleteSession(id: string): Promise<void> {
  expectOk(
    await client.DELETE('/api/session/{id}', { params: { path: { id } } }),
    'Failed to delete session',
  );
}

/** Archive a session (hide from default listing). */
export async function archiveSession(id: string): Promise<void> {
  expectOk(
    await client.POST('/api/session/{id}/archive', { params: { path: { id } } }),
    'Failed to archive session',
  );
}

/** Unarchive a session (restore to default listing). */
export async function unarchiveSession(id: string): Promise<void> {
  expectOk(
    await client.POST('/api/session/{id}/unarchive', { params: { path: { id } } }),
    'Failed to unarchive session',
  );
}

/** Cancel the current agent operation in a session. */
export async function cancelSession(id: string): Promise<boolean> {
  return decode(
    await client.POST('/api/session/{id}/cancel', { params: { path: { id } } }),
    'Failed to cancel session',
  ).cancelled;
}

/** List available models for a session. */
export async function listModels(sessionId: string): Promise<string[]> {
  return decode(
    await client.GET('/api/session/{id}/models', { params: { path: { id: sessionId } } }),
    'Failed to list models',
    { notify: true },
  ).models;
}

/**
 * The status slots plugins published for a session.
 *
 * There is no SSE event for plugin status, so callers fetch on session change
 * rather than subscribing.
 */
export async function getSessionStatus(sessionId: string): Promise<SessionStatusSlot[]> {
  return decode(
    await client.GET('/api/session/{id}/status', { params: { path: { id: sessionId } } }),
    'Failed to load session status',
  ).status;
}

/** List the modes a session may enter, and the one it is in. */
/**
 * Which settings this session can change.
 *
 * A settings panel asks before it draws: an ACP session runs its own turn
 * loop, so the daemon's caps and context policy, and offering one is a control that changes nothing.
 */
export async function listKnobs(sessionId: string): Promise<SessionKnobSupport> {
  return decode(
    await client.GET('/api/session/{id}/knobs', { params: { path: { id: sessionId } } }),
    'Failed to list settings',
  );
}

/**
 * The settings this session's external agent advertised for itself.
 *
 * Empty until the first message: an agent says what it has when the daemon
 * connects to it. Empty always for an internal agent.
 */
export async function listAgentOptions(sessionId: string): Promise<AgentConfigOptions> {
  return decode(
    await client.GET('/api/session/{id}/config/agent-options', {
      params: { path: { id: sessionId } },
    }),
    'Failed to list agent settings',
  );
}

/** Set one of the agent's own settings. */
export async function setAgentOption(
  sessionId: string,
  optionId: string,
  value: string,
): Promise<void> {
  expectOk(
    await client.POST('/api/session/{id}/config/agent-options', {
      params: { path: { id: sessionId } },
      body: { option_id: optionId, value },
    }),
    'Failed to set agent setting',
  );
}

export async function listModes(sessionId: string): Promise<SessionModes> {
  return decode(
    await client.GET('/api/session/{id}/modes', { params: { path: { id: sessionId } } }),
    'Failed to list modes',
    { notify: true },
  );
}

/** Switch the model for a session. */
export async function switchModel(sessionId: string, modelId: string): Promise<void> {
  expectOk(
    await client.POST('/api/session/{id}/model', {
      params: { path: { id: sessionId } },
      body: { model_id: modelId },
    }),
    'Failed to switch model',
  );
}

/** Set the session mode (normal/plan/auto). Confirmation echoes back as a
 * mode_changed SSE event. */
export async function setSessionMode(sessionId: string, mode: string): Promise<void> {
  expectOk(
    await client.POST('/api/session/{id}/mode', {
      params: { path: { id: sessionId } },
      body: { mode },
    }),
    'Failed to set session mode',
  );
}

/** Set the title for a session. */
export async function setSessionTitle(sessionId: string, title: string): Promise<void> {
  expectOk(
    await client.PUT('/api/session/{id}/title', {
      params: { path: { id: sessionId } },
      body: { title },
    }),
    'Failed to set session title',
  );
}

export async function getSessionHistory(
  sessionId: string,
  limit?: number,
  offset?: number,
  signal?: AbortSignal,
): Promise<SessionHistoryResponse> {
  return decode(
    await client.GET('/api/session/{id}/history', {
      params: { path: { id: sessionId }, query: { limit, offset } },
      signal,
    }),
    'Failed to load session history',
  );
}

/** List available LLM providers and their models. */
export async function listProviders(): Promise<ProviderInfo[]> {
  const data = decode(await client.GET('/api/providers'), 'Failed to list providers');
  return expectList(data.providers, 'providers', 'Failed to list providers');
}

/** Attach a kiln to the session's kiln set. Idempotent. */
export async function connectSessionKiln(sessionId: string, kiln: string): Promise<SessionScope> {
  return decode(
    await client.POST('/api/session/{id}/kilns/connect', {
      params: { path: { id: sessionId } },
      body: { kiln },
    }),
    'Failed to attach kiln',
  );
}

/** Detach a kiln from the session's kiln set. Any member may be detached. */
export async function disconnectSessionKiln(
  sessionId: string,
  kiln: string,
): Promise<SessionScope> {
  return decode(
    await client.POST('/api/session/{id}/kilns/disconnect', {
      params: { path: { id: sessionId } },
      body: { kiln },
    }),
    'Failed to detach kiln',
  );
}


/** List ACP agent profiles with probed availability. */
export async function listAgents(): Promise<AgentProfileEntry[]> {
  return decode(await client.GET('/api/agents'), 'Failed to list agents').agents;
}

/**
 * List all chat models across providers — no session required.
 *
 * Takes no kiln. The route used to accept `?kiln=<path>` and forward the raw
 * directory to the daemon's classification resolver, and no caller ever sent
 * one; the parameter is gone from both sides rather than converted to a name.
 */
export async function listAllModels(): Promise<string[]> {
  return decode(await client.GET('/api/models'), 'Failed to list models', { notify: true })
    .models;
}

// =============================================================================
// Session Config Endpoints
// =============================================================================

/** Get the precognition state for a session. */
export async function getPrecognition(sessionId: string): Promise<boolean> {
  return decode(
    await client.GET('/api/session/{id}/config/precognition', {
      params: { path: { id: sessionId } },
    }),
    'Failed to get precognition',
  ).precognition_enabled;
}

/** Set the precognition state for a session. */
export async function setPrecognition(sessionId: string, enabled: boolean): Promise<void> {
  expectOk(
    await client.PUT('/api/session/{id}/config/precognition', {
      params: { path: { id: sessionId } },
      body: { enabled },
    }),
    'Failed to set precognition',
  );
}

// -----------------------------------------------------------------------------
// The nine session config knobs the daemon advertised but the web could not
// reach. Gate A2e (crucible-cli/tests/architecture_tests.rs) fails when a knob
// in the daemon's METHODS list has no route; gate A2c fails when a path named
// here has no backend route, so these two directions are both covered.
//
// The request/response field names are the DAEMON's wire names, which are not
// always the knob name — `execution-timeout` carries `timeout_secs`. Renaming
// one of these to match its route would 200 and drop the value.
// -----------------------------------------------------------------------------

/** Get the context-assembly strategy, by its string spelling. */
export async function getContextStrategy(sessionId: string): Promise<string | null> {
  return decode(
    await client.GET('/api/session/{id}/config/context-strategy', {
      params: { path: { id: sessionId } },
    }),
    'Failed to get context strategy',
  ).context_strategy ?? null;
}

/**
 * Set the context-assembly strategy.
 *
 * No client-side allowlist of names: the daemon parses the string and answers
 * 422 for one it does not know, so a list here would be a second place to update
 * every time the enum grows.
 */
export async function setContextStrategy(sessionId: string, strategy: string): Promise<void> {
  expectOk(
    await client.PUT('/api/session/{id}/config/context-strategy', {
      params: { path: { id: sessionId } },
      body: { context_strategy: strategy },
    }),
    'Failed to set context strategy',
  );
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

/** Execute a slash command in a session. */
export async function executeCommand(sessionId: string, command: string): Promise<CommandResult> {
  return decode(
    await client.POST('/api/session/{id}/command', {
      params: { path: { id: sessionId } },
      body: { command },
    }),
    'Failed to execute command',
  );
}

/** One completable slash command. `args` is the argument placeholder, empty
 * for nullary commands. */
export type SlashCommand = Schemas['SlashCommand'];

/**
 * The slash commands the composer can complete.
 *
 * Served by the server from the same constant `execute_command` dispatches on,
 * so the completion list can't drift from what actually runs — the previously
 * hand-maintained frontend copy had already lost `/models`.
 */
export async function listSlashCommands(): Promise<SlashCommand[]> {
  return decode(await client.GET('/api/commands'), 'Failed to list commands').commands;
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
 * Every surface a plugin declared (`GET /api/surfaces`).
 *
 * Rows come with the list, so a panel draws on first paint rather than showing an
 * empty sidebar and filling in.
 */
export async function getSurfaces(): Promise<Surface[]> {
  return decode(await client.GET('/api/surfaces'), 'Failed to list plugin surfaces').surfaces;
}

/**
 * Subscribe to surface changes (`GET /api/surfaces/events`). Mirrors
 * `subscribeToFsEvents`: one `EventSource` with exponential-backoff reconnect.
 * Returns a cleanup function that closes the stream.
 */
export function subscribeToSurfaceEvents(
  onEvent: (event: SurfaceChangedEvent) => void,
): () => void {
  const url = '/api/surfaces/events';
  let source: EventSource | null = null;
  let reconnectAttempts = 0;
  let reconnectTimeout: ReturnType<typeof setTimeout> | null = null;
  let closed = false;

  function connect() {
    if (closed) return;
    source = new EventSource(url);

    source.addEventListener('surface_changed', (e: MessageEvent) => {
      reconnectAttempts = 0;
      try {
        // The payload carries no tag of its own — the stream has one event
        // name — so the check is the three fields the document requires.
        onEvent(
          decodeEvent<SurfaceChangedEvent>(
            'surface',
            'surface_changed',
            e.data,
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
        console.warn('Failed to parse surface SSE event:', e.data);
      }
    });

    source.onerror = () => {
      if (closed) return;
      source?.close();
      source = null;
      reconnectAttempts++;
      const delay = Math.min(1000 * Math.pow(2, reconnectAttempts - 1), 30000);
      reconnectTimeout = setTimeout(connect, delay);
    };
  }

  connect();

  return () => {
    closed = true;
    if (reconnectTimeout) clearTimeout(reconnectTimeout);
    source?.close();
  };
}

/**
 * Rich plugin metadata from `GET /api/plugins`.
 *
 * `version` is null when the plugin has no `spec.luau`: the version is declared
 * in its fragment. `last_error` says why a plugin is not Active, and is null
 * for a healthy one.
 */
export type PluginInfo = Schemas['PluginRow'];

/** Plugin reload response (counts of reloaded capabilities). */
export type PluginReloadResult = Schemas['PluginReloadResponse'];

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

export type InstallPluginParams = Schemas['InstallRequest'];

/**
 * What an install did.
 *
 * `manifest` is the path of the file the install wrote — the field is NOT
 * `plugins_toml`, which is what this file used to call it. `loaded` says
 * whether the plugin actually activated on the running daemon: "installed"
 * must not read as success while the plugin sits broken.
 */
export type InstallPluginResult = Schemas['PluginInstallResponse'];

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
export type RemovePluginResult = Schemas['PluginRemoveResponse'];

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
// Skills Endpoints
// =============================================================================

export type SkillDetail = Schemas['SkillDetail'];

/** List skills discovered for a kiln, optionally filtered by scope. */
export async function listSkills(kiln: string, scope?: string): Promise<SkillSummary[]> {
  return decode(
    await client.GET('/api/skills', { params: { query: { kiln, scope } } }),
    'Failed to list skills',
  ).skills;
}

/** Fetch a skill's full body and metadata. */
export async function getSkill(name: string, kiln: string): Promise<SkillDetail> {
  return decode(
    await client.GET('/api/skills/{name}', { params: { path: { name }, query: { kiln } } }),
    'Failed to load skill',
  );
}

/** Server-side skills search (case-insensitive name + description match). */
export async function searchSkills(
  query: string,
  kiln: string,
  limit?: number,
): Promise<SkillSummary[]> {
  return decode(
    await client.GET('/api/skills/search', { params: { query: { kiln, q: query, limit } } }),
    'Failed to search skills',
  ).skills;
}

// =============================================================================
// MCP Endpoints
// =============================================================================

/** Get MCP server status. */
/**
 * Whether the kiln's MCP server runs, and how to reach it.
 *
 * A two-arm union, not one open record: a stopped server answers `running`
 * alone, and only a running one names its transport, its port and its kiln.
 * Narrow on `running` before reading the rest.
 */
export type McpStatus = Schemas['McpStatus'];

export async function getMcpStatus(): Promise<McpStatus> {
  return decode(await client.GET('/api/mcp/status'), 'Failed to get MCP status');
}

// =============================================================================
// Search Endpoints
// =============================================================================

/**
 * List available kilns. Returns the daemon's object shape verbatim
 * (`{ path, name, last_access_secs_ago }`) — see `KilnListEntry`. The route
 * (`GET /api/kilns`) wraps the array under `{ kilns }`.
 */
export async function listKilns(): Promise<KilnListEntry[]> {
  return decode(await client.GET('/api/kilns'), 'Failed to list kilns').kilns;
}

export async function listNotes(kiln: string, pathFilter?: string): Promise<NoteEntry[]> {
  return decode(
    await client.GET('/api/notes', { params: { query: { kiln, path_filter: pathFilter } } }),
    'Failed to list notes',
  ).notes;
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

/** Unregister a project. */
export async function unregisterProject(path: string): Promise<void> {
  expectOk(
    await client.POST('/api/project/unregister', { body: { path } }),
    'Failed to unregister project',
  );
}

/** List all registered projects. */
export async function listProjects(): Promise<Project[]> {
  return decode(await client.GET('/api/project/list'), 'Failed to list projects');
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

/** Get project by path. */
export async function getProject(path: string): Promise<Project | null> {
  try {
    return decode(
      await client.GET('/api/project/get', { params: { query: { path } } }),
      'Failed to get project',
    );
  } catch (err) {
    if ((err as ApiError).status === 404) {
      return null;
    }
    throw err;
  }
}

/** List files in a kiln directory. */
export async function listFiles(path: string): Promise<FileEntry[]> {
  return decode(
    await client.GET('/api/kiln/files', { params: { query: { kiln: path } } }),
    'Failed to list files',
  ).files;
}

/** List kiln notes. */
export async function listKilnNotes(kilnPath: string): Promise<FileEntry[]> {
  return decode(
    await client.GET('/api/kiln/notes', { params: { query: { kiln: kilnPath } } }),
    'Failed to list kiln notes',
  ).files;
}

/** Full note-link graph of a kiln (nodes + resolved/unresolved edges). */
export async function getKilnGraph(kilnPath: string): Promise<import('./graph/types').GraphDto> {
  return decode(
    await client.GET('/api/kiln/graph', { params: { query: { kiln: kilnPath } } }),
    'Failed to load graph',
  );
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
  expectOk(
    await client.PUT('/api/kiln/file', { body: { path, content } }),
    'Failed to save file',
  );
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
 * Goes through the generated client like every other call: a refused root
 * used to surface as `listDir failed: 422`, the status and nothing else, while
 * the daemon had said in a sentence which root it refused and why. The query
 * parameters are the document's own (`root` / `rel_path` / `show_ignored` /
 * `show_hidden`), so a rename in Rust fails the build here.
 */
export async function listDir(
  root: string,
  relPath = '',
  showHidden = false,
): Promise<FsListing> {
  return decode(
    await client.GET('/api/fs/list', {
      params: {
        query: { root, rel_path: relPath, show_ignored: true, show_hidden: showHidden },
      },
    }),
    `Failed to list ${root}`,
    { notify: true },
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
  kind: 'project' | 'kiln',
  fromRel: string,
  toRel: string,
): Promise<FsMoveOutcome> {
  return decode(
    await client.POST('/api/fs/move', {
      body: { root, kind, from_rel: fromRel, to_rel: toRel },
    }),
    'move failed',
  );
}

/** Create a folder (and missing parents) inside one root. */
export async function fsMkdir(
  root: string,
  kind: 'project' | 'kiln',
  relPath: string,
): Promise<void> {
  expectOk(
    await client.POST('/api/fs/mkdir', { body: { root, kind, rel_path: relPath } }),
    'mkdir failed',
  );
}

/**
 * Move a file or directory to the root's `.crucible/trash/` (recoverable by
 * hand; the trash dir is excluded from indexing/watching). Kiln notes leave
 * the link index immediately so backlinks re-resolve.
 */
export async function fsTrash(
  root: string,
  kind: 'project' | 'kiln',
  relPath: string,
): Promise<void> {
  expectOk(
    await client.POST('/api/fs/trash', { body: { root, kind, rel_path: relPath } }),
    'trash failed',
  );
}

/**
 * SSE event names the `/api/fs/events` stream emits. Kept in lockstep with the
 * Rust `FsEvent::event_name()` (web/fs_events.rs). Each event's `data` parses
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
 * Subscribe to live filesystem-change events (`GET /api/fs/events`). Mirrors
 * `subscribeToEvents`: one `EventSource`, exponential-backoff reconnect, cookie
 * auth. In Phase 1 only watched kiln directories emit these. Returns a cleanup
 * function that closes the stream.
 */
export function subscribeToFsEvents(onEvent: (event: FsEvent) => void): () => void {
  const url = '/api/fs/events';
  let source: EventSource | null = null;
  let reconnectAttempts = 0;
  let reconnectTimeout: ReturnType<typeof setTimeout> | null = null;
  let closed = false;

  function connect() {
    if (closed) return;

    source = new EventSource(url);

    for (const eventType of FS_SSE_EVENT_TYPES) {
      source.addEventListener(eventType, (e: MessageEvent) => {
        reconnectAttempts = 0;
        try {
          onEvent(
            decodeEvent<FsEvent>(
              'file-system',
              eventType,
              e.data,
              (payload) =>
                'type' in payload &&
                typeof payload.type === 'string' &&
                FS_EVENT_TAG_SET.has(payload.type),
            ),
          );
        } catch {
          console.warn(`Failed to parse FS SSE event (${eventType}):`, e.data);
        }
      });
    }

    source.onerror = () => {
      if (closed) return;
      source?.close();
      source = null;
      reconnectAttempts++;
      const delay = Math.min(1000 * Math.pow(2, reconnectAttempts - 1), 30000);
      reconnectTimeout = setTimeout(connect, delay);
    };
  }

  connect();

  return () => {
    closed = true;
    if (reconnectTimeout) clearTimeout(reconnectTimeout);
    source?.close();
  };
}

/**
 * Subscribe to plugin publication changes (`GET /api/plugins/events`).
 *
 * The one stream that does not reconnect: an error closes it, and a consumer
 * that must get back on reconnects the shared root in `lib/query/sse.ts`.
 * Returns a cleanup function that closes the stream.
 */
export function subscribeToPluginEvents(
  onEvent: (event: { plugin: string; key: string }) => void,
  onOpen: () => void,
): () => void {
  // EventSource cannot set headers; the HttpOnly session cookie (set by
  // login()) authenticates the stream for non-localhost clients.
  const source = new EventSource('/api/plugins/events');
  source.addEventListener('publication_changed', (e: MessageEvent) => {
    try {
      onEvent(
        decodeEvent<{ plugin: string; key: string }>(
          'plugin',
          'publication_changed',
          e.data,
          (payload) =>
            'plugin' in payload &&
            typeof payload.plugin === 'string' &&
            'key' in payload &&
            typeof payload.key === 'string',
        ),
      );
    } catch {
      // A malformed frame is not worth tearing the stream down for; the next
      // one will arrive, and a stale block is better than a dead one.
      console.warn('Failed to parse plugin SSE event:', e.data);
    }
  });
  source.onopen = () => onOpen();
  return () => source.close();
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
    decode(await client.GET('/api/canvas', { params: { query: { path } } }), 'Failed to load canvas'),
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

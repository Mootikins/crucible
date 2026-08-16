import type { CanvasDoc, CanvasResponse } from './canvas-types';
import type {
  AgentProfileEntry,
  ChatEvent,
  CreateSessionParams,
  Session,
  Project,
  FileEntry,
  NoteEntry,
  NoteContent,
  BacklinksResponse,
  ProviderInfo,
  KilnListEntry,
  FsListing,
  FsEvent,
  SessionModes,
} from './types';

export interface Config {
  kiln_path: string;
  /** Server allows non-loopback terminal/shell (opt-in env + API key). */
  remote_shell?: boolean;
}

/**
 * What plugins published about themselves, as `key -> plugin -> value`.
 *
 * The generic contribution channel. Values are opaque here so a contribution
 * kind added later needs no change in this file — a plugin states what it
 * offers and clients render it.
 */
export type PluginPublications = Record<string, Record<string, unknown>>;


/**
 * A plugin that provides session targets on one of the two axes.
 *
 * **workspace** answers *where do the files live?* — a worktree, a checkout on
 * another machine. **runtime** answers *where does the process run?* — a
 * container, an ssh host. They are orthogonal and compose: a session can run in
 * a container against a worktree, which is why one setting could never have
 * carried both.
 *
 * The targets themselves are not here. They are enumerated on demand through
 * `targets_command`, because the workspace axis is context-dependent — the
 * branch list depends on which project is selected, and changes when someone
 * creates a branch outside the app.
 */
export interface TargetProvider {
  /** The publishing plugin, and the prefix in a `provider:target` spec. */
  plugin: string;
  axis: 'workspace' | 'runtime';
  label: string;
  /** Plugin command that lists this provider's targets. */
  targets_command?: string;
  /**
   * Plugin command that materialises one target and answers with a path.
   * Workspace-axis only — a runtime provider resolves nothing, it relocates
   * the process.
   */
  resolve_command?: string;
}

/** One target a provider offered. */
export interface ProviderTarget {
  value: string;
  label: string;
  hint?: string;
  disabled?: boolean;
  /** `provider:target` — what `session.create` takes, built once here. */
  spec: string;
  /**
   * An existing path this target already resolves to, when the provider knows
   * one. The worktree provider fills it for branches that have a checkout,
   * which is what lets the session tree label a checkout with its branch and
   * the files-pane picker jump to it — without either asking the daemon for
   * its own copy of the branch list.
   */
  path?: string;
  /** Set when this target is the one currently in effect. */
  current?: boolean;
}

/**
 * One node of a plugin's settings tree.
 *
 * A projection of the plugin's Lua declaration, and deliberately shallow: the
 * renderer switches on `type` and reads `name`/`desc`, and knows nothing about
 * any particular option. `type` stays a plain string for the same reason
 * `level` does on a status slot — the moment this becomes a union, a plugin
 * declaring a widget kind added later renders as nothing instead of
 * degrading to a sensible default.
 *
 * `args` is present on groups. `values` on a select, already ordered by the
 * daemon. `writable` is false when no `set` is inherited, so a read-only
 * option renders as one rather than offering an edit that will be refused.
 */
export interface PluginOptionNode {
  key?: string;
  type: string;
  name?: string;
  desc?: string;
  order?: number;
  min?: number;
  max?: number;
  step?: number;
  values?: { value: unknown; label: string }[];
  disabled?: boolean;
  hidden?: boolean;
  writable?: boolean;
  args?: PluginOptionNode[];
}

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
export interface SessionStatusSlot {
  key: string;
  plugin: string;
  text: string;
  level: string;
}

type HttpMethod = 'GET' | 'POST' | 'PUT' | 'DELETE' | 'PATCH';

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
    // The success counterpart to `crucible:auth-required`. Modules that gave up
    // on a 401 need a signal to re-ask; without one, anything that cached a
    // failure stayed broken for the life of the page even after signing in
    // (the terminal's availability check did exactly that).
    if (res.ok) window.dispatchEvent(new CustomEvent('crucible:auth-ok'));
    return res.ok;
  } catch {
    return false;
  }
}

// Throttled so a burst of parallel 401s produces one prompt, not a storm.
let lastAuthNotify = 0;
function notifyAuthRequired(): void {
  try {
    const now = Date.now();
    if (now - lastAuthNotify < 5000) return;
    lastAuthNotify = now;
    window.dispatchEvent(new CustomEvent('crucible:auth-required'));
  } catch {
    // non-browser context
  }
}

export interface RequestOptions extends Omit<RequestInit, 'method'> {
  errorMessage?: string;
  parseAs?: 'json' | 'text' | 'none';
  includeErrorText?: boolean;
}

/**
 * The human half of a `WebError`, which every crucible-web route serializes as
 * `{"error": {code, message}}`.
 *
 * Throwing the raw body instead puts a JSON blob in a toast: the user reads
 * `{"error":{"code":422,"message":"Hunk no longer exists"}}` where the server
 * went to the trouble of writing a sentence. Non-envelope bodies (a plain-text
 * 500 from a proxy, an empty body) fall through unchanged.
 */
function errorBodyMessage(text: string): string {
  try {
    const message = (JSON.parse(text) as { error?: { message?: unknown } })?.error?.message;
    return typeof message === 'string' && message ? message : text;
  } catch {
    return text;
  }
}

interface ApiError extends Error {
  status: number;
}

function jsonRequest(body: unknown): Pick<RequestOptions, 'headers' | 'body'> {
  return {
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  };
}

/**
 * The one HTTP helper. Exported so feature slices (`review-api.ts`) get the
 * 401 re-prompt and the error-envelope unwrapping instead of reimplementing
 * "throw on !ok" and quietly losing both.
 */
export async function request<T>(
  method: HttpMethod,
  url: string,
  options: RequestOptions = {},
): Promise<T> {
  const { errorMessage = 'Request failed', parseAs = 'json', includeErrorText = false, ...init } = options;
  const res = await fetch(url, { method, ...init });

  if (!res.ok) {
    let errorText = '';
    if (includeErrorText) {
      errorText = errorBodyMessage(await res.text().catch(() => ''));
    }
    if (res.status === 401) {
      notifyAuthRequired();
    }
    const hint =
      res.status === 401
        ? ' — Unauthorized: sign in with the API key (from `cru web key` on the host)'
        : '';
    throw Object.assign(new Error((errorText || `${errorMessage}: HTTP ${res.status}`) + hint), {
      status: res.status,
    }) as ApiError;
  }

  if (parseAs === 'none') {
    return undefined as T;
  }

  if (parseAs === 'text') {
    return (await res.text()) as T;
  }

  return (await res.json()) as T;
}

// =============================================================================
// Chat Endpoints
// =============================================================================

/**
 * Send a chat message to a session.
 * Returns the assigned message_id. Does NOT stream events —
 * subscribe to events separately via `subscribeToEvents`.
 */
/**
 * Transcript id for the assistant response of a turn. The backend keys a
 * whole turn by one message_id (send response, user_message echo, and
 * message_complete all carry it); the user message takes the id itself and
 * the response takes this derived form, so live streaming, late-attaching
 * viewers, and history reconstruction all converge on identical ids.
 */
export function turnResponseId(messageId: string): string {
  return `${messageId}-response`;
}

/**
 * Transcript id for a pre-tool narration segment of a turn. A segmented turn
 * (text → tool → text) freezes each pre-tool text run into its own bubble;
 * the daemon's `segment_complete` event carries the turn's message_id and the
 * segment's 0-based index, and both live streaming and history reconstruction
 * derive the same id from them — so segmented turns converge on identical
 * bubbles across viewers and reload (mirrors `turnResponseId`).
 */
export function turnSegmentId(messageId: string, index: number): string {
  return `${messageId}-seg-${index}`;
}

/**
 * Strip the concatenated frozen-segment prefix off a turn's accumulated text
 * so the final bubble carries only the trailing (post-last-tool) narration.
 * The daemon's `message_complete` deliberately carries the WHOLE turn's text;
 * segments render as their own bubbles, so the final bubble must drop the
 * already-rendered prefix. Whitespace-exact; if `fullText` doesn't start with
 * the prefix (daemon shape drift) the text is returned verbatim. Shared by the
 * live reducer and history reconstruction so both produce identical final-bubble
 * content.
 */
export function stripFrozenPrefix(fullText: string, frozenSegments: string[]): string {
  const prefix = frozenSegments.join('');
  return prefix && fullText.startsWith(prefix) ? fullText.slice(prefix.length) : fullText;
}

export async function sendChatMessage(
  sessionId: string,
  content: string,
): Promise<string> {
  return (
    await request<{ message_id: string }>('POST', '/api/chat/send', {
      errorMessage: 'Failed to send message',
      ...jsonRequest({ session_id: sessionId, content }),
    })
  ).message_id;
}

/**
 * Subscribe to SSE events for a session.
 * Returns a cleanup function that closes the EventSource.
 *
 * Call this BEFORE sending a message so no events are missed.
 * Automatically reconnects on disconnect with exponential backoff.
 */
/**
 * The set of SSE event types the daemon emits and the frontend listens for.
 * Exported so tests can assert this list stays in sync with the reducer's
 * switch (in `chatEventReducer.ts`). When a new ChatEvent variant is added,
 * append it here and the reducer test will catch missing reducer handling.
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
] as const;

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
          const parsed = JSON.parse(e.data) as ChatEvent;
          onEvent(parsed);
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
export interface PendingInteractionEntry {
  session_id: string;
  request_id: string;
  request: import('./types').InteractionRequest;
}

/**
 * Aggregate pending interactions across all sessions (Inbox poll).
 * Returns [] on any failure so callers degrade gracefully against daemons
 * that predate the endpoint.
 */
export async function listPendingInteractions(): Promise<PendingInteractionEntry[]> {
  try {
    const resp = await request<{ pending: PendingInteractionEntry[] }>(
      'GET',
      '/api/interactions/pending',
      { errorMessage: 'Failed to list pending interactions' },
    );
    return resp.pending ?? [];
  } catch {
    return [];
  }
}

export async function respondToInteraction(
  sessionId: string,
  requestId: string,
  response: unknown,
): Promise<void> {
  await request<void>('POST', '/api/interaction/respond', {
    errorMessage: 'Failed to respond',
    parseAs: 'none',
    ...jsonRequest({ session_id: sessionId, request_id: requestId, response }),
  });
}

// =============================================================================
// Config Endpoints
// =============================================================================

/** Get server configuration including the configured kiln path. */
export async function getConfig(): Promise<Config> {
  return request<Config>('GET', '/api/config', { errorMessage: 'Failed to get config' });
}

/** Everything plugins published, keyed by contribution kind then plugin. */
export async function getPluginPublications(): Promise<PluginPublications> {
  const body = await request<{ publications?: PluginPublications }>(
    'GET',
    '/api/plugins/publications',
    { errorMessage: 'Failed to get plugin publications' },
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
  const body = await request<{ options?: PluginOptions }>('GET', '/api/plugins/options', {
    errorMessage: 'Failed to get plugin settings',
  });
  return body.options ?? {};
}

/** Read one option's current value. */
export async function getPluginOption(plugin: string, path: string[]): Promise<unknown> {
  const body = await request<{ value?: unknown }>(
    'POST',
    `/api/plugins/${encodeURIComponent(plugin)}/option`,
    { ...jsonRequest({ action: 'get', path }), errorMessage: 'Failed to read plugin setting' },
  );
  return body.value;
}

/** Write one option. The plugin's own setter decides what that means. */
export async function setPluginOption(
  plugin: string,
  path: string[],
  value: unknown,
): Promise<void> {
  await request('POST', `/api/plugins/${encodeURIComponent(plugin)}/option`, {
    ...jsonRequest({ action: 'set', path, value }),
    errorMessage: 'Failed to change plugin setting',
  });
}

/** Press a `type = "execute"` node. */
export async function executePluginOption(plugin: string, path: string[]): Promise<void> {
  await request('POST', `/api/plugins/${encodeURIComponent(plugin)}/option`, {
    ...jsonRequest({ action: 'execute', path }),
    errorMessage: 'Plugin action failed',
  });
}


/**
 * Invoke a plugin command and hand back what it returned.
 *
 * Untyped by design — the caller knows the shape it asked for, and a schema
 * here would be one only today's plugins could satisfy.
 */
export async function runPluginCommand(name: string, args: unknown = {}): Promise<unknown> {
  return request<unknown>('POST', '/api/plugins/command', {
    ...jsonRequest({ name, args }),
    errorMessage: `Plugin command '${name}' failed`,
  });
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
  const path = (answer as { path?: unknown } | null)?.path;
  if (typeof path !== 'string' || !path) {
    throw new Error(`Plugin '${plugin}' resolved '${spec}' to no path`);
  }
  return path;
}

// =============================================================================
// Session Endpoints
// =============================================================================

interface RawSession {
  session_id: string;
  type: Session['session_type'];
  kilns?: string[];
  workspace: string;
  state: Session['state'];
  title: string | null;
  // Two endpoint shapes: session.list sends a flattened top-level `agent_model`;
  // session.get sends the nested `agent` object (model/mode live inside it) and
  // NO top-level agent_model. mapSession reads both so getSession()'s model
  // isn't silently null.
  agent_model?: string | null;
  agent?: { model?: string | null; mode?: string | null } | null;
  started_at: string;
  last_activity?: string | null;
  event_count?: number;
  archived?: boolean;
}

function mapSession(raw: RawSession): Session {
  return {
    id: raw.session_id,
    session_type: raw.type,
    kilns: raw.kilns ?? [],
    workspace: raw.workspace,
    state: raw.state,
    title: raw.title,
    agent_model: raw.agent_model ?? raw.agent?.model ?? null,
    agent_mode: raw.agent?.mode ?? null,
    started_at: raw.started_at,
    last_activity: raw.last_activity ?? null,
    event_count: raw.event_count ?? 0,
    archived: raw.archived ?? false,
  };
}

export async function createSession(params: CreateSessionParams): Promise<Session> {
  return mapSession(
    await request<RawSession>('POST', '/api/session', {
      errorMessage: 'Failed to create session',
      ...jsonRequest(params),
    }),
  );
}

/** List sessions with optional filters. */
export async function listSessions(filters?: {
  kiln?: string;
  workspace?: string;
  type?: string;
  state?: string;
  includeArchived?: boolean;
}): Promise<Session[]> {
  const params = new URLSearchParams();
  if (filters?.kiln) params.set('kiln', filters.kiln);
  if (filters?.workspace) params.set('workspace', filters.workspace);
  if (filters?.type) params.set('type', filters.type);
  if (filters?.state) params.set('state', filters.state);
  if (filters?.includeArchived) params.set('include_archived', 'true');

  const qs = params.toString();
  const url = qs ? `/api/session/list?${qs}` : '/api/session/list';

  const data = await request<{ sessions: RawSession[]; total: number }>('GET', url, {
    errorMessage: 'Failed to list sessions',
  });
  return data.sessions.map(mapSession);
}

/**
 * Search sessions by title/content.
 *
 * `kilns` is the scope, and the scope rule is kiln-set *overlap* — a result
 * needs to share at least one kiln with it. Pass the caller's whole set, not
 * one member: a member stands only for the sessions that share that member.
 */
export async function searchSessions(
  query: string,
  kilns?: string | string[],
  limit?: number,
): Promise<Session[]> {
  const params = new URLSearchParams({ q: query });
  for (const kiln of typeof kilns === 'string' ? [kilns] : kilns ?? []) {
    if (kiln) params.append('kiln', kiln);
  }
  if (limit !== undefined) params.set('limit', limit.toString());

  const data = await request<RawSession[]>('GET', `/api/sessions/search?${params.toString()}`, {
    errorMessage: 'Failed to search sessions',
  });
  return data.map(mapSession);
}

export async function getSession(id: string): Promise<Session> {
  return mapSession(
    await request<RawSession>('GET', `/api/session/${encodeURIComponent(id)}`, {
      errorMessage: 'Failed to get session',
    }),
  );
}

// =============================================================================
// Content Search (ripgrep) — POST /api/search/grep
// =============================================================================

/** One matched line. `matchStart`/`matchEnd` are char offsets into `text`. */
export interface GrepHit {
  path: string;
  relPath: string;
  line: number;
  text: string;
  matchStart: number;
  matchEnd: number;
}
export interface GrepResponse {
  hits: GrepHit[];
  truncated: boolean;
}

interface RawGrepHit {
  path: string;
  rel_path: string;
  line: number;
  text: string;
  match_start: number;
  match_end: number;
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
  const data = await request<{ hits: RawGrepHit[]; truncated: boolean }>(
    'POST',
    '/api/search/grep',
    {
      errorMessage: 'Search failed',
      ...jsonRequest({
        root,
        query,
        glob: opts?.glob ?? null,
        limit: opts?.limit ?? 100,
        case_insensitive: opts?.caseInsensitive ?? true,
      }),
    },
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

/** One semantically-matched note. `score` is a bounded similarity (higher =
 * closer). `path` is absolute (open in the editor); `relPath` is kiln-relative
 * (display). Requires the kiln's notes to be embedded/processed. */
export interface SemanticHit {
  path: string;
  relPath: string;
  score: number;
}

interface RawSemanticHit {
  path: string;
  rel_path: string;
  document_id: string;
  score: number;
}

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
  const data = await request<{ results: RawSemanticHit[] }>('POST', '/api/search/semantic', {
    errorMessage: 'Semantic search failed',
    ...jsonRequest({ kiln, query, limit }),
  });
  return data.results.map((r) => ({
    path: r.path,
    relPath: r.rel_path,
    score: r.score,
  }));
}

/** Pause a session. */
export async function pauseSession(id: string): Promise<void> {
  await request<void>('POST', `/api/session/${encodeURIComponent(id)}/pause`, {
    errorMessage: 'Failed to pause session',
    parseAs: 'none',
  });
}

/** Resume a session (also auto-subscribes to events on the backend). */
export async function resumeSession(id: string): Promise<void> {
  await request<void>('POST', `/api/session/${encodeURIComponent(id)}/resume`, {
    errorMessage: 'Failed to resume session',
    parseAs: 'none',
  });
}

/** End a session. */
export async function endSession(id: string): Promise<void> {
  await request<void>('POST', `/api/session/${encodeURIComponent(id)}/end`, {
    errorMessage: 'Failed to end session',
    parseAs: 'none',
  });
}

/** Delete a session permanently. */
export async function deleteSession(id: string): Promise<void> {
  await request<void>('DELETE', `/api/session/${encodeURIComponent(id)}`, {
    errorMessage: 'Failed to delete session',
    parseAs: 'none',
  });
}

/** Archive a session (hide from default listing). */
export async function archiveSession(id: string): Promise<void> {
  await request<void>('POST', `/api/session/${encodeURIComponent(id)}/archive`, {
    errorMessage: 'Failed to archive session',
    parseAs: 'none',
  });
}

/** Unarchive a session (restore to default listing). */
export async function unarchiveSession(id: string): Promise<void> {
  await request<void>('POST', `/api/session/${encodeURIComponent(id)}/unarchive`, {
    errorMessage: 'Failed to unarchive session',
    parseAs: 'none',
  });
}

/** Cancel the current agent operation in a session. */
export async function cancelSession(id: string): Promise<boolean> {
  return (
    await request<{ cancelled: boolean }>('POST', `/api/session/${encodeURIComponent(id)}/cancel`, {
      errorMessage: 'Failed to cancel session',
    })
  ).cancelled;
}

/** List available models for a session. */
export async function listModels(sessionId: string): Promise<string[]> {
  return (
    await request<{ models: string[] }>('GET', `/api/session/${encodeURIComponent(sessionId)}/models`, {
      errorMessage: 'Failed to list models',
    })
  ).models;
}

/**
 * The status slots plugins published for a session.
 *
 * There is no SSE event for plugin status, so callers fetch on session change
 * rather than subscribing.
 */
export async function getSessionStatus(sessionId: string): Promise<SessionStatusSlot[]> {
  return (
    await request<{ status: SessionStatusSlot[] }>(
      'GET',
      `/api/session/${encodeURIComponent(sessionId)}/status`,
      { errorMessage: 'Failed to load session status' },
    )
  ).status;
}

/** List the modes a session may enter, and the one it is in. */
export async function listModes(sessionId: string): Promise<SessionModes> {
  return request<SessionModes>('GET', `/api/session/${encodeURIComponent(sessionId)}/modes`, {
    errorMessage: 'Failed to list modes',
  });
}

/** Switch the model for a session. */
export async function switchModel(sessionId: string, modelId: string): Promise<void> {
  await request<void>('POST', `/api/session/${encodeURIComponent(sessionId)}/model`, {
    errorMessage: 'Failed to switch model',
    parseAs: 'none',
    ...jsonRequest({ model_id: modelId }),
  });
}

/** Set the session mode (normal/plan/auto). Confirmation echoes back as a
 * mode_changed SSE event. */
export async function setSessionMode(sessionId: string, mode: string): Promise<void> {
  await request<void>('POST', `/api/session/${encodeURIComponent(sessionId)}/mode`, {
    errorMessage: 'Failed to set session mode',
    parseAs: 'none',
    ...jsonRequest({ mode }),
  });
}

/** Set the title for a session. */
export async function setSessionTitle(sessionId: string, title: string): Promise<void> {
  await request<void>('PUT', `/api/session/${encodeURIComponent(sessionId)}/title`, {
    errorMessage: 'Failed to set session title',
    parseAs: 'none',
    ...jsonRequest({ title }),
  });
}

/** Raw daemon event from session.jsonl (SessionEventMessage format). */
export interface DaemonHistoryEvent {
  /** Always "event" for persisted events. */
  type: string;
  session_id: string;
  /** Event kind: "user_message", "message_complete", "text_delta", "thinking", "tool_call", etc. */
  event: string;
  data: {
    content?: string;
    full_response?: string;
    message_id?: string;
    [key: string]: unknown;
  };
  timestamp?: string;
  seq?: number;
}

export interface SessionHistoryResponse {
  session_id: string;
  history: DaemonHistoryEvent[];
  total_events: number;
}

export async function getSessionHistory(
  sessionId: string,
  limit?: number,
  offset?: number,
  signal?: AbortSignal,
): Promise<SessionHistoryResponse> {
  const params = new URLSearchParams();
  if (limit !== undefined) params.set('limit', limit.toString());
  if (offset !== undefined) params.set('offset', offset.toString());

  return request<SessionHistoryResponse>(
    'GET',
    `/api/session/${encodeURIComponent(sessionId)}/history?${params.toString()}`,
    {
      errorMessage: 'Failed to load session history',
      signal,
    },
  );
}

/** List available LLM providers and their models. */
export async function listProviders(): Promise<ProviderInfo[]> {
  return (await request<{ providers: ProviderInfo[] }>('GET', '/api/providers', {
    errorMessage: 'Failed to list providers',
  })).providers;
}

/** Session scope echoed by kiln/workspace mutations. */
export interface SessionScope {
  session_id: string;
  kilns: string[];
  workspace: string;
}

/** Attach a kiln to the session's kiln set. Idempotent. */
export async function connectSessionKiln(sessionId: string, kiln: string): Promise<SessionScope> {
  return request<SessionScope>(
    'POST',
    `/api/session/${encodeURIComponent(sessionId)}/kilns/connect`,
    { errorMessage: 'Failed to attach kiln', ...jsonRequest({ kiln }) },
  );
}

/** Detach a kiln from the session's kiln set. Any member may be detached. */
export async function disconnectSessionKiln(
  sessionId: string,
  kiln: string,
): Promise<SessionScope> {
  return request<SessionScope>(
    'POST',
    `/api/session/${encodeURIComponent(sessionId)}/kilns/disconnect`,
    { errorMessage: 'Failed to detach kiln', ...jsonRequest({ kiln }) },
  );
}

/** Set (string) or detach (null) the session's workspace. */
export async function setSessionWorkspace(
  sessionId: string,
  workspace: string | null,
): Promise<SessionScope> {
  return request<SessionScope>(
    'PUT',
    `/api/session/${encodeURIComponent(sessionId)}/workspace`,
    { errorMessage: 'Failed to update workspace', ...jsonRequest({ workspace }) },
  );
}

/** List ACP agent profiles with probed availability. */
export async function listAgents(): Promise<AgentProfileEntry[]> {
  return (await request<{ agents: AgentProfileEntry[] }>('GET', '/api/agents', {
    errorMessage: 'Failed to list agents',
  })).agents;
}

/** List all chat models across providers — no session required. */
export async function listAllModels(kiln?: string): Promise<string[]> {
  const url = kiln ? `/api/models?kiln=${encodeURIComponent(kiln)}` : '/api/models';
  return (await request<{ models: string[] }>('GET', url, {
    errorMessage: 'Failed to list models',
  })).models;
}

// =============================================================================
// Session Config Endpoints
// =============================================================================

/** Get the thinking budget for a session. */
export async function getThinkingBudget(sessionId: string): Promise<number | null> {
  return (
    await request<{ thinking_budget: number | null }>(
      'GET',
      `/api/session/${encodeURIComponent(sessionId)}/config/thinking-budget`,
      { errorMessage: 'Failed to get thinking budget' },
    )
  ).thinking_budget;
}

/** Set the thinking budget for a session. */
export async function setThinkingBudget(sessionId: string, budget: number | null): Promise<void> {
  await request<void>('PUT', `/api/session/${encodeURIComponent(sessionId)}/config/thinking-budget`, {
    errorMessage: 'Failed to set thinking budget',
    parseAs: 'none',
    ...jsonRequest({ thinking_budget: budget }),
  });
}

/** Get the temperature for a session. */
export async function getTemperature(sessionId: string): Promise<number | null> {
  return (
    await request<{ temperature: number | null }>(
      'GET',
      `/api/session/${encodeURIComponent(sessionId)}/config/temperature`,
      { errorMessage: 'Failed to get temperature' },
    )
  ).temperature;
}

/** Set the temperature for a session. */
export async function setTemperature(sessionId: string, temperature: number): Promise<void> {
  await request<void>('PUT', `/api/session/${encodeURIComponent(sessionId)}/config/temperature`, {
    errorMessage: 'Failed to set temperature',
    parseAs: 'none',
    ...jsonRequest({ temperature }),
  });
}

/** Get the max tokens for a session. */
export async function getMaxTokens(sessionId: string): Promise<number | null> {
  return (
    await request<{ max_tokens: number | null }>(
      'GET',
      `/api/session/${encodeURIComponent(sessionId)}/config/max-tokens`,
      { errorMessage: 'Failed to get max tokens' },
    )
  ).max_tokens;
}

/** Set the max tokens for a session (null = unlimited). */
export async function setMaxTokens(sessionId: string, maxTokens: number | null): Promise<void> {
  await request<void>('PUT', `/api/session/${encodeURIComponent(sessionId)}/config/max-tokens`, {
    errorMessage: 'Failed to set max tokens',
    parseAs: 'none',
    ...jsonRequest({ max_tokens: maxTokens }),
  });
}

/** Get the precognition state for a session. */
export async function getPrecognition(sessionId: string): Promise<boolean> {
  return (
    await request<{ precognition_enabled: boolean }>(
      'GET',
      `/api/session/${encodeURIComponent(sessionId)}/config/precognition`,
      { errorMessage: 'Failed to get precognition' },
    )
  ).precognition_enabled;
}

/** Set the precognition state for a session. */
export async function setPrecognition(sessionId: string, enabled: boolean): Promise<void> {
  await request<void>('PUT', `/api/session/${encodeURIComponent(sessionId)}/config/precognition`, {
    errorMessage: 'Failed to set precognition',
    parseAs: 'none',
    ...jsonRequest({ enabled }),
  });
}

/** Get the precognition results-per-query count (1..=20) for a session. */
export async function getPrecognitionResults(sessionId: string): Promise<number> {
  return (
    await request<{ precognition_results: number }>(
      'GET',
      `/api/session/${encodeURIComponent(sessionId)}/config/precognition/results`,
      { errorMessage: 'Failed to get precognition results' },
    )
  ).precognition_results;
}

/** Set the precognition results-per-query count (1..=20) for a session. */
export async function setPrecognitionResults(sessionId: string, count: number): Promise<void> {
  await request<void>(
    'PUT',
    `/api/session/${encodeURIComponent(sessionId)}/config/precognition/results`,
    {
      errorMessage: 'Failed to set precognition results',
      parseAs: 'none',
      ...jsonRequest({ count }),
    },
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

/** Get the context budget (tokens of history a turn may carry). */
export async function getContextBudget(sessionId: string): Promise<number | null> {
  return (
    await request<{ context_budget: number | null }>(
      'GET',
      `/api/session/${encodeURIComponent(sessionId)}/config/context-budget`,
      { errorMessage: 'Failed to get context budget' },
    )
  ).context_budget;
}

/** Set the context budget. `null` restores the daemon's default. */
export async function setContextBudget(sessionId: string, budget: number | null): Promise<void> {
  await request<void>('PUT', `/api/session/${encodeURIComponent(sessionId)}/config/context-budget`, {
    errorMessage: 'Failed to set context budget',
    parseAs: 'none',
    ...jsonRequest({ context_budget: budget }),
  });
}

/** Get the context window size. */
export async function getContextWindow(sessionId: string): Promise<number | null> {
  return (
    await request<{ context_window: number | null }>(
      'GET',
      `/api/session/${encodeURIComponent(sessionId)}/config/context-window`,
      { errorMessage: 'Failed to get context window' },
    )
  ).context_window;
}

/** Set the context window size. `null` restores the daemon's default. */
export async function setContextWindow(sessionId: string, window: number | null): Promise<void> {
  await request<void>('PUT', `/api/session/${encodeURIComponent(sessionId)}/config/context-window`, {
    errorMessage: 'Failed to set context window',
    parseAs: 'none',
    ...jsonRequest({ context_window: window }),
  });
}

/** Get the autocompact threshold (0..1 fraction of the window). */
export async function getAutocompactThreshold(sessionId: string): Promise<number | null> {
  return (
    await request<{ autocompact_threshold: number | null }>(
      'GET',
      `/api/session/${encodeURIComponent(sessionId)}/config/autocompact-threshold`,
      { errorMessage: 'Failed to get autocompact threshold' },
    )
  ).autocompact_threshold;
}

/** Set the autocompact threshold. `null` restores the daemon's default. */
export async function setAutocompactThreshold(
  sessionId: string,
  threshold: number | null,
): Promise<void> {
  await request<void>(
    'PUT',
    `/api/session/${encodeURIComponent(sessionId)}/config/autocompact-threshold`,
    {
      errorMessage: 'Failed to set autocompact threshold',
      parseAs: 'none',
      ...jsonRequest({ autocompact_threshold: threshold }),
    },
  );
}

/** Get the agent-loop iteration cap. */
export async function getMaxIterations(sessionId: string): Promise<number | null> {
  return (
    await request<{ max_iterations: number | null }>(
      'GET',
      `/api/session/${encodeURIComponent(sessionId)}/config/max-iterations`,
      { errorMessage: 'Failed to get max iterations' },
    )
  ).max_iterations;
}

/** Set the agent-loop iteration cap. `null` restores the daemon's default. */
export async function setMaxIterations(sessionId: string, max: number | null): Promise<void> {
  await request<void>('PUT', `/api/session/${encodeURIComponent(sessionId)}/config/max-iterations`, {
    errorMessage: 'Failed to set max iterations',
    parseAs: 'none',
    ...jsonRequest({ max_iterations: max }),
  });
}

/**
 * Get the per-turn execution timeout, in seconds.
 *
 * The field is `timeout_secs`, not `execution_timeout`: the RPC method is
 * `session.set_execution_timeout` but its wire field never matched its name.
 */
export async function getExecutionTimeout(sessionId: string): Promise<number | null> {
  return (
    await request<{ timeout_secs: number | null }>(
      'GET',
      `/api/session/${encodeURIComponent(sessionId)}/config/execution-timeout`,
      { errorMessage: 'Failed to get execution timeout' },
    )
  ).timeout_secs;
}

/** Set the per-turn execution timeout. `null` restores the daemon's default. */
export async function setExecutionTimeout(sessionId: string, secs: number | null): Promise<void> {
  await request<void>(
    'PUT',
    `/api/session/${encodeURIComponent(sessionId)}/config/execution-timeout`,
    {
      errorMessage: 'Failed to set execution timeout',
      parseAs: 'none',
      ...jsonRequest({ timeout_secs: secs }),
    },
  );
}

/** Get how many times a failed output validation is retried. */
export async function getValidationRetries(sessionId: string): Promise<number | null> {
  return (
    await request<{ validation_retries: number | null }>(
      'GET',
      `/api/session/${encodeURIComponent(sessionId)}/config/validation-retries`,
      { errorMessage: 'Failed to get validation retries' },
    )
  ).validation_retries;
}

/** Set how many times a failed output validation is retried. Required, not nullable. */
export async function setValidationRetries(sessionId: string, retries: number): Promise<void> {
  await request<void>(
    'PUT',
    `/api/session/${encodeURIComponent(sessionId)}/config/validation-retries`,
    {
      errorMessage: 'Failed to set validation retries',
      parseAs: 'none',
      ...jsonRequest({ validation_retries: retries }),
    },
  );
}

/** Get the context-assembly strategy, by its string spelling. */
export async function getContextStrategy(sessionId: string): Promise<string | null> {
  return (
    await request<{ context_strategy: string | null }>(
      'GET',
      `/api/session/${encodeURIComponent(sessionId)}/config/context-strategy`,
      { errorMessage: 'Failed to get context strategy' },
    )
  ).context_strategy;
}

/**
 * Set the context-assembly strategy.
 *
 * No client-side allowlist of names: the daemon parses the string and answers
 * 422 for one it does not know, so a list here would be a second place to update
 * every time the enum grows.
 */
export async function setContextStrategy(sessionId: string, strategy: string): Promise<void> {
  await request<void>(
    'PUT',
    `/api/session/${encodeURIComponent(sessionId)}/config/context-strategy`,
    {
      errorMessage: 'Failed to set context strategy',
      parseAs: 'none',
      ...jsonRequest({ context_strategy: strategy }),
    },
  );
}

/** Get the output-validation mode, by its string spelling. */
export async function getOutputValidation(sessionId: string): Promise<string | null> {
  return (
    await request<{ output_validation: string | null }>(
      'GET',
      `/api/session/${encodeURIComponent(sessionId)}/config/output-validation`,
      { errorMessage: 'Failed to get output validation' },
    )
  ).output_validation;
}

/** Set the output-validation mode. The daemon validates the name; see above. */
export async function setOutputValidation(sessionId: string, validation: string): Promise<void> {
  await request<void>(
    'PUT',
    `/api/session/${encodeURIComponent(sessionId)}/config/output-validation`,
    {
      errorMessage: 'Failed to set output validation',
      parseAs: 'none',
      ...jsonRequest({ output_validation: validation }),
    },
  );
}

/** Get the session's system prompt override. */
export async function getSystemPrompt(sessionId: string): Promise<string | null> {
  return (
    await request<{ system_prompt: string | null }>(
      'GET',
      `/api/session/${encodeURIComponent(sessionId)}/config/system-prompt`,
      { errorMessage: 'Failed to get system prompt' },
    )
  ).system_prompt;
}

/** Set the session's system prompt override. */
export async function setSystemPrompt(sessionId: string, prompt: string): Promise<void> {
  await request<void>('PUT', `/api/session/${encodeURIComponent(sessionId)}/config/system-prompt`, {
    errorMessage: 'Failed to set system prompt',
    parseAs: 'none',
    ...jsonRequest({ system_prompt: prompt }),
  });
}

/**
 * Get the session mode.
 *
 * `setMode` has existed all along with no reader, so a panel could set a mode
 * and then keep rendering whatever it last guessed.
 */
export async function getMode(sessionId: string): Promise<string | null> {
  return (
    await request<{ mode: string | null }>(
      'GET',
      `/api/session/${encodeURIComponent(sessionId)}/mode`,
      { errorMessage: 'Failed to get session mode' },
    )
  ).mode;
}

// =============================================================================
// Session Export
// =============================================================================

/** Export a session to markdown. Returns the raw markdown string. */
export async function exportSession(sessionId: string): Promise<string> {
  return request<string>('POST', `/api/session/${encodeURIComponent(sessionId)}/export`, {
    errorMessage: 'Failed to export session',
    parseAs: 'text',
  });
}

// =============================================================================
// Slash Command Execution
// =============================================================================

export interface CommandResult {
  result: string;
  type: string;
}

/** Execute a slash command in a session. */
export async function executeCommand(sessionId: string, command: string): Promise<CommandResult> {
  return request<CommandResult>('POST', `/api/session/${encodeURIComponent(sessionId)}/command`, {
    errorMessage: 'Failed to execute command',
    ...jsonRequest({ command }),
  });
}

export interface SlashCommand {
  name: string;
  /** Argument placeholder, empty for nullary commands. */
  args: string;
  description: string;
}

/**
 * The slash commands the composer can complete.
 *
 * Served by the server from the same constant `execute_command` dispatches on,
 * so the completion list can't drift from what actually runs — the previously
 * hand-maintained frontend copy had already lost `/models`.
 */
export async function listSlashCommands(): Promise<SlashCommand[]> {
  return (
    await request<{ commands: SlashCommand[] }>('GET', '/api/commands', {
      errorMessage: 'Failed to list commands',
    })
  ).commands;
}

// =============================================================================
// Shell Execution Endpoints
// =============================================================================

export interface ShellEvent {
  type: 'stdout' | 'stderr' | 'exit' | 'error';
  data?: string;
  code?: number;
  message?: string;
}

/**
 * Execute a shell command and stream SSE events.
 * Uses fetch + ReadableStream since POST SSE can't use EventSource (GET-only).
 * Returns an AbortController to cancel the request.
 */
export function executeShell(
  command: string,
  onEvent: (event: ShellEvent) => void,
  onDone?: () => void,
  cwd?: string,
  timeoutSecs?: number,
): AbortController {
  const controller = new AbortController();

  const body: Record<string, unknown> = { command };
  if (cwd) body.cwd = cwd;
  if (timeoutSecs !== undefined) body.timeout_secs = timeoutSecs;

  fetch('/api/shell/exec', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
    signal: controller.signal,
  })
    .then(async (res) => {
      if (!res.ok) {
        onEvent({ type: 'error', message: `HTTP ${res.status}: ${res.statusText}` });
        onDone?.();
        return;
      }

      const reader = res.body?.getReader();
      if (!reader) {
        onEvent({ type: 'error', message: 'No response body' });
        onDone?.();
        return;
      }

      const decoder = new TextDecoder();
      let buffer = '';

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });

        // Parse SSE lines: "data: {...}\n\n"
        const lines = buffer.split('\n');
        buffer = lines.pop() ?? '';

        for (const line of lines) {
          const trimmed = line.trim();
          if (!trimmed || trimmed.startsWith(':')) continue;
          if (trimmed.startsWith('data: ')) {
            try {
              const parsed = JSON.parse(trimmed.slice(6)) as ShellEvent;
              onEvent(parsed);
            } catch {
              // Ignore malformed SSE data
            }
          }
        }
      }

      onDone?.();
    })
    .catch((err) => {
      if (err instanceof DOMException && err.name === 'AbortError') {
        // User cancelled — not an error
        onDone?.();
        return;
      }
      onEvent({ type: 'error', message: String(err) });
      onDone?.();
    });

  return controller;
}
// =============================================================================
// Plugin Endpoints
// =============================================================================

/**
 * Rich plugin metadata returned by `GET /api/plugins`. Mirrors the
 * `plugin_info` array in the daemon's `plugin.list` response. The legacy
 * `path` / `plugin_type` / `healthy` fields are gone — the daemon now
 * carries provenance (source), lifecycle state, capability counts, and
 * an absolute `dir`.
 */
export interface PluginInfo {
  name: string;
  version: string;
  source: 'User' | 'Runtime' | 'EnvPath' | 'Builtin' | string;
  state: 'Active' | 'Error' | 'Disabled' | string;
  /** Why the plugin is not Active. Null for healthy plugins. */
  last_error?: string | null;
  dir: string;
  tools: number;
  commands: number;
  handlers: number;
  services: number;
}

/** Plugin reload response (counts of reloaded capabilities). */
export interface PluginReloadResult {
  name: string;
  reloaded: boolean;
  tools: number;
  commands: number;
  handlers: number;
  services: number;
}

/** List discovered plugins with rich metadata. */
export async function getPlugins(): Promise<PluginInfo[]> {
  return (await request<{ plugins: PluginInfo[] }>('GET', `/api/plugins`, {
    errorMessage: 'Failed to list plugins',
  })).plugins;
}

/** Reload a plugin by name. Returns the daemon's capability counts. */
export async function reloadPlugin(name: string): Promise<PluginReloadResult> {
  return request<PluginReloadResult>(
    'POST',
    `/api/plugins/${encodeURIComponent(name)}/reload`,
    { errorMessage: 'Failed to reload plugin' },
  );
}

export interface InstallPluginParams {
  url: string;
  branch?: string;
  pin?: string;
}

export interface InstallPluginResult {
  name: string;
  outcome: { kind: 'cloned'; dest: string } | { kind: 'already_present' } | { kind: 'disabled' };
  plugins_toml: string;
  installed: boolean;
  // Whether the plugin actually activated on the running daemon. "Installed"
  // must not read as success while the plugin sits broken in the daemon.
  loaded: boolean;
  tools: number;
  commands: number;
  services: number;
  error: string | null;
}

/**
 * Install a plugin by URL. Synchronous — can take 10+ seconds for a
 * fresh clone over a slow network. Caller should show a spinner.
 */
export async function installPlugin(params: InstallPluginParams): Promise<InstallPluginResult> {
  return request<InstallPluginResult>('POST', '/api/plugins', {
    errorMessage: 'Failed to install plugin',
    ...jsonRequest(params),
  });
}

export interface RemovePluginResult {
  name: string;
  plugins_toml: string;
  purged_dir: string | null;
  // The TOML removal succeeded but deleting the directory failed.
  purge_error: string | null;
  // Removed without purge: the directory remains and loads again on the
  // next daemon restart or plugin install.
  kept_dir: string | null;
}

/** Remove a plugin by name. If `purge`, the cloned directory is also deleted. */
export async function removePlugin(name: string, purge = false): Promise<RemovePluginResult> {
  const params = new URLSearchParams();
  if (purge) params.set('purge', 'true');
  const query = params.toString() ? `?${params.toString()}` : '';
  return request<RemovePluginResult>(
    'DELETE',
    `/api/plugins/${encodeURIComponent(name)}${query}`,
    { errorMessage: 'Failed to remove plugin' },
  );
}

// =============================================================================
// Skills Endpoints
// =============================================================================

export interface SkillSummary {
  name: string;
  scope: string;
  description: string;
  shadowed_count: number;
}

export interface SkillDetail {
  name: string;
  scope: string;
  description: string;
  source_path: string;
  agent?: string | null;
  license?: string | null;
  body: string;
}

/** List skills discovered for a kiln, optionally filtered by scope. */
export async function listSkills(kiln: string, scope?: string): Promise<SkillSummary[]> {
  const params = new URLSearchParams({ kiln });
  if (scope) params.set('scope', scope);
  return (await request<{ skills: SkillSummary[] }>('GET', `/api/skills?${params.toString()}`, {
    errorMessage: 'Failed to list skills',
  })).skills;
}

/** Fetch a skill's full body and metadata. */
export async function getSkill(name: string, kiln: string): Promise<SkillDetail> {
  const params = new URLSearchParams({ kiln });
  return request<SkillDetail>(
    'GET',
    `/api/skills/${encodeURIComponent(name)}?${params.toString()}`,
    { errorMessage: 'Failed to load skill' },
  );
}

/** Server-side skills search (case-insensitive name + description match). */
export async function searchSkills(
  query: string,
  kiln: string,
  limit?: number,
): Promise<SkillSummary[]> {
  const params = new URLSearchParams({ kiln, q: query });
  if (limit !== undefined) params.set('limit', String(limit));
  return (await request<{ skills: SkillSummary[] }>(
    'GET',
    `/api/skills/search?${params.toString()}`,
    { errorMessage: 'Failed to search skills' },
  )).skills;
}

// =============================================================================
// MCP Endpoints
// =============================================================================

/** Get MCP server status. */
export async function getMcpStatus(): Promise<Record<string, unknown>> {
  return request<Record<string, unknown>>('GET', '/api/mcp/status', {
    errorMessage: 'Failed to get MCP status',
  });
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
  return (await request<{ kilns: KilnListEntry[] }>('GET', '/api/kilns', {
    errorMessage: 'Failed to list kilns',
  })).kilns;
}

export async function listNotes(kiln: string, pathFilter?: string): Promise<NoteEntry[]> {
  const params = new URLSearchParams({ kiln });
  if (pathFilter) params.set('path_filter', pathFilter);

  return (
    await request<{ notes: NoteEntry[] }>('GET', `/api/notes?${params.toString()}`, {
      errorMessage: 'Failed to list notes',
      includeErrorText: true,
    })
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
  const params = new URLSearchParams({ kiln, name });
  return request('GET', `/api/notes/resolve?${params.toString()}`, {
    errorMessage: 'Failed to resolve note',
    includeErrorText: true,
  });
}

export async function getNote(name: string, kiln: string): Promise<NoteContent> {
  const params = new URLSearchParams({ kiln });
  return request<NoteContent>('GET', `/api/notes/${encodeURIComponent(name)}?${params.toString()}`, {
    errorMessage: 'Failed to get note',
    includeErrorText: true,
  });
}

/**
 * Linked + unlinked mentions for a note. `note` accepts a note name or
 * kiln-relative path (fuzzy-resolved server-side).
 */
export async function getBacklinks(kiln: string, note: string): Promise<BacklinksResponse> {
  const params = new URLSearchParams({ kiln, note });
  return request<BacklinksResponse>('GET', `/api/backlinks?${params.toString()}`, {
    errorMessage: 'Failed to get backlinks',
    includeErrorText: true,
  });
}

export async function saveNote(name: string, kiln: string, content: string): Promise<void> {
  await request<void>('PUT', `/api/notes/${encodeURIComponent(name)}`, {
    errorMessage: 'Failed to save note',
    parseAs: 'none',
    includeErrorText: true,
    ...jsonRequest({ kiln, content }),
  });
}

/** Perform a vector search. */
export async function searchVectors(
  kiln: string,
  vector: number[],
  limit?: number,
): Promise<unknown[]> {
  const body: Record<string, unknown> = { kiln, vector };
  if (limit !== undefined) body.limit = limit;

  return (
    await request<{ results: unknown[] }>('POST', '/api/search/vectors', {
      errorMessage: 'Failed to search vectors',
      ...jsonRequest(body),
    })
  ).results;
}

// =============================================================================
// Project Endpoints
// =============================================================================

/** Register a project. */
export async function registerProject(path: string): Promise<Project> {
  return request<Project>('POST', '/api/project/register', {
    errorMessage: 'Failed to register project',
    ...jsonRequest({ path }),
  });
}

/** Unregister a project. */
export async function unregisterProject(path: string): Promise<void> {
  await request<void>('POST', '/api/project/unregister', {
    errorMessage: 'Failed to unregister project',
    parseAs: 'none',
    ...jsonRequest({ path }),
  });
}

/** List all registered projects. */
export async function listProjects(): Promise<Project[]> {
  return request<Project[]>('GET', '/api/project/list', { errorMessage: 'Failed to list projects' });
}

// =============================================================================
// SCM Endpoints (branch/worktree browsing)
// =============================================================================



export interface ScmWorktreeAddResponse {
  path: string;
  project: Project;
  warning: string | null;
}


export interface ScmCloneResponse {
  path: string;
  project: Project;
}

/** True when the add-project input reads as a REMOTE git repo rather than a
 * local path: https/ssh URLs and `owner/repo` GitHub shorthand. */
export function isGitRepoUrl(input: string): boolean {
  const s = input.trim();
  if (/^(https?:\/\/|git@)\S+$/.test(s)) return true;
  return /^[\w.-]+\/[\w.-]+$/.test(s) && !s.startsWith('.');
}


/** Clone a remote repo into `[scm] projects_dir` and register it as a
 * project. Slow (network clone) — no client-side timeout beyond fetch's. */
export async function scmClone(url: string): Promise<ScmCloneResponse> {
  return request<ScmCloneResponse>('POST', '/api/scm/clone', {
    errorMessage: 'Failed to clone repository',
    includeErrorText: true,
    ...jsonRequest({ url }),
  });
}

/** Get project by path. */
export async function getProject(path: string): Promise<Project | null> {
  const params = new URLSearchParams({ path });
  try {
    return await request<Project>('GET', `/api/project/get?${params.toString()}`, {
      errorMessage: 'Failed to get project',
    });
  } catch (err) {
    if ((err as ApiError).status === 404) {
      return null;
    }
    throw err;
  }
}

/** List files in a kiln directory. */
export async function listFiles(path: string): Promise<FileEntry[]> {
  const params = new URLSearchParams({ kiln: path });
  return (await request<{ files: FileEntry[] }>('GET', `/api/kiln/files?${params.toString()}`, {
    errorMessage: 'Failed to list files',
  })).files;
}

/** List kiln notes. */
export async function listKilnNotes(kilnPath: string): Promise<FileEntry[]> {
  const params = new URLSearchParams({ kiln: kilnPath });
  return (await request<{ files: FileEntry[] }>('GET', `/api/kiln/notes?${params.toString()}`, {
    errorMessage: 'Failed to list kiln notes',
  })).files;
}

/** Full note-link graph of a kiln (nodes + resolved/unresolved edges). */
export async function getKilnGraph(kilnPath: string): Promise<import('./graph/types').GraphDto> {
  const params = new URLSearchParams({ kiln: kilnPath });
  return request('GET', `/api/kiln/graph?${params.toString()}`, {
    errorMessage: 'Failed to load graph',
  });
}

/** Get file content by path. */
export async function getFileContent(path: string): Promise<string> {
  const params = new URLSearchParams({ path });
  return (await request<{ content: string }>('GET', `/api/kiln/file?${params.toString()}`, {
    errorMessage: 'Failed to get file content',
  })).content;
}

/** Save file content by path. */
export async function saveFileContent(path: string, content: string): Promise<void> {
  await request<void>('PUT', '/api/kiln/file', {
    errorMessage: 'Failed to save file',
    parseAs: 'none',
    ...jsonRequest({ path, content }),
  });
}

// =============================================================================
// Utilities
// =============================================================================

export function generateMessageId(): string {
  return `msg_${Date.now()}_${Math.random().toString(36).substring(2, 9)}`;
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

import type { SerializedLayout } from './layout-serializer';

export async function saveLayout(layout: SerializedLayout): Promise<void> {
  try {
    await request<void>('POST', '/api/layout', {
      errorMessage: 'Failed to save layout',
      parseAs: 'none',
      ...jsonRequest(layout),
    });
  } catch (err) {
    console.warn(err instanceof Error ? err.message : 'Failed to save layout');
  }
}

export async function loadLayout(): Promise<SerializedLayout | null> {
  try {
    return await request<SerializedLayout>('GET', '/api/layout', {
      errorMessage: 'Failed to load layout',
    });
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
    await request<void>('DELETE', '/api/layout', {
      errorMessage: 'Failed to reset layout',
      parseAs: 'none',
    });
  } catch (err) {
    console.warn(err instanceof Error ? err.message : 'Failed to reset layout');
  }
}

// =============================================================================
// Recently Opened Files (server-side, stored next to the layout blob)
// =============================================================================

interface RawRecent {
  abs_path: string;
  name: string;
  opened_at: number;
}

/** Server-persisted recents, newest first. */
export async function fetchRecents(): Promise<{ absPath: string; name: string }[]> {
  const raw = await request<{ recents: RawRecent[] }>('GET', '/api/recents', {
    errorMessage: 'Failed to load recents',
  });
  return raw.recents.map((r) => ({ absPath: r.abs_path, name: r.name }));
}

/** Record a file open (fire-and-forget from the caller's perspective). */
export async function recordRecent(absPath: string, name: string): Promise<void> {
  await request<unknown>('POST', '/api/recents', {
    errorMessage: 'Failed to record recent file',
    ...jsonRequest({ abs_path: absPath, name }),
  });
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
 * Bypasses `request()` to preserve the exact query-string contract the daemon
 * route parses (`root` / `rel_path` / `show_ignored` / `show_hidden`).
 */
export async function listDir(
  root: string,
  relPath = '',
  showHidden = false,
): Promise<FsListing> {
  const q = new URLSearchParams({
    root,
    rel_path: relPath,
    show_ignored: 'true',
    show_hidden: String(showHidden),
  });
  const res = await fetch(`/api/fs/list?${q}`, { credentials: 'same-origin' });
  if (!res.ok) {
    if (res.status === 401) notifyAuthRequired();
    throw new Error(`listDir failed: ${res.status}`);
  }
  return res.json();
}

/** Outcome of a move: kiln `.md` moves carry the wikilink-rewrite report. */
export interface FsMoveOutcome {
  moved: boolean;
  /** Sources whose inbound links were rewritten (kiln .md moves only). */
  rewritten_sources?: string[];
  /** Inbound links intentionally left untouched (ambiguous / stale). */
  skipped?: { source_path: string; raw_target: string; reason: string }[];
}

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
  const res = await fetch('/api/fs/move', {
    method: 'POST',
    credentials: 'same-origin',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ root, kind, from_rel: fromRel, to_rel: toRel }),
  });
  if (!res.ok) {
    if (res.status === 401) notifyAuthRequired();
    let detail = '';
    try {
      detail = ((await res.json()) as { error?: string }).error ?? '';
    } catch {
      // non-JSON error body — status alone is the message
    }
    throw new Error(detail || `move failed: ${res.status}`);
  }
  return (await res.json()) as FsMoveOutcome;
}

/** Create a folder (and missing parents) inside one root. */
export async function fsMkdir(
  root: string,
  kind: 'project' | 'kiln',
  relPath: string,
): Promise<void> {
  const res = await fetch('/api/fs/mkdir', {
    method: 'POST',
    credentials: 'same-origin',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ root, kind, rel_path: relPath }),
  });
  if (!res.ok) {
    if (res.status === 401) notifyAuthRequired();
    throw new Error(`mkdir failed: ${res.status}`);
  }
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
  const res = await fetch('/api/fs/trash', {
    method: 'POST',
    credentials: 'same-origin',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ root, kind, rel_path: relPath }),
  });
  if (!res.ok) {
    if (res.status === 401) notifyAuthRequired();
    throw new Error(`trash failed: ${res.status}`);
  }
}

/**
 * SSE event names the `/api/fs/events` stream emits. Kept in lockstep with the
 * Rust `FsEvent::event_name()` (web/fs_events.rs). Each event's `data` parses
 * to the `FsEvent` discriminated union.
 */
export const FS_SSE_EVENT_TYPES = ['fs_changed', 'fs_deleted', 'fs_moved'] as const;

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
          onEvent(JSON.parse(e.data) as FsEvent);
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
  const params = new URLSearchParams({ path });
  return request<CanvasResponse>('GET', `/api/canvas?${params.toString()}`, {
    errorMessage: 'Failed to load canvas',
    includeErrorText: true,
  });
}

/** Write a `.canvas` document. Refused server-side if any reference escapes the kiln. */
export async function saveCanvas(path: string, canvas: CanvasDoc): Promise<void> {
  await request<void>('PUT', '/api/canvas', {
    errorMessage: 'Failed to save canvas',
    parseAs: 'none',
    includeErrorText: true,
    ...jsonRequest({ path, content: JSON.stringify(canvas) }),
  });
}

/** URL serving a file's raw bytes, for canvas media nodes and inline images. */
export function rawFileUrl(absolutePath: string): string {
  return `/api/file/raw?path=${encodeURIComponent(absolutePath)}`;
}

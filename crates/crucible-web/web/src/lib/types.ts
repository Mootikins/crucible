/** Token usage data for a completed message */
export interface TokenUsage {
  promptTokens: number;
  completionTokens: number;
  totalTokens: number;
  cacheReadTokens?: number;
  cacheCreationTokens?: number;
}

/** Message in the chat */
export interface Message {
  id: string;
  role: 'user' | 'assistant' | 'system' | 'tool';
  content: string;
  timestamp: number;
  /**
   * For role "tool": the tool invocation this transcript entry represents.
   * Tool calls are first-class transcript entries (like Claude Code / VS Code
   * agent chat) so they persist after the turn instead of vanishing.
   */
  toolCall?: ToolCallDisplay;
  /** Message subtype (e.g., 'precognition' for auto-injected context) */
  type?: string;
  /** Thinking block data (extended thinking / reasoning) */
  thinking?: ThinkingBlock;
  /** Token usage data (populated on message_complete) */
  usage?: TokenUsage;
  /**
   * Precognition (auto-RAG) enrichment metadata, attached to the user message
   * that triggered the daemon's first-turn note retrieval. Used by
   * PrecognitionBadge to show what context was injected.
   */
  precognition?: {
    notesCount: number;
    notes: { name: string; relevance: number }[];
  };
}


// =============================================================================
// Session Types (matching Rust SessionSummary)
// =============================================================================

export type SessionState = 'active' | 'paused' | 'compacting' | 'ended';
export type SessionType = 'chat' | 'agent' | 'workflow';

export interface Session {
  id: string;
  session_type: SessionType;
  /**
   * Every kiln this session can query, by registry NAME — flat,
   * order-preserving, no member privileged. `kilns[0]` is only read for the
   * one thing that still needs exactly one: a display label.
   *
   * Names, not paths. A name is what the daemon accepts back (`session.create`,
   * `session.connect_kiln`, the search filter) because it resolves against the
   * `[kilns]` registry; a path names a directory the registration floor never
   * saw. Anything here that needs a DIRECTORY — grep, wikilink resolution,
   * note listing — joins through `kilnPathForName()` against `GET /api/kilns`,
   * and treats an unresolved name as no directory rather than as the root.
   */
  kilns: string[];
  /**
   * Where the session acts, or `null` when it has no workspace at all —
   * a tools-only agent, or one whose workspace was detached. Read it through
   * `sessionWorkspace()`, which also folds the empty string a pre-nullable
   * payload carries.
   */
  workspace: string | null;
  state: SessionState;
  title: string | null;
  agent_model: string | null;
  /** Persisted session mode (normal/plan/auto); null when never set. */
  agent_mode: string | null;
  started_at: string; // ISO datetime
  /** ISO datetime of the last session event; null/absent for legacy sessions. */
  last_activity?: string | null;
  event_count: number;
  archived?: boolean;
}

export interface CreateSessionParams {
  session_type?: SessionType;
  /**
   * The session's kiln set, by registry NAME. Omitted or empty is a literal
   * empty set — a session with no corpus — NOT a request for a default; the
   * daemon stopped substituting its data root, which is the parent of the
   * session store.
   */
  kilns?: string[];
  workspace?: string;
  provider?: string;
  model?: string;
  endpoint?: string;
  /** "internal" (default) or "acp". */
  agent_type?: string;
  /** ACP agent profile name; required when agent_type is "acp". */
  agent_name?: string;
  /**
   * The RUNTIME axis — where the session's process runs. Forwarded untouched:
   * `false` = unisolated even if the project asks otherwise, `true` = the
   * server's default, a string = a named profile, and `{plugin, target}` = a
   * target addressed to the provider that offered it. Omit to let the server
   * resolve normally — omitted and `false` are different instructions.
   *
   * Loosely typed on purpose. The daemon forwards this to whichever plugin
   * claims it without parsing it, so a shape only a future provider
   * understands has to survive the trip.
   */
  isolation?: string | boolean | { plugin: string; target?: string };
  /**
   * The WORKSPACE axis — where the session's files live, as a
   * `provider:target` spec (e.g. `worktree:feat/x`). The daemon resolves it to
   * a path *before* creating the session, so a target it cannot resolve
   * refuses the create rather than silently running against the main checkout.
   */
  workspace_target?: string;
}

/** ACP agent profile entry from GET /api/agents. */
export interface AgentProfileEntry {
  name: string;
  description: string;
  command: string;
  is_builtin: boolean;
  /** Probed daemon-side: binary found on PATH and answering. */
  available: boolean;
}

export interface ProviderInfo {
  name: string;
  provider_type: string;
  available: boolean;
  default_model: string | null;
  models: string[];
  endpoint?: string;
  reason?: string;
  is_local: boolean;
}

// =============================================================================
// File Entry Types
// =============================================================================

export interface FileEntry {
  name: string;
  path: string;
  is_dir: boolean;
}

export interface NoteEntry {
  name: string;
  path: string;
  title: string | null;
  tags: string[];
  updated_at: string;
}

export interface NoteContent {
  /** Several fields are NOT sent by GET /api/notes/{name} — the daemon payload
   * carries only path/title/tags/links_to/content_hash. Derive display names
   * by falling through title → name → file stem; content/updated_at are
   * absent (typing them required yielded `undefined` at runtime). */
  name?: string;
  path: string;
  content?: string;
  title: string | null;
  tags: string[];
  updated_at?: string;
}

/** A note that wikilinks to the focused note. */
interface BacklinkEntry {
  name: string;
  path: string;
  abs_path: string;
  title: string | null;
  /** Byte span of the first link occurrence in the source (from the
   * daemon's link index); absent for span-less legacy index rows. */
  span_start?: number;
  span_end?: number;
}

/** A plain-text mention of another note inside the focused note. */
export interface UnlinkedMention {
  mention: string;
  target: string;
  offset: number;
}

/** Response of `GET /api/backlinks` — linked + unlinked mentions for a note. */
export interface BacklinksResponse {
  note: { path: string; abs_path: string; title: string | null };
  linked: BacklinkEntry[];
  unlinked: UnlinkedMention[];
}

// =============================================================================
// Project Types
// =============================================================================

interface KilnInfo {
  path: string;
  name: string | null;
}

/** SCM info attached to a Project when `[scm]` detection found a repo.
 * Wire shape of crucible-core's `RepositoryInfo` (snake_case contract). */
interface RepositoryInfo {
  /** Repo root — for worktrees, the MAIN checkout's root. */
  root: string;
  remote_url?: string;
  is_worktree?: boolean;
  main_repo_git_dir?: string;
}

export interface Project {
  path: string;
  name: string;
  kilns: KilnInfo[];
  last_accessed: string; // ISO datetime
  repository?: RepositoryInfo;
}

/**
 * One entry of `GET /api/kilns`. The daemon's `handle_kiln_list`
 * (crucible-daemon/src/server/kiln.rs) emits objects — `{ path, name,
 * last_access_secs_ago }` — surfaced verbatim by the web route
 * (routes/search.rs). NOT a bare string (the pre-file-tree `listKilns` mock
 * asserted a fictional string payload; see api.test.ts).
 */
export interface KilnListEntry {
  path: string;
  name: string | null;
  last_access_secs_ago?: number;
}

// =============================================================================
// File-System Explorer Types (Phase 1 web file tree)
// =============================================================================

/**
 * One directory entry from `GET /api/fs/list` (daemon `fs.list_dir`).
 * Wire shape is snake_case (Rust `FsEntry`); every field name is part of the
 * cross-language contract and must not drift.
 */
export interface FsEntry {
  name: string;
  rel_path: string;
  is_dir: boolean;
  size: number;
  /** Unix epoch seconds; `null` when the platform cannot report it. */
  modified: number | null;
  /** Phase-2/3 git/diff decoration seam — always `null` in Phase 1. */
  status: string | null;
}

/**
 * One level of a directory, plus whether the daemon's per-directory cap cut it
 * short. `truncated` exists because `target/debug/deps` is 1.47M entries: the
 * listing has to be able to say "there is more" rather than looking complete.
 */
export interface FsListing {
  entries: FsEntry[];
  truncated: boolean;
}

/**
 * A live filesystem-change event delivered over `GET /api/fs/events` (SSE).
 * Discriminated union mirroring the Rust `FsEvent` (web/fs_events.rs); paths
 * are ABSOLUTE. `moved` is decomposed into remove+add by the reconciler, so a
 * platform that emits `deleted`+`changed{created}` instead converges to the
 * same tree.
 */
export type FsEvent =
  | { type: 'changed'; path: string; kind: 'created' | 'modified' }
  | { type: 'deleted'; path: string }
  | { type: 'moved'; from: string; to: string };

// =============================================================================
// TUI Feature Types (for web port)
// =============================================================================

/** Thinking block with streaming state */
interface ThinkingBlock {
  content: string;
  isStreaming: boolean;
  tokenCount?: number;
}

/** Tool call display with execution status */
export interface ToolCallDisplay {
  id: string;
  name: string;
  args: string;
  result?: string;
  status: 'running' | 'complete' | 'error';
  callId?: string;
  /**
   * True if this tool signaled an early-stop and the agent turn ended after
   * its batch (daemon's conjunctive terminate check). UI renders a badge.
   */
  terminate?: boolean;
  /**
   * The daemon's projection of what this call is about. One answer shared with
   * the TUI and with the daemon's own deny messages, rather than each UI
   * keeping its own key-priority list. Optional: replayed transcripts and
   * older daemons predate the field, so every consumer needs a fallback.
   */
  display?: ToolDisplay;
  /**
   * Set when the permission gate granted this call without asking. Rendered
   * so an auto-approved call is distinguishable from one that never needed
   * permission — in auto mode, that difference is the whole audit trail.
   */
  autoApproved?: string;
}

/** What a tool call is about, for display. Mirrors `crucible_core::types::ToolDisplay`. */
interface ToolDisplay {
  kind: 'command' | 'path' | 'query' | 'other';
  primary?: string;
}

/** Subagent event (background task) */
export interface SubagentEvent {
  id: string;
  prompt: string;
  status: 'spawned' | 'completed' | 'failed';
  summary?: string;
  error?: string;
  targetAgent?: string;
}

/** A mode id. Modes are declared in Lua, so this cannot be a closed union —
 * see `session.list_modes` for what a given session actually offers. */
export type ChatMode = string;

/** One mode a session may enter, as the daemon describes it. */
export interface ModeDescriptor {
  id: string;
  name: string;
  description: string | null;
  icon: string | null;
  color: string | null;
}

/** Response of `GET /api/session/{id}/modes`. */
export interface SessionModes {
  current_mode_id: string;
  modes: ModeDescriptor[];
}

/** Context window usage */
export interface ContextUsage {
  used: number;
  total: number;
}

/** Notification type */
export type NotificationType = 'info' | 'warning' | 'error' | 'success';

/** Notification message */
export interface Notification {
  id: string;
  type: NotificationType;
  message: string;
  timestamp: number;
  /** Removed from the visible list. */
  dismissed: boolean;
  /** Seen by the user (clears the unread badge) but still listed. */
  read?: boolean;
  /** Optional action rendered as a button; actionable notifications never
   * auto-dismiss (the user must act or dismiss explicitly). */
  action?: { label: string; run: () => void };
}






// =============================================================================
// SSE Event Types (from Rust backend events.rs)
// =============================================================================

/** Token/chunk of the response */
interface TokenEvent {
  type: 'token';
  content: string;
}

/** Tool call event (from daemon tool_call event) */
interface ToolCallEvent {
  type: 'tool_call';
  id: string;
  title: string;
  arguments?: unknown;
  /** Daemon's projection of which argument matters. Absent on replayed events. */
  display?: ToolDisplay;
  /** Which layer granted permission without asking, if any. */
  auto_approved?: string;
}

/** Tool call result streaming delta */
interface ToolResultDeltaEvent {
  type: 'tool_result_delta';
  id: string;
  delta: string;
}

/** Tool call result streaming complete */
interface ToolResultCompleteEvent {
  type: 'tool_result_complete';
  id: string;
}

/** Tool call result error */
interface ToolResultErrorEvent {
  type: 'tool_result_error';
  id: string;
  error: string;
}

/** Tool call result */
interface ToolResultEvent {
  type: 'tool_result';
  id: string;
  result?: string;
  /**
   * True if this tool signaled an early-stop (the agent turn ended after
   * this batch via the daemon's conjunctive terminate check). UI renders
   * this as a badge on the tool card.
   */
  terminate?: boolean;
}

/** Subagent spawned event */
interface SubagentSpawnedEvent {
  type: 'subagent_spawned';
  id: string;
  prompt: string;
}

/** Subagent completed event */
interface SubagentCompletedEvent {
  type: 'subagent_completed';
  id: string;
  summary: string;
}

/** Subagent failed event */
interface SubagentFailedEvent {
  type: 'subagent_failed';
  id: string;
  error: string;
}

/** Delegation spawned event */
interface DelegationSpawnedEvent {
  type: 'delegation_spawned';
  id: string;
  prompt: string;
  target_agent?: string;
}

/** Delegation completed event */
interface DelegationCompletedEvent {
  type: 'delegation_completed';
  id: string;
  summary: string;
}

/** Delegation failed event */
interface DelegationFailedEvent {
  type: 'delegation_failed';
  id: string;
  error: string;
}

/** Agent is thinking/reasoning */
interface ThinkingEvent {
  type: 'thinking';
  content: string;
}

/** Context usage event */
interface ContextUsageEvent {
  type: 'context_usage';
  used: number;
  total: number;
}

/** Precognition result event */
interface PrecognitionResultEvent {
  type: 'precognition_result';
  notes_count: number;
  notes: { name: string; relevance: number }[];
}

/** Mode changed event */
interface ModeChangedEvent {
  type: 'mode_changed';
  mode: ChatMode;
}

/** Session title changed (daemon-side topic auto-title or manual rename) */
interface TitleChangedEvent {
  type: 'title_changed';
  title: string;
}

/**
 * A pre-tool narration segment finished (text → tool boundary). Carries the
 * turn's message_id and the segment's 0-based index so the client can build a
 * canonical bubble id that matches history reconstruction.
 */
interface SegmentCompleteEvent {
  type: 'segment_complete';
  message_id: string;
  index: number;
  content: string;
}

/** Message is complete */
interface MessageCompleteEvent {
  type: 'message_complete';
  id: string;
  content: string;
  prompt_tokens?: number;
  completion_tokens?: number;
  total_tokens?: number;
  cache_read_tokens?: number;
  cache_creation_tokens?: number;
}

/** An error occurred */
interface ErrorEvent {
  type: 'error';
  code: string;
  message: string;
}

/**
 * Transport-level connection status (SSE reconnecting/connected). Client-synthesized,
 * never from the daemon. Must NOT be routed through the daemon-error path — a
 * reconnect must not corrupt an in-flight streaming message.
 */
interface ConnectionEvent {
  type: 'connection';
  status: 'reconnecting' | 'connected';
  message?: string;
}

/** An interaction is requested from the user */
interface InteractionRequestedEvent {
  type: 'interaction_requested';
  id: string;
  [key: string]: unknown;
}

/** A session-level event (state change, etc.) */
interface SessionEventData {
  type: 'session_event';
  event_type: string;
  data: unknown;
}

/** Union of all SSE event types */
export type ChatEvent =
  | TokenEvent
  | ToolCallEvent
  | ToolResultEvent
  | ToolResultDeltaEvent
  | ToolResultCompleteEvent
  | ToolResultErrorEvent
  | ThinkingEvent
  | SegmentCompleteEvent
  | MessageCompleteEvent
  | ErrorEvent
  | ConnectionEvent
  | InteractionRequestedEvent
  | SessionEventData
  | SubagentSpawnedEvent
  | SubagentCompletedEvent
  | SubagentFailedEvent
  | DelegationSpawnedEvent
  | DelegationCompletedEvent
  | DelegationFailedEvent
  | ContextUsageEvent
  | PrecognitionResultEvent
  | ModeChangedEvent
  | TitleChangedEvent;


// =============================================================================
// Interaction Request/Response Types (from Rust core interaction.rs)
// =============================================================================

// The seven variants of Rust's `InteractionRequest`, which is internally
// tagged on `kind` (crucible-core/src/interaction/types.rs). The list is kept
// complete by `InteractionRequest::KINDS` on the Rust side and by
// `interaction-coverage.test.ts` here, which fails when a kind has no renderer
// — three of seven rendered in the browser is the state those guards exist to
// stop recurring.

/** Format hint carried by `edit` and `show`. */
export type ArtifactFormat = 'markdown' | 'code' | 'json' | 'plain';

export interface AskRequest {
  kind: 'ask';
  question: string;
  choices?: string[];
  multi_select?: boolean;
  allow_other?: boolean;
}

export interface AskQuestion {
  header: string;
  question: string;
  choices: string[];
  multi_select?: boolean;
  allow_other?: boolean;
}

export interface AskBatchRequest {
  kind: 'ask_batch';
  id: string;
  questions: AskQuestion[];
}

export interface EditRequest {
  kind: 'edit';
  content: string;
  format?: ArtifactFormat;
  hint?: string;
}

export interface ShowRequest {
  kind: 'show';
  content: string;
  format?: ArtifactFormat;
  title?: string;
}

interface PopupEntry {
  label: string;
  description?: string;
  data?: unknown;
}

export interface PopupRequest {
  kind: 'popup';
  title: string;
  entries: PopupEntry[];
  allow_other?: boolean;
}

export interface PanelItem {
  label: string;
  description?: string;
  data?: unknown;
}

export interface PanelHints {
  filterable?: boolean;
  multi_select?: boolean;
  allow_other?: boolean;
  initial_selection?: number[];
  initial_filter?: string;
}

export interface PanelRequest {
  kind: 'panel';
  header: string;
  items: PanelItem[];
  hints?: PanelHints;
}

type PermActionType = 'bash' | 'read' | 'write' | 'tool';

export interface PermRequest {
  kind: 'permission';
  action_type: PermActionType;
  tokens: string[];
  tool_name?: string;
  tool_args?: unknown;
}

/** The seven request bodies, exactly as the Rust enum serializes them. */
export type InteractionBody =
  | AskRequest
  | AskBatchRequest
  | EditRequest
  | ShowRequest
  | PermRequest
  | PopupRequest
  | PanelRequest;

/**
 * A request as a client receives it: the body plus the correlation `id`.
 *
 * `id` is NOT a field on any of the Rust structs — it is `request_id` from the
 * `interaction_requested` envelope, which the SSE reducer flattens onto the
 * body. Declaring it per-variant (as three of them used to) made it look like
 * part of the payload and left the four other kinds unable to be answered at
 * all, since responding needs exactly this value.
 */
export type InteractionRequest = InteractionBody & { id: string };

/** One correlated request, by kind — what a renderer for that kind receives. */
export type InteractionOf<K extends InteractionBody['kind']> = Extract<
  InteractionRequest,
  { kind: K }
>;

/** Every `InteractionRequest.kind`, for the coverage test to iterate. */
export const INTERACTION_KINDS = [
  'ask',
  'ask_batch',
  'edit',
  'show',
  'permission',
  'popup',
  'panel',
] as const satisfies readonly InteractionBody['kind'][];

// Responses carry `kind` explicitly. The server still infers a tag for the
// three bare shapes older clients sent (`tag_interaction_response` in
// routes/chat.rs), but inference cannot separate a panel result from an ask
// response — both carry `selected` — so new kinds must say what they are.

export interface AskResponse {
  kind: 'ask';
  selected: number[];
  other?: string;
}

export interface QuestionAnswer {
  selected: number[];
  other?: string;
}

export interface AskBatchResponse {
  kind: 'ask_batch';
  id: string;
  answers: QuestionAnswer[];
  cancelled?: boolean;
}

export interface EditResponse {
  kind: 'edit';
  modified: string;
}

export interface PopupResponse {
  kind: 'popup';
  selected_index?: number;
  other?: string;
}

export interface PanelResponse {
  kind: 'panel';
  cancelled?: boolean;
  selected: number[];
  other?: string;
}

/** `show` expects no answer; dismissing it reports cancellation. */
export interface CancelledResponse {
  kind: 'cancelled';
}

export type PermissionScope = 'once' | 'session' | 'project' | 'user';

export interface PermResponse {
  kind: 'permission';
  allowed: boolean;
  pattern?: string;
  scope: PermissionScope;
}

export type InteractionResponse =
  | AskResponse
  | AskBatchResponse
  | EditResponse
  | PopupResponse
  | PanelResponse
  | PermResponse
  | CancelledResponse;

// =============================================================================
// Editor Types
// =============================================================================

/** A file open in the editor */
export interface EditorFile {
  path: string;
  content: string;
  dirty: boolean;
}

// =============================================================================
// Context Types (re-exported from types/context.ts)
// =============================================================================





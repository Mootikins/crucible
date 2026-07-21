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
  kiln: string;
  workspace: string;
  /** Additional knowledge kilns attached to this session (primary excluded). */
  connected_kilns: string[];
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
  /** Omitted → daemon default (home kiln). */
  kiln?: string;
  /** Additional knowledge kilns to attach at creation. */
  connect_kilns?: string[];
  workspace?: string;
  provider?: string;
  model?: string;
  endpoint?: string;
  /** "internal" (default) or "acp". */
  agent_type?: string;
  /** ACP agent profile name; required when agent_type is "acp". */
  agent_name?: string;
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
   * carries only path/title/tags/links_to/content_hash. Deriving display names
   * via noteDisplayName; content/updated_at are absent (typing them required
   * yielded `undefined` at runtime). */
  name?: string;
  path: string;
  content?: string;
  title: string | null;
  tags: string[];
  updated_at?: string;
}

/** A note that wikilinks to the focused note. */
export interface BacklinkEntry {
  name: string;
  path: string;
  abs_path: string;
  title: string | null;
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

export interface KilnInfo {
  path: string;
  name: string | null;
}

export interface Project {
  path: string;
  name: string;
  kilns: KilnInfo[];
  last_accessed: string; // ISO datetime
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
export interface ThinkingBlock {
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

/** Chat mode type */
export type ChatMode = 'normal' | 'plan' | 'auto';

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
}

/** Precognition result (auto-injected context) */
export interface PrecognitionResult {
  notesCount: number;
  notes: { name: string; relevance: number }[];
}

/** Context management strategy */
export type ContextStrategy = 'truncate' | 'sliding_window';

/** Output validation mode */
export type OutputValidation =
  | { kind: 'none' }
  | { kind: 'json' }
  | { kind: 'regex'; pattern: string };

/** Session configuration */
export interface SessionConfig {
  thinkingBudget?: number;
  temperature?: number;
  maxTokens?: number | null;
  precognition?: boolean;
  // Execution controls
  maxIterations?: number;
  executionTimeoutSecs?: number;
  contextBudget?: number;
  contextStrategy?: ContextStrategy;
  contextWindow?: number;
  outputValidation?: OutputValidation;
  validationRetries?: number;
}

/** Panel identifier */
export type PanelId = 'chat' | 'files' | 'editor' | 'sessions' | 'settings' | 'activity' | 'notifications';


// =============================================================================
// SSE Event Types (from Rust backend events.rs)
// =============================================================================

/** Token/chunk of the response */
export interface TokenEvent {
  type: 'token';
  content: string;
}

/** Tool call is starting */
export interface ToolCallStartEvent {
  type: 'tool_call_start';
  id: string;
  name: string;
  arguments?: unknown;
}

/** Tool call event (from daemon tool_call event) */
export interface ToolCallEvent {
  type: 'tool_call';
  id: string;
  title: string;
  arguments?: unknown;
}

/** Tool call result streaming delta */
export interface ToolResultDeltaEvent {
  type: 'tool_result_delta';
  id: string;
  delta: string;
}

/** Tool call result streaming complete */
export interface ToolResultCompleteEvent {
  type: 'tool_result_complete';
  id: string;
}

/** Tool call result error */
export interface ToolResultErrorEvent {
  type: 'tool_result_error';
  id: string;
  error: string;
}

/** Tool call result */
export interface ToolResultEvent {
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
export interface SubagentSpawnedEvent {
  type: 'subagent_spawned';
  id: string;
  prompt: string;
}

/** Subagent completed event */
export interface SubagentCompletedEvent {
  type: 'subagent_completed';
  id: string;
  summary: string;
}

/** Subagent failed event */
export interface SubagentFailedEvent {
  type: 'subagent_failed';
  id: string;
  error: string;
}

/** Delegation spawned event */
export interface DelegationSpawnedEvent {
  type: 'delegation_spawned';
  id: string;
  prompt: string;
  target_agent?: string;
}

/** Delegation completed event */
export interface DelegationCompletedEvent {
  type: 'delegation_completed';
  id: string;
  summary: string;
}

/** Delegation failed event */
export interface DelegationFailedEvent {
  type: 'delegation_failed';
  id: string;
  error: string;
}

/** Agent is thinking/reasoning */
export interface ThinkingEvent {
  type: 'thinking';
  content: string;
}

/** Context usage event */
export interface ContextUsageEvent {
  type: 'context_usage';
  used: number;
  total: number;
}

/** Precognition result event */
export interface PrecognitionResultEvent {
  type: 'precognition_result';
  notes_count: number;
  notes: { name: string; relevance: number }[];
}

/** Mode changed event */
export interface ModeChangedEvent {
  type: 'mode_changed';
  mode: 'normal' | 'plan' | 'auto';
}

/** Session title changed (daemon-side topic auto-title or manual rename) */
export interface TitleChangedEvent {
  type: 'title_changed';
  title: string;
}

/** Message is complete */
export interface MessageCompleteEvent {
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
export interface ErrorEvent {
  type: 'error';
  code: string;
  message: string;
}

/**
 * Transport-level connection status (SSE reconnecting/connected). Client-synthesized,
 * never from the daemon. Must NOT be routed through the daemon-error path — a
 * reconnect must not corrupt an in-flight streaming message.
 */
export interface ConnectionEvent {
  type: 'connection';
  status: 'reconnecting' | 'connected';
  message?: string;
}

/** An interaction is requested from the user */
export interface InteractionRequestedEvent {
  type: 'interaction_requested';
  id: string;
  [key: string]: unknown;
}

/** A session-level event (state change, etc.) */
export interface SessionEventData {
  type: 'session_event';
  event_type: string;
  data: unknown;
}

/** Union of all SSE event types */
export type ChatEvent =
  | TokenEvent
  | ToolCallStartEvent
  | ToolCallEvent
  | ToolResultEvent
  | ToolResultDeltaEvent
  | ToolResultCompleteEvent
  | ToolResultErrorEvent
  | ThinkingEvent
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

/** SSE event type discriminator */
export type ChatEventType = ChatEvent['type'];

// =============================================================================
// Interaction Request/Response Types (from Rust core interaction.rs)
// =============================================================================

export interface AskRequest {
  kind: 'ask';
  id: string;
  question: string;
  choices?: string[];
  multi_select?: boolean;
  allow_other?: boolean;
}

export interface PopupEntry {
  label: string;
  description?: string;
  data?: unknown;
}

export interface PopupRequest {
  kind: 'popup';
  id: string;
  title: string;
  entries: PopupEntry[];
  allow_other?: boolean;
}

export type PermActionType = 'bash' | 'read' | 'write' | 'tool';

export interface PermRequest {
  kind: 'permission';
  id: string;
  action_type: PermActionType;
  tokens: string[];
  tool_name?: string;
  tool_args?: unknown;
}

export type InteractionRequest = AskRequest | PopupRequest | PermRequest;

export interface AskResponse {
  selected: number[];
  other?: string;
}

export interface PopupResponse {
  selected_index?: number;
  other?: string;
}

export type PermissionScope = 'once' | 'session' | 'project' | 'user';

export interface PermResponse {
  allowed: boolean;
  pattern?: string;
  scope: PermissionScope;
}

export type InteractionResponse = AskResponse | PopupResponse | PermResponse;

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


export type {
  ChatContextValue,
  SessionContextValue,
  ProjectContextValue,
  EditorContextValue,
} from './types/context';



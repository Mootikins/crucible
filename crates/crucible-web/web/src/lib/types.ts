import type { components } from './api-schema';

/**
 * Every wire shape below is an alias into the generated contract.
 *
 * `api-schema.d.ts` is produced from `crates/crucible-web/openapi.json`, which
 * `utoipa` writes from the axum router. A shape declared twice drifts; a shape
 * aliased once cannot. The names stay the ones the app already imports, so a
 * component reads `Session` and gets `SessionRow`.
 *
 * A type that describes CLIENT state keeps its hand-written form, and says so
 * on its declaration. Those shapes never cross the wire, so the daemon has no
 * opinion about them and the document cannot carry them.
 */
type Schemas = components['schemas'];

/** Token usage data for a completed message.
 *
 * Client-local: the reducer folds `message_complete`'s snake_case counters into
 * this camelCase record, and nothing sends it back. */
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
  /** When the daemon closed the turn (set on message_complete). A message
   * rebuilt from history has no value: history timestamps are synthetic. */
  completedAt?: number;
  /** True on the empty assistant bubble the client mints when a turn is
   * sent, before any token arrives. Only such a bubble may give up the
   * canonical response id at `message_complete`; a bubble from history that
   * happens to be empty is an answer, not a placeholder. */
  placeholder?: boolean;
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

export type SessionState = Schemas['SessionRow']['state'];
/** A session type prefix, such as `chat`. Open on the wire: the daemon names
 * the types and a client that closed the set would drop a new one. */
export type SessionType = Schemas['SessionRow']['type'];

/**
 * One session, as every session route answers with it.
 *
 * The id is `session_id` and the type is `type`; the browser used to rename
 * both through `mapSession`, and the two spellings disagreed about which
 * fields a create reply carries. `kilns` holds registry NAMES, not paths —
 * anything that needs a DIRECTORY joins through `kilnPathForName()` against
 * `GET /api/kilns`, and treats an unresolved name as no directory rather than
 * as the root. `workspace` is `null` when the session has none at all; read it
 * through `sessionWorkspace()`, which also folds the empty string a
 * pre-nullable payload carries.
 */
export type Session = Schemas['SessionRow'];

/**
 * The body of `POST /api/session`, with the two fields the document cannot
 * carry as they are.
 *
 * `isolation` is `Option<serde_json::Value>` in Rust and is forwarded to
 * whichever plugin claims it without being parsed, so the document says only
 * "an object". The union here is what the daemon actually accepts: `false` =
 * unisolated even if the project asks otherwise, `true` = the server's
 * default, a string = a named profile, `{plugin, target}` = a target addressed
 * to the provider that offered it. Omitted and `false` are different
 * instructions.
 *
 * `workspace_target` is NOT in the generated contract, and that is a drift the
 * contract exposed rather than one it caused: `CreateSessionRequest` in
 * `routes/session/mod.rs` declares no such field, so serde drops what the
 * composer sends. The field stays named here because the composer still sends
 * it; making it arrive needs the route to take it, which is a Rust change.
 */
export type CreateSessionParams = Omit<Schemas['CreateSessionRequest'], 'isolation'> & {
  isolation?: string | boolean | { plugin: string; target?: string } | null;
  workspace_target?: string;
};

/** ACP agent profile entry from GET /api/agents. */
export type AgentProfileEntry = Schemas['AgentProfileEntry'];

/** One provider and its models, from `GET /api/providers`. `endpoint` and
 * `reason` are nullable, not merely absent. */
export type ProviderInfo = Schemas['ProviderRow'];

// =============================================================================
// File Entry Types
// =============================================================================

export type FileEntry = Schemas['FileEntryRow'];

/**
 * One note's metadata, from `GET /api/notes`.
 *
 * `path` is RELATIVE to the kiln root, not absolute. Every consumer joins it:
 * `notesToTree(notes, kilnAbsRoot)` strips a leading slash and rebuilds from
 * the root it was handed. Code that passes this straight to a path-taking
 * endpoint gets a 404 — `GET /api/kiln/file` answers "File not within any open
 * kiln" for a bare `Seed.md`.
 */
export type NoteEntry = Schemas['NoteMetadataRow'];

/** A plain-text mention of another note inside the focused note. */
export type UnlinkedMention = Schemas['UnlinkedMentionRow'];

/** Response of `GET /api/backlinks` — linked + unlinked mentions for a note. */
export type BacklinksResponse = Schemas['BacklinksResponse'];

// =============================================================================
// Project Types
// =============================================================================

export type Project = Schemas['Project'];

/**
 * One entry of `GET /api/kilns`.
 *
 * `registered` says whether the kiln registry answers for this directory, and
 * therefore whether `name` is a name `POST /kilns/connect` accepts. The daemon
 * publishes no name it cannot resolve: a row it cannot name is an open
 * directory the registration floor refuses, and it arrives as `false` with an
 * empty name. No picker may offer such a row — see `attachableKilns`.
 *
 * `open` says whether the daemon currently holds the kiln open. A registered
 * kiln is listed whether or not it is open, and a closed row is not a dead
 * one: the first request that addresses the kiln opens it.
 */
export type KilnListEntry = Schemas['KilnRow'];

// =============================================================================
// File-System Explorer Types (Phase 1 web file tree)
// =============================================================================

/**
 * One directory entry from `GET /api/fs/list` (daemon `fs.list_dir`).
 *
 * `status` is the git/diff decoration seam and is `unknown` on the wire: the
 * daemon forwards whatever the decorator put there, so a reader narrows it
 * rather than trusting a shape declared here.
 */
export type FsEntry = Schemas['FsEntry'];

/**
 * One level of a directory, plus whether the daemon's per-directory cap cut it
 * short. `truncated` exists because `target/debug/deps` is 1.47M entries: the
 * listing has to be able to say "there is more" rather than looking complete.
 */
export type FsListing = Schemas['FsListing'];

/**
 * A live filesystem-change event delivered over `GET /api/fs/events` (SSE).
 * Paths are ABSOLUTE. `moved` is decomposed into remove+add by the reconciler,
 * so a platform that emits `deleted`+`changed` instead converges to the same
 * tree.
 */
export type FsEvent = Schemas['FsEvent'];

// =============================================================================
// TUI Feature Types (for web port)
// =============================================================================

/** Thinking block with streaming state. Client-local: the reducer builds it
 * from `thinking` deltas and nothing sends it back. */
interface ThinkingBlock {
  content: string;
  isStreaming: boolean;
  tokenCount?: number;
}

/** Tool call display with execution status. Client-local: the reducer folds
 * several stream events into one card, so no route answers this shape. */
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

/** Subagent event (background task). Client-local: the store collapses the
 * three `subagent_*` stream events into one row. */
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

/**
 * One mode a session may enter, as the daemon describes it.
 *
 * `review_policy` is part of the shape, not an extra: the daemon degrades it
 * per agent capability before sending it, so what arrives is what will
 * actually happen. `lib/review-types.ts` re-exports the policy values.
 */
export type ModeDescriptor = Schemas['ModeRow'];

/** Response of `GET /api/session/{id}/modes`. */
export type SessionModes = Schemas['SessionModesResponse'];

/**
 * Which settings a session can change.
 *
 * Not every agent has every setting: ACP has no temperature and no token cap,
 * so a panel that draws a fixed list offers controls the daemon refuses.
 */
export type SessionKnobSupport = Schemas['SessionKnobsResponse'];

/**
 * A setting an external agent advertised for itself.
 *
 * Not one of Crucible's: it belongs to the agent, a different agent advertises
 * different ones, and the daemon does not interpret them. The panel renders
 * what it is given and sends the chosen value back.
 *
 * `kind` and `current` are one tagged pair, not two fields: a `select` carries
 * a string `current` and its `choices`, a `toggle` carries a boolean `current`
 * and no choices. Narrow on `kind` before reading either.
 */
export type AgentConfigOption = Schemas['AgentOptionRow'];

export type AgentConfigOptions = Schemas['AgentOptionsResponse'];

/** Context window usage. Client-local: held as a signal, never sent. */
export interface ContextUsage {
  used: number;
  total: number;
}

/** Notification type. Client-local: the toast store's own vocabulary. */
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
// SSE Event Types (generated from the Rust `ChatEvent` in events.rs)
// =============================================================================

/**
 * Transport health of the chat event stream.
 *
 * `reconnecting` means the stream dropped and a backoff timer is running — the
 * surface owes the user a retry control, because the wait is skippable.
 */
export type ConnectionStatus = 'reconnecting' | 'connected';

/**
 * Client-synthesized, never from the daemon.
 *
 * `subscribeToEvents` mints it when the `EventSource` drops and again when it
 * reopens, so it is deliberately absent from the generated union. It must NOT
 * be routed through the daemon-error path: a reconnect must not corrupt an
 * in-flight streaming message.
 */
interface ConnectionEvent {
  type: 'connection';
  status: ConnectionStatus;
  message?: string;
}

/**
 * Everything a chat stream handler receives.
 *
 * The daemon's half comes from the contract, so a variant added in Rust
 * reaches the reducer's exhaustiveness check without anyone editing a list
 * here. `ConnectionEvent` is the client's own half — see above.
 */
export type ChatEvent = Schemas['ChatEvent'] | ConnectionEvent;



// =============================================================================
// Interaction Request/Response Types (from Rust core interaction.rs)
// =============================================================================

// The seven variants of Rust's `InteractionRequest`, which is internally
// tagged on `kind` (crucible-core/src/interaction/types.rs). The list is kept
// complete by `InteractionRequest::KINDS` on the Rust side and by
// `interaction-coverage.test.ts` here, which fails when a kind has no renderer
// — three of seven rendered in the browser is the state those guards exist to
// stop recurring.
//
// They stay hand-written because the contract cannot carry them: the web route
// forwards the body as an opaque object (`PendingInteraction.request` is
// `serde_json::Value`), so the document describes it as an open object and
// knows none of these fields. The owner is `crucible-core`, not `crucible-web`.

/** Format hint carried by `edit` and `show`. */
type ArtifactFormat = 'markdown' | 'code' | 'json' | 'plain';

interface AskRequest {
  kind: 'ask';
  question: string;
  choices?: string[];
  multi_select?: boolean;
  allow_other?: boolean;
}

interface AskQuestion {
  header: string;
  question: string;
  choices: string[];
  multi_select?: boolean;
  allow_other?: boolean;
}

interface AskBatchRequest {
  kind: 'ask_batch';
  id: string;
  questions: AskQuestion[];
}

interface EditRequest {
  kind: 'edit';
  content: string;
  format?: ArtifactFormat;
  hint?: string;
}

interface ShowRequest {
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

interface PopupRequest {
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

interface PanelHints {
  filterable?: boolean;
  multi_select?: boolean;
  allow_other?: boolean;
  initial_selection?: number[];
  initial_filter?: string;
}

interface PanelRequest {
  kind: 'panel';
  header: string;
  items: PanelItem[];
  hints?: PanelHints;
}

type PermActionType = 'bash' | 'read' | 'write' | 'tool';

interface PermRequest {
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

/** A file open in the editor. Client-local: the dirty flag, the base hash
 * and the base text are what the browser holds so a stale save can be merged
 * rather than refused. No route answers this shape. */
export interface EditorFile {
  path: string;
  content: string;
  dirty: boolean;
  /** The disk hash this buffer was read at.
   *
   * What every save is anchored on: the daemon compares it to the bytes on
   * disk, so a note someone else changed meanwhile is merged or refused
   * rather than overwritten. Every open sets it from the read, and a landed
   * write moves it. */
  baseHash: string;
  /**
   * The note as it was at `baseHash`: the text this buffer was read FROM.
   *
   * The third text a three-way merge needs, and the browser is the only party
   * that holds it — so a save the daemon would refuse as stale is merged
   * against the disk instead. The pair is one fact: a base that moves with no
   * text beside it clears this, because a hash and a text that do not belong
   * together is what the route refuses as a caller bug.
   */
  baseText?: string;
  /**
   * The kiln watcher says this note moved on disk since the buffer last agreed
   * with it.
   *
   * Only a DIRTY buffer carries it: a clean one re-reads and the flag never
   * rises. It is the panel's banner — the user chooses between their text and
   * the disk's, because nothing else can. A landed save clears it.
   */
  changedOnDisk?: boolean;
}

// =============================================================================
// Context Types (re-exported from types/context.ts)
// =============================================================================



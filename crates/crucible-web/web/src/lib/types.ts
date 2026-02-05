/** Message in the chat */
export interface Message {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  timestamp: number;
  /** Tool calls made during this message */
  toolCalls?: ToolCallSummary[];
}

/** Summary of a tool call */
export interface ToolCallSummary {
  id: string;
  title: string;
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
  state: SessionState;
  title: string | null;
  agent_model: string | null;
  started_at: string; // ISO datetime
  event_count: number;
}

export interface CreateSessionParams {
  session_type?: SessionType;
  kiln: string;
  workspace?: string;
  provider?: string;
  model?: string;
  endpoint?: string;
}

export interface ProviderInfo {
  name: string;
  provider_type: string;
  available: boolean;
  default_model: string | null;
  models: string[];
  endpoint?: string;
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
  name: string;
  path: string;
  content: string;
  title: string | null;
  tags: string[];
  updated_at: string;
}

// =============================================================================
// Project Types
// =============================================================================

export interface Project {
  path: string;
  name: string;
  kilns: string[];
  last_accessed: string; // ISO datetime
}

// =============================================================================
// SSE Event Types (from Rust backend events.rs)
// =============================================================================

/** Token/chunk of the response */
export interface TokenEvent {
  type: 'token';
  content: string;
}

/** A tool call is being made */
export interface ToolCallEvent {
  type: 'tool_call';
  id: string;
  title: string;
  arguments?: unknown;
}

/** Tool call result */
export interface ToolResultEvent {
  type: 'tool_result';
  id: string;
  result?: string;
}

/** Agent is thinking/reasoning */
export interface ThinkingEvent {
  type: 'thinking';
  content: string;
}

/** Message is complete */
export interface MessageCompleteEvent {
  type: 'message_complete';
  id: string;
  content: string;
  tool_calls: ToolCallSummary[];
}

/** An error occurred */
export interface ErrorEvent {
  type: 'error';
  code: string;
  message: string;
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
  | ToolCallEvent
  | ToolResultEvent
  | ThinkingEvent
  | MessageCompleteEvent
  | ErrorEvent
  | InteractionRequestedEvent
  | SessionEventData;

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
// Session Context Types
// =============================================================================

export interface SessionContextValue {
  currentSession: () => Session | null;
  sessions: () => Session[];
  isLoading: () => boolean;
  error: () => string | null;
  availableModels: () => string[];
  createSession: (params: CreateSessionParams) => Promise<Session>;
  selectSession: (id: string) => Promise<void>;
  refreshSessions: (filters?: { kiln?: string; workspace?: string }) => Promise<void>;
  pauseSession: () => Promise<void>;
  resumeSession: () => Promise<void>;
  endSession: () => Promise<void>;
  cancelCurrentOperation: () => Promise<boolean>;
  switchModel: (modelId: string) => Promise<void>;
  refreshModels: () => Promise<void>;
}

export interface ProjectContextValue {
  currentProject: () => Project | null;
  projects: () => Project[];
  isLoading: () => boolean;
  error: () => string | null;
  registerProject: (path: string) => Promise<Project>;
  unregisterProject: (path: string) => Promise<void>;
  selectProject: (path: string) => Promise<void>;
  refreshProjects: () => Promise<void>;
  clearProject: () => void;
}

// =============================================================================
// Editor Types
// =============================================================================

/** A file open in the editor */
export interface EditorFile {
  path: string;
  content: string;
  dirty: boolean;
}

export interface EditorContextValue {
  openFiles: () => EditorFile[];
  activeFile: () => string | null;
  openFile: (path: string) => Promise<void>;
  closeFile: (path: string) => void;
  saveFile: (path: string) => Promise<void>;
  setActiveFile: (path: string) => void;
  updateFileContent: (path: string, content: string) => void;
  isLoading: () => boolean;
  error: () => string | null;
}

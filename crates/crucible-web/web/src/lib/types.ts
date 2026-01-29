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

/** Chat context value exposed to components */
export interface ChatContextValue {
  messages: () => Message[];
  isLoading: () => boolean;
  pendingInteraction: () => InteractionRequest | null;
  sendMessage: (content: string) => Promise<void>;
  respondToInteraction: (response: InteractionResponse) => void;
  clearMessages: () => void;
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

/** Union of all SSE event types */
export type ChatEvent =
  | TokenEvent
  | ToolCallEvent
  | ToolResultEvent
  | ThinkingEvent
  | MessageCompleteEvent
  | ErrorEvent;

/** SSE event type discriminator */
export type ChatEventType = ChatEvent['type'];

// =============================================================================
// Interaction Request/Response Types (from Rust core interaction.rs)
// =============================================================================

export interface AskRequest {
  type: 'ask';
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
  type: 'popup';
  id: string;
  title: string;
  entries: PopupEntry[];
  allow_other?: boolean;
}

export type PermActionType = 'bash' | 'read' | 'write' | 'tool';

export interface PermRequest {
  type: 'permission';
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

/**
 * Consolidated context interface definitions
 * Single source of truth for all context value types
 */

import type {
  Message,
  InteractionRequest,
  InteractionResponse,
  SubagentEvent,
  ChatMode,
  ModeDescriptor,
  Session,
  CreateSessionParams,
  ProviderInfo,
  Project,
  EditorFile,
  ConnectionStatus,
} from '../types';
import type { Accessor } from 'solid-js';
import type { SessionScope } from '../types';

// =============================================================================
// Chat Context
// =============================================================================

export interface ChatContextValue {
  /** The session this chat panel is bound to (undefined in fallback). */
  sessionId: Accessor<string | undefined>;
  messages: Accessor<Message[]>;
  isLoading: Accessor<boolean>;
  isStreaming: Accessor<boolean>;
  pendingInteraction: Accessor<InteractionRequest | null>;
  error: Accessor<string | null>;
  /** Transport health of the SSE stream. Separate from `error`, which also
   * carries daemon-side failures that no reconnect can fix. */
  connectionStatus: Accessor<ConnectionStatus>;
  /** Drop the pending backoff and re-open the stream now. A no-op when no
   * session is bound — there is then nothing to re-subscribe to. */
  retryConnection: () => void;
  subagentEvents: Accessor<SubagentEvent[]>;
  chatMode: Accessor<ChatMode>;
  /** Modes this session may enter, from `session.list_modes`. */
  availableModes: Accessor<ModeDescriptor[]>;
  isLoadingHistory: Accessor<boolean>;
  setChatMode: (mode: ChatMode) => void;
  /** Set the mode UI-side AND persist it daemon-side (POST /mode). */
  switchMode: (mode: ChatMode) => void;
  sendMessage: (content: string) => Promise<void>;
  respondToInteraction: (response: InteractionResponse) => Promise<void>;
  clearMessages: () => void;
  cancelStream: () => Promise<void>;
  addSystemMessage: (content: string) => void;
}

// =============================================================================
// Session Context
// =============================================================================

export interface SessionContextValue {
  currentSession: Accessor<Session | null>;
  sessions: Accessor<Session[]>;
  isLoading: Accessor<boolean>;
  error: Accessor<string | null>;
  availableModels: Accessor<string[]>;
  providers: Accessor<ProviderInfo[]>;
  /** False until the first provider probe resolves — "no providers" claims
   * must wait for this, or the loading state reads as an error. */
  providersLoaded: Accessor<boolean>;
  selectedProvider: Accessor<ProviderInfo | null>;
  createSession: (
    params: CreateSessionParams,
    opts?: { initialMessage?: string; model?: string },
  ) => Promise<Session>;
  /** Fold a kiln/workspace mutation's echoed scope into the session store. */
  applySessionScope: (scope: SessionScope) => void;
  selectSession: (id: string) => Promise<void>;
  /**
   * Re-read the session list, or switch to the other variant of it.
   *
   * The kiln and the workspace are gone from the filter: the daemon was never
   * asked to scope this list, because the tree groups and filters it on the
   * client, and a scoped fetch made "No project" sessions flash then vanish.
   */
  refreshSessions: (filters?: { includeArchived?: boolean }) => Promise<void>;
  pauseSession: () => Promise<void>;
  resumeSession: () => Promise<void>;
  endSession: () => Promise<void>;
  cancelCurrentOperation: () => Promise<boolean>;
  switchModel: (modelId: string) => Promise<void>;
  /** Omit the override to use the current session; pass one to refresh for a
   * session that is still being adopted (it isn't `currentSession` yet). */
  refreshModels: (sessionOverride?: Session) => Promise<void>;
  setSessionTitle: (title: string) => Promise<void>;
  refreshProviders: () => Promise<void>;
  selectProvider: (providerType: string) => void;
  deleteSession: (sessionId: string) => Promise<void>;
  archiveSession: (sessionId: string) => Promise<void>;
  unarchiveSession: (sessionId: string) => Promise<void>;
}

// =============================================================================
// Project Context
// =============================================================================

export interface ProjectContextValue {
  currentProject: Accessor<Project | null>;
  projects: Accessor<Project[]>;
  isLoading: Accessor<boolean>;
  error: Accessor<string | null>;
  registerProject: (path: string) => Promise<Project>;
  unregisterProject: (path: string) => Promise<void>;
  selectProject: (path: string) => Promise<void>;
  refreshProjects: () => Promise<void>;
  clearProject: () => void;
}

// =============================================================================
// Editor Context
// =============================================================================

export interface EditorContextValue {
  openFiles: Accessor<EditorFile[]>;
  activeFile: Accessor<string | null>;
  openFile: (path: string, opts?: { background?: boolean }) => Promise<void>;
  closeFile: (path: string, opts?: { force?: boolean }) => void;
  saveFile: (path: string) => Promise<void>;
  setActiveFile: (path: string) => void;
  updateFileContent: (path: string, content: string) => void;
  /** Move a buffer's base to the hash the daemon answered with. An anchored
   * edit that landed changed the note on disk without a whole save, so the
   * next save would be stale without this.
   *
   * `text` is the note as it is at that hash. A caller that knows it keeps
   * the buffer mergeable; a caller that does not CLEARS the base text, so the
   * next save carries a hash alone rather than a hash paired with a text that
   * is no longer its own. */
  setBaseHash: (path: string, hash: string, text?: string) => void;
  /** Take the note as the disk holds it now: text, base hash and base text.
   *
   * A dirty buffer is asked first — the bytes it holds exist nowhere else, so
   * this is the same discard the close path guards. */
  reloadFile: (path: string) => Promise<void>;
  isLoading: Accessor<boolean>;
  error: Accessor<string | null>;
  /** Re-issue the call that produced `error()`, or null when nothing failed.
   * A failed save leaves the buffer dirty, so the user needs the save back —
   * not a reload that would discard the edit. */
  retryFailedOperation: Accessor<(() => Promise<void>) | null>;
}

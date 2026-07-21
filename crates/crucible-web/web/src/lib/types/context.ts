/**
 * Consolidated context interface definitions
 * Single source of truth for all context value types
 */

import type {
  Message,
  InteractionRequest,
  InteractionResponse,
  SubagentEvent,
  ContextUsage,
  ChatMode,
  Session,
  CreateSessionParams,
  ProviderInfo,
  Project,
  EditorFile,
} from '../types';
import type { Accessor } from 'solid-js';
import type { SessionScope } from '@/lib/api';

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
  subagentEvents: Accessor<SubagentEvent[]>;
  contextUsage: Accessor<ContextUsage | null>;
  chatMode: Accessor<ChatMode>;
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
  refreshSessions: (filters?: { kiln?: string; workspace?: string; includeArchived?: boolean }) => Promise<void>;
  pauseSession: () => Promise<void>;
  resumeSession: () => Promise<void>;
  endSession: () => Promise<void>;
  cancelCurrentOperation: () => Promise<boolean>;
  switchModel: (modelId: string) => Promise<void>;
  refreshModels: () => Promise<void>;
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
  isLoading: Accessor<boolean>;
  error: Accessor<string | null>;
}

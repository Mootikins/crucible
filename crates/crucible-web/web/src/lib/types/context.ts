/**
 * Consolidated context interface definitions
 * Single source of truth for all context value types
 */

import type {
  Message,
  InteractionRequest,
  InteractionResponse,
  ToolCallDisplay,
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

// =============================================================================
// Chat Context
// =============================================================================

export interface ChatContextValue {
  messages: Accessor<Message[]>;
  isLoading: Accessor<boolean>;
  isStreaming: Accessor<boolean>;
  pendingInteraction: Accessor<InteractionRequest | null>;
  error: Accessor<string | null>;
  activeTools: Accessor<ToolCallDisplay[]>;
  subagentEvents: Accessor<SubagentEvent[]>;
  contextUsage: Accessor<ContextUsage | null>;
  chatMode: Accessor<ChatMode>;
  isLoadingHistory: Accessor<boolean>;
  setChatMode: (mode: ChatMode) => void;
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
  selectedProvider: Accessor<ProviderInfo | null>;
  createSession: (params: CreateSessionParams) => Promise<Session>;
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
  openFile: (path: string) => Promise<void>;
  closeFile: (path: string) => void;
  saveFile: (path: string) => Promise<void>;
  setActiveFile: (path: string) => void;
  updateFileContent: (path: string, content: string) => void;
  isLoading: Accessor<boolean>;
  error: Accessor<string | null>;
}

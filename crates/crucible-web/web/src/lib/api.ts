import type {
  ChatEvent,
  CreateSessionParams,
  Session,
  Project,
  FileEntry,
  NoteEntry,
  NoteContent,
  ProviderInfo,
} from './types';

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
  const res = await fetch('/api/chat/send', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ session_id: sessionId, content }),
  });

  if (!res.ok) {
    throw new Error(`Failed to send message: HTTP ${res.status}`);
  }

  const data = (await res.json()) as { message_id: string };
  return data.message_id;
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
): () => void {
  const url = `/api/chat/events/${encodeURIComponent(sessionId)}`;
  let source: EventSource | null = null;
  let reconnectAttempts = 0;
  let reconnectTimeout: ReturnType<typeof setTimeout> | null = null;
  let closed = false;

  const eventTypes = [
    'token',
    'tool_call',
    'tool_call_start',
    'tool_result',
    'tool_result_delta',
    'tool_result_complete',
    'tool_result_error',
    'thinking',
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
  ] as const;

  function connect() {
    if (closed) return;
    
    source = new EventSource(url);

    for (const eventType of eventTypes) {
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
      onEvent({ type: 'error', code: 'sse_reconnecting', message: 'Reconnecting...' });
      
      reconnectTimeout = setTimeout(connect, delay);
    };

    source.onopen = () => {
      reconnectAttempts = 0;
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
export async function respondToInteraction(
  sessionId: string,
  requestId: string,
  response: unknown,
): Promise<void> {
  const res = await fetch('/api/interaction/respond', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ session_id: sessionId, request_id: requestId, response }),
  });

  if (!res.ok) {
    throw new Error(`Failed to respond: HTTP ${res.status}`);
  }
}

// =============================================================================
// Session Endpoints
// =============================================================================

interface RawSession {
  session_id: string;
  type: Session['session_type'];
  kiln: string;
  workspace: string;
  state: Session['state'];
  title: string | null;
  agent_model?: string | null;
  started_at: string;
  event_count?: number;
}

function mapSession(raw: RawSession): Session {
  return {
    id: raw.session_id,
    session_type: raw.type,
    kiln: raw.kiln,
    workspace: raw.workspace,
    state: raw.state,
    title: raw.title,
    agent_model: raw.agent_model ?? null,
    started_at: raw.started_at,
    event_count: raw.event_count ?? 0,
  };
}

export async function createSession(params: CreateSessionParams): Promise<Session> {
  const res = await fetch('/api/session', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(params),
  });

  if (!res.ok) {
    throw new Error(`Failed to create session: HTTP ${res.status}`);
  }

  const raw = (await res.json()) as RawSession;
  return mapSession(raw);
}

/** List sessions with optional filters. */
export async function listSessions(filters?: {
  kiln?: string;
  workspace?: string;
  type?: string;
  state?: string;
}): Promise<Session[]> {
  const params = new URLSearchParams();
  if (filters?.kiln) params.set('kiln', filters.kiln);
  if (filters?.workspace) params.set('workspace', filters.workspace);
  if (filters?.type) params.set('type', filters.type);
  if (filters?.state) params.set('state', filters.state);

  const qs = params.toString();
  const url = qs ? `/api/session/list?${qs}` : '/api/session/list';

  const res = await fetch(url);
  if (!res.ok) {
    throw new Error(`Failed to list sessions: HTTP ${res.status}`);
  }

  const data = (await res.json()) as { sessions: RawSession[]; total: number };
  return data.sessions.map(mapSession);
}

export async function getSession(id: string): Promise<Session> {
  const res = await fetch(`/api/session/${encodeURIComponent(id)}`);
  if (!res.ok) {
    throw new Error(`Failed to get session: HTTP ${res.status}`);
  }

  const raw = (await res.json()) as RawSession;
  return mapSession(raw);
}

/** Pause a session. */
export async function pauseSession(id: string): Promise<void> {
  const res = await fetch(`/api/session/${encodeURIComponent(id)}/pause`, { method: 'POST' });
  if (!res.ok) {
    throw new Error(`Failed to pause session: HTTP ${res.status}`);
  }
}

/** Resume a session (also auto-subscribes to events on the backend). */
export async function resumeSession(id: string): Promise<void> {
  const res = await fetch(`/api/session/${encodeURIComponent(id)}/resume`, { method: 'POST' });
  if (!res.ok) {
    throw new Error(`Failed to resume session: HTTP ${res.status}`);
  }
}

/** End a session. */
export async function endSession(id: string): Promise<void> {
  const res = await fetch(`/api/session/${encodeURIComponent(id)}/end`, { method: 'POST' });
  if (!res.ok) {
    throw new Error(`Failed to end session: HTTP ${res.status}`);
  }
}

/** Cancel the current agent operation in a session. */
export async function cancelSession(id: string): Promise<boolean> {
  const res = await fetch(`/api/session/${encodeURIComponent(id)}/cancel`, { method: 'POST' });
  if (!res.ok) {
    throw new Error(`Failed to cancel session: HTTP ${res.status}`);
  }

  const data = (await res.json()) as { cancelled: boolean };
  return data.cancelled;
}

/** List available models for a session. */
export async function listModels(sessionId: string): Promise<string[]> {
  const res = await fetch(`/api/session/${encodeURIComponent(sessionId)}/models`);
  if (!res.ok) {
    throw new Error(`Failed to list models: HTTP ${res.status}`);
  }

  const data = (await res.json()) as { models: string[] };
  return data.models;
}

/** Switch the model for a session. */
export async function switchModel(sessionId: string, modelId: string): Promise<void> {
  const res = await fetch(`/api/session/${encodeURIComponent(sessionId)}/model`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ model_id: modelId }),
  });

  if (!res.ok) {
    throw new Error(`Failed to switch model: HTTP ${res.status}`);
  }
}

/** Set the title for a session. */
export async function setSessionTitle(sessionId: string, title: string): Promise<void> {
  const res = await fetch(`/api/session/${encodeURIComponent(sessionId)}/title`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ title }),
  });

  if (!res.ok) {
    throw new Error(`Failed to set session title: HTTP ${res.status}`);
  }
}

export interface SessionHistoryEvent {
  role: 'user' | 'assistant' | 'system' | 'tool';
  content: string;
  timestamp?: string;
  tool_calls?: Array<{
    id: string;
    name: string;
    arguments?: unknown;
  }>;
  tool_call_id?: string;
}

export interface SessionHistoryResponse {
  session_id: string;
  history: SessionHistoryEvent[];
  total_events: number;
}

export async function getSessionHistory(
  sessionId: string,
  kiln: string,
  limit?: number,
  offset?: number,
): Promise<SessionHistoryResponse> {
  const params = new URLSearchParams({ kiln });
  if (limit !== undefined) params.set('limit', limit.toString());
  if (offset !== undefined) params.set('offset', offset.toString());

  const res = await fetch(
    `/api/session/${encodeURIComponent(sessionId)}/history?${params.toString()}`,
  );
  if (!res.ok) {
    throw new Error(`Failed to load session history: HTTP ${res.status}`);
  }

  return (await res.json()) as SessionHistoryResponse;
}

/** List available LLM providers and their models. */
export async function listProviders(): Promise<ProviderInfo[]> {
  const res = await fetch('/api/providers');
  if (!res.ok) {
    throw new Error(`Failed to list providers: HTTP ${res.status}`);
  }

  const data = (await res.json()) as { providers: ProviderInfo[] };
  return data.providers;
}

// =============================================================================
// Session Config Endpoints
// =============================================================================

/** Get the thinking budget for a session. */
export async function getThinkingBudget(sessionId: string): Promise<number | null> {
  const res = await fetch(`/api/session/${encodeURIComponent(sessionId)}/config/thinking-budget`);
  if (!res.ok) {
    throw new Error(`Failed to get thinking budget: HTTP ${res.status}`);
  }
  const data = (await res.json()) as { thinking_budget: number | null };
  return data.thinking_budget;
}

/** Set the thinking budget for a session. */
export async function setThinkingBudget(sessionId: string, budget: number | null): Promise<void> {
  const res = await fetch(`/api/session/${encodeURIComponent(sessionId)}/config/thinking-budget`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ thinking_budget: budget }),
  });
  if (!res.ok) {
    throw new Error(`Failed to set thinking budget: HTTP ${res.status}`);
  }
}

/** Get the temperature for a session. */
export async function getTemperature(sessionId: string): Promise<number | null> {
  const res = await fetch(`/api/session/${encodeURIComponent(sessionId)}/config/temperature`);
  if (!res.ok) {
    throw new Error(`Failed to get temperature: HTTP ${res.status}`);
  }
  const data = (await res.json()) as { temperature: number | null };
  return data.temperature;
}

/** Set the temperature for a session. */
export async function setTemperature(sessionId: string, temperature: number): Promise<void> {
  const res = await fetch(`/api/session/${encodeURIComponent(sessionId)}/config/temperature`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ temperature }),
  });
  if (!res.ok) {
    throw new Error(`Failed to set temperature: HTTP ${res.status}`);
  }
}

/** Get the max tokens for a session. */
export async function getMaxTokens(sessionId: string): Promise<number | null> {
  const res = await fetch(`/api/session/${encodeURIComponent(sessionId)}/config/max-tokens`);
  if (!res.ok) {
    throw new Error(`Failed to get max tokens: HTTP ${res.status}`);
  }
  const data = (await res.json()) as { max_tokens: number | null };
  return data.max_tokens;
}

/** Set the max tokens for a session (null = unlimited). */
export async function setMaxTokens(sessionId: string, maxTokens: number | null): Promise<void> {
  const res = await fetch(`/api/session/${encodeURIComponent(sessionId)}/config/max-tokens`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ max_tokens: maxTokens }),
  });
  if (!res.ok) {
    throw new Error(`Failed to set max tokens: HTTP ${res.status}`);
  }
}

/** Get the precognition state for a session. */
export async function getPrecognition(sessionId: string): Promise<boolean> {
  const res = await fetch(`/api/session/${encodeURIComponent(sessionId)}/config/precognition`);
  if (!res.ok) {
    throw new Error(`Failed to get precognition: HTTP ${res.status}`);
  }
  const data = (await res.json()) as { precognition_enabled: boolean };
  return data.precognition_enabled;
}

/** Set the precognition state for a session. */
export async function setPrecognition(sessionId: string, enabled: boolean): Promise<void> {
  const res = await fetch(`/api/session/${encodeURIComponent(sessionId)}/config/precognition`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ enabled }),
  });
  if (!res.ok) {
    throw new Error(`Failed to set precognition: HTTP ${res.status}`);
  }
}

// =============================================================================
// Plugin Endpoints
// =============================================================================

export interface PluginInfo {
  name: string;
  path: string;
  plugin_type: string;
  healthy?: boolean;
}

/** List discovered plugins for a kiln. */
export async function getPlugins(kiln: string): Promise<PluginInfo[]> {
  const params = new URLSearchParams({ kiln });
  const res = await fetch(`/api/plugins?${params.toString()}`);
  if (!res.ok) {
    throw new Error(`Failed to list plugins: HTTP ${res.status}`);
  }
  const data = (await res.json()) as { plugins: PluginInfo[] };
  return data.plugins;
}

/** Reload a plugin by name. */
export async function reloadPlugin(name: string): Promise<{ healthy: boolean; message?: string }> {
  const res = await fetch(`/api/plugins/${encodeURIComponent(name)}/reload`, {
    method: 'POST',
  });
  if (!res.ok) {
    throw new Error(`Failed to reload plugin: HTTP ${res.status}`);
  }
  return (await res.json()) as { healthy: boolean; message?: string };
}

// =============================================================================
// MCP Endpoints
// =============================================================================

/** Get MCP server status. */
export async function getMcpStatus(): Promise<Record<string, unknown>> {
  const res = await fetch('/api/mcp/status');
  if (!res.ok) {
    throw new Error(`Failed to get MCP status: HTTP ${res.status}`);
  }
  return (await res.json()) as Record<string, unknown>;
}

// =============================================================================
// Search Endpoints
// =============================================================================

/** List available kilns. */
export async function listKilns(): Promise<string[]> {
  const res = await fetch('/api/kilns');
  if (!res.ok) {
    throw new Error(`Failed to list kilns: HTTP ${res.status}`);
  }

  const data = (await res.json()) as { kilns: string[] };
  return data.kilns;
}

export async function listNotes(kiln: string, pathFilter?: string): Promise<NoteEntry[]> {
  const params = new URLSearchParams({ kiln });
  if (pathFilter) params.set('path_filter', pathFilter);

  const res = await fetch(`/api/notes?${params.toString()}`);
  if (!res.ok) {
    const errorText = await res.text().catch(() => '');
    throw new Error(errorText || `Failed to list notes: HTTP ${res.status}`);
  }

  const data = (await res.json()) as { notes: NoteEntry[] };
  return data.notes;
}

export async function getNote(name: string, kiln: string): Promise<NoteContent> {
  const params = new URLSearchParams({ kiln });
  const res = await fetch(`/api/notes/${encodeURIComponent(name)}?${params.toString()}`);
  if (!res.ok) {
    const errorText = await res.text().catch(() => '');
    throw new Error(errorText || `Failed to get note: HTTP ${res.status}`);
  }

  return (await res.json()) as NoteContent;
}

export async function saveNote(name: string, kiln: string, content: string): Promise<void> {
  const res = await fetch(`/api/notes/${encodeURIComponent(name)}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ kiln, content }),
  });

  if (!res.ok) {
    const errorText = await res.text().catch(() => '');
    throw new Error(errorText || `Failed to save note: HTTP ${res.status}`);
  }
}

/** Perform a vector search. */
export async function searchVectors(
  kiln: string,
  vector: number[],
  limit?: number,
): Promise<unknown[]> {
  const body: Record<string, unknown> = { kiln, vector };
  if (limit !== undefined) body.limit = limit;

  const res = await fetch('/api/search/vectors', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });

  if (!res.ok) {
    throw new Error(`Failed to search vectors: HTTP ${res.status}`);
  }

  const data = (await res.json()) as { results: unknown[] };
  return data.results;
}

// =============================================================================
// Project Endpoints
// =============================================================================

/** Register a project. */
export async function registerProject(path: string): Promise<Project> {
  const res = await fetch('/api/project/register', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path }),
  });

  if (!res.ok) {
    throw new Error(`Failed to register project: HTTP ${res.status}`);
  }

  return (await res.json()) as Project;
}

/** Unregister a project. */
export async function unregisterProject(path: string): Promise<void> {
  const res = await fetch('/api/project/unregister', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path }),
  });

  if (!res.ok) {
    throw new Error(`Failed to unregister project: HTTP ${res.status}`);
  }
}

/** List all registered projects. */
export async function listProjects(): Promise<Project[]> {
  const res = await fetch('/api/project/list');
  if (!res.ok) {
    throw new Error(`Failed to list projects: HTTP ${res.status}`);
  }

  return (await res.json()) as Project[];
}

/** Get project by path. */
export async function getProject(path: string): Promise<Project | null> {
  const params = new URLSearchParams({ path });
  const res = await fetch(`/api/project/get?${params.toString()}`);
  
  if (res.status === 404) {
    return null;
  }
  
  if (!res.ok) {
    throw new Error(`Failed to get project: HTTP ${res.status}`);
  }

  return (await res.json()) as Project;
}

/** List files in a kiln directory. */
export async function listFiles(path: string): Promise<FileEntry[]> {
  const params = new URLSearchParams({ kiln: path });
  const res = await fetch(`/api/kiln/files?${params.toString()}`);
  if (!res.ok) {
    throw new Error(`Failed to list files: HTTP ${res.status}`);
  }
  const data = (await res.json()) as { files: FileEntry[] };
  return data.files;
}

/** List kiln notes. */
export async function listKilnNotes(kilnPath: string): Promise<FileEntry[]> {
  const params = new URLSearchParams({ kiln: kilnPath });
  const res = await fetch(`/api/kiln/notes?${params.toString()}`);
  if (!res.ok) {
    throw new Error(`Failed to list kiln notes: HTTP ${res.status}`);
  }
  const data = (await res.json()) as { files: FileEntry[] };
  return data.files;
}

/** Get file content by path. */
export async function getFileContent(path: string): Promise<string> {
  const params = new URLSearchParams({ path });
  const res = await fetch(`/api/kiln/file?${params.toString()}`);
  if (!res.ok) {
    throw new Error(`Failed to get file content: HTTP ${res.status}`);
  }
  const data = (await res.json()) as { content: string };
  return data.content;
}

/** Save file content by path. */
export async function saveFileContent(path: string, content: string): Promise<void> {
  const res = await fetch('/api/kiln/file', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path, content }),
  });
  if (!res.ok) {
    throw new Error(`Failed to save file: HTTP ${res.status}`);
  }
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
  const res = await fetch('/api/layout', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(layout),
  });
  if (!res.ok) {
    console.warn(`Failed to save layout: HTTP ${res.status}`);
  }
}

export async function loadLayout(): Promise<SerializedLayout | null> {
  try {
    const res = await fetch('/api/layout');
    if (res.status === 404) return null;
    if (!res.ok) {
      console.warn(`Failed to load layout: HTTP ${res.status}`);
      return null;
    }
    return (await res.json()) as SerializedLayout;
  } catch (err) {
    console.warn('Failed to load layout:', err);
    return null;
  }
}

export async function resetLayout(): Promise<void> {
  const res = await fetch('/api/layout', { method: 'DELETE' });
  if (!res.ok) {
    console.warn(`Failed to reset layout: HTTP ${res.status}`);
  }
}

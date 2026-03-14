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

export interface Config {
  kiln_path: string;
}

type HttpMethod = 'GET' | 'POST' | 'PUT' | 'DELETE' | 'PATCH';

interface RequestOptions extends Omit<RequestInit, 'method'> {
  errorMessage?: string;
  parseAs?: 'json' | 'text' | 'none';
  includeErrorText?: boolean;
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

async function request<T>(
  method: HttpMethod,
  url: string,
  options: RequestOptions = {},
): Promise<T> {
  const { errorMessage = 'Request failed', parseAs = 'json', includeErrorText = false, ...init } = options;
  const res = await fetch(url, { method, ...init });

  if (!res.ok) {
    let errorText = '';
    if (includeErrorText) {
      errorText = await res.text().catch(() => '');
    }
    throw Object.assign(new Error(errorText || `${errorMessage}: HTTP ${res.status}`), {
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
  archived?: boolean;
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

/** Search sessions by title/content. */
export async function searchSessions(query: string, kiln?: string, limit?: number): Promise<Session[]> {
  const params = new URLSearchParams({ q: query });
  if (kiln) params.set('kiln', kiln);
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

/** Switch the model for a session. */
export async function switchModel(sessionId: string, modelId: string): Promise<void> {
  await request<void>('POST', `/api/session/${encodeURIComponent(sessionId)}/model`, {
    errorMessage: 'Failed to switch model',
    parseAs: 'none',
    ...jsonRequest({ model_id: modelId }),
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

/** Generate a title for a session using LLM. */
export async function generateSessionTitle(sessionId: string): Promise<string> {
  return (
    await request<{ title: string }>('POST', `/api/session/${encodeURIComponent(sessionId)}/generate-title`, {
      errorMessage: 'Failed to generate title',
    })
  ).title;
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
  kiln: string,
  limit?: number,
  offset?: number,
  signal?: AbortSignal,
): Promise<SessionHistoryResponse> {
  const params = new URLSearchParams({ kiln });
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

export interface PluginInfo {
  name: string;
  path: string;
  plugin_type: string;
  healthy?: boolean;
}

/** List discovered plugins for a kiln. */
export async function getPlugins(kiln: string): Promise<PluginInfo[]> {
  const params = new URLSearchParams({ kiln });
  return (await request<{ plugins: PluginInfo[] }>('GET', `/api/plugins?${params.toString()}`, {
    errorMessage: 'Failed to list plugins',
  })).plugins;
}

/** Reload a plugin by name. */
export async function reloadPlugin(name: string): Promise<{ healthy: boolean; message?: string }> {
  return request<{ healthy: boolean; message?: string }>(
    'POST',
    `/api/plugins/${encodeURIComponent(name)}/reload`,
    { errorMessage: 'Failed to reload plugin' },
  );
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

/** List available kilns. */
export async function listKilns(): Promise<string[]> {
  return (await request<{ kilns: string[] }>('GET', '/api/kilns', {
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

export async function getNote(name: string, kiln: string): Promise<NoteContent> {
  const params = new URLSearchParams({ kiln });
  return request<NoteContent>('GET', `/api/notes/${encodeURIComponent(name)}?${params.toString()}`, {
    errorMessage: 'Failed to get note',
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

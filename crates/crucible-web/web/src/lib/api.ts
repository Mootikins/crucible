import type {
  ChatEvent,
  CreateSessionParams,
  Session,
  Project,
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
    'tool_result',
    'thinking',
    'message_complete',
    'error',
    'interaction_requested',
    'session_event',
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

/** Create a new session. */
export async function createSession(params: CreateSessionParams): Promise<Session> {
  const res = await fetch('/api/session', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(params),
  });

  if (!res.ok) {
    throw new Error(`Failed to create session: HTTP ${res.status}`);
  }

  return (await res.json()) as Session;
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

  const data = (await res.json()) as { sessions: Session[]; total: number };
  return data.sessions;
}

/** Get a session by ID. */
export async function getSession(id: string): Promise<Session> {
  const res = await fetch(`/api/session/${encodeURIComponent(id)}`);
  if (!res.ok) {
    throw new Error(`Failed to get session: HTTP ${res.status}`);
  }

  return (await res.json()) as Session;
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

/** List notes in a kiln. */
export async function listNotes(kiln: string, pathFilter?: string): Promise<unknown[]> {
  const params = new URLSearchParams({ kiln });
  if (pathFilter) params.set('path_filter', pathFilter);

  const res = await fetch(`/api/notes?${params.toString()}`);
  if (!res.ok) {
    throw new Error(`Failed to list notes: HTTP ${res.status}`);
  }

  const data = (await res.json()) as { notes: unknown[] };
  return data.notes;
}

/** Get a note by name. */
export async function getNote(name: string, kiln: string): Promise<unknown> {
  const params = new URLSearchParams({ kiln });
  const res = await fetch(`/api/notes/${encodeURIComponent(name)}?${params.toString()}`);
  if (!res.ok) {
    throw new Error(`Failed to get note: HTTP ${res.status}`);
  }

  return await res.json();
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

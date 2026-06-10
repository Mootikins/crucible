// RawSession shape (what Axum returns, before mapSession() transforms it)
export const MOCK_SESSION = {
  session_id: 'test-session-001',
  type: 'chat' as const,
  kiln: '/home/user/.crucible/kiln',
  workspace: '/home/user/project',
  state: 'active' as const,
  title: 'Test Session',
  agent_model: 'llama3.2',
  started_at: '2026-01-01T00:00:00Z',
  event_count: 0,
};

export const MOCK_SESSION_2 = {
  session_id: 'test-session-002',
  type: 'chat' as const,
  kiln: '/home/user/.crucible/kiln',
  workspace: '/home/user/project',
  state: 'active' as const,
  title: 'Second Session',
  agent_model: 'llama3.2',
  started_at: '2026-01-01T01:00:00Z',
  event_count: 5,
};

export const MOCK_PROVIDERS = {
  providers: [
    {
      name: 'ollama',
      provider_type: 'ollama',
      available: true,
      default_model: 'llama3.2',
      models: ['llama3.2', 'mistral'],
      endpoint: 'http://localhost:11434',
    },
  ],
};

export const MOCK_KILNS = {
  kilns: ['/home/user/.crucible/kiln'],
};

export const MOCK_CONFIG = {
  kiln_path: '/home/user/.crucible/kiln',
};

export const MOCK_PROJECT = {
  path: '/home/user/project',
  name: 'project',
  kilns: [{ path: '/home/user/.crucible/kiln', name: 'My Kiln' }],
  last_accessed: '2026-01-01T00:00:00Z',
};

/** Build token + message_complete events for a chat response */
export function chatTokenEvents(
  content: string,
  messageId = 'msg-001',
): Array<{ type: string; data: object }> {
  // Split content into ~10-char chunks
  const chunks: string[] = [];
  for (let i = 0; i < content.length; i += 10) {
    chunks.push(content.slice(i, i + 10));
  }
  return [
    ...chunks.map((chunk) => ({ type: 'token', data: { content: chunk } })),
    {
      type: 'message_complete',
      data: { id: messageId, content, tool_calls: [] },
    },
  ];
}

/** Build tool call lifecycle events */
export function toolCallEvents(
  toolName: string,
  args: object,
  result: string,
  toolId = 'tool-001',
): Array<{ type: string; data: object }> {
  return [
    { type: 'tool_call_start', data: { id: toolId, name: toolName, arguments: args } },
    { type: 'tool_result_delta', data: { id: toolId, delta: result } },
    { type: 'tool_result_complete', data: { id: toolId } },
  ];
}

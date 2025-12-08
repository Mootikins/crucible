/**
 * SSE client for chat streaming
 */

export interface ChatEvent {
  type: 'token' | 'tool_call' | 'tool_result' | 'thinking' | 'message_complete' | 'error';
  content?: string;
  id?: string;
  title?: string;
  arguments?: unknown;
  result?: string;
  tool_calls?: { id: string; title: string }[];
  code?: string;
  message?: string;
}

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  toolCalls?: { id: string; title: string }[];
  isStreaming?: boolean;
}

/**
 * Send a chat message and return a stream of events
 */
export async function sendChatMessage(
  message: string,
  onEvent: (event: ChatEvent) => void
): Promise<void> {
  const response = await fetch('/api/chat', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({ message }),
  });

  if (!response.ok) {
    throw new Error(`HTTP error: ${response.status}`);
  }

  const reader = response.body?.getReader();
  if (!reader) {
    throw new Error('No response body');
  }

  const decoder = new TextDecoder();
  let buffer = '';

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;

    buffer += decoder.decode(value, { stream: true });

    // Parse SSE events from buffer
    const lines = buffer.split('\n');
    buffer = lines.pop() || ''; // Keep incomplete line in buffer

    let currentEvent = '';
    let currentData = '';

    for (const line of lines) {
      if (line.startsWith('event: ')) {
        currentEvent = line.slice(7);
      } else if (line.startsWith('data: ')) {
        currentData = line.slice(6);
      } else if (line === '' && currentData) {
        // Empty line = end of event
        try {
          const event = JSON.parse(currentData) as ChatEvent;
          onEvent(event);
        } catch (e) {
          console.error('Failed to parse SSE event:', e, currentData);
        }
        currentEvent = '';
        currentData = '';
      }
    }
  }
}

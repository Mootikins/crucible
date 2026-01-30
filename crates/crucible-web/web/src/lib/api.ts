import type { ChatEvent } from './types';

/**
 * Send a chat message and receive SSE events.
 * @param message - The user's message
 * @param onEvent - Callback for each SSE event
 */
export async function sendChatMessage(
  message: string,
  onEvent: (event: ChatEvent) => void
): Promise<void> {
  const response = await fetch('/api/chat', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ message }),
  });

  if (!response.ok) {
    onEvent({ type: 'error', code: 'http_error', message: `HTTP ${response.status}` });
    return;
  }

  if (!response.body) {
    onEvent({ type: 'error', code: 'no_body', message: 'Response has no body' });
    return;
  }

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';

  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });

      // Parse SSE format: "event: <type>\ndata: <json>\n\n"
      const events = buffer.split('\n\n');
      buffer = events.pop() || '';

      for (const eventBlock of events) {
        if (!eventBlock.trim()) continue;

        const lines = eventBlock.split('\n');
        let eventType = '';
        let eventData = '';

        for (const line of lines) {
          if (line.startsWith('event: ')) {
            eventType = line.slice(7);
          } else if (line.startsWith('data: ')) {
            eventData = line.slice(6);
          }
        }

        if (eventData) {
          try {
            const parsed = JSON.parse(eventData) as ChatEvent;
            onEvent(parsed);

            // Stream ends on message_complete or error
            if (parsed.type === 'message_complete' || parsed.type === 'error') {
              return;
            }
          } catch {
            console.warn('Failed to parse SSE event:', eventData);
          }
        }
      }
    }
  } finally {
    reader.releaseLock();
  }
}

export async function respondToInteraction(
  requestId: string,
  response: unknown
): Promise<void> {
  const res = await fetch('/api/interaction/respond', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ request_id: requestId, response }),
  });

  if (!res.ok) {
    throw new Error(`Failed to respond: HTTP ${res.status}`);
  }
}

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

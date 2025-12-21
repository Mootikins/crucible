// SSE chat client for Solid.js

export type ChatEvent =
  | { type: 'token'; content: string }
  | { type: 'tool_call'; id: string; title: string; arguments?: unknown }
  | { type: 'tool_result'; id: string; result?: string }
  | { type: 'thinking'; content: string }
  | { type: 'message_complete'; id: string; content: string; tool_calls?: ToolCallSummary[] }
  | { type: 'error'; code: string; message: string }

export interface ToolCallSummary {
  id: string
  title: string
}

export interface ChatMessage {
  id: string
  role: 'user' | 'assistant'
  content: string
  isStreaming?: boolean
  toolCalls?: ToolCallSummary[]
}

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
  })

  if (!response.ok) {
    throw new Error(`HTTP error! status: ${response.status}`)
  }

  if (!response.body) {
    throw new Error('Response body is null')
  }

  const reader = response.body.getReader()
  const decoder = new TextDecoder()
  let buffer = ''

  try {
    while (true) {
      const { done, value } = await reader.read()
      if (done) break

      buffer += decoder.decode(value, { stream: true })
      const lines = buffer.split('\n')
      buffer = lines.pop() || '' // Keep incomplete line in buffer

      let eventType = ''
      let eventData = ''

      for (const line of lines) {
        if (line.startsWith('event: ')) {
          eventType = line.slice(7).trim()
        } else if (line.startsWith('data: ')) {
          eventData = line.slice(6).trim()
        } else if (line === '') {
          // Empty line indicates end of event
          if (eventType && eventData) {
            try {
              const event: ChatEvent = JSON.parse(eventData)
              onEvent(event)

              // Stop on completion or error
              if (event.type === 'message_complete' || event.type === 'error') {
                return
              }
            } catch (err) {
              console.error('Failed to parse SSE event:', err, { eventType, eventData })
            }
          }
          eventType = ''
          eventData = ''
        }
      }
    }
  } finally {
    reader.releaseLock()
  }
}


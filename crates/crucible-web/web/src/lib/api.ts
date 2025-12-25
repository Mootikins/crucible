/**
 * Mock API for standalone development.
 * Replace with real SSE client when connecting to Axum backend.
 */

const delay = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));

/**
 * Simulates sending a chat message and streaming a response.
 * @param message - The user's message
 * @param onChunk - Callback for each chunk of the response
 */
export async function sendChatMessage(
  message: string,
  onChunk: (chunk: string) => void
): Promise<void> {
  // Simulate network delay
  await delay(300);

  // Mock response based on input
  const response = getMockResponse(message);

  // Simulate streaming by sending character by character
  for (const char of response) {
    await delay(15);
    onChunk(char);
  }
}

function getMockResponse(message: string): string {
  const lower = message.toLowerCase();

  if (lower.includes('hello') || lower.includes('hi')) {
    return "Hello! I'm a mock assistant running entirely in your browser. How can I help you today?";
  }

  if (lower.includes('test')) {
    return "This is a test response. The chat is working correctly!";
  }

  if (lower.includes('mic') || lower.includes('voice') || lower.includes('audio')) {
    return "Voice input will use browser-based Whisper (WebGPU) for transcription. Hold the mic button to record, and your speech will be converted to text.";
  }

  return `You said: "${message}"\n\nThis is a mock response. Connect to the Axum backend for real AI responses.`;
}

/**
 * Generate a unique message ID
 */
export function generateMessageId(): string {
  return `msg_${Date.now()}_${Math.random().toString(36).substring(2, 9)}`;
}

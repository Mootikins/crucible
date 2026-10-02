import { test, expect } from 'bun:test';
import { startFakeOllama } from './fake-ollama';

test('background completions receive one JSON answer when stream is false', async () => {
  const server = await startFakeOllama({ rules: [], fallback: 'Canonical session title' });
  try {
    const response = await fetch(`http://127.0.0.1:${server.port}/api/chat`, {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ stream: false, messages: [{ role: 'user', content: 'Name this conversation' }] }),
    });
    expect(response.headers.get('content-type')).toBe('application/json');
    const body = await response.json() as { message: { content: string }; done: boolean };
    expect(body.message.content).toBe('Canonical session title');
    expect(body.done).toBe(true);
  } finally {
    await server.close();
  }
});

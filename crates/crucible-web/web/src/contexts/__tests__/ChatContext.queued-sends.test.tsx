import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';
import { ChatProvider, useChat } from '../ChatContext';
import type { ChatContextValue } from '@/lib/types/context';
import { resetTranscriptsForTests } from '../transcriptStore';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { FakeEventSource, installFakeEventSource } from '@/test-utils/sse';

// Mid-turn input is a queue, not a rejection and not an interleave. A message
// typed while the agent's turn streams joins the transcript at the end of the
// current block (below the streaming bubble), waits there unsent, and becomes
// its own turn the moment the stream goes idle — in the order it was queued.

const SESSION = {
  session_id: 's1', type: 'chat', title: 'T', state: 'active', kilns: ['k'],
  workspace: '/w', agent: { model: null }, started_at: '', event_count: 0, archived: false,
};

const SEND = 'POST /api/chat/send';
const CANCEL = 'POST /api/session/s1/cancel';

/** The bodies that reached the send route, in order. */
const sentTurns: { session_id: string; content: string }[] = [];
let turnCounter = 0;
/** When set, the send route holds each request until the case releases it. */
let holdSend: ((answer: unknown) => void) | null = null;

/** The daemon's refusal, as its routes serialise it. */
const refusal = (status: number, message: string): Response =>
  new Response(JSON.stringify({ error: { code: status, message } }), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });

let env: TestQueryEnv;

beforeEach(() => {
  installFakeEventSource();
  sentTurns.length = 0;
  turnCounter = 0;
  holdSend = null;
  env = createTestQueryEnv({
    'GET /api/interactions/pending': () => ({ pending: [] }),
    'GET /api/session/s1': () => SESSION,
    'GET /api/session/s1/history': () => ({ session_id: 's1', history: [], total_events: 0 }),
    [SEND]: async (request) => {
      sentTurns.push((await request.clone().json()) as { session_id: string; content: string });
      if (holdSend) {
        return new Promise((resolve) => holdSend!(resolve));
      }
      return { message_id: `turn-${++turnCounter}` };
    },
    [CANCEL]: () => ({ cancelled: true }),
  });
});

afterEach(() => {
  env?.restore();
  resetTranscriptsForTests();
});

function mountProvider(): ChatContextValue {
  let ctx!: ChatContextValue;
  const Probe = () => {
    ctx = useChat();
    return null;
  };
  render(() => (
    <ChatProvider sessionId="s1">
      <Probe />
    </ChatProvider>
  ));
  return ctx;
}

const stream = () => {
  expect(FakeEventSource.instances.length).toBeGreaterThanOrEqual(1);
  return FakeEventSource.instances[FakeEventSource.instances.length - 1]!;
};

describe('ChatContext queues mid-turn sends', () => {
  it('holds a message typed mid-turn at the end of the streaming block and sends it when the turn ends', async () => {
    const ctx = mountProvider();

    // Turn one is in flight.
    void ctx.sendMessage('first');
    await waitFor(() => expect(sentTurns.map((t) => t.content)).toEqual(['first']));
    stream().emit('token', { type: 'token', content: 'working' });

    // The user types again mid-turn.
    void ctx.sendMessage('second');

    // It was NOT sent: still one POST.
    await waitFor(() =>
      expect(ctx.messages().some((m) => m.role === 'user' && m.content === 'second')).toBe(true),
    );
    expect(sentTurns.map((t) => t.content)).toEqual(['first']);

    // It renders at the end of the streaming block — below the in-flight
    // assistant bubble — marked queued.
    const msgs = ctx.messages();
    const userIdx = msgs.findIndex((m) => m.role === 'user' && m.content === 'second');
    const asstIdx = msgs.findIndex((m) => m.id === 'turn-1-response');
    expect(userIdx).toBeGreaterThan(asstIdx);
    expect(msgs[userIdx]?.queued).toBe(true);

    // The turn ends; the queued message becomes its own turn.
    stream().emit('message_complete', { type: 'message_complete', id: 'turn-1', content: 'working', total_tokens: 8 });
    await waitFor(() => expect(sentTurns.map((t) => t.content)).toEqual(['first', 'second']));
    await waitFor(() =>
      expect(ctx.messages().find((m) => m.content === 'second')?.queued).toBe(false),
    );

    stream().emit('token', { type: 'token', content: 'second answer' });
    stream().emit('message_complete', { type: 'message_complete', id: 'turn-2', content: 'second answer', total_tokens: 13 });

    // Transcript order: the queued prompt sits between the two turns.
    await waitFor(() => expect(ctx.messages()).toHaveLength(4));
    expect(ctx.messages().map((m) => `${m.role}:${m.id}`)).toEqual([
      'user:turn-1',
      'assistant:turn-1-response',
      'user:turn-2',
      'assistant:turn-2-response',
    ]);
  });

  it('queues several messages and dispatches them in order, one turn at a time', async () => {
    const ctx = mountProvider();

    void ctx.sendMessage('first');
    await waitFor(() => expect(sentTurns.map((t) => t.content)).toEqual(['first']));
    stream().emit('token', { type: 'token', content: 'working' });

    void ctx.sendMessage('second');
    void ctx.sendMessage('third');
    await waitFor(() =>
      expect(ctx.messages().filter((m) => m.role === 'user' && m.queued)).toHaveLength(2),
    );
    expect(sentTurns).toHaveLength(1);

    // Turn one ends: only the FIRST queued message may dispatch — the daemon
    // takes one turn at a time, and firing both would race the slot again.
    stream().emit('message_complete', { type: 'message_complete', id: 'turn-1', content: 'working', total_tokens: 8 });
    await waitFor(() => expect(sentTurns.map((t) => t.content)).toEqual(['first', 'second']));
    expect(sentTurns).toHaveLength(2);

    // Turn two ends: now the third goes.
    stream().emit('message_complete', { type: 'message_complete', id: 'turn-2', content: 'second answer', total_tokens: 13 });
    await waitFor(() => expect(sentTurns.map((t) => t.content)).toEqual(['first', 'second', 'third']));
  });

  it('re-queues a send the daemon refused as concurrent instead of surfacing an error', async () => {
    const ctx = mountProvider();
    await runTurn(ctx, 'first', 'first answer');

    // Hold our POST, and let a foreign client's turn take the stream while
    // it is in flight — the shape of losing an admission race.
    let release!: (answer: unknown) => void;
    const gated = new Promise<void>((resolve) => {
      holdSend = (routeResolve) => {
        release = routeResolve as (answer: unknown) => void;
        resolve();
      };
    });

    void ctx.sendMessage('raced');
    stream().emit('thinking', { type: 'thinking', content: 'foreign turn working ' });
    await gated;
    await waitFor(() => expect(ctx.isStreaming()).toBe(true));

    // The daemon refuses our POST: the session already runs a turn. (The
    // message is AgentError::ConcurrentRequest's Display, which the API
    // layer surfaces verbatim — the requeue discriminator matches it.)
    release(refusal(422, 'Concurrent request in progress for session: s1'));

    // The refusal must not become an error banner, and the message must not
    // be dropped: it stays queued...
    expect(ctx.error()).toBeNull();
    expect(ctx.messages().some((m) => m.role === 'system')).toBe(false);
    await waitFor(() =>
      expect(ctx.messages().find((m) => m.content === 'raced')?.queued).toBe(true),
    );

    // ...and once the foreign turn ends, the flush dispatches it.
    stream().emit('message_complete', { type: 'message_complete', id: 'turn-2', content: 'foreign answer', total_tokens: 10 });
    await waitFor(() => expect(sentTurns.some((t) => t.content === 'raced')).toBe(true));
  });
});

describe('ChatContext closes a cancelled turn cleanly', () => {
  it('finalizes the thinking block and frees the turn for the next send', async () => {
    const ctx = mountProvider();

    void ctx.sendMessage('first');
    await waitFor(() => expect(sentTurns.map((t) => t.content)).toEqual(['first']));
    stream().emit('thinking', { type: 'thinking', content: 'deep in thought' });
    await waitFor(() =>
      expect(ctx.messages().some((m) => m.thinking?.content === 'deep in thought')).toBe(true),
    );
    expect(ctx.messages().find((m) => m.thinking)?.thinking?.isStreaming).toBe(true);

    await ctx.cancelStream();

    // The turn ended without a message_complete (the daemon cancels silently),
    // so the thinking block must not be left streaming — it would render
    // "Thinking…" with the animated wave for the rest of the transcript.
    const thought = ctx.messages().find((m) => m.thinking);
    expect(thought?.thinking?.isStreaming).toBe(false);
    expect(thought?.thinking?.tokenCount).toBe('deep in thought'.length);

    // The turn slot is free: the next send dispatches immediately.
    await ctx.sendMessage('second');
    await waitFor(() => expect(sentTurns.map((t) => t.content)).toEqual(['first', 'second']));
  });
});

/** One full turn: send, think, answer, complete. */
async function runTurn(ctx: ChatContextValue, content: string, answer: string) {
  void ctx.sendMessage(content);
  await waitFor(() => expect(sentTurns.some((t) => t.content === content)).toBe(true));
  const id = `turn-${turnCounter}`;
  stream().emit('thinking', { type: 'thinking', content: `reasoning about ${content} ` });
  stream().emit('token', { type: 'token', content: answer });
  stream().emit('message_complete', { type: 'message_complete', id, content: answer, total_tokens: 10 });
  await waitFor(() => expect(ctx.isStreaming()).toBe(false));
}

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';
import { ChatProvider, useChat } from '../ChatContext';
import type { ChatContextValue } from '@/lib/types/context';
import { resetTranscriptsForTests } from '../transcriptStore';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { FakeEventSource, installFakeEventSource } from '@/test-utils/sse';
import { emitOps, historyOf, segment, upsert, userTurn } from '@/test-utils/transcript';

// Mid-turn input is a queue, not a rejection and not an interleave. A message
// typed while the agent's turn streams shows at the end of the transcript,
// waits there unsent, and becomes its own turn the moment the stream goes
// idle — in the order it was queued. The daemon's transcript arrives as ops.

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
    'GET /api/session/s1/history': () => historyOf('s1', [], 0),
    [SEND]: async (request) => {
      sentTurns.push((await request.clone().json()) as { session_id: string; content: string });
      if (holdSend) {
        return new Promise((resolve) => holdSend!(resolve));
      }
      return { outcome: 'turn', message_id: `turn-${++turnCounter}` };
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

/** The seq of the next frame the fake daemon sends. */
let seq = 0;
/** The daemon echoes the turn `id` and streams its first segment. */
function daemonTurn(id: string, prompt: string, text: string): void {
  emitOps(stream(), ++seq, [upsert(userTurn(id, prompt))]);
  emitOps(stream(), ++seq, [upsert(segment(id, 0, text, { streaming: true }))]);
  stream().emit('text_delta', { event: 'text_delta', data: { content: text } });
}
/** The daemon ends the turn `id`. */
function daemonTurnEnds(id: string, text: string): void {
  emitOps(stream(), ++seq, [upsert(segment(id, 0, text))]);
  stream().emit('message_complete', { event: 'message_complete', data: { message_id: id, full_response: text } });
}

describe('ChatContext queues mid-turn sends', () => {
  it('holds a message typed mid-turn at the end of the streaming block and sends it when the turn ends', async () => {
    const ctx = mountProvider();

    // Turn one is in flight.
    await waitFor(() => expect(env.fetch.calls('GET /api/session/s1/history')).toBe(1));
    seq = 0;
    void ctx.sendMessage('first');
    await waitFor(() => expect(sentTurns.map((t) => t.content)).toEqual(['first']));
    daemonTurn('turn-1', 'first', 'working');

    // The user types again mid-turn.
    void ctx.sendMessage('second');

    // It was NOT sent: still one POST.
    await waitFor(() =>
      expect(ctx.messages().some((m) => m.role === 'user' && m.content === 'second')).toBe(true),
    );
    expect(sentTurns.map((t) => t.content)).toEqual(['first']);

    // It renders at the end of the transcript — below the in-flight
    // assistant segment — marked queued.
    const msgs = ctx.messages();
    const userIdx = msgs.findIndex((m) => m.role === 'user' && m.content === 'second');
    const asstIdx = msgs.findIndex((m) => m.id === 'turn-1-seg-0');
    expect(asstIdx).toBeGreaterThan(-1);
    expect(userIdx).toBeGreaterThan(asstIdx);
    expect(msgs[userIdx]?.queued).toBe(true);

    // The turn ends; the queued message becomes its own turn.
    daemonTurnEnds('turn-1', 'working');
    await waitFor(() => expect(sentTurns.map((t) => t.content)).toEqual(['first', 'second']));
    await waitFor(() =>
      expect(ctx.messages().find((m) => m.content === 'second')?.queued).toBeUndefined(),
    );

    daemonTurn('turn-2', 'second', 'second answer');
    daemonTurnEnds('turn-2', 'second answer');

    // Transcript order: the queued prompt sits between the two turns, and the
    // daemon's user turn replaced the optimistic entry.
    await waitFor(() => expect(ctx.messages()).toHaveLength(4));
    expect(ctx.messages().map((m) => `${m.role}:${m.id}`)).toEqual([
      'user:turn-1',
      'assistant:turn-1-seg-0',
      'user:turn-2',
      'assistant:turn-2-seg-0',
    ]);
  });

  it('queues several messages and dispatches them in order, one turn at a time', async () => {
    const ctx = mountProvider();

    await waitFor(() => expect(env.fetch.calls('GET /api/session/s1/history')).toBe(1));
    seq = 0;
    void ctx.sendMessage('first');
    await waitFor(() => expect(sentTurns.map((t) => t.content)).toEqual(['first']));
    daemonTurn('turn-1', 'first', 'working');

    void ctx.sendMessage('second');
    void ctx.sendMessage('third');
    await waitFor(() =>
      expect(ctx.messages().filter((m) => m.role === 'user' && m.queued)).toHaveLength(2),
    );
    expect(sentTurns).toHaveLength(1);

    // Turn one ends: only the FIRST queued message may dispatch — the daemon
    // takes one turn at a time, and firing both would race the slot again.
    daemonTurnEnds('turn-1', 'working');
    await waitFor(() => expect(sentTurns.map((t) => t.content)).toEqual(['first', 'second']));
    expect(sentTurns).toHaveLength(2);

    // Turn two ends: now the third goes.
    daemonTurn('turn-2', 'second', 'second answer');
    daemonTurnEnds('turn-2', 'second answer');
    await waitFor(() => expect(sentTurns.map((t) => t.content)).toEqual(['first', 'second', 'third']));
  });

  it('re-queues a send the daemon refused as concurrent instead of surfacing an error', async () => {
    const ctx = mountProvider();
    await waitFor(() => expect(env.fetch.calls('GET /api/session/s1/history')).toBe(1));
    seq = 0;
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
    stream().emit('thinking', { event: 'thinking', data: { content: 'foreign turn working ' } });
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
    stream().emit('turn_finished', { event: 'turn_finished', data: { status: 'completed' } });
    await waitFor(() => expect(sentTurns.some((t) => t.content === 'raced')).toBe(true));
  });
});

describe('ChatContext closes a cancelled turn cleanly', () => {
  it('closes the turn when the daemon broadcasts ended, freeing the next send', async () => {
    const ctx = mountProvider();
    await waitFor(() => expect(env.fetch.calls('GET /api/session/s1/history')).toBe(1));

    void ctx.sendMessage('first');
    await waitFor(() => expect(sentTurns.map((t) => t.content)).toEqual(['first']));
    const thinking = segment('turn-1', 0, '', { streaming: true, thinking: 'deep in thought' });
    emitOps(stream(), 1, [upsert(userTurn('turn-1', 'first')), upsert(thinking)]);
    await waitFor(() =>
      expect(ctx.messages().some((m) => m.thinking?.content === 'deep in thought')).toBe(true),
    );
    expect(ctx.messages().find((m) => m.thinking)?.thinking?.isStreaming).toBe(true);

    await ctx.cancelStream();
    // The daemon records `turn_finished` BEFORE the cancel call resolves,
    // and every subscriber receives it, with the ops that close the turn.
    stream().emit('turn_finished', { event: 'turn_finished', data: { status: 'cancelled' } });
    emitOps(stream(), 2, [upsert(segment('turn-1', 0, '', { thinking: 'deep in thought' }))]);

    // The daemon closed the segment, so the thinking block is not left
    // streaming — it would render "Thinking…" for the rest of the transcript.
    const thought = ctx.messages().find((m) => m.thinking);
    expect(thought?.thinking?.isStreaming).toBe(false);
    expect(thought?.thinking?.tokenCount).toBe(4); // ~4 tokens at chars/4 for 15 chars

    // The turn slot is free: the next send dispatches immediately.
    await ctx.sendMessage('second');
    await waitFor(() => expect(sentTurns.map((t) => t.content)).toEqual(['first', 'second']));
  });

  it('a foreign cancel (no local cancel call) closes the turn too', async () => {
    // T1's bug: a cancel issued from another client left THIS pane streaming
    // forever, because the old client-side patch only ever ran for a cancel
    // this pane issued itself. The `turn_finished` reaches every subscriber.
    const ctx = mountProvider();

    void ctx.sendMessage('foreign');
    await waitFor(() => expect(sentTurns.map((t) => t.content)).toEqual(['foreign']));
    stream().emit('text_delta', { event: 'text_delta', data: { content: 'partial' } });
    stream().emit('turn_finished', { event: 'turn_finished', data: { status: 'cancelled' } });

    await waitFor(() => expect(ctx.isStreaming()).toBe(false));
    await waitFor(() => expect(ctx.isLoading()).toBe(false));
    // The turn slot is free here as well.
    await ctx.sendMessage('next');
    await waitFor(() => expect(sentTurns.map((t) => t.content)).toEqual(['foreign', 'next']));
  });
});

/** One full turn: send, answer, complete. */
async function runTurn(ctx: ChatContextValue, content: string, answer: string) {
  void ctx.sendMessage(content);
  await waitFor(() => expect(sentTurns.some((t) => t.content === content)).toBe(true));
  const id = `turn-${turnCounter}`;
  daemonTurn(id, content, answer);
  daemonTurnEnds(id, answer);
  await waitFor(() => expect(ctx.isStreaming()).toBe(false));
}

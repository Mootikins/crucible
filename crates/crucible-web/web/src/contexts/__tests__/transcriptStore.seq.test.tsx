import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { FakeEventSource, installFakeEventSource } from '@/test-utils/sse';
import { resetSseForTests, sessionCursor } from '@/lib/query/sse';
import {
  recordTranscriptHydration,
  retainTranscript,
  resetTranscriptsForTests,
  transcriptMessages,
} from '../transcriptStore';
// No `vi.mock('@/lib/api')`. The stream is the FakeEventSource, and the store
// folds it through the REAL sessionEvents root — so what these cases prove is
// the dedup contract the replay path rides on: a stamped event applies once,
// at its seq, and everything at or below the last applied seq is a replay.

const ID = 'seq-session';

beforeEach(() => {
  installFakeEventSource();
  resetSseForTests();
  resetTranscriptsForTests();
});

afterEach(() => {
  resetTranscriptsForTests();
  resetSseForTests();
});

function start(): void {
  retainTranscript(ID);
}

/** One stamped frame, as the route writes it: the seq rides `id:`. */
function frame(seq: number, type: string, data: Record<string, unknown>): void {
  FakeEventSource.instances[0]!.emit(type, data, { lastEventId: String(seq) });
}

/** The transcript's message contents, in order. */
function contents(): string[] {
  return transcriptMessages(ID)
    .filter((message) => message.role !== 'tool')
    .map((message) => message.content);
}

describe('the transcript store dedups by seq', () => {
  it('applies a stamped event once and drops its replay', () => {
    start();

    frame(3, 'message_complete', { type: 'message_complete', id: 'm1', content: 'one' });
    expect(contents()).toEqual(['one']);

    // The same seq again — a reconnect that replayed it — draws nothing.
    frame(3, 'message_complete', { type: 'message_complete', id: 'm1', content: 'one' });
    // A LOWER seq is stale by definition: everything it carried is subsumed.
    frame(2, 'message_complete', { type: 'message_complete', id: 'm0', content: 'zero' });

    expect(contents()).toEqual(['one']);
    expect(sessionCursor(ID)).toBe(3);
  });

  it('applies the next stamped event and advances the watermark', () => {
    start();

    frame(3, 'message_complete', { type: 'message_complete', id: 'm1', content: 'one' });
    frame(4, 'message_complete', { type: 'message_complete', id: 'm2', content: 'two' });

    expect(contents()).toEqual(['one', 'two']);
    expect(sessionCursor(ID)).toBe(4);
  });

  it('still identity-dedups a one-shot event that carries no seq', () => {
    start();

    // Synthetic frames (the web tier's own stream_gap, a client-minted
    // connection event) have no seq to compare; the identity set still
    // stands guard over the one-shots among them.
    FakeEventSource.instances[0]!.emit('tool_call', {
      type: 'tool_call', id: 'call-1', title: 'update_note',
    });
    FakeEventSource.instances[0]!.emit('tool_call', {
      type: 'tool_call', id: 'call-1', title: 'update_note',
    });

    const tools = transcriptMessages(ID).filter((message) => message.role === 'tool');
    expect(tools).toHaveLength(1);
  });
});

describe('history hydration records the watermark', () => {
  it('drops streamed events the hydration already covered', () => {
    start();

    // The fold of a history document holding seqs 1-5 finished; the seam
    // records its max AFTER the update, as one call.
    recordTranscriptHydration(ID, 5);

    // The stream (or a reconnect's replay) redelivers seq 5: already folded.
    frame(5, 'message_complete', { type: 'message_complete', id: 'm1', content: 'from stream' });
    expect(contents()).toEqual([]);

    // Above the hydration: live, applies.
    frame(6, 'message_complete', { type: 'message_complete', id: 'm2', content: 'new' });
    expect(contents()).toEqual(['new']);
    expect(sessionCursor(ID)).toBe(6);
  });

  it('never walks the watermark back behind a live-applied seq', () => {
    start();

    frame(9, 'message_complete', { type: 'message_complete', id: 'm1', content: 'live' });
    // A hydration racing the stream reads an older snapshot: seqs 1-4.
    recordTranscriptHydration(ID, 4);

    expect(sessionCursor(ID)).toBe(9);
    // And the older watermark did not reopen a hole: seq 5 is still stale.
    frame(5, 'message_complete', { type: 'message_complete', id: 'm2', content: 'stale' });
    expect(contents()).toEqual(['live']);
  });

  it('takes nothing from a document whose events carry no seq', () => {
    start();

    recordTranscriptHydration(ID, undefined);
    recordTranscriptHydration(ID, 0);

    expect(sessionCursor(ID)).toBeUndefined();
  });
});

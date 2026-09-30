/**
 * Transcript items and frames for a test, in the daemon's wire form.
 *
 * The daemon folds the events (`crates/crucible-core/src/transcript/`), so a
 * test states the fold's result: a snapshot, and the ops of a `transcript`
 * frame. It never states the events and folds them in TypeScript.
 */
import type { Transcript, TranscriptItem, TranscriptOp } from '@/lib/transcript';
import type { FakeEventSource } from './sse';

/** A user turn. The id is the turn id. */
export function userTurn(
  id: string,
  content: string,
  extra: Partial<Extract<TranscriptItem, { type: 'user_turn' }>> = {},
): TranscriptItem {
  return { id, turn_id: id, type: 'user_turn', content, ...extra };
}

/** An answer segment of the turn `turn`. */
export function segment(
  turn: string,
  index: number,
  text: string,
  extra: Partial<Extract<TranscriptItem, { type: 'assistant_segment' }>> = {},
): TranscriptItem {
  return {
    id: `${turn}-seg-${index}`,
    turn_id: turn,
    type: 'assistant_segment',
    index,
    text,
    streaming: false,
    ...extra,
  };
}

/** A tool card of the turn `turn`. */
export function toolCard(
  turn: string,
  callId: string,
  extra: Partial<Extract<TranscriptItem, { type: 'tool_card' }>> = {},
): TranscriptItem {
  return {
    id: `tool-${callId}`,
    turn_id: turn,
    type: 'tool_card',
    call_id: callId,
    name: 'tool',
    args: {},
    status: 'complete',
    ...extra,
  };
}

/** A notice of the daemon. */
export function notice(
  id: string,
  body: Extract<TranscriptItem, { type: 'notice' }>['notice'],
  turn?: string,
): TranscriptItem {
  return { id, ...(turn ? { turn_id: turn } : {}), type: 'notice', notice: body };
}

/** The history document of a session whose transcript is `items`. */
export function historyOf(sessionId: string, items: TranscriptItem[], asOfSeq = items.length) {
  const transcript: Transcript = { as_of_seq: asOfSeq, items };
  return { session_id: sessionId, history: [], total_events: 0, transcript };
}

/** An upsert op. */
export function upsert(item: TranscriptItem, before?: string): TranscriptOp {
  return { op: 'upsert', item, ...(before ? { before } : {}) };
}

/** An append op. `at` counts UTF-8 bytes, as the daemon does. */
export function append(id: string, at: number, text: string, field: 'text' | 'thinking' = 'text'): TranscriptOp {
  return { op: 'append', id, field, at, text };
}

/**
 * Acts as the server: one `transcript` frame with the seq `seq`.
 *
 * `topic` names the session the frame belongs to — required once more than
 * one session's topic travels the shared connection at once (Simplification
 * Plan step 19), since then the connection cannot infer it. A single-session
 * test may omit it: `lib/api.ts`'s dispatcher reads a topic-less frame as the
 * one topic joined, when there is only one.
 */
export function emitOps(source: FakeEventSource, seq: number, ops: TranscriptOp[], topic?: string): void {
  source.emit(
    'transcript',
    { ...(topic ? { topic } : {}), type: 'transcript', seq, ops },
    { lastEventId: topic ? `${topic}:${seq}` : String(seq) },
  );
}

import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, cleanup } from '@solidjs/testing-library';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve as resolvePath } from 'node:path';
import type { Message } from '@/lib/types';
import {
  applyTranscriptOp,
  itemToMessage,
  utf8Length,
  type Transcript,
  type TranscriptItem,
} from '../transcript';
import { append, notice, segment, toolCard, upsert, userTurn } from '@/test-utils/transcript';

let messages: Message[] = [];
vi.mock('@/contexts/ChatContext', () => ({
  useChatSafe: () => ({
    messages: () => messages,
    isStreaming: () => false,
    sessionId: () => 's1',
    sendMessage: async () => {},
    pendingInteraction: () => null,
    respondToInteraction: async () => {},
  }),
}));
vi.mock('@/contexts/SessionContext', () => ({
  useSessionSafe: () => ({ currentSession: () => null, sessions: () => [] }),
}));

// Import AFTER the mocks.
import { MessageList } from '@/components/MessageList';

afterEach(() => {
  cleanup();
  messages = [];
});

const of = (...items: TranscriptItem[]): Transcript => ({ as_of_seq: 0, items });
const ids = (transcript: Transcript | null) => transcript?.items.map((item) => item.id);

// ============================================================================
// The port of `Transcript::apply` (crates/crucible-core/src/transcript/mod.rs)
// ============================================================================

describe('applyTranscriptOp', () => {
  it('replaces an item with the id of the upsert, where it stands', () => {
    const t = of(userTurn('a', 'one'), userTurn('b', 'two'));
    const next = applyTranscriptOp(t, upsert(userTurn('a', 'ONE')));
    expect(ids(next)).toEqual(['a', 'b']);
    expect(next?.items[0]).toMatchObject({ content: 'ONE' });
    // The unchanged item is the same object.
    expect(next?.items[1]).toBe(t.items[1]);
  });

  it('puts a new item before the item `before` names, or at the end', () => {
    const t = of(userTurn('a', 'one'), userTurn('b', 'two'));
    expect(ids(applyTranscriptOp(t, upsert(userTurn('c', 'x'), 'b')))).toEqual(['a', 'c', 'b']);
    expect(ids(applyTranscriptOp(t, upsert(userTurn('c', 'x'))))).toEqual(['a', 'b', 'c']);
    // A `before` that is not here puts the item at the end, as the daemon does.
    expect(ids(applyTranscriptOp(t, upsert(userTurn('c', 'x'), 'gone')))).toEqual(['a', 'b', 'c']);
  });

  it('appends text and thinking at the length of the field', () => {
    let t: Transcript | null = of(segment('t', 0, 'ab', { streaming: true }));
    t = applyTranscriptOp(t, append('t-seg-0', 2, 'c'));
    t = t && applyTranscriptOp(t, append('t-seg-0', 0, 'why', 'thinking'));
    expect(t?.items[0]).toMatchObject({ text: 'abc', thinking: 'why' });
  });

  it('refuses an append that does not fit', () => {
    const t = of(segment('t', 0, 'ab'), userTurn('u', 'q'));
    // At the wrong offset: an op was lost.
    expect(applyTranscriptOp(t, append('t-seg-0', 1, 'c'))).toBeNull();
    // To an item that is not here.
    expect(applyTranscriptOp(t, append('nope', 0, 'c'))).toBeNull();
    // To an item that is not a segment.
    expect(applyTranscriptOp(t, append('u', 1, 'c'))).toBeNull();
  });

  it('counts the offset in UTF-8 bytes, as the daemon does', () => {
    // "é" is two bytes, "日" three, and "😀" four (two UTF-16 units).
    expect(utf8Length('aé日😀')).toBe(1 + 2 + 3 + 4);
    const t = of(segment('t', 0, 'é😀'));
    expect(applyTranscriptOp(t, append('t-seg-0', 3, '!'))).toBeNull();
    expect(applyTranscriptOp(t, append('t-seg-0', 6, '!'))?.items[0]).toMatchObject({ text: 'é😀!' });
  });
});

// ============================================================================
// The adapter: a pure map from an item to the view model
// ============================================================================

describe('itemToMessage', () => {
  it('maps the status of a tool card', () => {
    const card = (status: 'running' | 'complete' | 'failed' | 'incomplete', extra = {}) =>
      itemToMessage(toolCard('t', 'c', { status, ...extra }))?.toolCall;
    expect(card('running')).toMatchObject({ status: 'running', callId: 'c' });
    expect(card('complete', { result: 'ok' })).toMatchObject({ status: 'complete', result: 'ok' });
    expect(card('failed', { error: 'denied' })).toMatchObject({ status: 'error', result: 'denied' });
    expect(card('incomplete')).toMatchObject({ status: 'error', result: 'tool did not complete' });
  });

  it('maps the reasoning of a segment, which streams until its text starts', () => {
    const thinking = (item: TranscriptItem) => itemToMessage(item)?.thinking;
    expect(thinking(segment('t', 0, '', { streaming: true, thinking: 'hmm' }))).toMatchObject({
      isStreaming: true,
    });
    expect(thinking(segment('t', 0, 'a', { streaming: true, thinking: 'hmm' }))).toMatchObject({
      isStreaming: false,
      tokenCount: 1,
    });
  });

  it('maps a notice to the words the daemon gave', () => {
    expect(itemToMessage(notice('c', { kind: 'context_cleared', plugin: 'alpha' }))).toMatchObject({
      role: 'system',
      type: 'clear',
      content: '↻ alpha cleared the context',
    });
    expect(
      itemToMessage(notice('s', { kind: 'stop_reason', reason: 'max_tokens', text: 'cut off' })),
    ).toMatchObject({ role: 'system', content: 'cut off' });
  });

  it('draws no injected context', () => {
    expect(
      itemToMessage({ id: 'x', type: 'injected_context', role: 'user', content: 'ctx' }),
    ).toBeNull();
  });
});

// ============================================================================
// The golden folds of the daemon, drawn by the web client
// ============================================================================

const GOLDEN_DIR = resolvePath(process.cwd(), '../../../assets/fixtures/golden/transcript');
const goldens = readdirSync(GOLDEN_DIR).filter((name) => name.endsWith('.json'));

/**
 * The start of a text, as letters and digits. The markdown render drops the
 * marks, so the rows compare the words.
 */
const words = (text: string) => text.replace(/[^\p{L}\p{N}]/gu, '').slice(0, 32);

/** What the web client should draw for an item, in the order of the fold. */
function expectedRow(item: TranscriptItem): string | null {
  switch (item.type) {
    case 'user_turn':
      return `user:${item.content}`;
    case 'assistant_segment':
      return item.text.trim() === '' && !item.thinking ? null : `segment:${words(item.text)}`;
    case 'tool_card':
      return `tool:${item.call_id}`;
    case 'notice':
      return 'notice';
    default:
      return null;
  }
}

/** What the transcript drew, in document order. */
function drawnRows(container: HTMLElement): string[] {
  const rows: string[] = [];
  for (const el of container.querySelectorAll<HTMLElement>(
    '[data-testid="message-user"], [data-testid="message-system"], [data-testid="message-assistant"], [data-tool-call-id]',
  )) {
    if (el.dataset.toolCallId) rows.push(`tool:${el.dataset.toolCallId}`);
    else if (el.dataset.testid === 'message-user')
      rows.push(`user:${el.querySelector('p')?.textContent ?? ''}`);
    else if (el.dataset.testid === 'message-system') rows.push('notice');
    else {
      // The answer text only: the reasoning draws in its own block above it.
      const prose = el.querySelector<HTMLElement>('[class*="prose"]');
      rows.push(`segment:${words(prose?.textContent ?? '')}`);
    }
  }
  return rows;
}

describe('the golden transcripts', () => {
  it('has fixtures to draw', () => {
    expect(goldens.length).toBeGreaterThan(0);
  });

  it.each(goldens)('%s draws its turns, segments and tool cards in order', (name) => {
    const golden = JSON.parse(readFileSync(resolvePath(GOLDEN_DIR, name), 'utf8')) as Transcript;
    messages = golden.items.map(itemToMessage).filter((m): m is Message => m !== null);

    const { container } = render(() => <MessageList />);

    const expected = golden.items.map(expectedRow).filter((row): row is string => row !== null);
    expect(drawnRows(container)).toEqual(expected);
    // Each turn of the fold is one turn block after its prompt.
    const turns = new Set(
      golden.items
        .filter((item) => item.type === 'assistant_segment' || item.type === 'tool_card')
        .map((item) => item.turn_id),
    );
    expect(container.querySelectorAll('[data-testid="assistant-turn"]').length).toBeGreaterThanOrEqual(
      turns.size,
    );
  });
});

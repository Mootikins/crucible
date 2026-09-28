/**
 * The daemon's folded transcript, as the web client holds it.
 *
 * The daemon folds the events of a session once
 * (`crates/crucible-core/src/transcript/`). The history route serves the fold
 * as a snapshot, and the chat stream sends the ops of each live event in a
 * `transcript` frame. The client applies the ops to the snapshot with
 * `applyTranscriptOp`, a port of `Transcript::apply`. It does not fold events
 * itself.
 *
 * `itemToMessage` is the one adapter from an item to the view model that the
 * message components draw. It is a pure map: the order, the merges and the
 * status of each item come from the daemon.
 */
import type { components } from './api-schema';
import type { Message, SubagentEvent, ToolCallDisplay } from './types';
import { estimateThinkingTokens, originName } from './turn';

type Schemas = components['schemas'];

export type Transcript = Schemas['Transcript'];
export type TranscriptItem = Schemas['TranscriptItem'];
export type TranscriptOp = Schemas['TranscriptOp'];

/** The body of an item of one `type`. */
type ItemOf<T extends TranscriptItem['type']> = Extract<TranscriptItem, { type: T }>;

/**
 * The length of `text` in UTF-8 bytes. The `at` of an append op counts
 * bytes, because the daemon measures a Rust `String`.
 */
export function utf8Length(text: string): number {
  let bytes = 0;
  for (let i = 0; i < text.length; i++) {
    const code = text.charCodeAt(i);
    if (code < 0x80) bytes += 1;
    else if (code < 0x800) bytes += 2;
    else if (code >= 0xd800 && code <= 0xdbff && i + 1 < text.length) {
      // A surrogate pair is one code point of four bytes.
      bytes += 4;
      i++;
    } else bytes += 3;
  }
  return bytes;
}

/**
 * Applies one op, and answers the new transcript, or `null` when the op does
 * not fit. An op does not fit when it appends to an item that is not here,
 * to an item that is not a segment, or at an offset that is not the length
 * of the field. The caller then reads a new snapshot.
 *
 * The answer shares each unchanged item with `transcript`, so a view that
 * keys on the item object redraws only the item that changed.
 */
export function applyTranscriptOp(transcript: Transcript, op: TranscriptOp): Transcript | null {
  const items = transcript.items;
  if (op.op === 'upsert') {
    const index = items.findIndex((item) => item.id === op.item.id);
    const next = [...items];
    if (index !== -1) {
      next[index] = op.item;
    } else {
      const before = op.before == null ? -1 : items.findIndex((item) => item.id === op.before);
      next.splice(before === -1 ? items.length : before, 0, op.item);
    }
    return { ...transcript, items: next };
  }
  const index = items.findIndex((item) => item.id === op.id);
  if (index === -1) return null;
  const item = items[index];
  if (item.type !== 'assistant_segment') return null;
  const current = op.field === 'text' ? item.text : (item.thinking ?? '');
  if (utf8Length(current) !== op.at) return null;
  const next = [...items];
  next[index] =
    op.field === 'text'
      ? { ...item, text: current + op.text }
      : { ...item, thinking: current + op.text };
  return { ...transcript, items: next };
}

/** The label of a context divider. */
function clearedLabel(plugin: string | null | undefined): string {
  return plugin ? `↻ ${plugin} cleared the context` : 'Context cleared';
}

function toolCard(item: ItemOf<'tool_card'>): ToolCallDisplay {
  const failed = item.status === 'failed' || item.status === 'incomplete';
  const result =
    item.error ?? item.result ?? (item.status === 'incomplete' ? 'tool did not complete' : undefined);
  return {
    id: item.call_id,
    callId: item.call_id,
    name: item.name,
    args: item.args == null ? '' : JSON.stringify(item.args),
    status: item.status === 'running' ? 'running' : failed ? 'error' : 'complete',
    ...(result != null ? { result } : {}),
    ...(item.terminate ? { terminate: true } : {}),
    // The document declares the canonical call as an open object.
    ...(item.display ? { display: item.display as unknown as ToolCallDisplay['display'] } : {}),
    ...(item.auto_approved ? { autoApproved: item.auto_approved } : {}),
  };
}

function delegation(item: ItemOf<'delegation'>): SubagentEvent {
  const status =
    item.status === 'running' ? 'spawned' : item.status === 'complete' ? 'completed' : 'failed';
  return {
    id: item.delegation_id,
    prompt: item.prompt,
    status,
    ...(item.target_agent ? { targetAgent: item.target_agent } : {}),
    ...(item.status === 'complete' && item.outcome ? { summary: item.outcome } : {}),
    ...(item.status === 'failed' && item.outcome ? { error: item.outcome } : {}),
  };
}

/**
 * The view model of one transcript item, or `null` for an item that the web
 * client does not draw (injected context).
 *
 * An item from an older daemon carries no time. Its `timestamp` is 0, and the
 * components draw no time for it.
 */
export function itemToMessage(item: TranscriptItem): Message | null {
  const base = { id: item.id, timestamp: item.timestamp ? Date.parse(item.timestamp) : 0 };
  switch (item.type) {
    case 'user_turn': {
      const plugin = originName(item.origin, 'plugin');
      const notes = (item.precognition?.notes ?? []) as unknown as {
        title?: string;
        score?: number;
      }[];
      return {
        ...base,
        role: plugin ? 'system' : 'user',
        content: item.content,
        ...(plugin ? { plugin } : {}),
        ...(originName(item.origin, 'relay') ? { via: originName(item.origin, 'relay') } : {}),
        ...(item.precognition
          ? {
              precognition: {
                notesCount: item.precognition.notes_count,
                notes: notes.map((note) => ({ name: note.title ?? '', relevance: note.score ?? 0 })),
              },
            }
          : {}),
      };
    }
    case 'assistant_segment': {
      const thinking = item.thinking ?? '';
      const thinkingStreams = item.streaming && item.text === '';
      return {
        ...base,
        role: 'assistant',
        content: item.text,
        streaming: item.streaming,
        ...(thinking !== ''
          ? {
              thinking: {
                content: thinking,
                isStreaming: thinkingStreams,
                ...(thinkingStreams ? {} : { tokenCount: estimateThinkingTokens(thinking) }),
              },
            }
          : {}),
        ...(item.usage?.total_tokens != null
          ? {
              usage: {
                promptTokens: item.usage.prompt_tokens ?? 0,
                completionTokens: item.usage.completion_tokens ?? 0,
                totalTokens: item.usage.total_tokens,
                ...(item.usage.cache_read_tokens != null
                  ? { cacheReadTokens: item.usage.cache_read_tokens }
                  : {}),
              },
            }
          : {}),
      };
    }
    case 'tool_card':
      return { ...base, role: 'tool', content: '', toolCall: toolCard(item) };
    case 'delegation':
      return { ...base, role: 'system', type: 'delegation', content: item.prompt, delegation: delegation(item) };
    case 'injected_context':
      return null;
    case 'notice': {
      const notice = item.notice;
      switch (notice.kind) {
        case 'context_cleared':
          return { ...base, role: 'system', type: 'clear', content: clearedLabel(notice.plugin) };
        case 'stop_reason':
          return { ...base, role: 'system', content: notice.text };
        case 'turn_failed':
          return {
            ...base,
            role: 'system',
            content: notice.error ? `Error: ${notice.error}` : `The turn ended: ${notice.status}`,
          };
      }
    }
  }
}

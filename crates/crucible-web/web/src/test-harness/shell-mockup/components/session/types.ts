/**
 * The view model of a transcript. The real app's source is ChatContext's
 * `Message` (`lib/types.ts`):
 *
 * - `user`: a user message; `queued` is `Message.queued`.
 * - `precog`: `Message.precognition` (note name, relevance).
 * - `thinking`: `Message.thinking`.
 * - `record`: a system line (`role: 'system'`).
 * - `text`: an assistant message. `elapsed` comes from `completedAt`, `tokens`
 *   from `usage.totalTokens`.
 * - `tool`: `Message.toolCall` (`ToolCallDisplay`). `st` folds its `status`
 *   with the pending permission (`ask`) and the review state (`review`).
 */
import type { HunkState } from '../review/types';

export type ToolState = 'ok' | 'ask' | 'err' | 'review' | 'run';

export type TranscriptItem =
  | { t: 'user'; text: string; time: string; queued?: boolean }
  | { t: 'precog'; notes: [string, number][] }
  | { t: 'thinking'; secs: number }
  | { t: 'record'; text: string }
  | { t: 'text'; md: string; elapsed?: string; tokens?: string }
  | {
      t: 'tool';
      id: string;
      name: string;
      path?: string;
      arg?: string;
      /** The id of the hunk this call wrote. The real call carries its `diffs` instead. */
      hunk?: string;
      st: ToolState;
      out?: string;
    };

export type ToolItem = Extract<TranscriptItem, { t: 'tool' }>;

/** The hunk that a tool call wrote, as the tool line shows it. */
export interface ToolHunkView {
  state: HunkState;
  path: string;
  del: string[];
  add: string[];
}

/** What a tool line needs from its owner. One object, because every tool line in a transcript shares it. */
export interface ToolLineHandlers {
  /** Is the line (a call id, or a group id) open? */
  isOpen: (id: string) => boolean;
  onToggle: (id: string) => void;
  hunkFor: (hunkId: string) => ToolHunkView | undefined;
  onOpenPath: (path: string) => void;
  onDecide: (hunkId: string, accept: boolean) => void;
}

import type { AnchoredEdit } from '@/lib/api';

/**
 * Fold anchored edits into text this device holds.
 *
 * The outbox keeps ONE entry per note. When an anchored edit arrives for a
 * note whose whole write is queued, the edit is applied to the queued body
 * here, so the drain sends one write with one base. This is the client half
 * of the rule `crucible-core/src/note_edit.rs` applies, for a whole-line
 * `expect`: the `occurrence`-th line equal to `expect` becomes `replace`.
 *
 * The edits apply in order against the running text. A line that is absent
 * refuses the whole batch and names the edit, so the caller queues nothing
 * and can revert. Nothing here guesses: a refused fold is an answer.
 */
export type FoldOutcome = { ok: true; text: string } | { ok: false; index: number };

export function applyAnchoredEdits(text: string, edits: AnchoredEdit[]): FoldOutcome {
  const lines = text.split('\n');
  for (const [index, edit] of edits.entries()) {
    const at = lineOf(lines, edit.expect, edit.occurrence ?? 0);
    if (at === -1) return { ok: false, index };
    // A CRLF line keeps its ending: the compare dropped it, so put it back.
    lines[at] = lines[at].endsWith('\r') ? `${edit.replace}\r` : edit.replace;
  }
  return { ok: true, text: lines.join('\n') };
}

/** The index of the `occurrence`-th line equal to `expect`, or -1. */
function lineOf(lines: string[], expect: string, occurrence: number): number {
  let seen = 0;
  for (const [i, line] of lines.entries()) {
    const bare = line.endsWith('\r') ? line.slice(0, -1) : line;
    if (bare !== expect) continue;
    if (seen === occurrence) return i;
    seen += 1;
  }
  return -1;
}

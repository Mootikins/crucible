import type { AnchoredEdit } from '@/lib/types';

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
 * and can revert. Nothing here guesses: a refused fold is an answer. An
 * `expect` with no `occurrence` that matches more than one line is refused
 * too, because the daemon refuses it as ambiguous.
 *
 * A multi-line `expect` never matches here. The daemon accepts one, but the
 * only producer, `taskEditForLine`, emits single lines.
 */
export type FoldOutcome = { ok: true; text: string } | { ok: false; index: number };

export function applyAnchoredEdits(text: string, edits: AnchoredEdit[]): FoldOutcome {
  const lines = text.split('\n');
  for (const [index, edit] of edits.entries()) {
    const at = lineOf(lines, edit.expect, edit.occurrence ?? undefined);
    if (at === -1) return { ok: false, index };
    // A CRLF line keeps its ending: the compare dropped it, so put it back.
    lines[at] = lines[at].endsWith('\r') ? `${edit.replace}\r` : edit.replace;
  }
  return { ok: true, text: lines.join('\n') };
}

/**
 * The index of the `occurrence`-th line equal to `expect`, or -1.
 *
 * With no `occurrence`, the line must be the ONLY match. This mirrors the
 * daemon's `Ambiguous` refusal in `note_edit.rs`.
 */
function lineOf(lines: string[], expect: string, occurrence: number | undefined): number {
  const found: number[] = [];
  for (const [i, line] of lines.entries()) {
    const bare = line.endsWith('\r') ? line.slice(0, -1) : line;
    if (bare === expect) found.push(i);
  }
  if (occurrence === undefined) return found.length === 1 ? found[0] : -1;
  return found[occurrence] ?? -1;
}

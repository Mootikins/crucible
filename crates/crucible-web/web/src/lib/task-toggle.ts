import type { AnchoredEdit } from '@/lib/api';

/**
 * Ticking a checkbox in the reading view, as a one-line anchored edit.
 *
 * This is the case section 13 of the mobile design note was written for: a
 * user moving a task from `[ ]` to `[x]` changes ONE line, and sending the
 * whole note back to say so is what lets a browser overwrite an agent's edit
 * to a different paragraph of the same file.
 *
 * Pure, and separate from the component, because the interesting part is the
 * anchoring — which line, and which of several identical ones.
 */

const MARKER = /^(\s*(?:[-*+]|\d+[.)])\s+)\[([ xX])\](\s)/;

/** Whether a source line is a task item at all. */
export function isTaskLine(line: string): boolean {
  return MARKER.test(line);
}

/** The same line with its box flipped, or null when it is not a task line. */
export function flipTaskLine(line: string): string | null {
  const m = MARKER.exec(line);
  if (!m) return null;
  const next = m[2] === ' ' ? 'x' : ' ';
  return `${m[1]}[${next}]${m[3]}${line.slice(m[0].length)}`;
}

/**
 * The edit that ticks (or unticks) the task on `lineIndex`, zero-based.
 *
 * `occurrence` is the heart of it. `apply_anchored_edits` refuses an anchor
 * that matches more than once unless the caller names which match it meant —
 * and a list of identical `- [ ] ping` lines is ordinary, not exotic. The
 * count here is of IDENTICAL earlier lines, which is exactly the index the
 * daemon counts by.
 *
 * Answers null when the line is not a task line, so a stale render cannot
 * send an edit for a line that has since become prose.
 */
export function taskEditForLine(content: string, lineIndex: number): AnchoredEdit | null {
  const lines = content.split('\n');
  const line = lines[lineIndex];
  if (line === undefined) return null;
  const flipped = flipTaskLine(line);
  if (flipped === null) return null;

  let occurrence = 0;
  for (let i = 0; i < lineIndex; i += 1) {
    if (lines[i] === line) occurrence += 1;
  }
  const total = lines.filter((l) => l === line).length;

  return {
    expect: line,
    replace: flipped,
    // Omitted when the line is unique: an absent occurrence is the daemon's
    // "must appear exactly once", which is a stronger check than naming 0.
    ...(total > 1 ? { occurrence } : {}),
  };
}

/**
 * The same flip applied locally, for the buffer the user is looking at.
 *
 * The daemon is still the one that decides — this only keeps the screen from
 * lagging a round trip behind the tap. A refusal replaces it with the truth.
 */
export function applyTaskToggle(content: string, lineIndex: number): string | null {
  const lines = content.split('\n');
  const flipped = lines[lineIndex] === undefined ? null : flipTaskLine(lines[lineIndex]);
  if (flipped === null) return null;
  lines[lineIndex] = flipped;
  return lines.join('\n');
}

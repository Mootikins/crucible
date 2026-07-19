import { diffLines } from 'diff';

export interface DiffLine {
  type: 'add' | 'remove' | 'context';
  content: string;
  oldLineNum: number | null;
  newLineNum: number | null;
}

export interface DiffAnalysis {
  lines: DiffLine[];
  additions: number;
  deletions: number;
}

/**
 * Compute the per-line diff between `oldContent` and `newContent`, returning
 * the DiffLine[] used by DiffViewer plus the +/- counts used by the headers.
 *
 * Single pass so MultiEditDiff (which sums stats across N edits AND renders
 * an inner DiffViewer per edit) can compute once per edit and reuse via
 * DiffViewer's `precomputedAnalysis` prop. Without this, `diffLines()` would
 * run twice per edit and the two count paths could silently drift.
 */
export function analyzeDiff(oldContent: string, newContent: string): DiffAnalysis {
  const changes = diffLines(oldContent, newContent);
  const lines: DiffLine[] = [];
  let oldLine = 1;
  let newLine = 1;
  let additions = 0;
  let deletions = 0;

  for (const change of changes) {
    const parts = change.value.replace(/\n$/, '').split('\n');
    // Empty-input edge case: `''.split('\n')` is `['']`, which would otherwise
    // produce a phantom empty line for an empty file/edit side.
    if (parts.length === 1 && parts[0] === '' && change.value === '') continue;

    for (const line of parts) {
      if (change.added) {
        lines.push({ type: 'add', content: line, oldLineNum: null, newLineNum: newLine });
        newLine++;
        additions++;
      } else if (change.removed) {
        lines.push({ type: 'remove', content: line, oldLineNum: oldLine, newLineNum: null });
        oldLine++;
        deletions++;
      } else {
        lines.push({ type: 'context', content: line, oldLineNum: oldLine, newLineNum: newLine });
        oldLine++;
        newLine++;
      }
    }
  }

  return { lines, additions, deletions };
}

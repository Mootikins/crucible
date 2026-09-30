/**
 * The diff of a pending permission, from the mock store. The real request
 * carries the old and the new content in its `diffs`; the mockup makes
 * them from the note text and the lines that the edit inserts.
 */
import { analyzeDiff } from '@/lib/diff-stats';
import type { PermissionDiffProps } from '../components/session/PermissionDiff';

/** The note as the disk holds it: the new lines of each pending hunk are written. */
export function diskText(src: string): string {
  return src.replace(/:::hunk \w+\n([\s\S]*?)\n:::\n?/g, (_b, body: string) => {
    const keep = body
      .split('\n')
      .filter((l) => l.startsWith('+'))
      .map((l) => l.slice(1))
      .join('\n');
    return keep ? `${keep}\n` : '';
  });
}

/**
 * Insert `lines` and a blank line above the line `before`, and keep only the
 * changed rows and `context` rows round them, as `DiffViewer` folds a diff.
 */
export function permissionDiff(path: string, src: string, before: string, lines: readonly string[], context = 3): PermissionDiffProps {
  const old = diskText(src);
  const next = old.includes(before) ? old.replace(before, `${lines.join('\n')}\n\n${before}`) : `${old}${lines.join('\n')}\n`;
  const a = analyzeDiff(old, next);
  const near = (i: number) => a.lines.slice(Math.max(0, i - context), i + context + 1).some((l) => l.type !== 'context');
  return { path, add: a.additions, del: a.deletions, rows: a.lines.filter((_, i) => near(i)) };
}

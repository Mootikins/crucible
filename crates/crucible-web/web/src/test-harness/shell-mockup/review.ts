/** Design fixtures only: session records are read-only; proposals wait without changing notes. */
import { SERVER_BASE, SERVER_CURRENT } from '../review-fixture';
import { analyzeDiff } from '@/lib/diff-stats';
import { createStore } from 'solid-js/store';
import { hunkLines, setState, state } from './state';

export type ReviewSource = 'record' | 'proposal';
export const [review, setReview] = createStore({
  files: {} as Record<string, 'accepted' | 'rejected'>,
  comments: [] as {
    id: number;
    path: string;
    text: string;
    resolved: boolean;
    attached: boolean;
    sent: boolean;
  }[],
});
export const proposalPaths = () => [
  ...new Set(
    Object.values(state.hunks)
      .filter((h) => h.session === 's3')
      .map((h) => h.path),
  ),
];
export const proposalPending = () => proposalPaths().filter((path) => !review.files[path]);
export function reviewFiles(sid: string) {
  if (sid === 'code-preview') {
    const hunks = [
      [13, 20, 20],
      [28, 35, 38],
    ].map(([start, oldEnd, newEnd], i) => {
      const before = SERVER_BASE.split('\n').slice(start, oldEnd);
      const after = SERVER_CURRENT.split('\n').slice(start, newEnd);
      const diff = analyzeDiff(before.join('\n'), after.join('\n'));
      return {
        id: `code-${i}`,
        del: before,
        add: after,
        additions: diff.additions,
        deletions: diff.deletions,
        rows: diff.lines.map((row) => ({
          ...row,
          oldLineNum: row.oldLineNum === null ? null : row.oldLineNum + start,
          newLineNum: row.newLineNum === null ? null : row.newLineNum + start,
        })),
      };
    });
    return [
      {
        path: 'src/server.rs',
        code: true,
        hunks,
        add: hunks.reduce((n, h) => n + h.additions, 0),
        del: hunks.reduce((n, h) => n + h.deletions, 0),
      },
    ];
  }
  const paths = [
    ...new Set(
      Object.values(state.hunks)
        .filter((h) => h.session === sid && h.state !== 'absent')
        .map((h) => h.path),
    ),
  ];
  return paths.map((path) => {
    const hunks = Object.entries(state.hunks)
      .filter(([, h]) => h.session === sid && h.path === path && h.state !== 'absent')
      .map(([id]) => {
        const lines = hunkLines(id);
        const diff = analyzeDiff(lines.del.join('\n'), lines.add.join('\n'));
        return {
          id,
          ...lines,
          rows: diff.lines,
          additions: diff.additions,
          deletions: diff.deletions,
        };
      });
    return {
      path,
      code: false,
      hunks,
      add: hunks.reduce((n, h) => n + h.additions, 0),
      del: hunks.reduce((n, h) => n + h.deletions, 0),
    };
  });
}
export function decideProposal(path: string, accept: boolean) {
  if (review.files[path]) return;
  if (accept) {
    const file = reviewFiles('s3').find((f) => f.path === path);
    if (file)
      for (const h of file.hunks) {
        const before = state.notes[path] ?? '';
        const old = h.del.join('\n');
        const next = h.add.join('\n');
        const marker = new RegExp(`:::hunk ${h.id}\\n[\\s\\S]*?\\n:::`);
        setState(
          'notes',
          path,
          marker.test(before)
            ? before.replace(marker, next)
            : old && before.includes(old)
              ? before.replace(old, next)
              : `${before}\n${next}\n`,
        );
      }
  }
  setReview('files', path, accept ? 'accepted' : 'rejected');
  for (const [id, h] of Object.entries(state.hunks))
    if (h.session === 's3' && h.path === path)
      setState('hunks', id, 'state', accept ? 'accepted' : 'rejected');
}
export function addReviewComment(path: string, text: string) {
  if (!text.trim()) return;
  setReview('comments', (c) => [
    ...c,
    { id: Date.now(), path, text: text.trim(), resolved: false, attached: true, sent: false },
  ]);
}

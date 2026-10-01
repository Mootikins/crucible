import { reconcile } from 'solid-js/store';
import { afterEach, describe, expect, it } from 'vitest';
import { decideProposal, proposalPending, review, reviewFiles, setReview } from '../review';
import { setState, state } from '../state';

const path = 'Help/Tags';
const original = state.notes[path];
const kilnPath = 'Help/Concepts/Kilns';
const originalKiln = state.notes[kilnPath];
afterEach(() => {
  setState('notes', path, original);
  setState('notes', kilnPath, originalKiln);
  setState('hunks', 'h3', 'state', 'pending');
  setState('hunks', 'h4', 'state', 'pending');
  setReview('files', reconcile({}));
});

describe('proposal design fixtures', () => {
  it('numbers appended changes at their full-note position before and after acceptance', () => {
    const note = '# Tags\n\nExisting text\n';
    setState('notes', path, note);
    const expected = note.split('\n').length + 1;
    expect(reviewFiles('s3').find((file) => file.path === path)!.hunks[0].rows[0].newLineNum).toBe(
      expected,
    );
    decideProposal(path, true);
    expect(reviewFiles('s3').find((file) => file.path === path)!.hunks[0].rows[0].newLineNum).toBe(
      expected,
    );
  });

  it('numbers marker changes without counting fixture metadata as note lines', () => {
    const file = reviewFiles('s3').find((file) => file.path === 'Help/Concepts/Kilns')!;
    const prefix = state.notes[file.path].split(':::hunk h3')[0];
    expect(file.hunks[0].rows[0].newLineNum).toBe(prefix.split('\n').length);
    const rows = file.hunks[0].rows;
    decideProposal(file.path, true);
    expect(reviewFiles('s3').find((next) => next.path === file.path)!.hunks[0].rows).toEqual(rows);
  });

  it('accounts for earlier hunks changing the current-side line positions', () => {
    setState(
      'notes',
      kilnPath,
      '# Kilns\n:::hunk earlier\n-one\n+one\n+two\n:::\nBetween\n:::hunk h3\n-old\n+new\n:::',
    );
    const row = reviewFiles('s3').find((file) => file.path === kilnPath)!.hunks[0].rows;
    expect(row.find((line) => line.type === 'remove')!.oldLineNum).toBe(4);
    expect(row.find((line) => line.type === 'add')!.newLineNum).toBe(5);
  });

  it('rejects without touching the note and removes the pending file', () => {
    decideProposal(path, false);
    expect(state.notes[path]).toBe(original);
    expect(review.files[path]).toBe('rejected');
    expect(proposalPending()).not.toContain(path);
  });

  it('writes accepted text once and keeps a subsequent decision from rewriting it', () => {
    decideProposal(path, true);
    expect(state.notes[path]).toContain('Tags in frontmatter give precognition more signal');
    const accepted = state.notes[path];
    decideProposal(path, false);
    expect(state.notes[path]).toBe(accepted);
    expect(review.files[path]).toBe('accepted');
  });
});

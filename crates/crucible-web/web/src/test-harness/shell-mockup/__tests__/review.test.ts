import { reconcile } from 'solid-js/store';
import { afterEach, describe, expect, it } from 'vitest';
import { decideProposal, proposalPending, review, setReview } from '../review';
import { setState, state } from '../state';

const path = 'Help/Tags';
const original = state.notes[path];
afterEach(() => {
  setState('notes', path, original);
  setState('hunks', 'h4', 'state', 'pending');
  setReview('files', reconcile({}));
});

describe('proposal design fixtures', () => {
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

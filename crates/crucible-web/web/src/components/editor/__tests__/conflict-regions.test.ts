import { describe, it, expect, vi, afterEach } from 'vitest';
import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import type { MergeRegion } from '@/lib/api';
import {
  conflictRegions,
  openConflictRegions,
  regionChoiceText,
  resolveConflictRegion,
  seedConflictRegions,
} from '../conflict-regions';

// Real CodeMirror mounts under jsdom; every view must be destroyed by hand
// (testing-library cleanup does not know about them).
const views: EditorView[] = [];
afterEach(() => {
  while (views.length) views.pop()!.destroy();
});

/**
 * The merged text a two-region conflict leaves: ours is in the document and
 * theirs waits in the region.
 */
const DOC = ['one', 'MINE1', 'three', 'MINE2', 'five', ''].join('\n');

function region(over: Partial<MergeRegion> = {}): MergeRegion {
  return { start_line: 2, end_line: 3, base: 'BASE1\n', ours: 'MINE1\n', theirs: 'THEIRS1\n', ...over };
}

const SECOND = region({
  start_line: 4,
  end_line: 5,
  base: 'BASE2\n',
  ours: 'MINE2\n',
  theirs: 'THEIRS2\n',
});

function mount(onChange = vi.fn()): EditorView {
  const parent = document.createElement('div');
  document.body.appendChild(parent);
  const view = new EditorView({
    state: EditorState.create({ doc: DOC, extensions: [conflictRegions({ onChange })] }),
    parent,
  });
  views.push(view);
  return view;
}

describe('conflictRegions', () => {
  it('a region widget maps when text above it is edited', () => {
    const view = mount();
    seedConflictRegions(view, [region()]);
    const before = openConflictRegions(view.state)[0];
    expect(view.state.doc.sliceString(before.from, before.to)).toBe('MINE1\n');

    view.dispatch({ changes: { from: 0, insert: 'zero\n' } });

    const after = openConflictRegions(view.state)[0];
    expect(after.from).toBe(before.from + 'zero\n'.length);
    expect(after.to).toBe(before.to + 'zero\n'.length);
    // The span still names the SAME lines, which is what the widget sits above
    // and what a choice replaces.
    expect(view.state.doc.sliceString(after.from, after.to)).toBe('MINE1\n');
  });

  it('a choice replaces the region and drops it from the open list', () => {
    const onChange = vi.fn();
    const view = mount(onChange);
    seedConflictRegions(view, [region(), SECOND]);
    expect(openConflictRegions(view.state)).toHaveLength(2);

    resolveConflictRegion(view, 1, 'theirs');

    expect(view.state.doc.toString()).toBe(['one', 'MINE1', 'three', 'THEIRS2', 'five', ''].join('\n'));
    const open = openConflictRegions(view.state);
    expect(open.map((r) => r.id)).toEqual([0]);
    // The remaining region still names its own lines after the one below it
    // changed length.
    expect(view.state.doc.sliceString(open[0].from, open[0].to)).toBe('MINE1\n');
    expect(onChange).toHaveBeenLastCalledWith(open);
  });

  it('keeping both writes ours then theirs, with a line between them', () => {
    expect(regionChoiceText({ ours: 'MINE1\n', theirs: 'THEIRS1\n' }, 'both')).toBe(
      'MINE1\nTHEIRS1\n',
    );
    // A region our side DELETED has no text of its own; keeping both is then
    // simply theirs, with no blank line invented for it.
    expect(regionChoiceText({ ours: '', theirs: 'THEIRS1\n' }, 'both')).toBe('THEIRS1\n');
    // The last line of a note carries no newline, so one is put between.
    expect(regionChoiceText({ ours: 'mine', theirs: 'theirs' }, 'both')).toBe('mine\ntheirs');
  });

  it('draws one block widget per region, with the three choices', () => {
    const view = mount();
    seedConflictRegions(view, [region(), SECOND]);

    expect(view.dom.querySelectorAll('.cm-conflict-region')).toHaveLength(2);
    for (const id of [0, 1]) {
      for (const choice of ['mine', 'theirs', 'both']) {
        const button = view.dom.querySelector(`[data-testid="keep-${choice}-${id}"]`);
        expect(button, `keep-${choice}-${id}`).not.toBeNull();
        // A phone target, the same size the merge view's controls carry.
        expect((button as HTMLElement).className).toContain('min-h-11');
        expect((button as HTMLElement).className).toContain('min-w-11');
      }
    }
  });

  it('keeping mine over a region the daemon answered leaves the note as it is', () => {
    // The merge shortened the note: our side dropped the final newline of its
    // line, and their side appended after it. The daemon's region names lines
    // 2..3 of the merged text, and that span INCLUDES the newline the merged
    // text gave that line back — so its `ours` carries it too
    // (`crucible_core::note_merge`). Keeping mine is then a no-op on the
    // document; a region that stopped at 'X' would glue X to D.
    const parent = document.createElement('div');
    document.body.appendChild(parent);
    const merged = 'A\nX\nD\n';
    const view = new EditorView({
      state: EditorState.create({ doc: merged, extensions: [conflictRegions({})] }),
      parent,
    });
    views.push(view);
    seedConflictRegions(view, [
      { start_line: 2, end_line: 3, base: 'B\n', ours: 'X\n', theirs: 'Y\nC\n' },
    ]);

    const tracked = openConflictRegions(view.state)[0];
    expect(view.state.doc.sliceString(tracked.from, tracked.to)).toBe(tracked.ours);

    resolveConflictRegion(view, 0, 'mine');

    expect(view.state.doc.toString()).toBe(merged);
    expect(openConflictRegions(view.state)).toHaveLength(0);
  });

  it('marks the words that differ on each side', () => {
    const view = mount();
    seedConflictRegions(view, [
      region({ ours: 'the quick fox\n', theirs: 'the slow fox\n', base: 'the fox\n' }),
    ]);

    const mine = view.dom.querySelector('[data-testid="region-mine-0"]')!;
    const theirs = view.dom.querySelector('[data-testid="region-theirs-0"]')!;
    expect(mine.textContent).toContain('quick');
    expect(theirs.textContent).toContain('slow');
    // The shared words are not marked; only what each side alone has.
    expect([...mine.querySelectorAll('.cm-conflict-word')].map((el) => el.textContent)).toEqual([
      'quick',
    ]);
    expect([...theirs.querySelectorAll('.cm-conflict-word')].map((el) => el.textContent)).toEqual([
      'slow',
    ]);
  });
});

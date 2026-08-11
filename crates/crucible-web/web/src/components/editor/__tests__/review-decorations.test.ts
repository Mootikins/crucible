import { describe, it, expect, vi, afterEach } from 'vitest';
import { EditorState, StateEffect } from '@codemirror/state';
import { EditorView, lineNumbers } from '@codemirror/view';
import {
  applyReviewHunks,
  ensureReviewLayer,
  reviewDecorations,
  reviewHunksField,
  syncReviewLayer,
  type ReviewHunkMark,
} from '../review-decorations';

// Real CodeMirror mounts under jsdom; every view must be destroyed by hand
// (testing-library cleanup does not know about them).
const views: EditorView[] = [];
afterEach(() => {
  while (views.length) views.pop()!.destroy();
});

const DOC = ['one', 'two', 'three', 'four', 'five'].join('\n');

function mount(extensions: unknown[] = []): EditorView {
  const parent = document.createElement('div');
  document.body.appendChild(parent);
  const view = new EditorView({
    state: EditorState.create({ doc: DOC, extensions: [lineNumbers(), ...(extensions as [])] }),
    parent,
  });
  views.push(view);
  return view;
}

function mark(over: Partial<ReviewHunkMark> = {}): ReviewHunkMark {
  return {
    id: 'h1',
    start: 2,
    end: 4,
    state: 'unreviewed',
    external: false,
    label: 'edit_file',
    toolCallId: 'call-1',
    ...over,
  };
}

describe('reviewDecorations', () => {
  it('tones every line of a hunk, and only those lines', () => {
    const onReveal = vi.fn();
    const view = mount([reviewDecorations({ onReveal })]);
    applyReviewHunks(view, [mark()]);

    const toned = view.dom.querySelectorAll('.cm-review-unreviewed');
    // Half-open [2,4) is lines 2 and 3.
    expect(toned).toHaveLength(2);
    expect([...toned].map((el) => el.textContent)).toEqual(['two', 'three']);
  });

  it('a pure deletion still marks the line it collapsed onto', () => {
    const view = mount([reviewDecorations({ onReveal: vi.fn() })]);
    applyReviewHunks(view, [mark({ start: 3, end: 3 })]);
    expect(view.dom.querySelectorAll('.cm-review-unreviewed')).toHaveLength(1);
  });

  it('accepted and external hunks get their own tone', () => {
    const view = mount([reviewDecorations({ onReveal: vi.fn() })]);
    applyReviewHunks(view, [
      mark({ id: 'a', start: 1, end: 2, state: 'accepted' }),
      mark({ id: 'b', start: 4, end: 5, external: true, toolCallId: null }),
    ]);
    expect(view.dom.querySelectorAll('.cm-review-accepted')).toHaveLength(1);
    expect(view.dom.querySelectorAll('.cm-review-external')).toHaveLength(1);
  });

  it('hunks out of document order still build a valid decoration set', () => {
    // RangeSetBuilder rejects out-of-order adds; the composed diff is ordered
    // per file, not per buffer, so the sort is load-bearing.
    const view = mount([reviewDecorations({ onReveal: vi.fn() })]);
    expect(() =>
      applyReviewHunks(view, [mark({ id: 'b', start: 4, end: 5 }), mark({ id: 'a', start: 1, end: 2 })]),
    ).not.toThrow();
    expect(view.dom.querySelectorAll('.cm-review-unreviewed')).toHaveLength(2);
  });

  it('a hunk beyond the end of the buffer is dropped, not thrown on', () => {
    const view = mount([reviewDecorations({ onReveal: vi.fn() })]);
    applyReviewHunks(view, [mark({ start: 99, end: 101 })]);
    expect(view.dom.querySelectorAll('.cm-review-unreviewed')).toHaveLength(0);
  });

  it('puts ONE attribution chip on the hunk, on its first line', () => {
    const view = mount([reviewDecorations({ onReveal: vi.fn() })]);
    applyReviewHunks(view, [mark()]);
    const chips = view.dom.querySelectorAll('.cm-review-chip');
    expect(chips).toHaveLength(1);
    expect(chips[0].textContent).toBe('edit_file');
    expect(chips[0].getAttribute('data-hunk-id')).toBe('h1');
  });

  it('clicking the chip asks for the tool call that made the change', () => {
    const onReveal = vi.fn();
    const view = mount([reviewDecorations({ onReveal })]);
    applyReviewHunks(view, [mark()]);
    view.dom
      .querySelector('.cm-review-chip')!
      .dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    expect(onReveal).toHaveBeenCalledWith(expect.objectContaining({ toolCallId: 'call-1' }));
  });

  it('an external hunk has a chip but nothing to jump to', () => {
    const onReveal = vi.fn();
    const view = mount([reviewDecorations({ onReveal })]);
    applyReviewHunks(view, [mark({ external: true, toolCallId: null, label: 'external' })]);
    const chip = view.dom.querySelector('.cm-review-chip')!;
    chip.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    expect(onReveal).not.toHaveBeenCalled();
    expect(chip.className).toContain('cm-review-chip-external');
  });
});

describe('ensureReviewLayer', () => {
  it('installs once and is a no-op thereafter', () => {
    const view = mount();
    expect(view.state.field(reviewHunksField, false)).toBeUndefined();
    expect(ensureReviewLayer(view, { onReveal: vi.fn() })).toBe(true);
    expect(ensureReviewLayer(view, { onReveal: vi.fn() })).toBe(false);
  });

  it('reinstalls after the host reconfigures the editor out from under it', () => {
    // CodeMirrorEditor rebuilds its whole config on any of seven prop changes,
    // which discards appended extensions. If this ever stops being true the
    // extension can move into createExtensions() and this dance can go.
    const view = mount();
    ensureReviewLayer(view, { onReveal: vi.fn() });
    applyReviewHunks(view, [mark()]);
    expect(view.dom.querySelectorAll('.cm-review-chip')).toHaveLength(1);

    view.dispatch({ effects: StateEffect.reconfigure.of([lineNumbers()]) });
    expect(view.state.field(reviewHunksField, false)).toBeUndefined();

    expect(ensureReviewLayer(view, { onReveal: vi.fn() })).toBe(true);
    applyReviewHunks(view, [mark()]);
    expect(view.dom.querySelectorAll('.cm-review-chip')).toHaveLength(1);
  });

  it('pushing hunks at a view with no layer is harmless', () => {
    const view = mount();
    expect(() => applyReviewHunks(view, [mark()])).not.toThrow();
  });
});

describe('syncReviewLayer', () => {
  it('a file with nothing to review gets no gutter column', () => {
    // The column is 3.5rem of nothing on a file no agent has touched, which
    // is every file the user opens by hand — and in live preview, where the
    // line-number gutter is deliberately dropped so the buffer reads as prose,
    // an empty strip is the only gutter on screen.
    const view = mount();
    syncReviewLayer(view, [], { onReveal: vi.fn() });

    expect(view.dom.querySelector('.cm-review-gutter')).toBeNull();
    expect(view.state.field(reviewHunksField, false)).toBeUndefined();
  });

  it('installs the moment the first hunk lands', () => {
    const view = mount();
    syncReviewLayer(view, [], { onReveal: vi.fn() });
    syncReviewLayer(view, [mark()], { onReveal: vi.fn() });

    expect(view.dom.querySelector('.cm-review-gutter')).not.toBeNull();
    expect(view.dom.querySelectorAll('.cm-review-chip')).toHaveLength(1);
  });

  it('keeps pushing into a layer that is already up', () => {
    const view = mount();
    syncReviewLayer(view, [mark()], { onReveal: vi.fn() });
    syncReviewLayer(view, [mark({ id: 'h2', start: 1, end: 2 }), mark()], { onReveal: vi.fn() });

    expect(view.dom.querySelectorAll('.cm-review-chip')).toHaveLength(2);
  });

  it('empties an installed layer when the last hunk is resolved away', () => {
    // The column itself survives until the host reconfigures — it cannot be
    // removed in place — so what has to go is every marker in it. Its width
    // then comes from the chips that are no longer there.
    const view = mount();
    syncReviewLayer(view, [mark()], { onReveal: vi.fn() });
    syncReviewLayer(view, [], { onReveal: vi.fn() });

    expect(view.dom.querySelectorAll('.cm-review-chip')).toHaveLength(0);
    expect(view.dom.querySelectorAll('.cm-review-gutter .cm-gutterElement')).toHaveLength(0);
  });
});

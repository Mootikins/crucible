import { afterEach, describe, expect, it } from 'vitest';
import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { getChunks, MergeView } from '@codemirror/merge';
import { hidesFinalNewline, mergeViewExtensions, type MergeViewSetup } from '../merge-view';

// Twenty lines with one change in the middle. A margin of 3 leaves more
// than 4 unchanged lines on each side, so both sides collapse.
const ORIGINAL = Array.from({ length: 20 }, (_, i) => `line ${i + 1}`).join('\n');
const CHANGED = ORIGINAL.replace('line 10', 'line ten');

let view: EditorView | undefined;

function mount(setup: Partial<MergeViewSetup> = {}): EditorView {
  const parent = document.createElement('div');
  document.body.appendChild(parent);
  view = new EditorView({
    state: EditorState.create({
      doc: CHANGED,
      extensions: mergeViewExtensions({
        original: ORIGINAL,
        path: 'src/a.rs',
        collapse: { margin: 3, minSize: 4 },
        ...setup,
      }),
    }),
    parent,
  });
  return view;
}

function mountTexts(original: string, doc: string): EditorView {
  const parent = document.createElement('div');
  document.body.appendChild(parent);
  view = new EditorView({
    state: EditorState.create({ doc, extensions: mergeViewExtensions({ original, path: 'a.rs' }) }),
    parent,
  });
  return view;
}

/** The text of each word mark on the changed side. */
const marks = (v: EditorView) =>
  [...v.dom.querySelectorAll('.cm-wordChange')].map((m) => m.textContent ?? '');

afterEach(() => {
  view?.destroy();
  view = undefined;
  document.body.innerHTML = '';
});

describe('mergeViewExtensions', () => {
  it('builds a unified merge view with collapse', () => {
    const v = mount();
    const chunks = getChunks(v.state);
    expect(chunks?.side).toBe('b');
    expect(chunks?.chunks).toHaveLength(1);
    expect(v.state.readOnly).toBe(true);
    expect(v.dom.querySelectorAll('.cm-collapsedLines').length).toBe(2);
    // The removed line is a row of its own above the new line, as in a patch.
    // The changed word is marked on each side.
    expect(v.dom.querySelector('.cm-deletedChunk .cm-deletedLine')?.textContent).toBe('line 10');
    expect(v.dom.querySelector('.cm-deletedChunk .cm-deletedText')?.textContent).toBe('10');
    expect(v.dom.querySelector('.cm-changedLine')?.textContent).toBe('line ten');
    expect(marks(v)).toEqual(['ten']);
  });

  it('marks no word of a pure insertion', () => {
    const v = mountTexts('one\ntwo\nline 10', 'one\nnew line\ntwo\nline ten');
    expect(marks(v)).toEqual(['ten']);
  });

  it('marks no word of a new line inside a changed chunk', () => {
    // `bar()` has no line in the base. Only its brackets match base text, and
    // a bracket is not a word, so the line is new as a whole.
    const v = mountTexts('one\n    foo(1)\ntwo', 'one\n    foo(2)\n    bar()\ntwo');
    // The diff can pair `1` with `2)`, so check the words, not the exact marks.
    const marked = marks(v).join(' ');
    expect(marked).toContain('2');
    expect(marked).not.toContain('bar');
  });

  it('marks no indentation', () => {
    const v = mountTexts('a\nb(c).d\ne', 'a\nb(c)\n    .d\ne');
    for (const text of marks(v)) expect(text.trim()).toBe(text);
  });

  it('marks the changed words on the changed side of a split view', () => {
    const parent = document.createElement('div');
    document.body.appendChild(parent);
    const setup = { original: ORIGINAL, path: 'src/a.rs', split: true };
    const merge = new MergeView({
      a: { doc: ORIGINAL, extensions: mergeViewExtensions(setup) },
      b: { doc: CHANGED, extensions: mergeViewExtensions(setup) },
      parent,
    });
    try {
      expect(marks(merge.b)).toEqual(['ten']);
      expect(marks(merge.a)).toEqual([]);
    } finally {
      merge.destroy();
    }
  });

  it('omits controls when none are given', () => {
    const v = mount();
    expect(v.dom.querySelector('.cm-chunkButtons')).toBeNull();
    expect(v.dom.querySelectorAll('button')).toHaveLength(0);
  });

  it('draws the controls that the caller gives', () => {
    const v = mount({
      controls: (type) => {
        const b = document.createElement('button');
        b.textContent = type;
        return b;
      },
    });
    const labels = [...v.dom.querySelectorAll('button')].map((b) => b.textContent);
    expect(labels).toEqual(['accept', 'reject']);
  });

  it('with no collapse, every unchanged line stays visible', () => {
    const v = mount({ collapse: undefined });
    expect(v.dom.querySelector('.cm-collapsedLines')).toBeNull();
  });

  it('the split form gives only the editor extensions', () => {
    const v = mount({ split: true });
    expect(getChunks(v.state)).toBeNull();
    expect(v.state.readOnly).toBe(true);
  });

  it('wraps the lines only when wrap is on', () => {
    expect(mount({ wrap: true }).lineWrapping).toBe(true);
    view?.destroy();
    expect(mount({ wrap: false }).lineWrapping).toBe(false);
  });
});

describe('the final newline', () => {
  function unified(original: string, doc: string): EditorView {
    const parent = document.createElement('div');
    document.body.appendChild(parent);
    view = new EditorView({
      state: EditorState.create({
        doc,
        extensions: mergeViewExtensions({
          original,
          path: 'a.md',
          hideFinalNewline: hidesFinalNewline(original, doc),
        }),
      }),
      parent,
    });
    return view;
  }
  const rows = (v: EditorView) => [...v.dom.querySelectorAll('.cm-content > .cm-line')];

  it('draws no empty row after the last line', () => {
    const v = unified('a\nb\n', 'a\nc\n');
    expect(rows(v).map((row) => row.textContent)).toEqual(['a', 'c']);
  });

  it('keeps a removed last line', () => {
    const v = unified('a\nb\n', 'a\n');
    expect(v.dom.querySelector('.cm-deletedChunk .cm-deletedLine')?.textContent).toBe('b');
    expect(rows(v).map((row) => row.textContent)).toEqual(['a']);
  });

  it('hides it for an added file, and when both texts end in a newline', () => {
    expect(hidesFinalNewline('', 'a\n')).toBe(true);
    expect(hidesFinalNewline('a\n', '')).toBe(true);
    expect(hidesFinalNewline('a\n', 'b\n')).toBe(true);
    expect(hidesFinalNewline('a', 'b')).toBe(true);
  });

  it('keeps it when only one text ends in a newline, so the change shows', () => {
    expect(hidesFinalNewline('a\n', 'a')).toBe(false);
    expect(hidesFinalNewline('a', 'a\n')).toBe(false);
  });
});

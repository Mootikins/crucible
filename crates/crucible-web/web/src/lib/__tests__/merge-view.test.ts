import { afterEach, describe, expect, it } from 'vitest';
import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { getChunks, MergeView } from '@codemirror/merge';
import {
  diffHunks,
  hidesFinalNewline,
  mergeViewExtensions,
  setHiddenHunks,
  type MergeViewSetup,
} from '../merge-view';

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

describe('the hunks', () => {
  /** Twenty lines, with each of the given lines changed. */
  const changed = (...lines: number[]) =>
    ORIGINAL.split('\n')
      .map((text, i) => (lines.includes(i + 1) ? `${text} changed` : text))
      .join('\n');

  function unified(doc: string, hidden: (label: string) => boolean = () => false): EditorView {
    const parent = document.createElement('div');
    document.body.appendChild(parent);
    view = new EditorView({
      state: EditorState.create({
        doc,
        extensions: mergeViewExtensions({
          original: ORIGINAL,
          path: 'src/a.rs',
          collapse: { margin: 3, minSize: 4 },
          hunks: { current: doc, onToggle: (label) => toggled.push(label) },
        }),
      }),
      parent,
    });
    view.dispatch({ effects: setHiddenHunks.of(hidden) });
    return view;
  }
  let toggled: string[] = [];
  afterEach(() => {
    toggled = [];
  });

  const headers = (v: EditorView) => [
    ...v.dom.querySelectorAll<HTMLElement>('[data-testid="diff-hunk-toggle"]'),
  ];

  it('names the base and the current lines of a hunk, as a patch does', () => {
    const v = unified(CHANGED);
    // Line 10 changes. Three lines of context on each side: lines 7 to 13.
    expect(diffHunks(v.state, { margin: 3, minSize: 4 }).map((h) => h.label)).toEqual([
      '@@ -7,7 +7,7 @@',
    ]);
    expect(headers(v).map((h) => h.textContent)).toEqual(['@@ -7,7 +7,7 @@']);
    expect(headers(v)[0].getAttribute('aria-expanded')).toBe('true');
  });

  it('counts an added and a removed line on their own side', () => {
    // Lines 10b and 10c come after line 10, and line 15 goes. The context of
    // line 10b starts at line 8. The hunk runs to the end of the text.
    const doc = ORIGINAL.replace('line 10', 'line 10\nline 10b\nline 10c').replace('line 15\n', '');
    const v = unified(doc);
    expect(diffHunks(v.state, { margin: 3, minSize: 4 }).map((h) => h.label)).toEqual([
      '@@ -8,13 +8,14 @@',
    ]);
  });

  it('two chunks with shared context are one hunk', () => {
    // The context of line 10 ends at 13, and the context of line 15 starts at
    // 12, so the unchanged lines between them do not fold.
    const near = unified(changed(10, 15));
    expect(headers(near).map((h) => h.textContent)).toEqual(['@@ -7,14 +7,14 @@']);
    near.destroy();
    // Lines 3 and 15 are far apart. The lines between them fold.
    const far = unified(changed(3, 15));
    expect(headers(far).map((h) => h.textContent)).toEqual([
      '@@ -1,6 +1,6 @@',
      '@@ -12,9 +12,9 @@',
    ]);
  });

  it('a click on a header asks the owner to toggle its hunk', () => {
    const v = unified(CHANGED);
    headers(v)[0].click();
    expect(toggled).toEqual(['@@ -7,7 +7,7 @@']);
  });

  it('a hidden hunk shows only its header: no line and no removed row', () => {
    const v = unified(CHANGED, () => true);
    expect(headers(v).map((h) => h.getAttribute('aria-expanded'))).toEqual(['false']);
    const lines = [...v.dom.querySelectorAll('.cm-content > .cm-line')].map((l) => l.textContent);
    expect(lines.some((l) => /line (7|10|ten|13)$/.test(l ?? ''))).toBe(false);
    expect(v.dom.querySelector('.cm-deletedChunk')).toBeNull();
    // The owner shows it again.
    v.dispatch({ effects: setHiddenHunks.of(() => false) });
    expect(v.dom.querySelector('.cm-deletedChunk')).not.toBeNull();
    expect(v.dom.querySelector('.cm-changedLine')?.textContent).toBe('line ten');
  });

  it('a hidden hunk on the first line hides the removed row above that line', () => {
    // The removed row of a change on line 1 is a block before position 0.
    const v = unified(ORIGINAL.replace('line 1\n', 'line one\n'), () => true);
    expect(headers(v).map((h) => h.textContent)).toEqual(['@@ -1,4 +1,4 @@']);
    expect(v.dom.querySelector('.cm-deletedChunk')).toBeNull();
    expect(v.dom.querySelector('.cm-changedLine')).toBeNull();
  });

  it('a hidden hunk at the end of the text hides its removed last line', () => {
    const parent = document.createElement('div');
    document.body.appendChild(parent);
    const base = 'a\nb\n';
    const doc = 'a\n';
    view = new EditorView({
      state: EditorState.create({
        doc,
        extensions: mergeViewExtensions({
          original: base,
          path: 'a.md',
          collapse: { margin: 3, minSize: 4 },
          hideFinalNewline: hidesFinalNewline(base, doc),
          hunks: { current: doc, onToggle: () => undefined },
        }),
      }),
      parent,
    });
    expect(headers(view).map((h) => h.textContent)).toEqual(['@@ -1,2 +1,1 @@']);
    view.dispatch({ effects: setHiddenHunks.of(() => true) });
    expect(view.dom.querySelector('.cm-deletedChunk')).toBeNull();
  });

  describe('an empty side has no line', () => {
    // CodeMirror gives an empty text one empty line. A file with no text has
    // no line, so the patch range of that side is `0,0` and no row shows.
    const TEXT = 'x\ny\nz\n';
    const setup = (original: string, doc: string): MergeViewSetup => ({
      original,
      path: 'a.txt',
      collapse: { margin: 3, minSize: 4 },
      hideFinalNewline: hidesFinalNewline(original, doc),
      hunks: { current: doc, onToggle: () => undefined },
    });

    function unifiedOf(original: string, doc: string): EditorView {
      const parent = document.createElement('div');
      document.body.appendChild(parent);
      view = new EditorView({
        state: EditorState.create({ doc, extensions: mergeViewExtensions(setup(original, doc)) }),
        parent,
      });
      return view;
    }

    function splitOf(original: string, doc: string): MergeView {
      const parent = document.createElement('div');
      document.body.appendChild(parent);
      const both = { ...setup(original, doc), split: true };
      return new MergeView({
        a: { doc: original, extensions: mergeViewExtensions(both) },
        b: { doc, extensions: mergeViewExtensions(both) },
        parent,
        collapseUnchanged: { margin: 3, minSize: 4 },
      });
    }

    it('an added file, unified', () => {
      const v = unifiedOf('', TEXT);
      expect(headers(v).map((h) => h.textContent)).toEqual(['@@ -0,0 +1,3 @@']);
    });

    it('an added file, split: the base editor shows no row', () => {
      const merge = splitOf('', TEXT);
      try {
        expect(headers(merge.a).map((h) => h.textContent)).toEqual(['@@ -0,0 +1,3 @@']);
        expect(headers(merge.b).map((h) => h.textContent)).toEqual(['@@ -0,0 +1,3 @@']);
        expect(merge.a.dom.querySelectorAll('.cm-line')).toHaveLength(0);
      } finally {
        merge.destroy();
      }
    });

    it('a deleted file, unified: the current side shows no row', () => {
      const v = unifiedOf(TEXT, '');
      expect(headers(v).map((h) => h.textContent)).toEqual(['@@ -1,3 +0,0 @@']);
      expect(v.dom.querySelectorAll('.cm-line')).toHaveLength(0);
      // The removed rows stay.
      expect(v.dom.querySelectorAll('.cm-deletedChunk .cm-deletedLine')).toHaveLength(3);
    });

    it('a deleted file, split: the current editor shows no row', () => {
      const merge = splitOf(TEXT, '');
      try {
        expect(headers(merge.a).map((h) => h.textContent)).toEqual(['@@ -1,3 +0,0 @@']);
        expect(headers(merge.b).map((h) => h.textContent)).toEqual(['@@ -1,3 +0,0 @@']);
        expect(merge.b.dom.querySelectorAll('.cm-line')).toHaveLength(0);
      } finally {
        merge.destroy();
      }
    });
  });

  it('each editor of a split view names the same hunk', () => {
    // Line 10b comes after line 10: three lines of context on each side.
    // The pane gives both editors one setup, as `FileEditor` does. The editor
    // of the current side has no unified view to read the base text from.
    const doc = ORIGINAL.replace('line 10', 'line 10\nline 10b');
    const parent = document.createElement('div');
    document.body.appendChild(parent);
    const setup: MergeViewSetup = {
      original: ORIGINAL,
      path: 'src/a.rs',
      split: true,
      collapse: { margin: 3, minSize: 4 },
      hunks: { current: doc, onToggle: () => undefined },
    };
    const merge = new MergeView({
      a: { doc: ORIGINAL, extensions: mergeViewExtensions(setup) },
      b: { doc, extensions: mergeViewExtensions(setup) },
      parent,
      collapseUnchanged: { margin: 3, minSize: 4 },
    });
    try {
      expect(headers(merge.a).map((h) => h.textContent)).toEqual(['@@ -8,6 +8,7 @@']);
      expect(headers(merge.b).map((h) => h.textContent)).toEqual(['@@ -8,6 +8,7 @@']);
    } finally {
      merge.destroy();
    }
  });
});

import { afterEach, describe, expect, it } from 'vitest';
import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { getChunks } from '@codemirror/merge';
import { mergeViewExtensions, type MergeViewSetup } from '../merge-view';

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
    // The inline diff shows the base word as deleted text before the new one.
    expect(v.dom.querySelector('.cm-deletedText')?.textContent).toBe('10');
    expect(v.dom.querySelector('.cm-changedText')?.textContent).toBe('ten');
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

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { EditorView } from '@codemirror/view';
import { EditorState } from '@codemirror/state';
import { markdown, markdownLanguage } from '@codemirror/lang-markdown';
import { livePreview } from '../live-preview';

const DOC = [
  '# Title',
  '',
  'Some **bold** and `code` here.',
  '',
  'See [[Target|alias]] link.',
].join('\n');

function makeView(doc = DOC): EditorView {
  const parent = document.createElement('div');
  document.body.appendChild(parent);
  const view = new EditorView({
    state: EditorState.create({
      doc,
      extensions: [markdown({ base: markdownLanguage }), livePreview()],
    }),
    parent,
  });
  return view;
}

function cursorAt(view: EditorView, pos: number): void {
  view.dispatch({ selection: { anchor: pos } });
}

const text = (view: EditorView) => view.contentDOM.textContent ?? '';

let views: EditorView[] = [];
const track = (v: EditorView) => (views.push(v), v);

beforeEach(() => {
  views = [];
});

afterEach(() => {
  views.forEach((v) => v.destroy());
  document.body.innerHTML = '';
});

describe('live preview: styled everywhere except the construct at the cursor', () => {
  it('hides bold markers and styles the content when the cursor is elsewhere', () => {
    const view = track(makeView());
    cursorAt(view, DOC.length); // end of doc, away from everything on line 3
    expect(text(view)).not.toContain('**');
    expect(text(view)).toContain('bold');
    expect(view.dom.querySelector('.cm-lp-strong')).not.toBeNull();
  });

  it('reveals the raw markers for ONLY the construct under the cursor', () => {
    const view = track(makeView());
    const boldInner = DOC.indexOf('bold') + 2;
    cursorAt(view, boldInner);
    // The bold span shows its source again…
    expect(text(view)).toContain('**bold**');
    // …while the other constructs on the same line stay styled.
    expect(text(view)).not.toContain('`code`');
    expect(view.dom.querySelector('.cm-lp-code')).not.toBeNull();
  });

  it('inline code renders as a mono chip; backticks return at the cursor', () => {
    const view = track(makeView());
    cursorAt(view, 0);
    expect(text(view)).not.toContain('`');
    cursorAt(view, DOC.indexOf('code') + 1);
    expect(text(view)).toContain('`code`');
  });

  it('headings hide their # prefix until the cursor is on the line', () => {
    const view = track(makeView());
    cursorAt(view, DOC.length);
    expect(text(view)).not.toContain('# Title');
    expect(text(view)).toContain('Title');
    expect(view.dom.querySelector('.cm-lp-h1')).not.toBeNull();

    cursorAt(view, 2); // inside the heading line
    expect(text(view)).toContain('# Title');
  });

  it('aliased wikilinks show only the alias; raw form returns at the cursor', () => {
    const view = track(makeView());
    cursorAt(view, 0);
    expect(text(view)).toContain('alias');
    expect(text(view)).not.toContain('[[Target|');
    expect(text(view)).not.toContain(']]');

    cursorAt(view, DOC.indexOf('Target') + 1);
    expect(text(view)).toContain('[[Target|alias]]');
  });

  it('typing keeps the construct under the cursor raw (edit-in-place)', () => {
    const view = track(makeView());
    const at = DOC.indexOf('bold') + 4;
    cursorAt(view, at);
    view.dispatch({ changes: { from: at, insert: 'er' }, selection: { anchor: at + 2 } });
    expect(text(view)).toContain('**bolder**');
  });

  it('without the extension nothing is hidden (source mode)', () => {
    const parent = document.createElement('div');
    document.body.appendChild(parent);
    const view = track(
      new EditorView({
        state: EditorState.create({
          doc: DOC,
          extensions: [markdown({ base: markdownLanguage })],
        }),
        parent,
      }),
    );
    expect(text(view)).toContain('**bold**');
    expect(view.dom.querySelector('.cm-lp-strong')).toBeNull();
  });
});

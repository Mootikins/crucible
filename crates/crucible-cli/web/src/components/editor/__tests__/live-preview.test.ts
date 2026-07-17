import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { EditorView } from '@codemirror/view';
import { EditorState } from '@codemirror/state';
import { markdown, markdownLanguage } from '@codemirror/lang-markdown';
import { yamlFrontmatter } from '@codemirror/lang-yaml';
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

  it('markdown tables render as real HTML tables until the cursor enters', () => {
    const TABLE_DOC = [
      'Before.',
      '',
      '| Col A | Col B |',
      '| ----- | ----- |',
      '| one   | two   |',
      '',
      'After.',
    ].join('\n');
    const view = track(makeView(TABLE_DOC));
    cursorAt(view, 0);

    // Rendered: a real <table> with header + cells, raw pipes hidden.
    const widget = view.dom.querySelector('[data-testid="lp-table"]');
    expect(widget).not.toBeNull();
    expect(widget!.querySelector('table')).not.toBeNull();
    expect(widget!.querySelector('th')?.textContent).toBe('Col A');
    expect(widget!.querySelector('td')?.textContent).toBe('one');
    expect(text(view)).not.toContain('| ----- |');

    // Cursor inside the table reveals the raw source for editing.
    cursorAt(view, TABLE_DOC.indexOf('one'));
    expect(view.dom.querySelector('[data-testid="lp-table"]')).toBeNull();
    expect(text(view)).toContain('| one   | two   |');
  });

  it('wraps long prose lines (live preview only)', () => {
    const view = track(makeView());
    expect(view.contentDOM.classList.contains('cm-lineWrapping')).toBe(true);
  });

  it('frontmatter stays raw mono yaml — no markdown styling inside', () => {
    // Trailing blank line: the cursor parks there without touching (and
    // thus revealing) the heading.
    const FM_DOC = ['---', 'tags: [kiln]', 'title: A **note**', '---', '', '# Title', ''].join(
      '\n',
    );
    const parent = document.createElement('div');
    document.body.appendChild(parent);
    const view = track(
      new EditorView({
        state: EditorState.create({
          doc: FM_DOC,
          // Production language stack for markdown files.
          extensions: [
            yamlFrontmatter({ content: markdown({ base: markdownLanguage }) }),
            livePreview(),
          ],
        }),
        parent,
      }),
    );
    cursorAt(view, FM_DOC.length);
    // Every frontmatter line carries the mono line class, delimiters included.
    const fmLines = view.dom.querySelectorAll('.cm-lp-frontmatter');
    expect(fmLines.length).toBe(4);
    // Delimiters stay visible; the bold inside frontmatter is NOT styled prose.
    expect(text(view)).toContain('---');
    expect(text(view)).toContain('**note**');
    expect(view.dom.querySelector('.cm-lp-frontmatter .cm-lp-strong')).toBeNull();
    // The document body below still gets live-preview styling.
    expect(view.dom.querySelector('.cm-lp-h1')).not.toBeNull();
    expect(text(view)).not.toContain('# Title');
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

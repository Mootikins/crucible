import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
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

function makeView(doc = DOC, opts?: { baseDir?: string }): EditorView {
  const parent = document.createElement('div');
  document.body.appendChild(parent);
  const view = new EditorView({
    state: EditorState.create({
      doc,
      extensions: [markdown({ base: markdownLanguage }), livePreview(opts)],
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

  it('callouts render as fancy admonition blocks until the cursor enters', () => {
    const CALLOUT_DOC = [
      'Before.',
      '',
      '> [!warning] Watch out',
      '> Body text here.',
      '',
      'After.',
    ].join('\n');
    const view = track(makeView(CALLOUT_DOC));
    cursorAt(view, 0);

    // Rendered: the reading-mode .callout markup — icon, colored title row,
    // body — with the raw `> [!warning]` syntax hidden.
    const widget = view.dom.querySelector('[data-testid="lp-callout"]');
    expect(widget).not.toBeNull();
    const callout = widget!.querySelector('.callout');
    expect(callout).not.toBeNull();
    expect(callout!.getAttribute('data-callout')).toBe('warning');
    expect(widget!.querySelector('.callout-icon')).not.toBeNull();
    expect(widget!.querySelector('.callout-title-text')?.textContent).toBe('Watch out');
    expect(text(view)).not.toContain('[!warning]');
    expect(text(view)).toContain('Body text here.');

    // Cursor inside the callout reveals the raw source for editing.
    cursorAt(view, CALLOUT_DOC.indexOf('Body'));
    expect(view.dom.querySelector('[data-testid="lp-callout"]')).toBeNull();
    expect(text(view)).toContain('> [!warning] Watch out');
  });

  it('foldable callouts render as <details> honoring the collapse marker', () => {
    const view = track(
      makeView('> [!note]- Folded\n> Hidden body.\n\nProse after.\n'),
    );
    cursorAt(view, view.state.doc.length);
    const widget = view.dom.querySelector('[data-testid="lp-callout"]');
    expect(widget).not.toBeNull();
    const details = widget!.querySelector('details.callout');
    expect(details).not.toBeNull();
    expect(details!.hasAttribute('open')).toBe(false);
    expect(details!.querySelector('summary.callout-title')?.textContent).toContain(
      'Folded',
    );
  });

  it('a downward hop over a rendered callout lands inside it (vim j/k)', () => {
    const CALLOUT_DOC = [
      'Before.',
      '',
      '> [!tip] Title',
      '> Body.',
      '',
      'After.',
    ].join('\n');
    const view = track(makeView(CALLOUT_DOC));
    const blankAbove = CALLOUT_DOC.indexOf('\n\n>') + 1;
    const calloutFrom = CALLOUT_DOC.indexOf('> [!tip]');
    cursorAt(view, blankAbove);
    // Simulate the widget hop: moveVertically lands on the line below.
    cursorAt(view, CALLOUT_DOC.indexOf('After.') - 1);
    expect(view.state.selection.main.head).toBe(calloutFrom);
    // …and the callout reveals its raw source for editing.
    expect(view.dom.querySelector('[data-testid="lp-callout"]')).toBeNull();
  });

  it('renders an absolute-URL image inline; cursor reveals the source', () => {
    const doc = 'Before.\n\n![badge](https://ex.com/b.svg)\n\nAfter.';
    const view = track(makeView(doc));
    cursorAt(view, 0);
    const img = view.dom.querySelector<HTMLImageElement>('img.cm-lp-img');
    expect(img).not.toBeNull();
    expect(img!.getAttribute('src')).toBe('https://ex.com/b.svg');
    expect(text(view)).not.toContain('![badge]');

    cursorAt(view, doc.indexOf('badge'));
    expect(view.dom.querySelector('img.cm-lp-img')).toBeNull();
    expect(text(view)).toContain('![badge](https://ex.com/b.svg)');
  });

  it('resolves a relative image src against baseDir via the raw endpoint', () => {
    const view = track(
      makeView('![demo](assets/demo.gif)\n', { baseDir: '/home/u/proj' }),
    );
    cursorAt(view, view.state.doc.length);
    const img = view.dom.querySelector<HTMLImageElement>('img.cm-lp-img');
    expect(img).not.toBeNull();
    expect(img!.getAttribute('src')).toBe(
      `/api/file/raw?path=${encodeURIComponent('/home/u/proj/assets/demo.gif')}`,
    );
  });

  it('renders a badge (image wrapped in a link) as an inline image', () => {
    const view = track(
      makeView('[![CI](https://ex.com/ci.svg)](https://ex.com/ci)\n'),
    );
    cursorAt(view, view.state.doc.length);
    const img = view.dom.querySelector<HTMLImageElement>('img.cm-lp-img');
    expect(img).not.toBeNull();
    expect(img!.getAttribute('src')).toBe('https://ex.com/ci.svg');
  });

  it('renders an embedded HTML block as a widget; cursor reveals source', () => {
    const doc = [
      'Before.',
      '',
      '<p align="center">',
      '  <img src="https://ex.com/demo.gif" alt="demo" width="720" />',
      '</p>',
      '',
      'After.',
    ].join('\n');
    const view = track(makeView(doc));
    cursorAt(view, 0);
    const widget = view.dom.querySelector('[data-testid="lp-html"]');
    expect(widget).not.toBeNull();
    expect(widget!.querySelector('p[align="center"]')).not.toBeNull();
    expect(widget!.querySelector('img')).not.toBeNull();

    cursorAt(view, doc.indexOf('align'));
    expect(view.dom.querySelector('[data-testid="lp-html"]')).toBeNull();
    expect(text(view)).toContain('<p align="center">');
  });

  it('renders task-list markers as checkboxes; clicking toggles the source', () => {
    const doc = '- [ ] todo\n- [x] done';
    const view = track(makeView(doc));
    cursorAt(view, view.state.doc.length + 0); // park cursor at end, off line 1
    cursorAt(view, doc.length); // end of "done" line
    // Line 1 (unchecked) shows an unchecked checkbox widget.
    const boxes = view.dom.querySelectorAll('.cm-lp-checkbox');
    expect(boxes.length).toBeGreaterThanOrEqual(1);
    const unchecked = view.dom.querySelector('.cm-lp-checkbox:not(.is-checked)');
    expect(unchecked).not.toBeNull();

    // Toggling the unchecked one flips its source marker to [x].
    (unchecked as HTMLElement).dispatchEvent(
      new MouseEvent('mousedown', { bubbles: true }),
    );
    expect(view.state.doc.line(1).text).toBe('- [x] todo');
  });

  it('shows a checked checkbox for `- [x]`', () => {
    const view = track(makeView('- [x] done\n'));
    cursorAt(view, view.state.doc.length);
    expect(view.dom.querySelector('.cm-lp-checkbox.is-checked')).not.toBeNull();
  });

  describe('vertical cursor entry into rendered tables (vim j/k)', () => {
    // Blank lines around the table, as in real prose — without one after,
    // lezer's GFM parser absorbs the following paragraph into the Table.
    const TABLE_DOC = [
      'Before.',
      '',
      '| Col A | Col B |',
      '| ----- | ----- |',
      '| one   | two   |',
      '',
      'After.',
    ].join('\n');
    const blankAbove = TABLE_DOC.indexOf('\n| Col A') - 0; // empty line pos 8
    const tableFrom = TABLE_DOC.indexOf('| Col A');
    const lastLineFrom = TABLE_DOC.indexOf('| one');
    const blankBelow = TABLE_DOC.indexOf('\nAfter.'); // empty line pos

    it('a downward hop over the widget lands in the first table line', () => {
      const view = track(makeView(TABLE_DOC));
      cursorAt(view, blankAbove); // blank line directly above the table
      // moveVertically skips block widgets: vim j dispatches a selection on
      // the line BELOW the table. The filter must redirect into the table.
      view.dispatch({ selection: { anchor: blankBelow } });
      expect(view.state.selection.main.head).toBe(tableFrom);
      // …and the table reveals its raw source for editing.
      expect(view.dom.querySelector('[data-testid="lp-table"]')).toBeNull();
    });

    it('an upward hop over the widget lands in the last table line', () => {
      const view = track(makeView(TABLE_DOC));
      cursorAt(view, blankBelow); // blank line directly below the table
      view.dispatch({ selection: { anchor: blankAbove } }); // vim k skipped over
      expect(view.state.selection.main.head).toBe(lastLineFrom);
      expect(view.dom.querySelector('[data-testid="lp-table"]')).toBeNull();
    });

    it('column is preserved and clamped to the table line length', () => {
      const view = track(makeView(TABLE_DOC));
      cursorAt(view, 3); // "Bef|ore." — col 3, two lines above the table
      // A hop from a NON-adjacent line is not the skip pattern: untouched.
      view.dispatch({ selection: { anchor: blankBelow } });
      expect(view.state.selection.main.head).toBe(blankBelow);
    });

    it('long jumps far past the table are left alone', () => {
      const doc = TABLE_DOC + '\nMore.\nLines.';
      const view = track(makeView(doc));
      cursorAt(view, blankAbove);
      const far = doc.indexOf('Lines.');
      view.dispatch({ selection: { anchor: far } });
      expect(view.state.selection.main.head).toBe(far);
    });

    // Mermaid fences and `$$` blocks are widgeted too, but neither is a
    // `renderedBlockKind` node — they were skipped by the same motion that
    // tables handled, so a diagram could only be opened by clicking it.
    it('a downward hop over a mermaid diagram lands in the fence', () => {
      const doc = ['Before.', '', '```mermaid', 'flowchart LR', '  A --> B', '```', '', 'After.'].join('\n');
      const view = track(makeView(doc));
      const blankAbove = doc.indexOf('\n```mermaid');
      const fenceFrom = doc.indexOf('```mermaid');
      const blankBelow = doc.indexOf('\nAfter.');
      cursorAt(view, blankAbove);
      view.dispatch({ selection: { anchor: blankBelow } });
      expect(view.state.selection.main.head).toBe(fenceFrom);
      expect(view.dom.querySelector('[data-testid="lp-mermaid"]')).toBeNull();
    });

    it('an upward hop over a display-math block lands in its last line', () => {
      const doc = ['Before.', '', '$$', 'E = mc^2', '$$', '', 'After.'].join('\n');
      const view = track(makeView(doc));
      const blankAbove = doc.indexOf('\n$$');
      const lastLineFrom = doc.lastIndexOf('$$');
      const blankBelow = doc.indexOf('\nAfter.');
      cursorAt(view, blankBelow);
      view.dispatch({ selection: { anchor: blankAbove } });
      expect(view.state.selection.main.head).toBe(lastLineFrom);
    });
  });

  describe('display math vs code fences', () => {
    it('does not render a $$ block that is quoted inside a code fence', () => {
      const doc = ['Example:', '', '````markdown', '$$', 'E = mc^2', '$$', '````', '', 'After.'].join('\n');
      const view = track(makeView(doc));
      cursorAt(view, 0);
      // The fence is source: no math widget, and its lines stay in the doc.
      expect(view.dom.querySelector('.cm-lp-math-display')).toBeNull();
      expect(view.dom.textContent).toContain('E = mc^2');
    });

    it('still renders a real $$ block in the same document', () => {
      const doc = ['$$', 'E = mc^2', '$$', '', 'Text.'].join('\n');
      const view = track(makeView(doc));
      cursorAt(view, doc.indexOf('Text.'));
      expect(view.dom.querySelector('.cm-lp-math-display')).not.toBeNull();
    });
  });

  describe('table auto-format on cursor entry/exit', () => {
    const flush = () => new Promise((r) => setTimeout(r, 0));
    const RAGGED = ['Before.', '', '| a | long header |', '|---|---|', '| bbbb | c |', '', 'After.'].join('\n');

    it('aligns an unaligned table when the cursor enters it', async () => {
      const view = track(makeView(RAGGED));
      cursorAt(view, 0);
      await flush();
      cursorAt(view, RAGGED.indexOf('bbbb'));
      await flush();
      const text = view.state.doc.toString();
      expect(text).toContain('| a    | long header |');
      expect(text).toContain('| bbbb | c           |');
    });

    it('re-aligns after edits when the cursor leaves', async () => {
      const view = track(makeView(RAGGED));
      cursorAt(view, RAGGED.indexOf('bbbb'));
      await flush();
      // Widen a cell: type into "c" making it long, un-aligning the row.
      const cPos = view.state.doc.toString().indexOf('| c ') + 3;
      view.dispatch({ changes: { from: cPos, insert: 'wide-cell' }, selection: { anchor: cPos } });
      await flush();
      // Leave the table.
      cursorAt(view, 0);
      await flush();
      const text = view.state.doc.toString();
      const rows = text.split('\n').filter((l) => l.startsWith('|'));
      expect(new Set(rows.map((l) => l.length)).size).toBe(1);
      expect(text).toContain('cwide-cell');
    });

    it('leaves prose untouched when no table is involved', async () => {
      const view = track(makeView());
      cursorAt(view, 3);
      await flush();
      expect(view.state.doc.toString()).toBe(DOC);
    });
  });

  describe('wikilink treatment skips code contexts', () => {
    const TOML_DOC = [
      'See [[Real Note]] here.',
      '',
      '```toml',
      '[[mcp.upstreams]]',
      'name = "github"',
      '```',
      '',
      'Inline `[[not.a.link]]` too.',
    ].join('\n');

    it('does not hide TOML [[table]] brackets inside fenced code', () => {
      const view = track(makeView(TOML_DOC));
      cursorAt(view, 0);
      // The fence content keeps its raw double brackets…
      expect(text(view)).toContain('[[mcp.upstreams]]');
      // …while the prose wikilink still hides them.
      expect(text(view)).toContain('Real Note');
      expect(text(view)).not.toContain('[[Real Note]]');
    });

    it('does not hide brackets inside inline code', () => {
      const view = track(makeView(TOML_DOC));
      cursorAt(view, 0);
      expect(text(view)).toContain('[[not.a.link]]');
    });
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
    // Cursor OUTSIDE the frontmatter → the Properties card widget replaces
    // the raw yaml (delimiters and all).
    cursorAt(view, FM_DOC.length);
    const card = view.dom.querySelector('.cm-lp-fm [data-testid="fm-card"]');
    expect(card).not.toBeNull();
    expect(card!.textContent).toContain('title');
    expect(card!.querySelectorAll('.fm-pill').length).toBe(1); // tags: [kiln]
    expect(text(view)).not.toContain('---');
    // The document body below still gets live-preview styling.
    expect(view.dom.querySelector('.cm-lp-h1')).not.toBeNull();
    expect(text(view)).not.toContain('# Title');

    // Cursor INSIDE → raw mono source for editing; the bold inside
    // frontmatter is NOT styled prose.
    cursorAt(view, 5);
    expect(view.dom.querySelector('[data-testid="fm-card"]')).toBeNull();
    const fmLines = view.dom.querySelectorAll('.cm-lp-frontmatter');
    expect(fmLines.length).toBe(4);
    expect(text(view)).toContain('---');
    expect(text(view)).toContain('**note**');
    expect(view.dom.querySelector('.cm-lp-frontmatter .cm-lp-strong')).toBeNull();
  });

  /**
   * The widget turns a mousedown into a selection change so clicking rendered
   * prose puts the cursor in the source behind it. A `<summary>` is the
   * exception: it owns its own click. Only the callout title was exempt, so
   * clicking the Properties disclosure dropped the editor into raw YAML instead
   * of expanding it — the card was expandable in the reading view and nowhere
   * else, and nothing covered it.
   */
  it('lets the Properties disclosure toggle instead of jumping into the source', () => {
    const FM_DOC = ['---', 'tags: [kiln]', 'title: A **note**', '---', '', '# Title', ''].join('\n');
    const parent = document.createElement('div');
    document.body.appendChild(parent);
    const view = track(
      new EditorView({
        state: EditorState.create({
          doc: FM_DOC,
          extensions: [
            yamlFrontmatter({ content: markdown({ base: markdownLanguage }) }),
            livePreview(),
          ],
        }),
        parent,
      }),
    );
    cursorAt(view, FM_DOC.length);

    const summary = view.dom.querySelector(
      '.cm-lp-fm [data-testid="fm-summary"]',
    ) as HTMLElement | null;
    expect(summary).not.toBeNull();

    const event = new MouseEvent('mousedown', { bubbles: true, cancelable: true });
    summary!.dispatchEvent(event);

    // Not swallowed: the browser's own <details> toggle must still run.
    expect(event.defaultPrevented).toBe(false);
    // And the widget survives — a selection change here would replace it with
    // the raw yaml.
    expect(view.dom.querySelector('[data-testid="fm-card"]')).not.toBeNull();
    expect(text(view)).not.toContain('---');
  });

  /**
   * The frontmatter node ends AT the body's first position, so an unclamped
   * block replace reached into the next line and consumed it. When that line is
   * blank — the overwhelmingly common shape, `---` then a blank line then the
   * heading — CodeMirror emitted no `.cm-line` for it at all, and the cursor
   * that `CodeMirrorEditor` deliberately opens at `bodyStart` had no text line
   * to be measured against. It fell back to the widget's box, which is 4px tall
   * because the card's only content is floated, and vim's block cursor rendered
   * as a 3x4px speck beside the disclosure caret.
   *
   * The 4px geometry itself is not assertable here (jsdom has no float layout
   * and no line boxes) — an e2e spec covers that. What IS assertable, and is
   * the actual cause, is whether the blank line still exists as a line.
   */
  it('leaves the blank first body line as its own line instead of swallowing it', () => {
    const FM_DOC = ['---', 'tags: [kiln]', 'title: A **note**', '---', '', '# Title', ''].join('\n');
    const parent = document.createElement('div');
    document.body.appendChild(parent);
    const view = track(
      new EditorView({
        state: EditorState.create({
          doc: FM_DOC,
          extensions: [
            yamlFrontmatter({ content: markdown({ base: markdownLanguage }) }),
            livePreview(),
          ],
        }),
        parent,
      }),
    );
    cursorAt(view, FM_DOC.length);

    // The card still renders — the clamp must not push the selection into the
    // block and reveal raw yaml.
    expect(view.dom.querySelector('[data-testid="fm-card"]')).not.toBeNull();
    const lines = Array.from(view.dom.querySelectorAll('.cm-line')).map((l) => l.textContent);
    // Position, not mere presence: this doc ends with a newline, so a trailing
    // empty line exists either way and `toContain('')` passed pre-fix too.
    // What the bug removed is specifically the FIRST line after the widget.
    expect(lines[0]).toBe('');
    expect(lines[1]).toBe('Title');
    expect(view.dom.querySelector('.cm-lp-h1')).not.toBeNull();
  });

  /**
   * A `<details>` inside a widget changes the widget's height AFTER the editor
   * measured it, and the editor keeps positioning every following line from the
   * stale figure — expanding Properties pushed the note it belongs to up out of
   * the viewport.
   */
  it('re-measures the editor when the Properties card is expanded', () => {
    const FM_DOC = ['---', 'tags: [kiln]', 'title: A **note**', '---', '', '# Title', ''].join('\n');
    const parent = document.createElement('div');
    document.body.appendChild(parent);
    const view = track(
      new EditorView({
        state: EditorState.create({
          doc: FM_DOC,
          extensions: [
            yamlFrontmatter({ content: markdown({ base: markdownLanguage }) }),
            livePreview(),
          ],
        }),
        parent,
      }),
    );
    cursorAt(view, FM_DOC.length);

    const details = view.dom.querySelector(
      '.cm-lp-fm [data-testid="fm-card"]',
    ) as HTMLDetailsElement | null;
    expect(details).not.toBeNull();

    const measured = vi.spyOn(view, 'requestMeasure');
    details!.open = true;
    // `toggle` does not bubble; the widget listens in the capture phase.
    details!.dispatchEvent(new Event('toggle'));

    expect(measured).toHaveBeenCalled();
  });

  it('TOML (+++) frontmatter gets the Properties card too', () => {
    const DOC = ['+++', 'title = "T"', 'tags = ["a", "b"]', '+++', '', '# Body', ''].join('\n');
    const parent = document.createElement('div');
    document.body.appendChild(parent);
    const view = track(
      new EditorView({
        state: EditorState.create({
          doc: DOC,
          extensions: [
            yamlFrontmatter({ content: markdown({ base: markdownLanguage }) }),
            livePreview(),
          ],
        }),
        parent,
      }),
    );
    cursorAt(view, DOC.length);
    const card = view.dom.querySelector('.cm-lp-fm [data-testid="fm-card"]');
    expect(card).not.toBeNull();
    expect(card!.querySelectorAll('.fm-pill').length).toBe(2);
    expect(text(view)).not.toContain('+++');
    // Cursor inside reveals the raw TOML with the mono treatment.
    cursorAt(view, 5);
    expect(view.dom.querySelector('[data-testid="fm-card"]')).toBeNull();
    expect(text(view)).toContain('title = "T"');
    expect(view.dom.querySelectorAll('.cm-lp-frontmatter').length).toBe(4);
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

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { CompletionContext, type CompletionResult } from '@codemirror/autocomplete';
import { markdown, markdownLanguage } from '@codemirror/lang-markdown';
import { wikilinkCompletionSource } from '../wikilink-completion';

const listKilnNotesMock = vi.fn();
vi.mock('@/lib/api', () => ({ listKilnNotes: (p: string) => listKilnNotesMock(p) }));

const NOTES = [
  { name: 'Wikilinks', path: 'Help/Wikilinks.md' },
  { name: 'Tags', path: 'Help/Tags.md' },
  { name: 'Workflow Syntax', path: 'Help/Workflows/Workflow Syntax.md' },
];

/** Build a doc with the cursor at `|` and run the completion source over it. */
async function complete(
  docWithCursor: string,
  kiln: string | undefined = '/kiln',
): Promise<CompletionResult | null> {
  const pos = docWithCursor.indexOf('|');
  const doc = docWithCursor.replace('|', '');
  const state = EditorState.create({
    doc,
    selection: { anchor: pos },
    extensions: [markdown({ base: markdownLanguage })],
  });
  const source = wikilinkCompletionSource(() => kiln);
  return await source(new CompletionContext(state, pos, false));
}

describe('wikilinkCompletionSource', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    listKilnNotesMock.mockResolvedValue(NOTES);
  });

  it('offers kiln notes as soon as "[[" is typed', async () => {
    const result = await complete('see [[|');
    expect(result).not.toBeNull();
    expect(result!.options.map((o) => o.label)).toEqual([
      'Wikilinks',
      'Tags',
      'Workflow Syntax',
    ]);
  });

  it('anchors the replacement after the brackets, not over them', async () => {
    const result = await complete('see [[wiki|');
    // `from` must point just past "[[" so accepting doesn't eat the brackets.
    expect(result!.from).toBe('see [['.length);
  });

  it('does not fire outside a wikilink', async () => {
    expect(await complete('just prose|')).toBeNull();
  });

  it('does not fire once the link is closed', async () => {
    expect(await complete('see [[Tags]]|')).toBeNull();
  });

  it('does not fire inside a fenced code block', async () => {
    // TOML array-of-tables headers are the canonical false positive.
    const result = await complete('```toml\n[[mcp.upstreams|\n```\n');
    expect(result).toBeNull();
  });

  it('is inert without a kiln', async () => {
    // Built directly rather than through `complete()`: passing `undefined` to a
    // parameter with a default would silently fall back to the default kiln and
    // make this pass for the wrong reason.
    const state = EditorState.create({
      doc: 'see [[',
      selection: { anchor: 6 },
      extensions: [markdown({ base: markdownLanguage })],
    });
    const source = wikilinkCompletionSource(() => undefined);
    expect(await source(new CompletionContext(state, 6, false))).toBeNull();
    expect(listKilnNotesMock).not.toHaveBeenCalled();
  });

  it('closes the link on accept and leaves the cursor after it', async () => {
    const result = await complete('see [[wiki|');
    const view = new EditorView({
      state: EditorState.create({ doc: 'see [[wiki', selection: { anchor: 10 } }),
    });
    const option = result!.options.find((o) => o.label === 'Wikilinks')!;
    (option.apply as (v: EditorView, c: unknown, f: number, t: number) => void)(
      view,
      option,
      6,
      10,
    );
    expect(view.state.doc.toString()).toBe('see [[Wikilinks]]');
    expect(view.state.selection.main.head).toBe('see [[Wikilinks]]'.length);
    view.destroy();
  });

  it('does not duplicate brackets the user already typed', async () => {
    const result = await complete('see [[wiki|]]');
    const view = new EditorView({
      state: EditorState.create({ doc: 'see [[wiki]]', selection: { anchor: 10 } }),
    });
    const option = result!.options.find((o) => o.label === 'Wikilinks')!;
    (option.apply as (v: EditorView, c: unknown, f: number, t: number) => void)(
      view,
      option,
      6,
      10,
    );
    expect(view.state.doc.toString()).toBe('see [[Wikilinks]]');
    view.destroy();
  });

  it('shows the path so same-named notes in different folders are distinguishable', async () => {
    const result = await complete('see [[|');
    const option = result!.options.find((o) => o.label === 'Workflow Syntax')!;
    expect(option.detail).toBe('Help/Workflows/Workflow Syntax.md');
  });
});

describe('wikilinkCompletionSource note caching', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    listKilnNotesMock.mockResolvedValue(NOTES);
  });
  afterEach(() => vi.useRealTimers());

  /** Run a source directly so cache lifetime is observable. */
  async function fire(source: ReturnType<typeof wikilinkCompletionSource>) {
    const state = EditorState.create({
      doc: 'see [[',
      selection: { anchor: 6 },
      extensions: [markdown({ base: markdownLanguage })],
    });
    return source(new CompletionContext(state, 6, false));
  }

  it('coalesces the burst of fetches a user makes while typing', async () => {
    const source = wikilinkCompletionSource(() => '/kiln');
    await fire(source);
    await fire(source);
    await fire(source);
    expect(listKilnNotesMock).toHaveBeenCalledTimes(1);
  });

  it('picks up notes created since the last fetch', async () => {
    vi.useFakeTimers();
    const source = wikilinkCompletionSource(() => '/kiln');
    const first = await fire(source);
    expect(first!.options.map((o) => o.label)).not.toContain('Brand New');

    listKilnNotesMock.mockResolvedValue([...NOTES, { name: 'Brand New', path: 'Brand New.md' }]);
    // A long-lived cache would hide a note created in another pane until reload.
    await vi.advanceTimersByTimeAsync(6_000);

    const second = await fire(source);
    expect(second!.options.map((o) => o.label)).toContain('Brand New');
  });

  it('does not serve one editor stale notes fetched by another', async () => {
    const a = wikilinkCompletionSource(() => '/kiln');
    await fire(a);
    listKilnNotesMock.mockResolvedValue([{ name: 'Only In B', path: 'Only In B.md' }]);

    // A second editor gets its own cache rather than inheriting A's snapshot.
    const b = wikilinkCompletionSource(() => '/kiln');
    const result = await fire(b);
    expect(result!.options.map((o) => o.label)).toEqual(['Only In B']);
  });
});

describe('wikilinkCompletionSource with duplicate titles', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    listKilnNotesMock.mockResolvedValue([
      { name: 'Index', path: 'Help/Index.md' },
      { name: 'Index', path: 'Guides/Index.md' },
      { name: 'Unique', path: 'Help/Unique.md' },
    ]);
  });

  /** Apply an option against a scratch view and return the resulting doc. */
  function applied(option: { apply?: unknown }, doc: string, from: number, to: number): string {
    const view = new EditorView({
      state: EditorState.create({ doc, selection: { anchor: to } }),
    });
    (option.apply as (v: EditorView, c: unknown, f: number, t: number) => void)(
      view,
      option,
      from,
      to,
    );
    const out = view.state.doc.toString();
    view.destroy();
    return out;
  }

  it('inserts distinct targets for two notes sharing a title', async () => {
    const result = await complete('see [[|');
    const dupes = result!.options.filter((o) => o.label === 'Index');
    expect(dupes).toHaveLength(2);

    // Inserting the bare title would make both options produce `[[Index]]`,
    // which resolves to whichever note the kiln happens to pick first — the
    // detail line would distinguish them on screen but not in the document.
    const targets = dupes.map((o) => applied(o, 'see [[', 6, 6));
    expect(new Set(targets).size).toBe(2);
    expect(targets).toContain('see [[Help/Index]]');
    expect(targets).toContain('see [[Guides/Index]]');
  });

  it('keeps the bare title when it is unambiguous', async () => {
    const result = await complete('see [[|');
    const unique = result!.options.find((o) => o.label === 'Unique')!;
    // Path-qualifying every link would needlessly bloat the common case.
    expect(applied(unique, 'see [[', 6, 6)).toBe('see [[Unique]]');
  });
});

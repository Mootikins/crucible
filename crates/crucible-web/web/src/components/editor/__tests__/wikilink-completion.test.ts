import { describe, it, expect, vi, beforeEach } from 'vitest';
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

import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';

// `@/lib/api` is NOT mocked: the card reads the file through the shared cache
// of `lib/query/fs.ts`, and the point of several cases below is the COUNT of
// reads that reach the daemon. A module mock would sit in front of the cache
// and count the card's calls instead of the daemon's.
let env: TestQueryEnv;
/** The path of every read the daemon answered, in order. */
let reads: string[] = [];
/** The body of every write the daemon took, in order. */
let writes: { path: string; content: string }[] = [];
/** What the daemon answers a read with. Replaced by the failure case. */
let answerRead: (path: string) => unknown = () => ({
  content: '# Real note\n',
  content_hash: 'hash-1',
});

/** The two routes one note card uses: read the bytes, write them back. */
function fileRoutes() {
  return {
    'GET /api/kiln/file': (request: Request) => {
      const path = new URL(request.url).searchParams.get('path') ?? '';
      reads.push(path);
      return answerRead(path);
    },
    'PUT /api/kiln/file': async (request: Request) => {
      writes.push((await request.json()) as { path: string; content: string });
      return {};
    },
  };
}

// CodeMirror needs layout APIs jsdom lacks; the embed's own behaviour (load,
// dirty tracking, flush-on-unmount) is what matters here, so stand in a plain
// textarea that reports changes the same way.
vi.mock('../editor/CodeMirrorEditor', () => ({
  CodeMirrorEditor: (props: { content: string; onChange: (v: string) => void }) => (
    <textarea
      data-testid="stub-editor"
      value={props.content}
      onInput={(e) => props.onChange((e.currentTarget as HTMLTextAreaElement).value)}
    />
  ),
}));

import { CanvasNoteCard } from '../canvas/CanvasCard';

describe('CanvasNoteCard', () => {
  beforeEach(() => {
    reads = [];
    writes = [];
    answerRead = () => ({ content: '# Real note\n', content_hash: 'hash-1' });
    env = createTestQueryEnv(fileRoutes());
  });

  afterEach(() => {
    env.restore();
  });

  it('loads the real note content', async () => {
    const { findByTestId } = render(() => (
      <CanvasNoteCard absPath="/kiln/Notes/A.md" editable={false} />
    ));

    const embed = await findByTestId('canvas-note-embed');
    await waitFor(() => expect(embed.textContent).toContain('Real note'));
    expect(reads).toEqual(['/kiln/Notes/A.md']);
  });

  /** A card is a window onto the file, not a copy — so it is read-only until selected. */
  it('is read-only until the card is selected', async () => {
    const { queryByTestId, findByTestId } = render(() => (
      <CanvasNoteCard absPath="/kiln/Notes/A.md" editable={false} />
    ));

    await findByTestId('canvas-note-embed');
    await waitFor(() => expect(queryByTestId('stub-editor')).toBeNull());
  });

  it('mounts a live editor when the card is selected', async () => {
    const { findByTestId } = render(() => (
      <CanvasNoteCard absPath="/kiln/Notes/A.md" editable={true} />
    ));

    const editor = (await findByTestId('stub-editor')) as HTMLTextAreaElement;
    expect(editor.value).toContain('Real note');
  });

  /**
   * Virtualization unmounts cards that leave the viewport. Without a flush on
   * cleanup, panning away from a card mid-edit would silently discard it —
   * virtualization would become data loss.
   */
  it('flushes a pending edit when the card unmounts', async () => {
    const { findByTestId, unmount } = render(() => (
      <CanvasNoteCard absPath="/kiln/Notes/A.md" editable={true} />
    ));

    const editor = (await findByTestId('stub-editor')) as HTMLTextAreaElement;
    editor.value = '# Edited\n';
    editor.dispatchEvent(new Event('input', { bubbles: true }));

    await waitFor(() => expect(writes).toEqual([]));

    unmount();

    await waitFor(() =>
      expect(writes).toEqual([{ path: '/kiln/Notes/A.md', content: '# Edited\n' }]),
    );
  });

  /**
   * The card's path is derived from the canvas document, so reading it inside
   * an effect subscribed that effect to the whole document — which is replaced
   * on every drag frame. The file was refetched continuously, blanking the card
   * to "Loading…" mid-drag.
   */
  /**
   * The card's path is DERIVED from the canvas document, so reading it inside
   * an effect subscribed that effect to the whole document — replaced on every
   * drag frame — and the file was refetched continuously, blanking the card to
   * "Loading…" mid-drag.
   *
   * The derivation is driven by a signal here so the source identity really
   * changes while the resulting path does not; a constant literal would pass
   * even with the memo removed.
   */
  it('does not refetch while the document churns but the path is unchanged', async () => {
    const [tick, setTick] = createSignal(0);
    const derivedPath = () => `/kiln/Notes/${['A', 'A', 'A'][tick() % 3]}.md`;

    const { findByTestId } = render(() => (
      <CanvasNoteCard absPath={derivedPath()} editable={false} />
    ));

    await findByTestId('canvas-note-embed');
    await waitFor(() => expect(reads).toHaveLength(1));

    for (let i = 1; i <= 20; i++) setTick(i);
    await new Promise((r) => setTimeout(r, 30));

    expect(reads.length, 'a card must fetch once, not once per document change').toBe(1);
  });

  /** The complementary case: a REAL path change must refetch. */
  it('refetches when the path actually changes', async () => {
    const [name, setName] = createSignal('A');
    const { findByTestId } = render(() => (
      <CanvasNoteCard absPath={`/kiln/Notes/${name()}.md`} editable={false} />
    ));

    await findByTestId('canvas-note-embed');
    await waitFor(() => expect(reads).toHaveLength(1));

    setName('B');
    await waitFor(() => expect(reads).toHaveLength(2));
    expect(reads.at(-1)).toBe('/kiln/Notes/B.md');
  });

  it('surfaces a load failure', async () => {
    answerRead = () =>
      new Response(JSON.stringify({ error: { code: 404, message: 'File not found' } }), {
        status: 404,
        headers: { 'Content-Type': 'application/json' },
      });
    const { findByTestId } = render(() => (
      <CanvasNoteCard absPath="/kiln/Notes/Missing.md" editable={false} />
    ));

    // The banner carries what the read stack says, which is the daemon's
    // status and the call that failed. The card must not swallow it and paint
    // an empty note instead.
    const err = await findByTestId('canvas-embed-error');
    expect(err.textContent).toContain('Failed to read file');
    expect(err.textContent).toContain('404');
  });
});

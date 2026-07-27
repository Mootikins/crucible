import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';

const getFileContentMock = vi.fn();
const saveFileContentMock = vi.fn();

vi.mock('@/lib/api', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getFileContent: (...args: unknown[]) => getFileContentMock(...args),
  saveFileContent: (...args: unknown[]) => saveFileContentMock(...args),
}));

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
    getFileContentMock.mockReset();
    saveFileContentMock.mockReset();
    getFileContentMock.mockResolvedValue('# Real note\n');
    saveFileContentMock.mockResolvedValue(undefined);
  });

  it('loads the real note content', async () => {
    const { findByTestId } = render(() => (
      <CanvasNoteCard absPath="/kiln/Notes/A.md" editable={false} />
    ));

    const embed = await findByTestId('canvas-note-embed');
    await waitFor(() => expect(embed.textContent).toContain('Real note'));
    expect(getFileContentMock).toHaveBeenCalledWith('/kiln/Notes/A.md');
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

    await waitFor(() => expect(saveFileContentMock).not.toHaveBeenCalled());

    unmount();

    await waitFor(() =>
      expect(saveFileContentMock).toHaveBeenCalledWith('/kiln/Notes/A.md', '# Edited\n'),
    );
  });

  /**
   * The card's path is derived from the canvas document, so reading it inside
   * an effect subscribed that effect to the whole document — which is replaced
   * on every drag frame. The file was refetched continuously, blanking the card
   * to "Loading…" mid-drag.
   */
  it('does not refetch when unrelated props change', async () => {
    const [pos, setPos] = createSignal(0);
    const { findByTestId } = render(() => (
      <CanvasNoteCard absPath={`/kiln/Notes/A.md?${''}`.replace(/\?$/, '')} editable={pos() > -1} />
    ));

    await findByTestId('canvas-note-embed');
    await waitFor(() => expect(getFileContentMock).toHaveBeenCalledTimes(1));

    // Simulate a drag: many unrelated updates, same path.
    for (let i = 0; i < 20; i++) setPos(i);
    await new Promise((r) => setTimeout(r, 30));

    expect(
      getFileContentMock.mock.calls.length,
      'a card must fetch its file once, not once per frame',
    ).toBe(1);
  });

  /**
   * The hover popover controller listens at the DOCUMENT level, so it cannot
   * be handed the canvas's kiln as a prop — it can only read it off the DOM.
   * Without the marker every hovered wikilink in a canvas card resolves in
   * whatever kiln the status bar happens to point at, which is another kiln's
   * data appearing inside this one.
   */
  it('marks its kiln on the DOM so document-level link resolution stays in it', async () => {
    const { findByTestId } = render(() => (
      <CanvasNoteCard absPath="/canvas-demo/Notes/A.md" kiln="/canvas-demo" editable={false} />
    ));

    const embed = await findByTestId('canvas-note-embed');
    await waitFor(() => expect(embed.querySelector('[data-kiln]')).not.toBeNull());
    expect(embed.querySelector('[data-kiln]')!.getAttribute('data-kiln')).toBe('/canvas-demo');
  });

  it('surfaces a load failure', async () => {
    getFileContentMock.mockRejectedValue(new Error('File not found'));
    const { findByTestId } = render(() => (
      <CanvasNoteCard absPath="/kiln/Notes/Missing.md" editable={false} />
    ));

    const err = await findByTestId('canvas-embed-error');
    expect(err.textContent).toContain('File not found');
  });
});

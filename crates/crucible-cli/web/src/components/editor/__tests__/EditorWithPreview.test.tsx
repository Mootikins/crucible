import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, waitFor, fireEvent } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';

const openNoteInEditorMock = vi.fn();

vi.mock('@/lib/note-actions', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  openNoteInEditor: (...args: unknown[]) => openNoteInEditorMock(...args),
}));

import { EditorWithPreview } from '../EditorWithPreview';

const noop = () => {};

beforeEach(() => {
  vi.clearAllMocks();
  document.body.innerHTML = '';
});

describe('EditorWithPreview', () => {
  it('markdown files get a preview toggle; toggling renders the markdown', async () => {
    const { getByTestId, queryByTestId, container } = render(() => (
      <EditorWithPreview
        content={'# Heading\n\nSee [[Other Note]].'}
        path="/kiln/note.md"
        onChange={noop}
      />
    ));

    expect(container.querySelector('.cm-editor')).not.toBeNull();

    fireEvent.click(getByTestId('preview-toggle'));
    await waitFor(() => {
      expect(queryByTestId('markdown-preview')).not.toBeNull();
    });
    // Rendered, not source: heading element + wikilink anchor with data-note.
    const preview = getByTestId('markdown-preview');
    await waitFor(() => {
      expect(preview.querySelector('h1')?.textContent).toBe('Heading');
      expect(preview.querySelector('[data-note="Other Note"]')).not.toBeNull();
    });
    // The editor is swapped out while previewing.
    expect(container.querySelector('.cm-editor')).toBeNull();

    // Toggle back to the editor.
    fireEvent.click(getByTestId('preview-toggle'));
    await waitFor(() => {
      expect(container.querySelector('.cm-editor')).not.toBeNull();
    });
  });

  it('strips frontmatter from the preview', async () => {
    const { getByTestId } = render(() => (
      <EditorWithPreview
        content={'---\ntitle: X\n---\n\nBody only.'}
        path="/kiln/note.md"
        onChange={noop}
      />
    ));
    fireEvent.click(getByTestId('preview-toggle'));
    await waitFor(() => {
      const preview = getByTestId('markdown-preview');
      expect(preview.textContent).toContain('Body only.');
      expect(preview.textContent).not.toContain('title: X');
    });
  });

  it('clicking a wikilink in the preview opens the note', async () => {
    const { getByTestId } = render(() => (
      <EditorWithPreview content="Go to [[Other Note]]." path="/kiln/note.md" onChange={noop} />
    ));
    fireEvent.click(getByTestId('preview-toggle'));
    await waitFor(() => {
      expect(getByTestId('markdown-preview').querySelector('[data-note]')).not.toBeNull();
    });

    fireEvent.click(getByTestId('markdown-preview').querySelector('[data-note]')!);
    expect(openNoteInEditorMock).toHaveBeenCalledWith('Other Note', undefined);
  });

  it('non-markdown files get no toggle', () => {
    const { queryByTestId } = render(() => (
      <EditorWithPreview content="fn main() {}" path="/src/main.rs" onChange={noop} />
    ));
    expect(queryByTestId('preview-toggle')).toBeNull();
  });

  it('switching files drops back to edit mode', async () => {
    const [path, setPath] = createSignal('/kiln/a.md');
    const { getByTestId, queryByTestId, container } = render(() => (
      <EditorWithPreview content="text" path={path()} onChange={noop} />
    ));

    fireEvent.click(getByTestId('preview-toggle'));
    await waitFor(() => expect(queryByTestId('markdown-preview')).not.toBeNull());

    setPath('/kiln/b.md');
    await waitFor(() => {
      expect(queryByTestId('markdown-preview')).toBeNull();
      expect(container.querySelector('.cm-editor')).not.toBeNull();
    });
  });
});

describe('vim mode', () => {
  it('vimMode starts in normal mode: x deletes the character under the cursor', async () => {
    const onChange = vi.fn();
    const { container } = render(() => (
      <EditorWithPreview content="hello" path="/kiln/note.md" onChange={onChange} vimMode />
    ));
    const content = container.querySelector('.cm-content') as HTMLElement;
    expect(content).not.toBeNull();

    fireEvent.keyDown(content, { key: 'x' });
    await waitFor(() => {
      expect(onChange).toHaveBeenCalledWith('ello');
    });
  });

  it('without vimMode, x is not a command', async () => {
    const onChange = vi.fn();
    const { container } = render(() => (
      <EditorWithPreview content="hello" path="/kiln/note.md" onChange={onChange} />
    ));
    const content = container.querySelector('.cm-content') as HTMLElement;

    fireEvent.keyDown(content, { key: 'x' });
    await new Promise((r) => setTimeout(r, 50));
    expect(onChange).not.toHaveBeenCalledWith('ello');
  });
});

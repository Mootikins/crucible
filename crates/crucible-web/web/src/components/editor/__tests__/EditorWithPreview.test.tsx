import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, waitFor, fireEvent } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';

const openNoteInEditorMock = vi.fn();
const editNoteMock = vi.fn();
const addNotificationMock = vi.fn();

vi.mock('@/lib/note-actions', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  openNoteInEditor: (...args: unknown[]) => openNoteInEditorMock(...args),
}));
// The one note write door. The component must not reach `api` for a tick.
vi.mock('@/lib/offline/sync', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  editNote: (...args: unknown[]) => editNoteMock(...args),
}));
vi.mock('@/stores/notificationStore', () => ({
  notificationActions: { addNotification: (...args: unknown[]) => addNotificationMock(...args) },
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
        baseHash="h1"
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
        baseHash="h1"
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
      <EditorWithPreview content="Go to [[Other Note]]." path="/kiln/note.md" baseHash="h1" onChange={noop} />
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
      <EditorWithPreview content="fn main() {}" path="/src/main.rs" baseHash="h1" onChange={noop} />
    ));
    expect(queryByTestId('preview-toggle')).toBeNull();
    expect(queryByTestId('mode-toggle')).toBeNull();
  });

  it('markdown defaults to live preview: styled prose, syntax marks hidden', () => {
    const { container } = render(() => (
      <EditorWithPreview content="Some **bold** text." path="/kiln/note.md" baseHash="h1" onChange={noop} />
    ));
    expect(container.querySelector('.cm-lp-strong')).not.toBeNull();
    expect(container.querySelector('.cm-content')?.textContent).not.toContain('**');
  });

  it('the mode toggle switches to raw source and back', async () => {
    const { getByTestId, container } = render(() => (
      <EditorWithPreview content="Some **bold** text." path="/kiln/note.md" baseHash="h1" onChange={noop} />
    ));

    fireEvent.click(getByTestId('mode-toggle'));
    await waitFor(() => {
      expect(container.querySelector('.cm-content')?.textContent).toContain('**bold**');
      expect(container.querySelector('.cm-lp-strong')).toBeNull();
    });

    fireEvent.click(getByTestId('mode-toggle'));
    await waitFor(() => {
      expect(container.querySelector('.cm-lp-strong')).not.toBeNull();
    });
  });

  it('non-markdown files never get the live-preview extension', () => {
    const { container } = render(() => (
      <EditorWithPreview content="let x = 1; // **not md**" path="/src/main.rs" baseHash="h1" onChange={noop} />
    ));
    expect(container.querySelector('.cm-lp-strong')).toBeNull();
    expect(container.querySelector('.cm-content')?.textContent).toContain('**not md**');
  });

  it('switching files drops back to edit mode', async () => {
    const [path, setPath] = createSignal('/kiln/a.md');
    const { getByTestId, queryByTestId, container } = render(() => (
      <EditorWithPreview content="text" path={path()} baseHash="h1" onChange={noop} />
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

describe('reading-view parity (live mode)', () => {
  it('live preview has no line-number gutter; source mode does', async () => {
    const { getByTestId, container } = render(() => (
      <EditorWithPreview content="text" path="/kiln/note.md" baseHash="h1" onChange={noop} />
    ));
    expect(container.querySelector('.cm-lineNumbers')).toBeNull();

    fireEvent.click(getByTestId('mode-toggle'));
    await waitFor(() => {
      expect(container.querySelector('.cm-lineNumbers')).not.toBeNull();
    });
  });

  it('applies the readable line width to the live-preview content', () => {
    const { container } = render(() => (
      <EditorWithPreview content="text" path="/kiln/note.md" baseHash="h1" onChange={noop} lineWidth={500} />
    ));
    const content = container.querySelector('.cm-content') as HTMLElement;
    expect(content.style.maxWidth).toBe('500px');
  });

  it('initialMode="reading" opens markdown as the rendered view', async () => {
    const { container, queryByTestId } = render(() => (
      <EditorWithPreview
        content={'# H\n\nBody.'}
        path="/kiln/note.md"
        baseHash="h1"
        onChange={noop}
        initialMode="reading"
      />
    ));
    await waitFor(() => {
      expect(queryByTestId('markdown-preview')).not.toBeNull();
    });
    expect(container.querySelector('.cm-editor')).toBeNull();
  });

  it('reading view honors the readable line width', async () => {
    const { getByTestId } = render(() => (
      <EditorWithPreview
        content="Body."
        path="/kiln/note.md"
        baseHash="h1"
        onChange={noop}
        initialMode="reading"
        lineWidth={640}
      />
    ));
    await waitFor(() => {
      const prose = getByTestId('markdown-preview').firstElementChild as HTMLElement;
      expect(prose.style.maxWidth).toBe('640px');
    });
  });
});

describe('save keybinds', () => {
  it('Mod-Enter saves (off a wikilink)', async () => {
    const onSave = vi.fn();
    const { container } = render(() => (
      <EditorWithPreview content="plain text" path="/kiln/note.md" baseHash="h1" onChange={noop} onSave={onSave} />
    ));
    const content = container.querySelector('.cm-content') as HTMLElement;
    fireEvent.keyDown(content, { key: 'Enter', ctrlKey: true });
    await waitFor(() => expect(onSave).toHaveBeenCalled());
  });
});

describe('vim mode', () => {
  it('vimMode starts in normal mode: x deletes the character under the cursor', async () => {
    const onChange = vi.fn();
    const { container } = render(() => (
      <EditorWithPreview content="hello" path="/kiln/note.md" baseHash="h1" onChange={onChange} vimMode />
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
      <EditorWithPreview content="hello" path="/kiln/note.md" baseHash="h1" onChange={onChange} />
    ));
    const content = container.querySelector('.cm-content') as HTMLElement;

    // Without vim, 'x' is not a command — the document is untouched. CodeMirror
    // dispatches doc changes synchronously in response to the key event, so we
    // can assert immediately rather than racing an arbitrary sleep: the editor
    // still reads "hello" (not the vim-deleted "ello") and onChange never fired
    // with the deleted text.
    fireEvent.keyDown(content, { key: 'x' });
    expect(content.textContent).toContain('hello');
    expect(onChange).not.toHaveBeenCalledWith('ello');
  });
});

/**
 * A tick in the reading view is one anchored edit through the one note write
 * door. The box flips at once. The answer decides whether the flip stays.
 */
describe('task tick', () => {
  const TASKS = '- [ ] first\n- [ ] second\n';
  const TICKED = '- [x] first\n- [ ] second\n';
  const PATH = '/kiln/tasks.md';

  const tick = async (extra: { onBaseChange?: (hash: string) => void } = {}) => {
    const onChange = vi.fn();
    const { container } = render(() => (
      <EditorWithPreview
        content={TASKS}
        path={PATH}
        kiln="/kiln"
        baseHash="h1"
        onChange={onChange}
        initialMode="reading"
        {...extra}
      />
    ));
    let box: HTMLInputElement | null = null;
    await waitFor(() => {
      box = container.querySelector<HTMLInputElement>('input.task-checkbox[data-task-line="0"]');
      expect(box).not.toBeNull();
    });
    fireEvent.click(box!);
    await waitFor(() => expect(editNoteMock).toHaveBeenCalled());
    return onChange;
  };

  it('ticks a task through editNote and keeps the flip when it is queued offline', async () => {
    editNoteMock.mockResolvedValueOnce({ queued: true });

    const onChange = await tick();

    expect(editNoteMock).toHaveBeenCalledWith({
      path: PATH,
      edits: [{ expect: '- [ ] first', replace: '- [x] first' }],
      base: 'h1',
      kiln: '/kiln',
    });
    // Let the answer land. A queued tick is kept, and the app bar already
    // shows the pending count, so nothing else is said.
    await Promise.resolve();
    await Promise.resolve();
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenCalledWith(TICKED);
    expect(addNotificationMock).not.toHaveBeenCalled();
  });

  it('reverts the flip and names the reason when the daemon refuses', async () => {
    editNoteMock.mockResolvedValueOnce({
      queued: false,
      ok: false,
      failed: [{ index: 0, reason: 'stale' }],
      current_hash: 'h9',
      stale_base: true,
    });

    const onChange = await tick();

    await waitFor(() => expect(onChange).toHaveBeenCalledTimes(2));
    expect(onChange).toHaveBeenLastCalledWith(TASKS);
    expect(addNotificationMock).toHaveBeenCalledWith(
      'warning',
      expect.stringContaining('changed elsewhere'),
    );
  });

  it('a successful tick moves the base to the answered hash', async () => {
    editNoteMock.mockResolvedValueOnce({ queued: false, ok: true, hash: 'h2' });
    const onBaseChange = vi.fn();

    const onChange = await tick({ onBaseChange });

    await waitFor(() => expect(onBaseChange).toHaveBeenCalledWith('h2'));
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(addNotificationMock).not.toHaveBeenCalled();
  });

  it('reverts the flip and reports an error when the write throws', async () => {
    editNoteMock.mockRejectedValueOnce(new Error('boom'));

    const onChange = await tick();

    await waitFor(() => expect(onChange).toHaveBeenCalledTimes(2));
    expect(onChange).toHaveBeenLastCalledWith(TASKS);
    expect(addNotificationMock).toHaveBeenCalledWith('error', expect.stringContaining('boom'));
  });
});

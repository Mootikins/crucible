import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';

// get_note_by_name returns metadata only (no content), so the editor must load
// file bytes via GET /api/kiln/file (getFileContent). These mocks let us assert
// which endpoint openFile actually hits.
const getFileContent = vi.fn(async (_path: string) => '');
const saveFileContent = vi.fn(async (_path: string, _content: string) => {});
const getNote = vi.fn(async () => ({ name: '', path: '', content: '', title: null, tags: [], updated_at: '' }));

vi.mock('@/lib/api', () => ({
  getFileContent: (p: string) => getFileContent(p),
  saveFileContent: (p: string, c: string) => saveFileContent(p, c),
  getNote: () => getNote(),
}));

const KILN = '/home/user/kiln';

const { EditorProvider, useEditor } = await import('../EditorContext');

function withEditor(fn: (editor: ReturnType<typeof useEditor>) => void) {
  let captured: ReturnType<typeof useEditor> | undefined;
  const Probe = () => {
    captured = useEditor();
    return <div data-testid="probe">{captured.openFiles().length}</div>;
  };
  render(() => (
    <EditorProvider>
      <Probe />
    </EditorProvider>
  ));
  fn(captured!);
  return captured!;
}

describe('EditorContext — content load path (bug 8)', () => {
  beforeEach(() => {
    getFileContent.mockClear();
    saveFileContent.mockClear();
    getNote.mockClear();
  });

  it('openFile loads via getFileContent (GET /api/kiln/file), not getNote', async () => {
    const path = `${KILN}/notes/from-tui.md`;
    getFileContent.mockResolvedValueOnce('terminal was here\n');

    let editor: ReturnType<typeof useEditor>;
    editor = withEditor(() => {});
    await editor!.openFile(path);

    expect(getFileContent).toHaveBeenCalledWith(path);
    expect(getNote).not.toHaveBeenCalled();

    await waitFor(() => {
      const f = editor!.openFiles().find((x) => x.path === path);
      expect(f?.content).toBe('terminal was here\n');
      expect(f?.dirty).toBe(false);
    });
  });

  it('edit marks dirty; saveFile writes via saveFileContent and clears dirty', async () => {
    const path = `${KILN}/notes/from-tui.md`;
    getFileContent.mockResolvedValueOnce('start\n');

    const editor = withEditor(() => {});
    await editor.openFile(path);
    await waitFor(() => expect(editor.openFiles().length).toBe(1));

    editor.updateFileContent(path, 'start\nbrowser was here\n');
    expect(editor.openFiles()[0].dirty).toBe(true);

    await editor.saveFile(path);

    // Save goes through PUT /api/kiln/file (saveFileContent) by absolute path.
    expect(saveFileContent).toHaveBeenCalledWith(path, 'start\nbrowser was here\n');
    await waitFor(() => expect(editor.openFiles()[0].dirty).toBe(false));
  });
});

describe('EditorContext — unsaved-changes guard on close (bug 6)', () => {
  beforeEach(() => {
    getFileContent.mockClear();
    vi.restoreAllMocks();
  });

  const openDirtyFile = async (path: string) => {
    getFileContent.mockResolvedValueOnce('original\n');
    const editor = withEditor(() => {});
    await editor.openFile(path);
    await waitFor(() => expect(editor.openFiles().length).toBe(1));
    editor.updateFileContent(path, 'edited\n');
    expect(editor.openFiles()[0].dirty).toBe(true);
    return editor;
  };

  it('closing a dirty file asks for confirmation and keeps it open on cancel', async () => {
    const path = `${KILN}/notes/dirty.md`;
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false);
    const editor = await openDirtyFile(path);

    editor.closeFile(path);
    await Promise.resolve(); // eviction is deferred a microtask (see refcount)

    expect(confirm).toHaveBeenCalledOnce();
    expect(editor.openFiles().length).toBe(1);
  });

  it('closing a dirty file discards when the user confirms', async () => {
    const path = `${KILN}/notes/dirty.md`;
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const editor = await openDirtyFile(path);

    editor.closeFile(path);
    await Promise.resolve();

    expect(editor.openFiles().length).toBe(0);
  });

  it('closing a clean file never prompts', async () => {
    const path = `${KILN}/notes/clean.md`;
    const confirm = vi.spyOn(window, 'confirm');
    getFileContent.mockResolvedValueOnce('content\n');
    const editor = withEditor(() => {});
    await editor.openFile(path);
    await waitFor(() => expect(editor.openFiles().length).toBe(1));

    editor.closeFile(path);
    await Promise.resolve();

    expect(confirm).not.toHaveBeenCalled();
    expect(editor.openFiles().length).toBe(0);
  });

  it('force-close skips the prompt (tab-level guard already ran)', async () => {
    const path = `${KILN}/notes/dirty.md`;
    const confirm = vi.spyOn(window, 'confirm');
    const editor = await openDirtyFile(path);

    editor.closeFile(path, { force: true });
    await Promise.resolve();

    expect(confirm).not.toHaveBeenCalled();
    expect(editor.openFiles().length).toBe(0);
  });

  // Regression: moving/popping-out a dirty tab unmounts the source panel
  // (closeFile) and remounts a new one (openFile) for the same path. The buffer
  // must survive — no disk re-read, no silent loss of unsaved edits.
  it('preserves a dirty buffer across a move/pop-out remount (refcount)', async () => {
    const path = `${KILN}/notes/dirty.md`;
    const editor = await openDirtyFile(path); // content 'edited\n', dirty
    getFileContent.mockClear();

    // Target panel mounts (2nd holder) then source panel unmounts (force close).
    await editor.openFile(path);
    editor.closeFile(path, { force: true });
    await Promise.resolve();

    // Still open, still dirty, and disk was NOT re-read.
    expect(editor.openFiles().length).toBe(1);
    expect(editor.openFiles()[0].content).toBe('edited\n');
    expect(editor.openFiles()[0].dirty).toBe(true);
    expect(getFileContent).not.toHaveBeenCalled();

    // Last holder releases → evicted.
    editor.closeFile(path, { force: true });
    await Promise.resolve();
    expect(editor.openFiles().length).toBe(0);
  });

  // Even when the source unmounts BEFORE the target remounts, the deferred
  // eviction must be cancelled by the re-open.
  it('preserves the buffer when unmount precedes remount', async () => {
    const path = `${KILN}/notes/dirty.md`;
    const editor = await openDirtyFile(path);
    getFileContent.mockClear();

    editor.closeFile(path, { force: true }); // source unmounts first (refcount 0, deferred)
    await editor.openFile(path); // target remounts same tick, re-refs
    await Promise.resolve();

    expect(editor.openFiles().length).toBe(1);
    expect(editor.openFiles()[0].dirty).toBe(true);
    expect(getFileContent).not.toHaveBeenCalled();
  });
});

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';

// get_note_by_name returns metadata only (no content), so the editor must load
// file bytes via GET /api/kiln/file (getFileContent). These mocks let us assert
// which endpoint openFile actually hits.
const getFileContent = vi.fn(async (_path: string) => '');
const saveFileContent = vi.fn(async (_path: string, _content: string) => {});
const getNote = vi.fn(async () => ({ name: '', path: '', content: '', title: null, tags: [], updated_at: '' }));
// The daemon's answer to a whole write. The default writes through the body
// spy, so the older tests below keep one place to read. A test that needs a
// stale or a queued answer overrides one call.
const guardedSave = vi.fn(async (p: string, c: string, _base: string) => {
  await saveFileContent(p, c);
  return { ok: true, content_hash: 'written' } as
    | { ok: true; content_hash: string }
    | { ok: false; current_hash: string };
});
const addNotification = vi.fn();
/** The hash a read answers. A test that lands a write sets it to the landed hash. */
let readHash = 'base-hash';

const KILN = '/home/user/kiln';

vi.mock('@/lib/api', () => ({
  // The editor reads through the offline layer now, which asks for the hash
  // the buffer was read at; the transport underneath is the same endpoint.
  getFileWithHash: async (p: string) => ({
    content: await getFileContent(p),
    content_hash: readHash,
  }),
  getFileContent: (p: string) => getFileContent(p),
  saveFileContent: (p: string, c: string) => saveFileContent(p, c),
  saveFileIfUnchanged: (p: string, c: string, base: string) => guardedSave(p, c, base),
  getNote: () => getNote(),
  listKilns: async () => [{ path: KILN }],
  rawFileUrl: (p: string) => `/api/file/raw?path=${encodeURIComponent(p)}`,
  getConfig: async () => ({ kiln_path: KILN, config_root: '/etc/crucible' }),
  listNotes: async () => [],
}));


vi.mock('@/stores/notificationStore', () => ({
  notificationActions: { addNotification: (...a: unknown[]) => addNotification(...a) },
}));

const { EditorProvider, useEditor } = await import('../EditorContext');
const { setOfflineStore, syncNow } = await import('@/lib/offline/sync');
const { memoryStore } = await import('@/lib/offline/store');

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

  it('openFile loads the file bytes (GET /api/kiln/file), not getNote', async () => {
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

  it('edit marks dirty; saveFile writes the file and clears dirty', async () => {
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

/**
 * The daemon compares the base inside its write and answers. The buffer must
 * follow that answer: a refused write keeps the user's text on screen, and an
 * accepted one moves the base so the NEXT save is not stale by construction.
 */
describe('EditorContext — the buffer follows the answer to a whole write', () => {
  const PATH = `${KILN}/notes/shared.md`;
  const fileState = (editor: ReturnType<typeof useEditor>) =>
    editor.openFiles().find((f) => f.path === PATH)!;

  beforeEach(() => {
    getFileContent.mockClear();
    saveFileContent.mockClear();
    guardedSave.mockClear();
    addNotification.mockClear();
    setOfflineStore(memoryStore());
  });

  const openEdited = async () => {
    getFileContent.mockResolvedValueOnce('on disk\n');
    const editor = withEditor(() => {});
    await editor.openFile(PATH);
    await waitFor(() => expect(editor.openFiles().length).toBe(1));
    editor.updateFileContent(PATH, 'the unsaved text');
    return editor;
  };

  it('keeps the buffer dirty and offers a conflict copy when the save is stale', async () => {
    guardedSave.mockResolvedValueOnce({ ok: false, current_hash: 'h9' });
    const editor = await openEdited();

    await editor.saveFile(PATH);

    expect(fileState(editor).dirty).toBe(true);
    expect(fileState(editor).content).toBe('the unsaved text');
    expect(fileState(editor).baseHash, 'a refused write moves no base').toBe('base-hash');
    expect(saveFileContent, 'nothing is written without a choice').not.toHaveBeenCalled();
    expect(addNotification).toHaveBeenCalledWith(
      'warning',
      expect.stringContaining('changed elsewhere'),
      expect.objectContaining({ label: 'Save as conflict copy' }),
    );
    // A stale save is an answer, not a failure: the red retry line stays off.
    expect(editor.error()).toBeNull();
    expect(editor.retryFailedOperation()).toBeNull();
  });

  it('the conflict copy action writes the unsaved text beside the note', async () => {
    guardedSave.mockResolvedValueOnce({ ok: false, current_hash: 'h9' });
    const editor = await openEdited();
    await editor.saveFile(PATH);

    const action = addNotification.mock.calls[0][2] as { label: string; run: () => void };
    // The dated name is free.
    getFileContent.mockRejectedValueOnce(Object.assign(new Error('missing'), { status: 404 }));
    action.run();

    await waitFor(() =>
      expect(saveFileContent).toHaveBeenCalledWith(
        expect.stringMatching(/\/notes\/shared \(conflict, .*\)\.md$/),
        'the unsaved text',
      ),
    );
    expect(guardedSave, 'a copy is a fresh note, not a guarded write').toHaveBeenCalledTimes(1);
  });

  it('the conflict copy keeps the refused text even after the note was reopened', async () => {
    guardedSave.mockResolvedValueOnce({ ok: false, current_hash: 'h9' });
    const editor = await openEdited();
    await editor.saveFile(PATH);
    const action = addNotification.mock.calls[0][2] as { label: string; run: () => void };

    // The user reloads: the only path is to close the note and to open it again.
    editor.closeFile(PATH, { force: true });
    await Promise.resolve(); // the eviction is deferred by one microtask
    await waitFor(() => expect(editor.openFiles().length).toBe(0));
    getFileContent.mockResolvedValueOnce('server text\n');
    await editor.openFile(PATH);
    await waitFor(() => expect(fileState(editor).content).toBe('server text\n'));

    // The dated name is free.
    getFileContent.mockRejectedValueOnce(Object.assign(new Error('missing'), { status: 404 }));
    action.run();

    // The copy holds the text the daemon refused, not the text the buffer shows now.
    await waitFor(() =>
      expect(saveFileContent).toHaveBeenCalledWith(
        expect.stringMatching(/\/notes\/shared \(conflict, .*\)\.md$/),
        'the unsaved text',
      ),
    );
    await waitFor(() =>
      expect(addNotification).toHaveBeenCalledWith(
        'success',
        expect.stringMatching(/^Your version was saved as .*\. Reload the note to continue from the current text\.$/),
      ),
    );
  });

  it('marks the buffer clean and takes the answered hash on a clean save', async () => {
    guardedSave.mockResolvedValueOnce({ ok: true, content_hash: 'h2' });
    const editor = await openEdited();

    await editor.saveFile(PATH);

    await waitFor(() => expect(fileState(editor).dirty).toBe(false));
    expect(fileState(editor).baseHash).toBe('h2');

    // The next save is made FROM the text the daemon now holds.
    editor.updateFileContent(PATH, 'the unsaved text, again');
    await editor.saveFile(PATH);
    expect(guardedSave).toHaveBeenLastCalledWith(PATH, 'the unsaved text, again', 'h2');
  });

  it('a queued save marks the buffer clean and takes no base', async () => {
    // No status: the daemon never answered, so the outbox holds the writing.
    guardedSave.mockRejectedValueOnce(new TypeError('Failed to fetch'));
    const editor = await openEdited();

    await editor.saveFile(PATH);

    await waitFor(() => expect(fileState(editor).dirty).toBe(false));
    expect(fileState(editor).baseHash).toBe('base-hash');
    expect(editor.error()).toBeNull();
  });

  it('setBaseHash moves the base, and the next save is made from it', async () => {
    // An anchored edit that landed changed the note on disk. The component
    // that sent it reports the answered hash here, so the whole save that
    // follows carries the hash the daemon holds now.
    const editor = await openEdited();

    editor.setBaseHash(PATH, 'h2');

    expect(fileState(editor).baseHash).toBe('h2');
    expect(fileState(editor).dirty).toBe(true);
    await editor.saveFile(PATH);
    expect(guardedSave).toHaveBeenLastCalledWith(PATH, 'the unsaved text', 'h2');
  });
});

/**
 * A queued write drains while the note stays open. The daemon's hash moves,
 * so the buffer's base must move with it, or the next save from that buffer
 * is refused as stale for the user's own queued write. The drain names each
 * landed write, and the editor moves the buffer whose base the write came from.
 */
describe('EditorContext — a drained write moves the open buffer', () => {
  const PATH = `${KILN}/notes/queued.md`;
  const fileState = (editor: ReturnType<typeof useEditor>) =>
    editor.openFiles().find((f) => f.path === PATH)!;

  beforeEach(() => {
    getFileContent.mockClear();
    saveFileContent.mockClear();
    guardedSave.mockClear();
    addNotification.mockClear();
    readHash = 'base-hash';
    setOfflineStore(memoryStore());
  });

  /** Open the note, edit it, and save while the daemon does not answer. */
  const openAndQueue = async () => {
    getFileContent.mockResolvedValueOnce('on disk\n');
    const editor = withEditor(() => {});
    await editor.openFile(PATH);
    await waitFor(() => expect(editor.openFiles().length).toBe(1));
    editor.updateFileContent(PATH, 'queued text\n');
    guardedSave.mockRejectedValueOnce(new TypeError('Failed to fetch'));
    await editor.saveFile(PATH);
    await waitFor(() => expect(fileState(editor).dirty).toBe(false));
    expect(fileState(editor).baseHash, 'a queued save moves no base').toBe('base-hash');
    return editor;
  };

  it('a drained write moves the open buffer\'s base when the bases match', async () => {
    const editor = await openAndQueue();
    // The user keeps typing: the buffer is dirty, and its text is theirs.
    editor.updateFileContent(PATH, 'queued text, and more\n');

    guardedSave.mockResolvedValueOnce({ ok: true, content_hash: 'h2' });
    const result = await syncNow();

    expect(result.sent).toBe(1);
    expect(fileState(editor).baseHash).toBe('h2');
    expect(fileState(editor).content, 'a dirty buffer keeps its text').toBe('queued text, and more\n');
    expect(fileState(editor).dirty).toBe(true);
    // The next save is made from the hash the daemon holds now.
    guardedSave.mockResolvedValueOnce({ ok: true, content_hash: 'h3' });
    await editor.saveFile(PATH);
    expect(guardedSave).toHaveBeenLastCalledWith(PATH, 'queued text, and more\n', 'h2');
  });

  it('a drained write leaves a buffer with another base alone', async () => {
    const editor = await openAndQueue();
    // A tick landed online meanwhile and moved this buffer's base on its own.
    editor.setBaseHash(PATH, 'h7');
    editor.updateFileContent(PATH, 'typed after the tick\n');

    guardedSave.mockResolvedValueOnce({ ok: true, content_hash: 'h2' });
    await syncNow();

    expect(fileState(editor).baseHash).toBe('h7');
    expect(fileState(editor).content).toBe('typed after the tick\n');
  });

  it('a drained write refreshes a clean buffer\'s text', async () => {
    const editor = await openAndQueue();

    getFileContent.mockClear();
    guardedSave.mockResolvedValueOnce({ ok: true, content_hash: 'h2' });
    readHash = 'h2';
    getFileContent.mockResolvedValueOnce('queued text\n');
    await syncNow();

    await waitFor(() => expect(getFileContent).toHaveBeenCalledWith(PATH));
    await waitFor(() => expect(fileState(editor).content).toBe('queued text\n'));
    expect(fileState(editor).baseHash).toBe('h2');
    expect(fileState(editor).dirty).toBe(false);
  });

  it('a drained write refreshes no text the user typed during the read', async () => {
    const editor = await openAndQueue();

    getFileContent.mockClear();
    let answer: (text: string) => void = () => {};
    getFileContent.mockImplementationOnce(() => new Promise<string>((r) => (answer = r)));
    guardedSave.mockResolvedValueOnce({ ok: true, content_hash: 'h2' });
    readHash = 'h2';
    await syncNow();
    await waitFor(() => expect(getFileContent).toHaveBeenCalledWith(PATH));

    // The read is out. The user types before it returns.
    editor.updateFileContent(PATH, 'typed during the read\n');
    answer('queued text\n');
    await Promise.resolve();
    await Promise.resolve();

    expect(fileState(editor).content).toBe('typed during the read\n');
    expect(fileState(editor).dirty).toBe(true);
    expect(fileState(editor).baseHash).toBe('h2');
  });

  it('a drained write does not reset a base that moved during the read', async () => {
    const editor = await openAndQueue();

    getFileContent.mockClear();
    let answer: (text: string) => void = () => {};
    getFileContent.mockImplementationOnce(() => new Promise<string>((r) => (answer = r)));
    guardedSave.mockResolvedValueOnce({ ok: true, content_hash: 'h2' });
    readHash = 'h2';
    await syncNow();
    await waitFor(() => expect(getFileContent).toHaveBeenCalledWith(PATH));

    // The read is out. The user edits, and saves online before it returns.
    editor.updateFileContent(PATH, 'saved during the read\n');
    guardedSave.mockResolvedValueOnce({ ok: true, content_hash: 'h5' });
    await editor.saveFile(PATH);
    expect(fileState(editor).baseHash).toBe('h5');
    answer('queued text\n');
    // Let the read's whole chain settle before looking.
    await new Promise((r) => setTimeout(r, 0));

    expect(fileState(editor).content).toBe('saved during the read\n');
    expect(fileState(editor).baseHash, 'the read answered an older hash').toBe('h5');
    expect(fileState(editor).dirty).toBe(false);
  });

  it('a provider that unmounted hears no landed write', async () => {
    const editor = await openAndQueue();
    const { cleanup } = await import('@solidjs/testing-library');
    cleanup();

    guardedSave.mockResolvedValueOnce({ ok: true, content_hash: 'h2' });
    await syncNow();

    expect(fileState(editor).baseHash).toBe('base-hash');
  });
});

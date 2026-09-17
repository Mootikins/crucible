import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, waitFor } from '@solidjs/testing-library';
import type { FsEvent } from '@/lib/types';
import { FakeEventSource } from '@/test-utils/sse';

// get_note_by_name returns metadata only (no content), so the editor must load
// file bytes via GET /api/kiln/file. The read and the write answer as ROUTES
// below, scripted by the spies, so the assertions read what actually reached
// the daemon — which endpoint, which body, which base.
const getFileContent = vi.fn(async (_path: string) => '');
const saveFileContent = vi.fn(async (_path: string, _content: string) => {});
// The daemon's answer to a whole write. The default writes through the body
// spy, so the older tests below keep one place to read. A test that needs a
// stale or a queued answer overrides one call.
const guardedSave = vi.fn(async (p: string, c: string, _base: string, _baseText?: string) => {
  await saveFileContent(p, c);
  return { ok: true, content_hash: 'written' } as
    | { ok: true; content_hash: string; merged?: false }
    | { ok: true; content_hash: string; merged: true; content: string }
    | {
        ok: false;
        current_hash: string;
        current_content?: string;
        merged_content?: string;
        regions?: { start_line: number; end_line: number; base: string; ours: string; theirs: string }[];
      };
});
const addNotification = vi.fn();
/** The hash a read answers. A test that lands a write sets it to the landed hash. */
let readHash = 'base-hash';

const KILN = '/home/user/kiln';

// The moved helper (`lib/paths.ts`), stubbed where it lives now.
vi.mock('@/lib/paths', () => ({
  rawFileUrl: (p: string) => `/api/file/raw?path=${encodeURIComponent(p)}`,
}));

vi.mock('@/stores/notificationStore', () => ({
  notificationActions: { addNotification: (...a: unknown[]) => addNotification(...a) },
}));

// The conflict surface, mocked: the editor's job is to put the conflict where
// that surface reads it and to point at it. Drawing it is `ConflictsPanel`'s.
const openConflict = vi.fn();
const refreshConflicts = vi.fn(async () => {});
vi.mock('@/lib/conflicts', () => ({
  openConflict: (path?: string | null) => openConflict(path),
  conflictActions: { refresh: () => refreshConflicts() },
}));

const { createTestQueryEnv } = await import('@/test-utils/query');
const { resetKilnsForTests } = await import('@/lib/query/kilns');
const { resetSseForTests } = await import('@/lib/query/sse');
const { installFakeEventSource } = await import('@/test-utils/sse');

/** The one stream every watcher of the disk shares. */
const FS_STREAM = '/api/fs/events';
/** The watcher sources still open — a root that closed no longer hears. */
const openFsSources = () =>
  FakeEventSource.instances.filter((s) => s.url === FS_STREAM && !s.closed);
/** The disk spoke: one frame off the stream the panel and the editor share. */
const diskSays = (event: FsEvent) => {
  const source = openFsSources().at(-1);
  if (!source) throw new Error('no fs stream is open for the disk to speak on');
  source.emit(`fs_${event.type}`, event);
};

// The routes `kilnOf` resolves against and the offline layer reads and writes
// on, over the mocked fetch.
let kilnEnv: ReturnType<typeof createTestQueryEnv>;

beforeEach(() => {
  installFakeEventSource();
  resetKilnsForTests();
  kilnEnv = createTestQueryEnv({
    'GET /api/kilns': () => ({ kilns: [{ path: KILN, name: 'kiln' }] }),
    // The read: the bytes with the hash they were read at, so the buffer's
    // next save can name the base it edited from.
    'GET /api/kiln/file': async (request) => ({
      content: await getFileContent(new URL(request.url).searchParams.get('path')!),
      content_hash: readHash,
    }),
    // The whole write. The daemon compares the base INSIDE its write, so the
    // 409 arm carries the texts a refusal must hand back, and the merge arm
    // the text neither writer holds alone.
    'PUT /api/kiln/file': async (request) => {
      const body = (await request.clone().json()) as {
        path: string;
        content: string;
        base_hash?: string;
        base_text?: string;
      };
      if (body.base_hash === undefined) {
        // The blind write nothing here should issue; the spy keeps the claim
        // below checkable.
        await saveFileContent(body.path, body.content);
        return { ok: true, content_hash: 'written' };
      }
      const answer = await guardedSave(body.path, body.content, body.base_hash, body.base_text);
      if (answer.ok) {
        return 'merged' in answer && answer.merged
          ? { content_hash: answer.content_hash, merged: true, content: answer.content }
          : { content_hash: answer.content_hash };
      }
      return new Response(JSON.stringify(answer), {
        status: 409,
        headers: { 'Content-Type': 'application/json' },
      });
    },
    'GET /api/notes': () => ({ notes: [] }),
    'GET /api/config': () => ({ kiln_path: KILN, config_root: '/etc/crucible' }),
  });
});

afterEach(() => {
  kilnEnv.restore();
  resetKilnsForTests();
  // The editor and `FilesPanel` share one root in `lib/query/sse.ts`, and a
  // root outlives the test that opened it. Forget them between cases, so the
  // count of watcher subscriptions is this test's own.
  resetSseForTests();
});

const { EditorProvider, useEditor } = await import('../EditorContext');
const { pendingConflicts, setOfflineStore, syncNow } = await import('@/lib/offline/sync');
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
  });

  it('openFile loads the file bytes from GET /api/kiln/file, and only from it', async () => {
    const path = `${KILN}/notes/from-tui.md`;
    getFileContent.mockResolvedValueOnce('terminal was here\n');

    let editor: ReturnType<typeof useEditor>;
    editor = withEditor(() => {});
    await editor!.openFile(path);

    // One read of the file route, by the path the editor was asked for —
    // not a metadata lookup, not a second fetch of anything else.
    expect(getFileContent).toHaveBeenCalledTimes(1);
    expect(getFileContent).toHaveBeenCalledWith(path);

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
    openConflict.mockClear();
    refreshConflicts.mockClear();
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

  // A refusal the route could not merge. Nothing is written, the buffer keeps
  // the user's text, and the writing waits where a person settles it.
  it('a stale desktop save with regions opens the conflict view and keeps the buffer dirty', async () => {
    guardedSave.mockResolvedValueOnce({
      ok: false,
      current_hash: 'h9',
      current_content: 'their text',
      merged_content: 'merged, with a region',
      regions: [{ start_line: 1, end_line: 2, base: 'on disk\n', ours: 'the unsaved text', theirs: 'their text' }],
    });
    const editor = await openEdited();

    await editor.saveFile(PATH);

    expect(fileState(editor).dirty).toBe(true);
    expect(fileState(editor).content).toBe('the unsaved text');
    expect(fileState(editor).baseHash, 'a refused write moves no base').toBe('base-hash');
    expect(saveFileContent, 'nothing is written without a choice').not.toHaveBeenCalled();

    // The conflict is in the outbox, where every conflict surface reads it.
    const waiting = await pendingConflicts();
    expect(waiting).toHaveLength(1);
    expect(waiting[0]).toMatchObject({
      path: PATH,
      currentHash: 'h9',
      currentContent: 'their text',
      mergedContent: 'merged, with a region',
    });
    expect(waiting[0].regions).toHaveLength(1);

    // And the view is opened on it, after the store has been told to re-read.
    await waitFor(() => expect(openConflict).toHaveBeenCalledWith(PATH));
    expect(refreshConflicts).toHaveBeenCalled();
    // A stale save is an answer, not a failure: the red retry line stays off.
    expect(editor.error()).toBeNull();
    expect(editor.retryFailedOperation()).toBeNull();
  });

  // The route merged our text with the disk. What it wrote is neither text
  // alone, and this buffer is the only place it would otherwise be lost.
  it('a stale desktop save with a clean merge lands and moves the base', async () => {
    guardedSave.mockResolvedValueOnce({
      ok: true,
      content_hash: 'h5',
      merged: true,
      content: 'merged text',
    });
    const editor = await openEdited();

    await editor.saveFile(PATH);

    await waitFor(() => expect(fileState(editor).dirty).toBe(false));
    expect(fileState(editor).content).toBe('merged text');
    expect(fileState(editor).baseHash).toBe('h5');
    expect(await pendingConflicts(), 'a merge that landed is no conflict').toHaveLength(0);

    // The next save is made FROM what the daemon wrote, not from our half.
    guardedSave.mockResolvedValueOnce({ ok: true, content_hash: 'h6' });
    editor.updateFileContent(PATH, 'merged text, edited');
    await editor.saveFile(PATH);
    expect(guardedSave).toHaveBeenLastCalledWith(PATH, 'merged text, edited', 'h5', 'merged text');
  });

  // Bytes typed while the save was out are newer than the answer the daemon
  // gave, and they exist nowhere else. A merge that overwrote them would lose
  // them silently, with the buffer marked clean.
  it('a merge that lands while the user types keeps the newer text and stays dirty', async () => {
    let release: () => void = () => {};
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    const editor = await openEdited();
    guardedSave.mockImplementationOnce(async () => {
      await held;
      return { ok: true, content_hash: 'h5', merged: true, content: 'merged text' };
    });

    const saving = editor.saveFile(PATH);
    editor.updateFileContent(PATH, 'the unsaved text, and more');
    release();
    await saving;

    expect(fileState(editor).content).toBe('the unsaved text, and more');
    expect(fileState(editor).dirty, 'the newer bytes still owe a save').toBe(true);
    // The merge is on disk and in no buffer, so the user still has a choice
    // to make and the banner stands.
    expect(fileState(editor).changedOnDisk).toBe(true);
    // The base pair stays where this save was made from. Advancing it to the
    // merge would make the next save NOT stale, and the route writes a
    // non-stale body verbatim — the other writer's lines would go with no
    // refusal and no notice.
    expect(fileState(editor).baseHash).toBe('base-hash');
    expect(fileState(editor).baseText).toBe('on disk\n');
  });

  // The gate on the overwrite: the save that follows must reach the wire with
  // a base the daemon treats as stale, so it merges instead of writing over.
  it('the save after such a merge carries the pre-save base, so the daemon merges again', async () => {
    let release: () => void = () => {};
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    const editor = await openEdited();
    guardedSave.mockImplementationOnce(async () => {
      await held;
      return { ok: true, content_hash: 'h5', merged: true, content: 'merged text' };
    });

    const saving = editor.saveFile(PATH);
    editor.updateFileContent(PATH, 'the unsaved text, and more');
    release();
    await saving;

    guardedSave.mockResolvedValueOnce({ ok: true, content_hash: 'h6' });
    await editor.saveFile(PATH);

    expect(guardedSave).toHaveBeenLastCalledWith(
      PATH,
      'the unsaved text, and more',
      'base-hash',
      'on disk\n',
    );
  });

  // The pair is one fact. Without the text, a stale save can only be refused.
  it('a buffer opened from a read carries its text as the base text', async () => {
    getFileContent.mockResolvedValueOnce('on disk\n');
    const editor = withEditor(() => {});
    await editor.openFile(PATH);
    await waitFor(() => expect(editor.openFiles().length).toBe(1));

    expect(fileState(editor).baseText).toBe('on disk\n');

    guardedSave.mockResolvedValueOnce({ ok: true, content_hash: 'h2' });
    editor.updateFileContent(PATH, 'typed');
    await editor.saveFile(PATH);
    expect(guardedSave).toHaveBeenLastCalledWith(PATH, 'typed', 'base-hash', 'on disk\n');
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
    expect(guardedSave).toHaveBeenLastCalledWith(PATH, 'the unsaved text, again', 'h2', 'the unsaved text');
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
    // A base moved with no text beside it carries no base text: a hash and a
    // text that do not belong together is what the route refuses as a caller bug.
    expect(guardedSave).toHaveBeenLastCalledWith(PATH, 'the unsaved text', 'h2', undefined);
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
    expect(guardedSave).toHaveBeenLastCalledWith(PATH, 'queued text, and more\n', 'h2', undefined);
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

  it('a merged drain leaves a dirty buffer on its original base', async () => {
    const editor = await openAndQueue();
    const originalBase = fileState(editor).baseHash;
    const originalText = fileState(editor).baseText;
    editor.updateFileContent(PATH, 'newer local typing\n');
    guardedSave.mockResolvedValueOnce({ ok: false, current_hash: 'remote' });
    guardedSave.mockResolvedValueOnce({ ok: true, content_hash: 'merged', merged: true, content: 'queued text\nremote\n' });
    await syncNow();
    expect(fileState(editor).baseHash).toBe(originalBase);
    expect(fileState(editor).baseText).toBe(originalText);
    expect(fileState(editor).content).toBe('newer local typing\n');
    expect(fileState(editor).changedOnDisk).toBe(true);
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
    // The read answered a text and a hash that belong together, so the buffer
    // is mergeable again: the landed write moved the hash on its own first.
    // `waitFor`, not a bare read: the buffer already showed this text before
    // the drain, so only the base moving proves the READ landed.
    await waitFor(() => expect(fileState(editor).baseText).toBe('queued text\n'));
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

  /**
   * The daemon refuses the queued write, then refuses the merge the drain
   * retries with the entry's base text, naming the span it could not settle.
   */
  const conflictOnDrain = () => {
    guardedSave.mockResolvedValueOnce({ ok: false, current_hash: 'h9' });
    guardedSave.mockResolvedValueOnce({
      ok: false,
      current_hash: 'h9',
      current_content: 'their text\n',
      merged_content: 'queued text\n',
      regions: [
        { start_line: 1, end_line: 2, base: 'on disk\n', ours: 'queued text\n', theirs: 'their text\n' },
      ],
    });
  };

  // The queued write went clean at queue time. The drain could not merge it,
  // so the buffer shows text the note does not hold. A clean buffer here is a
  // lie the user acts on, and the notice must say where the text waits.
  it('a drained conflict marks the open buffer and names the conflict', async () => {
    const editor = await openAndQueue();
    conflictOnDrain();

    const result = await syncNow();

    expect(result.conflicted).toHaveLength(1);
    expect(saveFileContent, 'nothing is written beside the note').not.toHaveBeenCalled();
    expect(fileState(editor).dirty).toBe(true);
    expect(fileState(editor).content, 'the buffer keeps the user\'s text').toBe('queued text\n');
    expect(fileState(editor).baseHash, 'a refused write moves no base').toBe('base-hash');
    expect(addNotification).toHaveBeenCalledTimes(1);
    expect(addNotification).toHaveBeenCalledWith(
      'warning',
      'The note changed elsewhere while you were offline. Your text is kept as a conflict — open Conflicts to resolve it.',
    );
  });

  it('a buffer with another base is left alone', async () => {
    const editor = await openAndQueue();
    // A tick landed online meanwhile and moved this buffer's base on its own.
    editor.setBaseHash(PATH, 'h7');
    conflictOnDrain();

    await syncNow();

    expect(fileState(editor).dirty).toBe(false);
    expect(fileState(editor).baseHash).toBe('h7');
    // The drain's own notice still points at the conflict: nothing else does.
    expect(addNotification).toHaveBeenCalledTimes(1);
    expect(addNotification).toHaveBeenCalledWith(
      'warning',
      'The note changed elsewhere. Open Conflicts to resolve it.',
    );
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

/**
 * The kiln watcher says a note moved on disk while it is open here.
 *
 * A clean buffer has nothing of the user's in it, so it takes the new text:
 * showing yesterday's bytes of a note somebody else just wrote is a lie the
 * user edits from. A dirty buffer keeps the user's text — it exists nowhere
 * else — and raises a flag the panel draws as a banner, because only the user
 * can say whether their text or the disk's wins.
 */
describe('EditorContext — an open buffer hears the kiln watcher', () => {
  const PATH = `${KILN}/notes/watched.md`;
  const CHANGED: FsEvent = { type: 'changed', path: PATH, kind: 'modified' };
  const fileState = (editor: ReturnType<typeof useEditor>) =>
    editor.openFiles().find((f) => f.path === PATH)!;

  beforeEach(() => {
    getFileContent.mockClear();
    guardedSave.mockClear();
    addNotification.mockClear();
    installFakeEventSource();
    readHash = 'base-hash';
    setOfflineStore(memoryStore());
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  const openClean = async () => {
    getFileContent.mockResolvedValueOnce('on disk\n');
    const editor = withEditor(() => {});
    await editor.openFile(PATH);
    await waitFor(() => expect(editor.openFiles().length).toBe(1));
    await waitFor(() => expect(openFsSources()).toHaveLength(1));
    return editor;
  };

  // One stream, and only while there is a buffer it could speak about.
  it('listens to the watcher only while a file is open', async () => {
    const editor = withEditor(() => {});
    expect(FakeEventSource.instances.filter((s) => s.url === FS_STREAM)).toHaveLength(0);

    getFileContent.mockResolvedValueOnce('on disk\n');
    await editor.openFile(PATH);
    await waitFor(() => expect(openFsSources()).toHaveLength(1));

    editor.closeFile(PATH, { force: true });
    await waitFor(() => expect(openFsSources()).toHaveLength(0));
  });

  it('a clean buffer takes the disk text and the new base', async () => {
    const editor = await openClean();

    readHash = 'h9';
    getFileContent.mockResolvedValueOnce('another writer was here\n');
    diskSays(CHANGED);

    await waitFor(() => expect(fileState(editor).content).toBe('another writer was here\n'));
    expect(fileState(editor).baseHash).toBe('h9');
    // Text and hash arrive together, so the buffer stays mergeable.
    expect(fileState(editor).baseText).toBe('another writer was here\n');
    expect(fileState(editor).dirty).toBe(false);
    expect(fileState(editor).changedOnDisk, 'nothing is left to tell the user').toBeFalsy();
  });

  it('a dirty buffer keeps its text and says the disk moved', async () => {
    const editor = await openClean();
    editor.updateFileContent(PATH, 'my unsent text\n');
    getFileContent.mockClear();

    diskSays(CHANGED);

    await waitFor(() => expect(fileState(editor).changedOnDisk).toBe(true));
    expect(fileState(editor).content).toBe('my unsent text\n');
    expect(fileState(editor).dirty).toBe(true);
    expect(fileState(editor).baseHash, 'the base is what a merge is made from').toBe('base-hash');
    expect(getFileContent, 'no read behind the user\'s back').not.toHaveBeenCalled();
  });

  it('a note moved out from under a dirty buffer says the disk moved', async () => {
    const editor = await openClean();
    editor.updateFileContent(PATH, 'my unsent text\n');

    diskSays({ type: 'moved', from: PATH, to: `${KILN}/notes/renamed.md` });

    await waitFor(() => expect(fileState(editor).changedOnDisk).toBe(true));
    expect(fileState(editor).content).toBe('my unsent text\n');
  });

  // A read that cannot answer leaves the buffer stale, so the user is told
  // rather than left reading bytes the note no longer holds.
  it('a clean buffer whose re-read fails says the disk moved', async () => {
    const editor = await openClean();
    getFileContent.mockRejectedValueOnce(new Error('gone'));

    diskSays(CHANGED);

    await waitFor(() => expect(fileState(editor).changedOnDisk).toBe(true));
    expect(fileState(editor).content).toBe('on disk\n');
  });

  it('a path no buffer holds is left alone', async () => {
    const editor = await openClean();
    getFileContent.mockClear();

    diskSays({ type: 'changed', path: `${KILN}/notes/other.md`, kind: 'modified' });
    await Promise.resolve();
    await Promise.resolve();

    expect(getFileContent).not.toHaveBeenCalled();
    expect(fileState(editor).content).toBe('on disk\n');
    expect(fileState(editor).changedOnDisk).toBeFalsy();
  });

  it('reload takes the disk text and the new base', async () => {
    const editor = await openClean();
    editor.updateFileContent(PATH, 'my unsent text\n');
    diskSays(CHANGED);
    await waitFor(() => expect(fileState(editor).changedOnDisk).toBe(true));

    vi.spyOn(window, 'confirm').mockReturnValue(true);
    readHash = 'h9';
    getFileContent.mockResolvedValueOnce('their text\n');
    await editor.reloadFile(PATH);

    expect(fileState(editor).content).toBe('their text\n');
    expect(fileState(editor).baseHash).toBe('h9');
    expect(fileState(editor).baseText).toBe('their text\n');
    expect(fileState(editor).dirty).toBe(false);
    expect(fileState(editor).changedOnDisk).toBeFalsy();
  });

  // Reload over a dirty buffer discards bytes that exist nowhere else. The
  // same guard the close path carries (bug 6) applies to the same loss.
  it('reload asks before it discards unsaved edits', async () => {
    const editor = await openClean();
    editor.updateFileContent(PATH, 'my unsent text\n');
    diskSays(CHANGED);
    await waitFor(() => expect(fileState(editor).changedOnDisk).toBe(true));

    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false);
    getFileContent.mockClear();
    await editor.reloadFile(PATH);

    expect(confirm).toHaveBeenCalledOnce();
    expect(getFileContent).not.toHaveBeenCalled();
    expect(fileState(editor).content).toBe('my unsent text\n');
    expect(fileState(editor).dirty).toBe(true);
    expect(fileState(editor).changedOnDisk, 'the disk is still ahead').toBe(true);
  });

  // A buffer goes clean when its write QUEUES, but that writing exists nowhere
  // but the outbox. The quiet re-read would replace it with the other writer's
  // text, and nothing on screen would say the user's own words had gone.
  it('a buffer whose writing is still queued keeps it and says the disk moved', async () => {
    const editor = await openClean();
    editor.updateFileContent(PATH, 'my unsent text\n');
    guardedSave.mockRejectedValueOnce(new TypeError('Failed to fetch'));
    await editor.saveFile(PATH);
    await waitFor(() => expect(fileState(editor).dirty).toBe(false));

    getFileContent.mockClear();
    diskSays(CHANGED);

    await waitFor(() => expect(fileState(editor).changedOnDisk).toBe(true));
    expect(getFileContent, 'no read over writing the daemon has not received').not.toHaveBeenCalled();
    expect(
      fileState(editor).content,
      'the queued writing is still what this device owes the daemon',
    ).toBe('my unsent text\n');
    expect(fileState(editor).baseHash, 'the base the queued write carries').toBe('base-hash');
  });

  // A read that cannot reach the daemon answers the MIRROR for a kept kiln.
  // The mirror is not the disk, so taking it leaves the buffer showing text
  // the note no longer holds — silently, which is what the flag exists against.
  it('a re-read answered from the mirror says the disk moved', async () => {
    const db = memoryStore();
    setOfflineStore(db);
    const editor = await openClean();
    await db.put('mirror', PATH, {
      body: 'stale mirror\n',
      hash: 'mirror-hash',
      kiln: KILN,
      mirroredAt: Date.now(),
    });

    getFileContent.mockRejectedValueOnce(new Error('offline'));
    diskSays(CHANGED);

    await waitFor(() =>
      expect(
        fileState(editor).changedOnDisk,
        'the disk moved and this buffer did not follow it',
      ).toBe(true),
    );
    expect(fileState(editor).content).toBe('on disk\n');
    expect(fileState(editor).baseHash).toBe('base-hash');
  });

  // A browser with no IndexedDB (a private window) cannot say whether the
  // outbox holds writing for this path. The banner is the safe answer: it
  // loses nothing, while a quiet re-read over unsent writing would.
  it('a store that cannot be read says the disk moved rather than re-reading', async () => {
    const editor = await openClean();
    const broken = memoryStore();
    broken.get = () => Promise.reject(new Error('no IndexedDB on this browser'));
    setOfflineStore(broken);

    getFileContent.mockClear();
    diskSays(CHANGED);

    await waitFor(() =>
      expect(
        fileState(editor).changedOnDisk,
        'the store could not be asked, so the user is told',
      ).toBe(true),
    );
    expect(getFileContent, 'no read over writing that cannot be ruled out').not.toHaveBeenCalled();
    expect(fileState(editor).content).toBe('on disk\n');
  });

  // The merge landed on disk while the user typed on. Their newer bytes stay,
  // so the merged text is on disk and in no buffer: the flag keeps saying so,
  // and Reload is there to take it.
  it('a merge the buffer typed past leaves the disk-changed banner standing', async () => {
    const editor = await openClean();
    editor.updateFileContent(PATH, 'mine\n');

    type SaveAnswer = Awaited<ReturnType<typeof guardedSave>>;
    let answer: (value: SaveAnswer) => void = () => {};
    guardedSave.mockReturnValueOnce(
      new Promise<SaveAnswer>((resolve) => {
        answer = resolve;
      }),
    );
    const saving = editor.saveFile(PATH);
    editor.updateFileContent(PATH, 'mine, and more\n');
    answer({ ok: true, content_hash: 'hM', merged: true, content: 'mine\ntheirs\n' });
    await saving;

    expect(fileState(editor).content, 'the newer bytes are the only copy there is').toBe(
      'mine, and more\n',
    );
    expect(fileState(editor).dirty).toBe(true);
    expect(
      fileState(editor).changedOnDisk,
      'the merge is on disk and in no buffer, so the user still has a choice',
    ).toBe(true);
  });

  // Merge is the ordinary save. It carries the base text, so the route merges
  // our text against what the other writer left instead of refusing it.
  it('merge saves with the buffer\'s base text and clears the flag', async () => {
    const editor = await openClean();
    editor.updateFileContent(PATH, 'my unsent text\n');
    diskSays(CHANGED);
    await waitFor(() => expect(fileState(editor).changedOnDisk).toBe(true));

    guardedSave.mockResolvedValueOnce({
      ok: true,
      content_hash: 'h5',
      merged: true,
      content: 'both texts\n',
    });
    await editor.saveFile(PATH);

    expect(guardedSave).toHaveBeenLastCalledWith(PATH, 'my unsent text\n', 'base-hash', 'on disk\n');
    await waitFor(() => expect(fileState(editor).changedOnDisk).toBeFalsy());
    expect(fileState(editor).content).toBe('both texts\n');
    expect(fileState(editor).dirty).toBe(false);
  });
});

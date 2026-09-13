import {
  createContext,
  useContext,
  ParentComponent,
  createSignal,
  onCleanup,
} from 'solid-js';
import { createStore, produce } from 'solid-js/store';
import type { EditorFile } from '@/lib/types';
import type { EditorContextValue } from '@/lib/types/context';
import { listKilns } from '@/lib/api';
import { kilnForPath } from '@/lib/note-actions';
import { onNoteLanded, readNote, writeConflictCopy, writeNote, type Landed } from '@/lib/offline/sync';
import { notificationActions } from '@/stores/notificationStore';


const EditorContext = createContext<EditorContextValue>();

export const EditorProvider: ParentComponent = (props) => {
  const [openFilesStore, setOpenFiles] = createStore<EditorFile[]>([]);
  const [activeFile, setActiveFileSignal] = createSignal<string | null>(null);
  const [isLoading, setIsLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  /**
   * The call that produced `error()`, ready to be re-issued.
   *
   * A failed save used to strand a dirty buffer behind a red line with no way
   * out but Ctrl+S again — and nothing on screen said that Ctrl+S was the way
   * out. The banner needs the ACTUAL failed call, not a guess: the panel knows
   * the active file, but the failure may belong to a background open of a
   * different one.
   *
   * A setter given a function treats it as an updater, so the stored closure
   * is wrapped one level deeper on every write.
   */
  const [retryFailedOperation, setRetryFailedOperation] =
    createSignal<(() => Promise<void>) | null>(null);

    // Number of live panels holding each open path. A file's edited buffer
    // lives only here, so it must survive a panel unmount+remount (tab move or
    // pop-out) — we only evict when the last holder releases it.
    const openCounts = new Map<string, number>();

  // `background` opens the buffer WITHOUT making it the active file —
  // transient surfaces (wikilink hover windows) must not steal focus, or
  // everything keyed on activeFile (backlinks panel) flickers per hover.
  /** Which kiln owns a path, for the offline layer. Cached: the roster is
   * small and this runs on every open and save. */
  let kilnRoster: { path: string }[] | null = null;
  const kilnOf = async (path: string): Promise<string | null> => {
    if (!kilnRoster) kilnRoster = await listKilns().catch(() => []);
    return kilnForPath(path, kilnRoster) ?? null;
  };

  const openFile = async (path: string, opts?: { background?: boolean }) => {
    const existing = openFilesStore.find((f) => f.path === path);
    if (existing) {
      // Already open — take another reference and reuse the (possibly dirty)
      // buffer instead of re-reading disk and clobbering unsaved edits.
      openCounts.set(path, (openCounts.get(path) ?? 0) + 1);
      if (!opts?.background) setActiveFileSignal(path);
      return;
    }

    setIsLoading(true);
    setError(null);
    setRetryFailedOperation(null);

    try {
      // Load the raw file bytes from disk. get_note_by_name returns metadata
      // only (no content), so the note endpoint can't hydrate the editor —
      // GET /api/kiln/file reads the file itself and is the source of truth.
      //
      // Through the offline layer: the network when it answers, and the copy
      // this device keeps when it does not. It also carries the hash the file
      // was read at, which is what an offline save is anchored on.
      const { content, content_hash } = await readNote(path, await kilnOf(path));

      setOpenFiles(
        produce((files) => {
          files.push({ path, content, dirty: false, baseHash: content_hash });
        })
      );
      openCounts.set(path, 1);
      if (!opts?.background) setActiveFileSignal(path);
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Failed to open file';
      setError(msg);
      setRetryFailedOperation(() => () => openFile(path, opts));
      console.error('Failed to open file:', err);
    } finally {
      setIsLoading(false);
    }
  };

  const evictFile = (path: string, force?: boolean) => {
    const idx = openFilesStore.findIndex((f) => f.path === path);
    if (idx === -1) return;

    // Data-loss guard (bug 6): closing a dirty file must not silently discard
    // edits. `force` skips the prompt for callers whose close was already
    // confirmed upstream (e.g. a window tab close vetted by confirmTabClose).
    if (openFilesStore[idx].dirty && !force) {
      const filename = path.split('/').pop() ?? path;
      if (!window.confirm(`Discard unsaved changes to ${filename}?`)) return;
    }

    setOpenFiles(produce((files) => files.splice(idx, 1)));

    if (activeFile() === path) {
      const remaining = openFilesStore.filter((f) => f.path !== path);
      if (remaining.length > 0) {
        const newIdx = Math.min(idx, remaining.length - 1);
        setActiveFileSignal(remaining[newIdx].path);
      } else {
        setActiveFileSignal(null);
      }
    }
  };

  const closeFile = (path: string, opts?: { force?: boolean }) => {
    const remaining = (openCounts.get(path) ?? 1) - 1;
    if (remaining > 0) {
      // Another panel still holds this file open (e.g. mid tab-move). Keep the
      // buffer; just drop this reference.
      openCounts.set(path, remaining);
      return;
    }
    openCounts.set(path, 0);

    // Defer the actual eviction: a tab move / pop-out unmounts the source panel
    // and remounts a new one for the same path in the same tick. If openFile
    // re-takes a reference before this runs, we must NOT evict (that remount
    // would otherwise re-read disk and lose unsaved edits).
    queueMicrotask(() => {
      if ((openCounts.get(path) ?? 0) > 0) return;
      openCounts.delete(path);
      evictFile(path, opts?.force);
    });
  };

  /**
   * Keep the text the daemon refused beside the note it refused to overwrite.
   *
   * The caller captures `refused` at the moment of the refusal, not at the
   * moment the user chooses. Between the two, the user may close the note
   * and open it again: the buffer then holds the server text, and a copy of
   * that text is not "your version". Text typed after the refusal stays in
   * the dirty buffer on screen, so the snapshot loses nothing, and the
   * snapshot is what the refusal was about.
   */
  const keepAsConflictCopy = (path: string, refused: string) => {
    writeConflictCopy(path, refused, new Date())
      .then((copy) =>
        notificationActions.addNotification(
          'success',
          `Your version was saved as ${copy.split('/').pop()}. Reload the note to continue from the current text.`,
        ),
      )
      .catch((err: unknown) =>
        notificationActions.addNotification(
          'error',
          err instanceof Error ? err.message : 'Failed to save the conflict copy',
        ),
      );
  };

  const saveFile = async (path: string) => {
    const file = openFilesStore.find((f) => f.path === path);
    if (!file) return;

    setIsLoading(true);
    setError(null);
    setRetryFailedOperation(null);

    try {
      // Save by absolute path (symmetric with the load) — the editor addresses
      // files by path, and PUT /api/kiln/file writes within the open kiln.
      const outcome = await writeNote({
        path,
        body: file.content,
        base: file.baseHash,
        kiln: await kilnOf(path),
      });

      if (!outcome.queued && outcome.stale) {
        // The daemon answered: the note moved on since this buffer was read.
        // The buffer keeps its text and stays dirty, and nothing is written.
        // The user is present, so the copy is a choice and not an automatic
        // write: they may prefer to reload and re-apply their change. The
        // choice copies the text the daemon refused, whatever the buffer
        // holds when the user clicks.
        const refused = file.content;
        notificationActions.addNotification(
          'warning',
          'The note changed elsewhere. Reload it to see the current text, or keep yours as a copy.',
          { label: 'Save as conflict copy', run: () => keepAsConflictCopy(path, refused) },
        );
        return;
      }

      setOpenFiles(
        produce((files) => {
          const f = files.find((x) => x.path === path);
          if (!f) return;
          // A save the daemon cannot take is QUEUED, not lost: the buffer
          // goes clean because the writing is safe in the outbox, and the
          // app bar says how much is still owed. It keeps its base, which is
          // the base the queued write carries.
          f.dirty = false;
          // The daemon now holds this text under the hash it answered with.
          // Without this, the next save would carry the base of the FIRST
          // read and the daemon would refuse it as stale, by construction.
          if (!outcome.queued) f.baseHash = outcome.hash;
        })
      );
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Failed to save file';
      setError(msg);
      // The buffer is still dirty and still holds the user's bytes, so the
      // recovery is the SAME save — never a reload, which would drop them.
      setRetryFailedOperation(() => () => saveFile(path));
      console.error('Failed to save file:', err);
    } finally {
      setIsLoading(false);
    }
  };

  const setActiveFile = (path: string) => {
    const exists = openFilesStore.some((f) => f.path === path);
    if (exists) {
      setActiveFileSignal(path);
    }
  };

  const updateFileContent = (path: string, content: string) => {
    setOpenFiles(
      produce((files) => {
        const f = files.find((x) => x.path === path);
        if (f) {
          f.content = content;
          f.dirty = true;
        }
      })
    );
  };

  const setBaseHash = (path: string, hash: string) => {
    setOpenFiles(
      produce((files) => {
        const f = files.find((x) => x.path === path);
        if (f) f.baseHash = hash;
      })
    );
  };

  /**
   * A queued write landed while its note stays open.
   *
   * The daemon's hash moved, and the buffer's base did not, so the next save
   * would be refused as stale for the user's own queued write. The buffer
   * whose base the write was made from takes the answered hash. A buffer with
   * another base was moved by something else, and the queued write did not
   * come from it, so it is left alone.
   *
   * A clean buffer also takes the text the daemon holds now, so a landed
   * tick shows. The read answers a text and a hash that belong together, so
   * both are taken from it. A buffer the user typed into during the read
   * keeps its text: their bytes exist nowhere else. A buffer whose base
   * moved during the read was saved by a later write, and keeps that base.
   */
  const onLanded = (row: Landed) => {
    const file = openFilesStore.find((f) => f.path === row.path && f.baseHash === row.base);
    if (!file) return;
    setBaseHash(row.path, row.hash);
    if (file.dirty) return;
    void kilnOf(row.path)
      .then((kiln) => readNote(row.path, kiln))
      .then(({ content, content_hash }) => {
        setOpenFiles(
          produce((files) => {
            const f = files.find((x) => x.path === row.path);
            // A base that moved during the read belongs to a later write.
            if (!f || f.dirty || f.baseHash !== row.hash) return;
            f.content = content;
            f.baseHash = content_hash;
          }),
        );
      })
      .catch(() => {
        // The base already moved. The text refreshes on the next open.
      });
  };
  onCleanup(onNoteLanded(onLanded));

  const value: EditorContextValue = {
    openFiles: () => openFilesStore,
    activeFile,
    openFile,
    closeFile,
    saveFile,
    setActiveFile,
    updateFileContent,
    setBaseHash,
    isLoading,
    error,
    retryFailedOperation,
  };

  return (
    <EditorContext.Provider value={value}>
      {props.children}
    </EditorContext.Provider>
  );
};

export function useEditor(): EditorContextValue {
  const context = useContext(EditorContext);
  if (!context) {
    throw new Error('useEditor must be used within an EditorProvider');
  }
  return context;
}

const noopAsync = async () => {};

const fallbackEditorContext: EditorContextValue = {
  openFiles: () => [],
  activeFile: () => null,
  openFile: noopAsync,
  closeFile: () => {},
  saveFile: noopAsync,
  setActiveFile: () => {},
  updateFileContent: () => {},
  setBaseHash: () => {},
  isLoading: () => false,
  error: () => null,
  retryFailedOperation: () => null,
};

export function useEditorSafe(): EditorContextValue {
  const context = useContext(EditorContext);
  return context ?? fallbackEditorContext;
}

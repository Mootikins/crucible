import {
  createContext,
  useContext,
  ParentComponent,
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
} from 'solid-js';
import { createStore, produce } from 'solid-js/store';
import type { EditorFile, FsEvent } from '@/lib/types';
import type { EditorContextValue } from '@/lib/types/context';
import { fsEvents } from '@/lib/query/sse';
import { fetchKilnsOnce } from '@/lib/query/kilns';
import { kilnForPath } from '@/lib/note-actions';
import {
  hasQueuedWriting,
  onNoteConflicted,
  onNoteLanded,
  readNote,
  writeNote,
  type Conflicted,
  type Landed,
} from '@/lib/offline/sync';
import { conflictActions, openConflict } from '@/lib/conflicts';
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
  /** Which kiln owns a path, for the offline layer. The shared query holds the
   * roster, which this runs against on every open and save. */
  const kilnOf = async (path: string): Promise<string | null> => {
    const roster = await fetchKilnsOnce().catch(() => []);
    return kilnForPath(path, roster) ?? null;
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
      const { content, content_hash, baseText } = await readNote(path, await kilnOf(path));

      setOpenFiles(
        produce((files) => {
          // The text comes with the hash: the two are one fact, and this is
          // the only copy of the text a later merge can be made from.
          files.push({ path, content, dirty: false, baseHash: content_hash, baseText: baseText ?? content });
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

  const saveFile = async (path: string) => {
    const file = openFilesStore.find((f) => f.path === path);
    if (!file) return;

    setIsLoading(true);
    setError(null);
    setRetryFailedOperation(null);

    // The bytes this save is about. The buffer is a store proxy, so reading it
    // again after the await would be a different text.
    const sent = file.content;

    try {
      // Save by absolute path (symmetric with the load) — the editor addresses
      // files by path, and PUT /api/kiln/file writes within the open kiln.
      //
      // The base text goes with the base hash: it is what lets the route MERGE
      // a note someone else changed meanwhile, instead of refusing the save and
      // leaving the user to reconcile two texts by hand.
      const outcome = await writeNote({
        path,
        body: sent,
        base: file.baseHash,
        baseText: file.baseText,
        kiln: await kilnOf(path),
      });

      if (!outcome.queued && outcome.stale) {
        // The daemon answered: the note moved on since this buffer was read.
        // The buffer keeps its text and stays dirty, and nothing is written.
        if (outcome.conflict) {
          // The route merged as far as it could and a region was left. The
          // writing is held in the outbox now, so the choice is made on the
          // conflict surface, over both texts — not in a toast that copies
          // the note aside and leaves two files to reconcile.
          await conflictActions.refresh();
          openConflict(path);
          notificationActions.addNotification(
            'warning',
            'The note changed elsewhere. Choose between the two texts to finish the save.',
          );
          return;
        }
        notificationActions.addNotification(
          'warning',
          'The note changed elsewhere. Reload it to see the current text.',
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
          // Bytes typed while the save was out are newer than this answer and
          // exist nowhere else: keep them, and stay dirty so the next save
          // carries them.
          const stillSent = f.content === sent;
          f.dirty = !stillSent;
          if (outcome.queued) return;
          // The daemon MERGED, and the buffer typed past what was sent. The
          // merged text is on disk and in no buffer, and the newer bytes
          // descend from the pre-save base alone. Advancing the base pair to
          // the merge would make the next save NOT stale, so the route would
          // write these bytes verbatim over the other writer's lines with no
          // refusal and no notice. Keeping the pre-save pair leaves that save
          // stale on purpose, so the daemon merges against the merge instead.
          if (outcome.merged && !stillSent) {
            f.changedOnDisk = true;
            return;
          }
          // The daemon now holds this text under the hash it answered with.
          // Without this, the next save would carry the base of the FIRST
          // read and the daemon would refuse it as stale. That refusal is
          // wanted only in the merged-and-typed-past case handled above.
          f.baseHash = outcome.hash;
          // What the daemon holds under that hash: the text it merged, when
          // it merged, and otherwise the text we sent. A merged note is
          // neither writer's alone and exists nowhere else, so the buffer
          // takes it — a buffer left showing our half would send that half
          // back over the merge on the next save.
          if (outcome.merged && stillSent) f.content = outcome.content;
          f.baseText = outcome.merged ? outcome.content : sent;
          // The note on disk is what this buffer holds, so there is nothing
          // left to choose between. The merged-and-typed-past case returned
          // above, with the banner up.
          f.changedOnDisk = false;
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

  const setBaseHash = (path: string, hash: string, text?: string) => {
    setOpenFiles(
      produce((files) => {
        const f = files.find((x) => x.path === path);
        if (!f) return;
        f.baseHash = hash;
        // The pair moves together or not at all. A caller that does not know
        // the text at the new hash leaves the buffer with a hash alone, and
        // the next save is refused rather than merged — which is the honest
        // answer: a base text that is not the text of the base is a caller
        // bug the route refuses outright.
        f.baseText = text;
      })
    );
  };

  /**
   * Take the note as the disk holds it now.
   *
   * The banner's Reload, and the quiet path of the watcher below. A dirty
   * buffer is ASKED first: its bytes exist nowhere else, and a button beside
   * "This note changed on disk" is not consent to drop them — the same loss
   * the close path guards against.
   */
  const reloadFile = async (path: string) => {
    const file = openFilesStore.find((f) => f.path === path);
    if (!file) return;
    if (file.dirty) {
      const filename = path.split('/').pop() ?? path;
      if (!window.confirm(`Discard unsaved changes to ${filename}?`)) return;
    }

    setIsLoading(true);
    setError(null);
    setRetryFailedOperation(null);

    try {
      const { content, content_hash, baseText } = await readNote(path, await kilnOf(path));
      setOpenFiles(
        produce((files) => {
          const f = files.find((x) => x.path === path);
          if (!f) return;
          f.content = content;
          f.dirty = false;
          // The read answers a text and a hash that belong together, so the
          // buffer is mergeable again from the text the other writer left.
          f.baseHash = content_hash;
          f.baseText = baseText ?? content;
          f.changedOnDisk = false;
        }),
      );
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Failed to reload file';
      setError(msg);
      // The recovery is the SAME read: the buffer is showing bytes the note
      // no longer holds until one answers.
      setRetryFailedOperation(() => () => reloadFile(path));
      console.error('Failed to reload file:', err);
    } finally {
      setIsLoading(false);
    }
  };

  /** The note moved on disk and this buffer has not followed it. */
  const flagChangedOnDisk = (path: string) => {
    setOpenFiles(
      produce((files) => {
        const f = files.find((x) => x.path === path);
        if (f) f.changedOnDisk = true;
      }),
    );
  };

  /**
   * Take the disk quietly, for a buffer with nothing of the user's in it.
   *
   * A buffer the user typed into, or whose base moved, while the read was out
   * is left alone — those bytes are newer than this answer. A read that cannot
   * answer raises the flag instead, so a buffer showing text the note no
   * longer holds is never silent about it.
   */
  const refreshFromDisk = async (path: string) => {
    const before = openFilesStore.find((f) => f.path === path);
    if (!before) return;
    const baseAtRead = before.baseHash;
    try {
      const { content, content_hash, fromMirror, baseText } = await readNote(path, await kilnOf(path));
      // The mirror is the daemon's older copy, not the disk. A read that fell
      // back to it answers text the note may no longer hold, so the buffer is
      // told rather than quietly moved onto it.
      if (fromMirror) {
        flagChangedOnDisk(path);
        return;
      }
      setOpenFiles(
        produce((files) => {
          const f = files.find((x) => x.path === path);
          if (!f || f.dirty || f.baseHash !== baseAtRead) return;
          f.content = content;
          f.baseHash = content_hash;
          f.baseText = baseText ?? content;
          f.changedOnDisk = false;
        }),
      );
    } catch {
      flagChangedOnDisk(path);
    }
  };

  /**
   * The kiln watcher names a path, and this editor holds it open.
   *
   * A move names both ends: the note left one path and arrived at the other,
   * so a buffer on either side is looking at bytes that moved. A delete is not
   * acted on — there is no text to take and nothing for the banner to offer.
   *
   * The watcher also echoes this editor's OWN saves. The buffer is clean by
   * then and the re-read answers the text and hash it already holds, so the
   * echo costs one read and changes nothing.
   */
  const onFsEvent = async (event: FsEvent) => {
    const paths =
      event.type === 'changed' ? [event.path] : event.type === 'moved' ? [event.from, event.to] : [];
    for (const path of paths) {
      const file = openFilesStore.find((f) => f.path === path);
      if (!file) continue;
      // The user's text is the only copy there is: it is never replaced and
      // never re-read behind them. They choose on the banner instead. A
      // buffer that went clean because its write QUEUED still holds writing
      // the daemon has not received, so it counts as dirty here.
      // A store that cannot be read at all (a private window, no IndexedDB)
      // cannot rule that writing out, so the question fails towards the
      // banner: it tells the user and loses nothing.
      const queued = await hasQueuedWriting(path).catch(() => true);
      if (file.dirty || queued) flagChangedOnDisk(path);
      else void refreshFromDisk(path);
    }
  };

  // One stream, open while there is a buffer it could speak about and closed
  // when the last one goes. `FilesPanel` reads the same stream for the tree,
  // and both go through the shared root of `lib/query/sse.ts`: one
  // `EventSource` for the two of them, and one handler each.
  const anyFileOpen = createMemo(() => openFilesStore.length > 0);
  createEffect(() => {
    if (!anyFileOpen()) return;
    onCleanup(fsEvents().subscribe((event) => void onFsEvent(event)));
  });

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
    // A dirty buffer descends from the old base, not the merged disk text.
    // Keep it stale so its next save merges again instead of erasing the remote edit.
    if (row.merged && file.dirty) {
      flagChangedOnDisk(row.path);
      return;
    }
    setBaseHash(row.path, row.hash);
    if (file.dirty) return;
    void kilnOf(row.path)
      .then((kiln) => readNote(row.path, kiln))
      .then(({ content, content_hash, baseText }) => {
        setOpenFiles(
          produce((files) => {
            const f = files.find((x) => x.path === row.path);
            // A base that moved during the read belongs to a later write.
            if (!f || f.dirty || f.baseHash !== row.hash) return;
            f.content = content;
            f.baseHash = content_hash;
            // The read answers a text and a hash that belong together, so the
            // buffer is mergeable again — the landed write cleared the pair.
            f.baseText = baseText ?? content;
          }),
        );
      })
      .catch(() => {
        // The base already moved. The text refreshes on the next open.
      });
  };
  onCleanup(onNoteLanded(onLanded));

  /**
   * A queued whole write the drain could not settle. The buffer whose base
   * the write was made from went clean when the write queued, and the note
   * now holds someone else's text: a clean buffer here is a lie the user
   * acts on. The buffer goes dirty and keeps its text, which is also what the
   * conflict holds, and the notice points at where it waits. A buffer with
   * another base was moved by a later write and is left alone; the drain's
   * own notice covers it.
   */
  const onConflicted = (row: Conflicted): boolean => {
    const file = openFilesStore.find((f) => f.path === row.path && f.baseHash === row.base);
    if (!file) return false;
    setOpenFiles(
      produce((files) => {
        const f = files.find((x) => x.path === row.path);
        if (f) f.dirty = true;
      }),
    );
    notificationActions.addNotification(
      'warning',
      'The note changed elsewhere while you were offline. Your text is kept as a conflict — open Conflicts to resolve it.',
    );
    return true;
  };
  onCleanup(onNoteConflicted(onConflicted));

  const value: EditorContextValue = {
    openFiles: () => openFilesStore,
    activeFile,
    openFile,
    closeFile,
    saveFile,
    setActiveFile,
    updateFileContent,
    setBaseHash,
    reloadFile,
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
  reloadFile: noopAsync,
  isLoading: () => false,
  error: () => null,
  retryFailedOperation: () => null,
};

export function useEditorSafe(): EditorContextValue {
  const context = useContext(EditorContext);
  return context ?? fallbackEditorContext;
}

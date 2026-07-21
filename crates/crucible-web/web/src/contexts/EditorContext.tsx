import {
  createContext,
  useContext,
  ParentComponent,
  createSignal,
} from 'solid-js';
import { createStore, produce } from 'solid-js/store';
import type { EditorFile } from '@/lib/types';
import type { EditorContextValue } from '@/lib/types/context';
import { getFileContent, saveFileContent } from '@/lib/api';


const EditorContext = createContext<EditorContextValue>();

export const EditorProvider: ParentComponent = (props) => {
  const [openFilesStore, setOpenFiles] = createStore<EditorFile[]>([]);
  const [activeFile, setActiveFileSignal] = createSignal<string | null>(null);
  const [isLoading, setIsLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

    // Number of live panels holding each open path. A file's edited buffer
    // lives only here, so it must survive a panel unmount+remount (tab move or
    // pop-out) — we only evict when the last holder releases it.
    const openCounts = new Map<string, number>();

  // `background` opens the buffer WITHOUT making it the active file —
  // transient surfaces (wikilink hover windows) must not steal focus, or
  // everything keyed on activeFile (backlinks panel) flickers per hover.
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

    try {
      // Load the raw file bytes from disk. get_note_by_name returns metadata
      // only (no content), so the note endpoint can't hydrate the editor —
      // GET /api/kiln/file reads the file itself and is the source of truth.
      const content = await getFileContent(path);

      setOpenFiles(
        produce((files) => {
          files.push({ path, content, dirty: false });
        })
      );
      openCounts.set(path, 1);
      if (!opts?.background) setActiveFileSignal(path);
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Failed to open file';
      setError(msg);
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

    try {
      // Save by absolute path (symmetric with the load) — the editor addresses
      // files by path, and PUT /api/kiln/file writes within the open kiln.
      await saveFileContent(path, file.content);

      setOpenFiles(
        produce((files) => {
          const f = files.find((x) => x.path === path);
          if (f) f.dirty = false;
        })
      );
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Failed to save file';
      setError(msg);
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

  const value: EditorContextValue = {
    openFiles: () => openFilesStore,
    activeFile,
    openFile,
    closeFile,
    saveFile,
    setActiveFile,
    updateFileContent,
    isLoading,
    error,
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
  isLoading: () => false,
  error: () => null,
};

export function useEditorSafe(): EditorContextValue {
  const context = useContext(EditorContext);
  return context ?? fallbackEditorContext;
}

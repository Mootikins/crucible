import {
  createContext,
  useContext,
  ParentComponent,
  createSignal,
  Accessor,
} from 'solid-js';
import { createStore, produce } from 'solid-js/store';
import type { EditorFile } from '@/lib/types';
import { getFileContent, saveFileContent } from '@/lib/api';

export interface EditorContextValue {
  openFiles: Accessor<EditorFile[]>;
  activeFile: Accessor<string | null>;
  openFile: (path: string) => Promise<void>;
  closeFile: (path: string) => void;
  saveFile: (path: string) => Promise<void>;
  setActiveFile: (path: string) => void;
  updateFileContent: (path: string, content: string) => void;
  isLoading: Accessor<boolean>;
  error: Accessor<string | null>;
}

const EditorContext = createContext<EditorContextValue>();

export const EditorProvider: ParentComponent = (props) => {
  const [openFilesStore, setOpenFiles] = createStore<EditorFile[]>([]);
  const [activeFile, setActiveFileSignal] = createSignal<string | null>(null);
  const [isLoading, setIsLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  const openFile = async (path: string) => {
    const existing = openFilesStore.find((f) => f.path === path);
    if (existing) {
      setActiveFileSignal(path);
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const content = await getFileContent(path);
      setOpenFiles(
        produce((files) => {
          files.push({ path, content, dirty: false });
        })
      );
      setActiveFileSignal(path);
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Failed to open file';
      setError(msg);
      console.error('Failed to open file:', err);
    } finally {
      setIsLoading(false);
    }
  };

  const closeFile = (path: string) => {
    const idx = openFilesStore.findIndex((f) => f.path === path);
    if (idx === -1) return;

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

  const saveFile = async (path: string) => {
    const file = openFilesStore.find((f) => f.path === path);
    if (!file) return;

    setIsLoading(true);
    setError(null);

    try {
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

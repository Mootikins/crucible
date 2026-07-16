import { Component, For, Show } from 'solid-js';
import { useEditorSafe } from '@/contexts/EditorContext';
import { EditorWithPreview } from './editor/EditorWithPreview';
import { useSettingsSafe } from '@/contexts/SettingsContext';
import { getConfig, getNote } from '@/lib/api';
import { noteAbsolutePath } from '@/lib/note-actions';
import { notificationActions } from '@/stores/notificationStore';

const getFilename = (path: string): string => {
  return path.split('/').pop() ?? path;
};

const Tab: Component<{
  path: string;
  active: boolean;
  dirty: boolean;
  onSelect: () => void;
  onClose: () => void;
}> = (props) => {
  const handleClose = (e: MouseEvent) => {
    e.stopPropagation();
    props.onClose();
  };

  return (
    <button
      data-testid="editor-tab"
      class="flex items-center gap-2 px-3 py-1.5 text-sm border-b-2 transition-colors whitespace-nowrap"
      classList={{
        'border-primary text-neutral-100 bg-neutral-800': props.active,
        'border-transparent text-neutral-400 hover:text-neutral-200 hover:bg-neutral-800/50': !props.active,
      }}
      onClick={props.onSelect}
    >
      <span class="truncate max-w-[150px]">
        {props.dirty && <span class="text-primary mr-1">●</span>}
        {getFilename(props.path)}
      </span>
      <span
        class="text-neutral-500 hover:text-neutral-200 hover:bg-neutral-700 rounded px-1"
        onClick={handleClose}
      >
        ×
      </span>
    </button>
  );
};

export const EditorPanel: Component = () => {
  const { openFiles, activeFile, setActiveFile, closeFile, saveFile, updateFileContent, isLoading, error, openFile } = useEditorSafe();
  const { settings } = useSettingsSafe();

  const activeFileData = () => {
    const path = activeFile();
    if (!path) return null;
    return openFiles().find((f) => f.path === path) ?? null;
  };

  // Follow a [[wikilink]]: resolve the target note and open it as another
  // editor tab (this panel owns its own tab strip, unlike FileViewerPanel
  // which opens window tabs via openNoteInEditor).
  const followLink = async (target: string) => {
    try {
      const kiln = (await getConfig()).kiln_path;
      const note = await getNote(target, kiln);
      openFile(noteAbsolutePath(note.path, kiln));
    } catch {
      notificationActions.addNotification('warning', `Note not found: ${target}`);
    }
  };

  return (
    <div class="h-full flex flex-col bg-neutral-900 text-neutral-100 overflow-hidden">
      <Show
        when={openFiles().length > 0}
        fallback={
          <div class="flex-1 flex items-center justify-center text-neutral-500">
            <div class="text-center">
              <div class="text-4xl mb-4">📄</div>
              <div class="text-sm">No files open</div>
               <div class="text-xs text-neutral-500 mt-1">Click a note in the sidebar to open it</div>
            </div>
          </div>
        }
      >
        <div class="flex border-b border-neutral-800 bg-neutral-900 overflow-x-auto shrink-0">
          <For each={openFiles()}>
            {(file) => (
              <Tab
                path={file.path}
                active={file.path === activeFile()}
                dirty={file.dirty}
                onSelect={() => setActiveFile(file.path)}
                onClose={() => closeFile(file.path)}
              />
            )}
          </For>
        </div>

        <Show when={isLoading()}>
          <div class="absolute inset-0 flex items-center justify-center bg-neutral-900/80 z-10">
            <div class="flex items-center gap-3">
              <div class="w-5 h-5 border-2 border-neutral-600 border-t-neutral-300 rounded-full animate-spin" />
              <span class="text-neutral-400 text-sm">Loading file...</span>
            </div>
          </div>
        </Show>

        <Show when={error()}>
          <div class="mx-4 mt-2 px-3 py-2 text-sm text-red-400 bg-red-900/20 rounded border border-red-900/30 flex items-center gap-2">
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-4 h-4 shrink-0">
              <path fill-rule="evenodd" d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-8-5a.75.75 0 01.75.75v4.5a.75.75 0 01-1.5 0v-4.5A.75.75 0 0110 5zm0 10a1 1 0 100-2 1 1 0 000 2z" clip-rule="evenodd" />
            </svg>
            <span>{error()}</span>
          </div>
        </Show>

        <div class="flex-1 overflow-hidden relative">
          <Show when={activeFileData()}>
            {(file) => (
              <EditorWithPreview
                content={file().content}
                path={file().path}
                onChange={(content) => updateFileContent(file().path, content)}
                onSave={() => void saveFile(file().path)}
                onFollowLink={(target) => void followLink(target)}
                vimMode={settings.editor.vimMode}
              />
            )}
          </Show>
        </div>
      </Show>
    </div>
  );
};

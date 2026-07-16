import {
  Component,
  Show,
  createEffect,
  onCleanup,
  untrack,
} from 'solid-js';
import { useEditorSafe } from '@/contexts/EditorContext';
import { CodeMirrorEditor } from './editor/CodeMirrorEditor';
import { findTabByFilePath } from '@/lib/file-actions';
import { openNoteInEditor } from '@/lib/note-actions';
import { windowActions } from '@/stores/windowStore';
import { statusBarStore } from '@/stores/statusBarStore';
import { PanelShell } from './PanelShell';


interface FileViewerPanelProps {
  filePath?: string;
}

const FileViewerPanel: Component<FileViewerPanelProps> = (props) => {
  const { openFile, closeFile, openFiles, isLoading, error, updateFileContent, saveFile } = useEditorSafe();

  const fileData = () => openFiles().find(f => f.path === props.filePath) ?? null;

  const handleSave = () => {
    if (props.filePath) void saveFile(props.filePath);
  };

  createEffect(() => {
    if (props.filePath) {
      openFile(props.filePath);
    }
  });

  onCleanup(() => {
    if (props.filePath) {
      // Force: by unmount time the window tab is already gone, so a prompt
      // here could not veto anything — the confirm lives at the tab-close
      // call sites (confirmTabClose).
      closeFile(props.filePath, { force: true });
    }
  });

  // Sync EditorContext dirty state → windowStore tab isModified.
  // Depend ONLY on the editor's dirty flag: the tab lookup + updateTab write
  // must be untracked, otherwise findTabByFilePath reads windowStore.tabGroups
  // and updateTab writes it back in the same effect — a self-retriggering loop
  // that overflows the stack (updateTab replaces the whole tabs array).
  createEffect(() => {
    if (!props.filePath) return;
    const file = openFiles().find(f => f.path === props.filePath);
    const isModified = file?.dirty ?? false;
    untrack(() => {
      const tabInfo = findTabByFilePath(props.filePath!);
      if (tabInfo) {
        windowActions.updateTab(tabInfo.groupId, tabInfo.tab.id, { isModified });
      }
    });
  });

  // No file path provided — nothing to render
  if (!props.filePath) {
    return (
      <div class="h-full bg-neutral-900 p-4 flex items-center justify-center text-neutral-400 text-sm">
        No file selected
      </div>
    );
  }

  return (
    <PanelShell class="overflow-hidden relative">
      {/* Toolbar: Save affordance (Cmd/Ctrl-S also saves via the editor keymap). */}
      <div class="flex items-center justify-end gap-2 border-b border-neutral-800 px-3 py-1.5 shrink-0">
        <Show when={fileData()?.dirty}>
          <span
            data-testid="file-dirty-indicator"
            class="text-xs text-amber-500"
            title="Unsaved changes"
          >
            ●
          </span>
        </Show>
        <button
          data-testid="file-save"
          onClick={handleSave}
          disabled={!fileData()?.dirty}
          class="rounded bg-primary px-3 py-1 text-xs text-white hover:bg-primary-hover disabled:opacity-40 disabled:cursor-not-allowed"
          title="Save (⌘S)"
        >
          Save
        </button>
      </div>

      {/* Loading overlay */}
      <Show when={isLoading()}>
        <div class="absolute inset-0 flex items-center justify-center bg-neutral-900/80 z-10">
          <div class="flex items-center gap-3">
            <div class="w-5 h-5 border-2 border-neutral-600 border-t-neutral-300 rounded-full animate-spin" />
            <span class="text-neutral-400 text-sm">Loading file...</span>
          </div>
        </div>
      </Show>
      {/* Error bar */}
      <Show when={error()}>
        <div class="mx-4 mt-2 px-3 py-2 text-sm text-red-400 bg-red-900/20 rounded border border-red-900/30 flex items-center gap-2">
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-4 h-4 shrink-0">
            <path fill-rule="evenodd" d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-8-5a.75.75 0 01.75.75v4.5a.75.75 0 01-1.5 0v-4.5A.75.75 0 0110 5zm0 10a1 1 0 100-2 1 1 0 000 2z" clip-rule="evenodd" />
          </svg>
          <span>{error()}</span>
        </div>
      </Show>

      {/* Editor area */}
      <div class="flex-1 overflow-hidden">
        <Show
          when={fileData()}
          fallback={
            <Show when={!isLoading()}>
              <div class="h-full flex items-center justify-center text-neutral-500">
                <div class="text-center">
                  <div class="text-4xl mb-4">📄</div>
                  <div class="text-sm">Loading file...</div>
                </div>
              </div>
            </Show>
          }
        >
          {(file) => (
            <CodeMirrorEditor
              content={file().content}
              path={file().path}
              onChange={(content) => updateFileContent(file().path, content)}
              onSave={handleSave}
              onFollowLink={(target) =>
                void openNoteInEditor(target, statusBarStore.kilnPath() ?? undefined)
              }
            />
          )}
        </Show>
      </div>
    </PanelShell>
  );
};

export default FileViewerPanel;

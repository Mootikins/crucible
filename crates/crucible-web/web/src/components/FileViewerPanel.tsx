import {
  Component,
  Show,
  createEffect,
  onCleanup,
  untrack,
} from 'solid-js';
import { FileText } from '@/lib/icons';
import { useEditorSafe } from '@/contexts/EditorContext';
import { EditorWithPreview } from './editor/EditorWithPreview';
import { useSettingsSafe } from '@/contexts/SettingsContext';
import { findTabByFilePath } from '@/lib/file-actions';
import { openNoteInEditor } from '@/lib/note-actions';
import { windowActions } from '@/stores/windowStore';
import { statusBarStore } from '@/stores/statusBarStore';
import { PanelShell } from './PanelShell';
import { Menu } from '@ark-ui/solid';
import { Portal } from 'solid-js/web';
import { attachNativeMenuGuard } from '@/lib/context-menu';
import type { EditorView } from '@codemirror/view';


interface FileViewerPanelProps {
  filePath?: string;
  /** Mode markdown opens in ('reading' | 'live' | 'source') — hover
   * popovers set this via tab metadata. */
  initialMode?: string;
  /** Open the buffer WITHOUT claiming activeFile — hover popovers set this
   * so transient previews don't retarget focus-following panels
   * (backlinks). */
  background?: boolean;
  /** Scroll to the first wikilink pointing at this note key on open —
   * backlinks hover previews jump to the referencing section. */
  scrollToNote?: string;
  /** Exact referencing line (1-based) when the link index resolved one —
   * beats the scrollToNote scan in editor modes. */
  scrollToLine?: number;
}

const FileViewerPanel: Component<FileViewerPanelProps> = (props) => {
  const { openFile, closeFile, openFiles, isLoading, error, updateFileContent, saveFile } = useEditorSafe();
  const { settings } = useSettingsSafe();

  // Live CodeMirror view (source/live modes; undefined in reading mode) for
  // the context-menu clipboard actions.
  let editorView: EditorView | undefined;

  type EditorMenuAction = 'cut' | 'copy' | 'paste' | 'select-all' | 'copy-file-path';
  const onMenuAction = (action: EditorMenuAction) => {
    void (async () => {
      if (action === 'copy-file-path') {
        if (props.filePath) await navigator.clipboard.writeText(props.filePath);
        return;
      }
      const v = editorView;
      if (!v) {
        // Reading mode: rendered preview — only copy-selection is meaningful.
        if (action === 'copy') {
          const text = String(window.getSelection() ?? '');
          if (text) await navigator.clipboard.writeText(text);
        }
        return;
      }
      const sel = v.state.selection.main;
      const selected = v.state.sliceDoc(sel.from, sel.to);
      switch (action) {
        case 'copy':
          if (selected) await navigator.clipboard.writeText(selected);
          break;
        case 'cut':
          if (selected) {
            await navigator.clipboard.writeText(selected);
            v.dispatch({ changes: { from: sel.from, to: sel.to, insert: '' } });
          }
          break;
        case 'paste': {
          // Prompts for clipboard-read permission on first use.
          const text = await navigator.clipboard.readText().catch(() => '');
          if (text) {
            v.dispatch({
              changes: { from: sel.from, to: sel.to, insert: text },
              selection: { anchor: sel.from + text.length },
            });
          }
          break;
        }
        case 'select-all':
          v.dispatch({ selection: { anchor: 0, head: v.state.doc.length } });
          break;
      }
      v.focus();
    })();
  };

  const menuItemClass =
    'flex items-center gap-2 px-3 py-1.5 cursor-pointer data-[highlighted]:bg-hover-wash';

  const fileData = () => openFiles().find(f => f.path === props.filePath) ?? null;

  const handleSave = () => {
    if (props.filePath) void saveFile(props.filePath);
  };

  // Track ONLY props.filePath. openFile() begins with openFilesStore.find(),
  // a reactive store read; left tracked, this effect would subscribe to the
  // whole open-files array and re-run — re-entering the `existing` branch and
  // re-incrementing the open refcount — whenever ANY panel mutates the store
  // (even this file's own async load push, or a hover-preview popover opening
  // another file). The count then never returns to zero, so closeFile never
  // evicts and a "closed" dirty buffer resurrects with stale edits. untrack
  // keeps the reference-taking out of the tracking scope.
  createEffect(() => {
    const path = props.filePath;
    if (path) {
      untrack(() => openFile(path, { background: props.background }));
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

  // Autosave: a dirty buffer saves after `autosaveSeconds` of idle (each
  // edit resets the timer via the content dependency). 0 disables.
  createEffect(() => {
    const seconds = settings.editor.autosaveSeconds;
    const file = fileData();
    if (!seconds || seconds <= 0 || !file?.dirty) return;
    // Depend on content so every keystroke restarts the countdown.
    void file.content;
    const timer = window.setTimeout(() => {
      if (props.filePath) void saveFile(props.filePath);
    }, seconds * 1000);
    onCleanup(() => window.clearTimeout(timer));
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
      <div class="h-full bg-shell-bg p-4 flex items-center justify-center text-muted text-sm">
        No file selected
      </div>
    );
  }

  return (
    <PanelShell class="overflow-hidden relative">
      {/* No save toolbar: saving is Mod-S / Mod-Enter in the editor, the
          (configurable) status-bar save affordance, or autosave. */}
      {/* Loading overlay — only while THIS file has no content yet.
          EditorContext.isLoading is context-global: any other panel opening
          a file (e.g. a hover popover) would otherwise flash this overlay
          over every open editor. */}
      <Show when={isLoading() && !fileData()}>
        <div class="absolute inset-0 flex items-center justify-center bg-surface-base/80 z-10">
          <div class="flex items-center gap-3">
            <div class="w-5 h-5 border-2 border-hairline border-t-shell-body rounded-full animate-spin" />
            <span class="text-muted text-sm">Loading file...</span>
          </div>
        </div>
      </Show>
      {/* Error bar */}
      <Show when={error()}>
        <div class="mx-4 mt-2 px-3 py-2 text-sm text-error bg-error/10 rounded border border-error/30 flex items-center gap-2">
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-4 h-4 shrink-0">
            <path fill-rule="evenodd" d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-8-5a.75.75 0 01.75.75v4.5a.75.75 0 01-1.5 0v-4.5A.75.75 0 0110 5zm0 10a1 1 0 100-2 1 1 0 000 2z" clip-rule="evenodd" />
          </svg>
          <span>{error()}</span>
        </div>
      </Show>

      {/* Editor area. Right-click opens the app menu (clipboard actions for
          browser-stolen keybind parity); Shift+right-click and images/links
          inside the rendered preview keep the NATIVE menu so Copy Image /
          Save As stay available (capture guard). */}
      <div class="flex-1 overflow-hidden" ref={attachNativeMenuGuard}>
        <Menu.Root onSelect={(d) => onMenuAction(d.value as EditorMenuAction)}>
          {/* asChild div: never wrap an editor in the default BUTTON trigger. */}
          <Menu.ContextTrigger
            asChild={(triggerProps) => (
              <div {...triggerProps({ class: 'block h-full w-full text-left' })}>
            <Show
              when={fileData()}
              fallback={
                <Show when={!isLoading()}>
                  <div class="h-full flex items-center justify-center text-muted-dark">
                    <div class="text-center">
                      <FileText class="w-10 h-10 mx-auto mb-4 text-muted-dark" />
                      <div class="text-sm">Loading file...</div>
                    </div>
                  </div>
                </Show>
              }
            >
              {(file) => (
                <EditorWithPreview
                  content={file().content}
                  path={file().path}
                  onChange={(content) => updateFileContent(file().path, content)}
                  onSave={handleSave}
                  onFollowLink={(target) =>
                    void openNoteInEditor(target, statusBarStore.kilnPath() ?? undefined)
                  }
                  vimMode={settings.editor.vimMode}
                  lineWidth={settings.editor.maxLineWidth}
                  editorApiRef={(view) => (editorView = view)}
                  initialMode={
                    props.initialMode === 'reading' || props.initialMode === 'live' || props.initialMode === 'source'
                      ? props.initialMode
                      : undefined
                  }
                  scrollToNote={props.scrollToNote}
                  scrollToLine={props.scrollToLine}
                />
              )}
            </Show>
              </div>
            )}
          />
          <Portal>
          <Menu.Positioner>
            <Menu.Content class="min-w-[11rem] rounded border border-hairline bg-surface-elevated py-1 text-xs text-shell-ink shadow-lg focus:outline-none z-50">
              <Menu.Item value="cut" class={menuItemClass}>Cut</Menu.Item>
              <Menu.Item value="copy" class={menuItemClass}>Copy</Menu.Item>
              <Menu.Item value="paste" class={menuItemClass}>Paste</Menu.Item>
              <Menu.Item value="select-all" class={menuItemClass}>Select All</Menu.Item>
              <Menu.Separator class="my-1 border-t border-hairline" />
              <Menu.Item value="copy-file-path" class={menuItemClass}>Copy File Path</Menu.Item>
            </Menu.Content>
          </Menu.Positioner>
          </Portal>
        </Menu.Root>
      </div>
    </PanelShell>
  );
};

export default FileViewerPanel;

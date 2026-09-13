import { Component, For, Show, createSignal, onMount } from 'solid-js';
import { FileText } from '@/lib/icons';
import { useEditorSafe } from '@/contexts/EditorContext';
import { EditorWithPreview } from './editor/EditorWithPreview';
import { useSettingsSafe } from '@/contexts/SettingsContext';
import { listKilns, resolveNotePath } from '@/lib/api';
import { kilnForPath } from '@/lib/note-actions';
import { notificationActions } from '@/stores/notificationStore';
import { ConnectionBanner } from '@/components/ui/ConnectionBanner';

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
        'border-primary text-shell-ink bg-surface-elevated': props.active,
        'border-transparent text-muted hover:text-shell-ink hover:bg-hover-wash': !props.active,
      }}
      onClick={props.onSelect}
    >
      <span class="truncate max-w-[150px]">
        {props.dirty && <span class="text-primary mr-1">●</span>}
        {getFilename(props.path)}
      </span>
      <span
        class="text-muted-dark hover:text-shell-ink hover:bg-hover-wash rounded px-1"
        onClick={handleClose}
      >
        ×
      </span>
    </button>
  );
};

export const EditorPanel: Component = () => {
  const { openFiles, activeFile, setActiveFile, closeFile, saveFile, updateFileContent, setBaseHash, isLoading, error, openFile, retryFailedOperation } = useEditorSafe();
  const { settings } = useSettingsSafe();

  const activeFileData = () => {
    const path = activeFile();
    if (!path) return null;
    return openFiles().find((f) => f.path === path) ?? null;
  };

  // The kiln owning the file on screen, from the file's own path — not the
  // configured default, which resolved a buffer's links in a kiln that had
  // nothing to do with it.
  const [kilns, setKilns] = createSignal<{ path: string }[]>([]);
  onMount(() => void listKilns().then(setKilns).catch(() => undefined));
  const owningKiln = (path?: string) => (path ? kilnForPath(path, kilns()) : undefined);

  // Follow a [[wikilink]]: resolve the target and open it as another editor
  // tab (this panel owns its own tab strip, unlike FileViewerPanel which opens
  // window tabs).
  const followLink = async (target: string) => {
    const kiln = owningKiln(activeFile() ?? undefined);
    const hit = kiln ? await resolveNotePath(kiln, target).catch(() => null) : null;
    if (!hit) {
      notificationActions.addNotification('warning', `Note not found: ${target}`);
      return;
    }
    openFile(hit.absolutePath);
  };

  return (
    <div class="h-full flex flex-col bg-shell-panel text-shell-ink overflow-hidden">
      <Show
        when={openFiles().length > 0}
        fallback={
          <div class="flex-1 flex items-center justify-center text-muted-dark">
            <div class="text-center">
              <FileText class="w-10 h-10 mx-auto mb-4 text-muted-dark" />
              <div class="text-sm">No files open</div>
               <div class="text-xs text-muted-dark mt-1">Click a note in the sidebar to open it</div>
            </div>
          </div>
        }
      >
        <div class="flex border-b border-hairline bg-shell-panel overflow-x-auto shrink-0">
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
          <div class="absolute inset-0 flex items-center justify-center bg-surface-base/80 z-10">
            <div class="flex items-center gap-3">
              <div class="w-5 h-5 border-2 border-hairline border-t-shell-body rounded-full animate-spin" />
              <span class="text-muted text-sm">Loading file...</span>
            </div>
          </div>
        </Show>

        {/* A failed save leaves the buffer dirty and the bytes unwritten, so
            the banner carries the save back. `retryFailedOperation` is the
            actual failed call — see EditorContext. */}
        <Show when={error()}>
          {(message) => (
            <ConnectionBanner
              class="mx-4 mt-2"
              tone="error"
              message={message()}
              retryLabel="Retry"
              onRetry={
                retryFailedOperation()
                  ? () => void retryFailedOperation()?.()
                  : undefined
              }
              testid="editor-error-banner"
              retryTestid="editor-error-retry"
            />
          )}
        </Show>

        <div class="flex-1 overflow-hidden relative">
          <Show when={activeFileData()}>
            {(file) => (
              <EditorWithPreview
                content={file().content}
                path={file().path}
                // Declares the kiln for BOTH the click handler and the
                // document-level hover controller. Without it this panel's
                // wikilinks were inert: no popover, no Ctrl+Click.
                kiln={owningKiln(file().path)}
                baseHash={file().baseHash}
                onBaseChange={(hash) => setBaseHash(file().path, hash)}
                onChange={(content) => updateFileContent(file().path, content)}
                onSave={() => void saveFile(file().path)}
                onFollowLink={(target) => void followLink(target)}
                lineWidth={settings.editor.maxLineWidth}
                vimMode={settings.editor.vimMode}
                renderMath={settings.editor.renderMath}
                renderDiagrams={settings.editor.renderDiagrams}
                hideFrontmatterGap={settings.editor.hideFrontmatterGap}
              />
            )}
          </Show>
        </div>
      </Show>
    </div>
  );
};

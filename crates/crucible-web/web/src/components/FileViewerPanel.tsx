import {
  Component,
  For,
  Show,
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  untrack,
} from 'solid-js';
import { AlertTriangle, FileText, Pencil } from '@/lib/icons';
import { useEditorSafe } from '@/contexts/EditorContext';
import { menuContent, menuItem, menuSeparator } from '@/components/ui/menu-style';
import { EditorWithPreview } from './editor/EditorWithPreview';
import { useSettingsSafe } from '@/contexts/SettingsContext';
import { kilnForPath, openNoteInEditor } from '@/lib/note-actions';
import { rawFileUrl } from '@/lib/paths';
import { tabHost } from '@/lib/tab-host';
import { isCompact } from '@/stores/deviceStore';
import { hit } from '@/lib/touch';
import { compactEditorMode, setCompactEditorMode } from '@/stores/editorModeStore';
import { fetchKilnsOnce, useKilns } from '@/lib/query/kilns';
import { PanelShell } from './PanelShell';
import { ImageViewer } from './ImageViewer';
import { Menu } from '@ark-ui/solid';
import { Portal } from 'solid-js/web';
import { attachNativeMenuGuard } from '@/windowing';
import { EditorView } from '@codemirror/view';
import { useProposals } from '@/lib/query/proposals';
import { authorLabel, isPending, writePath } from '@/lib/proposal-api';
import { openDiff } from '@/lib/panel-actions';

/** Extensions the browser renders itself, kept in step with the canvas media
 * node (`CanvasNodeView.tsx`) — the other place raw bytes become an `<img>`. */
const IMAGE_EXT = /\.(png|jpe?g|gif|webp|svg|avif|bmp|ico)$/i;

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
  const {
    openFile,
    closeFile,
    openFiles,
    isLoading,
    error,
    updateFileContent,
    setBaseHash,
    saveFile,
    reloadFile,
  } = useEditorSafe();
  const { settings } = useSettingsSafe();

  // Live CodeMirror view (source/live modes; undefined in reading mode) for
  // the context-menu clipboard actions and the review layer. A signal, not a
  // plain `let`, because the review effects below must run once the view
  // exists — the ref callback fires after they first evaluate.
  const [editorView, setEditorView] = createSignal<EditorView | undefined>(undefined);

  // Which kiln owns the open file. A buffer's wikilinks resolve in the kiln
  // holding that FILE — following a link out of a note opened from another
  // kiln (a canvas card, a search hit, a hover popover) used to land in
  // whichever kiln the status bar pointed at, which is one kiln serving
  // another kiln's data.
  const kilnsQuery = useKilns();
  const kilns = (): { path: string }[] => kilnsQuery.data ?? [];

  /** Vim is a desktop default. A phone has no `Escape` and no modifier row,
   * and one shared key cannot serve both shells. */
  const effectiveVimMode = () =>
    isCompact() ? settings.editor.vimModeCompact : settings.editor.vimMode;
  const owningKiln = (path?: string) => (path ? kilnForPath(path, kilns()) : undefined);

  /**
   * Resolve the owning kiln, waiting for the roster on a cold start.
   *
   * The query paints nothing on a cold start (first run, cleared storage,
   * private mode), and a click in that window has no kiln to resolve against —
   * it would toast a misleading "Note not found" and never retry. This awaits
   * the fetch the query is already running, and reads the answer it returns
   * rather than the store, which the observer updates a tick later.
   */
  const owningKilnAsync = async (path?: string) => {
    if (!path) return undefined;
    const immediate = owningKiln(path);
    if (immediate || kilns().length > 0) return immediate;
    const roster = await fetchKilnsOnce().catch(() => []);
    return kilnForPath(path, roster);
  };

  type EditorMenuAction = 'cut' | 'copy' | 'paste' | 'select-all' | 'copy-file-path';
  const onMenuAction = (action: EditorMenuAction) => {
    void (async () => {
      if (action === 'copy-file-path') {
        if (props.filePath) await navigator.clipboard.writeText(props.filePath);
        return;
      }
      const v = editorView();
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

  /** Rendered as bytes, never opened as text — see the early return below. */
  const isImage = () => !!props.filePath && IMAGE_EXT.test(props.filePath);

  const fileData = () => openFiles().find((f) => f.path === props.filePath) ?? null;

  // The proposals that wait for a decision and write this note. The list is
  // the Inbox list, so a `proposal_changed` event updates both.
  const proposals = useProposals();
  const noteProposals = createMemo(() => {
    const path = props.filePath;
    if (!path) return [];
    return (proposals.data ?? []).filter(
      (proposal) =>
        isPending(proposal.state) && proposal.writes.some((write) => writePath(write) === path),
    );
  });

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
    // Images are displayed from their raw bytes and never enter the buffer
    // model. `openFile` is a TEXT read, so calling it here took a 404 for a
    // file the panel was already rendering correctly — the early return below
    // governs what is DRAWN, and cannot stop an effect declared above it.
    if (path && !isImage()) {
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

  // Autosave: a dirty NOTE saves after `autosaveSeconds` of idle (each edit
  // resets the timer via the content dependency). 0 disables. Only a file
  // inside a kiln qualifies: a project file — code, config — saves by hand,
  // because a save there can fire watchers and builds mid-edit.
  const autosaveOn = () =>
    settings.editor.autosaveSeconds > 0 && !!owningKiln(props.filePath);

  // The disk-changed banner pauses autosave. A save there is a Merge the user
  // did not click, and the banner would offer a choice that no longer exists.
  // The banner's own flag is the pause, so the two cannot disagree.
  createEffect(() => {
    const file = fileData();
    if (!autosaveOn() || !file?.dirty || file.changedOnDisk) return;
    // Depend on content so every keystroke restarts the countdown.
    void file.content;
    const timer = window.setTimeout(() => {
      if (props.filePath) void saveFile(props.filePath);
    }, settings.editor.autosaveSeconds * 1000);
    onCleanup(() => window.clearTimeout(timer));
  });

  // Sync EditorContext dirty state → windowStore tab isModified.
  // Depend ONLY on the editor's dirty flag: the tab lookup + updateTab write
  // must be untracked, otherwise findTabByFilePath reads windowStore.tabGroups
  // and updateTab writes it back in the same effect — a self-retriggering loop
  // that overflows the stack (updateTab replaces the whole tabs array).
  createEffect(() => {
    if (!props.filePath) return;
    const file = openFiles().find((f) => f.path === props.filePath);
    const isModified = file?.dirty ?? false;
    untrack(() => {
      const host = tabHost();
      const tab = host.find((t) => t.metadata?.filePath === props.filePath);
      if (tab) host.update(tab.id, { isModified });
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

  // An image never reaches the editor. The editor's load path is a TEXT read
  // (`/api/kiln/file`), which fails on the first non-UTF-8 byte — so opening a
  // PNG from the tree produced "Failed to get file content: HTTP 404" for a
  // file that was sitting right there. Bytes come from `/api/file/raw`, which
  // serves them under a sandbox CSP as an inert type.
  //
  // The load effect above is guarded on the same predicate; a return here
  // decides what is drawn and does nothing about what was already scheduled.
  if (isImage()) {
    return (
      <PanelShell class="overflow-hidden">
        {/* Keyed on the path: a different image in this pane starts over at
            fit instead of inheriting the previous image's zoom/pan. */}
        <Show when={props.filePath} keyed>
          {(path) => <ImageViewer src={rawFileUrl(path)} alt={path.split('/').pop() ?? path} />}
        </Show>
      </PanelShell>
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
          <svg
            xmlns="http://www.w3.org/2000/svg"
            viewBox="0 0 20 20"
            fill="currentColor"
            class="w-4 h-4 shrink-0"
          >
            <path
              fill-rule="evenodd"
              d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-8-5a.75.75 0 01.75.75v4.5a.75.75 0 01-1.5 0v-4.5A.75.75 0 0110 5zm0 10a1 1 0 100-2 1 1 0 000 2z"
              clip-rule="evenodd"
            />
          </svg>
          <span>{error()}</span>
        </div>
      </Show>

      {/* The kiln watcher moved this note while the buffer held unsent edits.
          Both texts exist and only the user can choose, so autosave waits
          for the choice (see the autosave effect). Reload takes the
          disk's (asking first, the bytes here exist nowhere else), Merge is
          the ordinary save, which carries this buffer's base text so the
          route merges the two instead of refusing the write. */}
      <Show when={fileData()?.changedOnDisk}>
        <div
          data-testid="disk-changed-banner"
          class="mx-3 mt-2 px-3 py-1.5 rounded-md border border-attention/50 bg-attention/[0.06] flex items-center gap-2 text-reading"
        >
          <AlertTriangle class="w-3.5 h-3.5 text-attention shrink-0" />
          <span class="text-shell-ink">This note changed on disk</span>
          <span class="text-muted-dark">— your unsaved edits are still here</span>
          <Show when={autosaveOn()}>
            <span aria-hidden="true" class="text-muted-dark">
              ·
            </span>
            <span data-testid="disk-changed-autosave-paused" class="text-muted-dark">
              Autosave paused until you choose
            </span>
          </Show>
          <button
            data-testid="disk-changed-reload"
            onClick={() => props.filePath && void reloadFile(props.filePath)}
            class={`ml-auto shrink-0 rounded px-2 py-0.5 text-muted-dark hover:text-shell-ink hover:bg-hover-wash ${hit()}`}
          >
            Reload
          </button>
          <button
            data-testid="disk-changed-merge"
            onClick={handleSave}
            class={`shrink-0 rounded px-2 py-0.5 text-muted-dark hover:text-shell-ink hover:bg-hover-wash ${hit()}`}
          >
            Merge
          </button>
        </div>
      </Show>

      {/* A proposal waits for the user and writes this note. The disk holds no
          change yet, so the bar is the only sign of it here. A superseded or
          decided proposal waits for nothing, and puts no bar. */}
      <For each={noteProposals()}>
        {(proposal) => (
          <div
            data-testid={`proposal-bar-${proposal.id}`}
            class="mx-3 mt-2 px-3 py-1 rounded-md border border-primary/40 bg-primary/[0.06] flex items-center gap-2 text-floor"
          >
            <Pencil class="w-3 h-3 text-primary shrink-0" />
            <span class="text-shell-ink truncate">{authorLabel(proposal)} proposes a change</span>
            <button
              type="button"
              data-testid={`proposal-bar-review-${proposal.id}`}
              onClick={() => openDiff({ kind: 'proposal', id: proposal.id })}
              class={`ml-auto shrink-0 rounded px-2 py-0.5 text-muted-dark hover:text-shell-ink hover:bg-hover-wash ${hit()}`}
            >
              Review
            </button>
          </div>
        )}
      </For>

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
                      kiln={owningKiln(file().path)}
                      baseHash={file().baseHash}
                      onBaseChange={(hash) => setBaseHash(file().path, hash)}
                      onFollowLink={(target) =>
                        // The file's own kiln, or none. Falling back to the active
                        // kiln let a project file — which belongs to no kiln —
                        // follow links into whichever kiln was showing.
                        void owningKilnAsync(file().path).then((kiln) =>
                          openNoteInEditor(target, kiln),
                        )
                      }
                      vimMode={effectiveVimMode()}
                      lineWidth={settings.editor.maxLineWidth}
                      renderMath={settings.editor.renderMath}
                      renderDiagrams={settings.editor.renderDiagrams}
                      hideFrontmatterGap={settings.editor.hideFrontmatterGap}
                      reflowParagraphs={settings.editor.reflowParagraphs}
                      editorApiRef={(view) => setEditorView(() => view)}
                      // The compact shell owns the mode: its app bar carries the
                      // Read/Write control, and the editor's own buttons are too
                      // small for a thumb.
                      mode={isCompact() ? compactEditorMode() : undefined}
                      onModeChange={(next) => {
                        if (next === 'reading' || next === 'live') setCompactEditorMode(next);
                      }}
                      initialMode={
                        props.initialMode === 'reading' ||
                        props.initialMode === 'live' ||
                        props.initialMode === 'source'
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
              <Menu.Content class={`${menuContent} z-50`}>
                <Menu.Item value="cut" class={menuItem}>
                  Cut
                </Menu.Item>
                <Menu.Item value="copy" class={menuItem}>
                  Copy
                </Menu.Item>
                <Menu.Item value="paste" class={menuItem}>
                  Paste
                </Menu.Item>
                <Menu.Item value="select-all" class={menuItem}>
                  Select All
                </Menu.Item>
                <Menu.Separator class={menuSeparator} />
                <Menu.Item value="copy-file-path" class={menuItem}>
                  Copy File Path
                </Menu.Item>
              </Menu.Content>
            </Menu.Positioner>
          </Portal>
        </Menu.Root>
      </div>
    </PanelShell>
  );
};

export default FileViewerPanel;

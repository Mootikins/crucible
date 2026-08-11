import {
  Component,
  Show,
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  onMount,
  untrack,
} from 'solid-js';
import { FileText, Pencil } from '@/lib/icons';
import { useEditorSafe } from '@/contexts/EditorContext';
import { EditorWithPreview } from './editor/EditorWithPreview';
import { pendingDiffStore, pendingDiffActions } from '@/stores/pendingDiffStore';
import { useSettingsSafe } from '@/contexts/SettingsContext';
import { findTabByFilePath } from '@/lib/file-actions';
import { kilnForPath, openNoteInEditor } from '@/lib/note-actions';
import { listKilns } from '@/lib/api';
import { swrLocal } from '@/lib/local-cache';
import { windowActions } from '@/stores/windowStore';
import { PanelShell } from './PanelShell';
import { Menu } from '@ark-ui/solid';
import { Portal } from 'solid-js/web';
import { attachNativeMenuGuard } from '@/lib/context-menu';
import { EditorView } from '@codemirror/view';
import { syncReviewLayer, type ReviewHunkMark } from './editor/review-decorations';
import { pendingReveal, reviewActions, reviewStore, toolCallLabel } from '@/lib/review-store';
import { isExternal } from '@/lib/review-types';


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
  // the context-menu clipboard actions and the review layer. A signal, not a
  // plain `let`, because the review effects below must run once the view
  // exists — the ref callback fires after they first evaluate.
  const [editorView, setEditorView] = createSignal<EditorView | undefined>(undefined);

  // Which kiln owns the open file. A buffer's wikilinks resolve in the kiln
  // holding that FILE — following a link out of a note opened from another
  // kiln (a canvas card, a search hit, a hover popover) used to land in
  // whichever kiln the status bar pointed at, which is one kiln serving
  // another kiln's data.
  const [kilns, setKilns] = createSignal<{ path: string }[]>([]);
  // Kept as a promise as well as a signal. `swrLocal` applies nothing
  // synchronously on a cold start (first run, cleared storage, private mode),
  // and a click in that window has no kiln to resolve against — it would toast
  // a misleading "Note not found" and never retry. Follow-link awaits this.
  let kilnsReady: Promise<void> | undefined;
  onMount(() => {
    kilnsReady = Promise.resolve(swrLocal('kilns', listKilns, setKilns)).then(() => undefined);
  });

  const owningKiln = (path?: string) => (path ? kilnForPath(path, kilns()) : undefined);

  /** Resolve the owning kiln, waiting for the roster on a cold start. */
  const owningKilnAsync = async (path?: string) => {
    if (!path) return undefined;
    const immediate = owningKiln(path);
    if (immediate || kilns().length > 0) return immediate;
    await kilnsReady?.catch(() => undefined);
    return owningKiln(path);
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

  const menuItemClass =
    'flex items-center gap-2 px-3 py-1.5 cursor-pointer data-[highlighted]:bg-hover-wash';

  const fileData = () => openFiles().find(f => f.path === props.filePath) ?? null;

  // A pending proposed edit for this file → the editor shows it as an inline
  // diff (openFileWithDiff). Cleared on Dismiss.
  const pendingDiff = () => (props.filePath ? pendingDiffStore.get(props.filePath) : undefined);

  /** A proposal can't be staged over unsaved work — see the effect below. */
  const blockedByUnsavedEdits = () => {
    const diff = pendingDiff();
    const file = fileData();
    return !!diff && !!file && file.dirty && file.content !== diff.proposed;
  };

  // Stage the proposal into the BUFFER MODEL, not just the editor view.
  // Accepting a merge chunk only drops the deletion widget — the doc already
  // holds the new text, so it fires no change event. If the model kept the
  // on-disk content, saving after accepting would write the ORIGINAL back and
  // silently discard the whole proposal. Staging makes "accept everything then
  // save" the identity it looks like, and a REJECT (which does edit the doc)
  // flows back through onChange as usual. Guarded on the staged text so that
  // reject-driven store updates don't get forced back to the proposal.
  //
  // NEVER over unsaved edits: the baseline came from disk, so staging would
  // overwrite the user's own in-buffer work with content it never contained
  // (and Dismiss would then "restore" the disk text, losing it for good). The
  // review waits instead — save or discard, and the effect stages on the next
  // run. Every other tool in this space diffs against disk and requires an
  // explicit accept; none silently clobbers a dirty buffer.
  let staged: string | null = null;
  createEffect(() => {
    const diff = pendingDiff();
    const path = props.filePath;
    const file = fileData();
    if (!diff || !path || !file) {
      staged = null;
      return;
    }
    if (staged === diff.proposed) return;
    if (file.dirty && file.content !== diff.proposed) return;
    staged = diff.proposed;
    untrack(() => updateFileContent(path, diff.proposed));
  });

  // A saved review is over. Without this the banner lingers over content that
  // is already on disk, and — because the store is global and path-keyed —
  // reopening the file later would re-stage the stale proposal over it.
  createEffect(() => {
    const diff = pendingDiff();
    const file = fileData();
    const path = props.filePath;
    if (!diff || !file || !path) return;
    if (file.dirty || file.content === diff.original) return;
    untrack(() => pendingDiffActions.clear(path));
  });

  onCleanup(() => {
    // Closing the buffer abandons the review; leaving the entry behind would
    // silently re-stage it the next time this path is opened.
    if (props.filePath) pendingDiffActions.clear(props.filePath);
  });

  const dismissDiff = () => {
    const path = props.filePath;
    if (!path) return;
    // Put the staged proposal back to the on-disk baseline — dismissing a
    // review must not leave the proposed text sitting in the buffer. Only when
    // it actually differs: updateFileContent always flags dirty, and a
    // never-staged file must not be left falsely modified.
    const diff = pendingDiffStore.get(path);
    if (diff && fileData()?.content !== diff.original) {
      updateFileContent(path, diff.original);
    }
    pendingDiffActions.clear(path);
  };

  const handleSave = () => {
    if (props.filePath) void saveFile(props.filePath);
  };

  // ===== Review layer =========================================================
  //
  // The composed diff for this buffer, projected onto its current line
  // numbers. Asked for by PATH rather than by session: a center-region buffer
  // does not belong to a chat tab, and with several sessions on one workspace
  // the honest answer is every session that touched it.
  const reviewMarks = createMemo<ReviewHunkMark[]>(() => {
    const path = props.filePath;
    if (!path) return [];
    return reviewStore.hunksForOpenPath(path).map(({ hunk }) => ({
      id: hunk.id,
      start: hunk.current_range.start,
      end: hunk.current_range.end,
      state: hunk.state,
      external: isExternal(hunk),
      // The tool name, not "turn 7 · Edit": the turn coordinate does not cross
      // the wire on a composed hunk, and a number inferred here would be a
      // guess dressed as attribution.
      label: hunk.tool_call_ids.length > 0 ? toolCallLabel(hunk.tool_call_ids[0]) : 'external',
      toolCallId: hunk.tool_call_ids[0] ?? null,
    }));
  });

  // Sync the layer with the file's hunks — which installs it on the first one
  // and leaves a hunk-free file with no gutter column at all.
  //
  // Deferred to a microtask because `CodeMirrorEditor` rebuilds its entire
  // configuration with `StateEffect.reconfigure` whenever one of seven props
  // changes, discarding anything appended from outside. Its effect is created
  // after this one and so runs later in the same flush; a microtask lands
  // after the whole flush, when the rebuilt config is in place.
  //
  // KNOWN GAP: the markdown reading/live mode toggle is internal to
  // EditorWithPreview, so it is not one of the dependencies below. The layer
  // reappears on the next store refresh (any tool result or turn end). The
  // real fix is one `extensions.push(reviewDecorations(…))` inside
  // `createExtensions()`, which belongs to that component.
  createEffect(() => {
    const marks = reviewMarks();
    // Reconfigure triggers visible from here — each one drops the layer.
    void props.filePath;
    void settings.editor.vimMode;
    void settings.editor.maxLineWidth;
    void settings.editor.renderMath;
    void settings.editor.renderDiagrams;
    void pendingDiff()?.original;
    const view = editorView();
    if (!view) return;
    queueMicrotask(() => {
      if (editorView() !== view) return;
      syncReviewLayer(view, marks, {
        onReveal: (h) => h.toolCallId && reviewActions.revealToolCall(h.toolCallId),
      });
    });
  });

  // A hunk picked in the Changes panel scrolls THIS buffer to it. Tab metadata
  // cannot carry this: a panel only re-renders when its active tab id changes,
  // so an already-open file would never see the request.
  createEffect(() => {
    const target = pendingReveal();
    const view = editorView();
    if (!target || !view || target.path !== props.filePath) return;
    const line = Math.min(Math.max(target.line, 1), view.state.doc.lines);
    const pos = view.state.doc.line(line).from;
    view.dispatch({
      selection: { anchor: pos },
      effects: EditorView.scrollIntoView(pos, { y: 'center' }),
    });
    reviewActions.clearReveal();
  });

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

      {/* Proposed-edit review banner — shown while an agent's diff is overlaid
          on this file (openFileWithDiff). Accept/reject per hunk lives in the
          editor gutter; this bar frames it and offers a one-click Dismiss. */}
      <Show when={pendingDiff()}>
        <div class="mx-3 mt-2 px-3 py-1.5 rounded-md border border-attention/50 bg-attention/[0.06] flex items-center gap-2 text-[12px]">
          <Pencil class="w-3.5 h-3.5 text-attention shrink-0" />
          <Show
            when={!blockedByUnsavedEdits()}
            fallback={
              <>
                <span class="text-shell-ink">Proposed change waiting</span>
                <span class="text-muted-dark">
                  — save or discard your unsaved edits and it will load here
                </span>
              </>
            }
          >
            <span class="text-shell-ink">Reviewing proposed change</span>
            <span class="text-muted-dark">
              — accept or reject each hunk in the gutter, then save to apply
            </span>
          </Show>
          <button
            onClick={dismissDiff}
            class="ml-auto shrink-0 rounded px-2 py-0.5 text-muted-dark hover:text-shell-ink hover:bg-hover-wash"
          >
            Dismiss
          </button>
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
                  // The proposal is staged INTO the buffer above, so the file
                  // content is the single source of truth here — per-hunk
                  // rejections stay put instead of being overwritten by a
                  // stale copy of the original proposal.
                  content={file().content}
                  // Not while unsaved edits block staging: diffing the user's
                  // own dirty buffer against disk would show hunks that are
                  // theirs, not the agent's.
                  diffOriginal={blockedByUnsavedEdits() ? undefined : pendingDiff()?.original}
                  path={file().path}
                  onChange={(content) => updateFileContent(file().path, content)}
                  onSave={handleSave}
                  kiln={owningKiln(file().path)}
                  onFollowLink={(target) =>
                    // The file's own kiln, or none. Falling back to the active
                    // kiln let a project file — which belongs to no kiln —
                    // follow links into whichever kiln was showing.
                    void owningKilnAsync(file().path).then((kiln) =>
                      openNoteInEditor(target, kiln),
                    )
                  }
                  vimMode={settings.editor.vimMode}
                  lineWidth={settings.editor.maxLineWidth}
                  renderMath={settings.editor.renderMath}
                  renderDiagrams={settings.editor.renderDiagrams}
                  editorApiRef={(view) => setEditorView(() => view)}
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

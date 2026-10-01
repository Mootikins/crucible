/**
 * Markdown editing with three modes, Obsidian-shaped:
 *
 * - **live** (default for markdown): prose-first live preview — styled
 *   text with syntax marks hidden, except the construct under the cursor
 *   (see live-preview.ts).
 * - **source**: the mono, everything-raw code-editor flow.
 * - **reading**: the fully rendered, non-editable view (Mod-Shift-E,
 *   since plain Ctrl-E belongs to vim's scroll-line).
 *
 * Non-markdown files are always source with no mode controls.
 */
import { Component, Show, createSignal, createEffect } from 'solid-js';
import { HistoryNav, type HistoryNavProps } from './HistoryNav';
import { Breadcrumb } from './Breadcrumb';
import type { FileOpenOptions } from '@/lib/file-actions';
import { CodeMirrorEditor } from './CodeMirrorEditor';
import { MarkdownPreview } from './MarkdownPreview';
import { NoteViewSwitch, type EditorMode } from './NoteViewSwitch';
import { editNote } from '@/lib/offline/sync';
import { applyTaskToggle, taskEditForLine } from '@/lib/task-toggle';
import { notificationActions } from '@/stores/notificationStore';
import { isMarkdownPath } from '@/lib/markdown-path';


export const EditorWithPreview: Component<{
  content: string;
  editorStates?: Map<string, import('@codemirror/state').EditorState>;
  history?: HistoryNavProps;
  path: string;
  onChange: (content: string) => void;
  onSave?: () => void;
  onFollowLink?: (target: string, options?: FileOpenOptions) => void;
  /**
   * Kiln owning the file in the buffer. Wikilinks resolve here — in the
   * rendered view, and (via `data-kiln`) in the document-level hover
   * popovers — so a buffer from one kiln can never follow a link into
   * another. Absent for files that belong to no kiln.
   */
  kiln?: string;
  /** The disk hash the buffer was read at. A task tick carries it: a PATCH
   * with a base is refused when the note moved on, even when the anchor still
   * applies, so a tick never lands on text the user did not see. The outbox
   * replay sends no base. */
  baseHash: string;
  /** A tick that landed changed the note on disk. The answered hash is the
   * buffer's new base, or the next whole save is stale by construction. */
  onBaseChange?: (hash: string) => void;
  vimMode?: boolean;
  /** Mode a markdown file opens in (hover popovers pass the configured
   * hover mode; default live). Non-markdown is always source. */
  initialMode?: EditorMode;
  /** Drive the mode from outside. The compact shell does: its app bar carries
   * the Read/Write control. Supplying it hides the desktop toolbar. */
  mode?: EditorMode;
  onModeChange?: (mode: EditorMode) => void;
  /** Readable line length in px (0 = full width). */
  lineWidth?: number;
  /** Render `$…$`/`$$…$$` as KaTeX in live preview (default true). */
  renderMath?: boolean;
  /** Render ```mermaid fences as diagrams in live preview (default true). */
  renderDiagrams?: boolean;
  /** Hide the blank lines between frontmatter and the first content line. */
  hideFrontmatterGap?: boolean;
  reflowParagraphs?: boolean;
  /** Hand the live EditorView up (context-menu clipboard ops). */
  editorApiRef?: (view: import('@codemirror/view').EditorView) => void;
  /** Scroll to the first wikilink targeting this note key on open. */
  scrollToNote?: string;
  /** Exact referencing line (1-based); beats the scrollToNote scan. */
  scrollToLine?: number;
}> = (props) => {
  const isMarkdown = () => isMarkdownPath(props.path);

  /**
   * Tick a task box from the reading view, as ONE anchored line edit.
   *
   * Not a whole write. A whole write sends the entire note, so a tick would
   * overwrite an agent's edit to a different paragraph of the same file. This
   * is the case section 13 of the mobile design note describes.
   *
   * The box flips locally first, so the tap feels immediate. The answer from
   * `editNote` decides what stands. A refusal names the edit that failed, and
   * the buffer goes back to what it was. A queued tick stays: the outbox
   * holds it, and the app bar already shows the pending count.
   */
  const toggleTask = (sourceLine: number) => {
    const before = props.content;
    const edit = taskEditForLine(before, sourceLine);
    const optimistic = applyTaskToggle(before, sourceLine);
    if (!edit || optimistic === null) return;

    props.onChange(optimistic);
    void editNote({
      path: props.path,
      edits: [edit],
      base: props.baseHash,
      kiln: props.kiln ?? null,
    })
      .then((answer) => {
        if (answer.queued) return;
        if (answer.ok) {
          props.onBaseChange?.(answer.hash);
          return;
        }
        props.onChange(before);
        notificationActions.addNotification(
          'warning',
          answer.stale_base
            ? 'The note changed elsewhere. Reopen it to see the current text.'
            : 'That task could not be ticked — the line it was on has moved.',
        );
      })
      .catch((error: unknown) => {
        props.onChange(before);
        notificationActions.addNotification('error', `Could not tick the task: ${error}`);
      });
  };
  const defaultMode = (): EditorMode => (isMarkdown() ? (props.initialMode ?? 'live') : 'source');
  const [ownMode, setOwnMode] = createSignal<EditorMode>(defaultMode());
  const controlled = () => props.mode !== undefined;
  const mode = () => (controlled() ? props.mode! : ownMode());
  const setMode = (next: EditorMode | ((m: EditorMode) => EditorMode)) => {
    const value = typeof next === 'function' ? next(mode()) : next;
    if (controlled()) props.onModeChange?.(value);
    else setOwnMode(value);
  };

  // A different file starts back in its default mode — reading is a
  // per-look choice, and markdown always leads with prose.
  createEffect(() => {
    props.path;
    // A controlled mode belongs to its owner; only the internal one resets.
    if (!controlled()) setOwnMode(defaultMode());
  });

  return (
    <div class="note-editor flex flex-col h-full w-full" data-kiln={props.kiln || undefined} onKeyDown={(event) => {
      if (!event.altKey || (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight')) return;
      const step = event.key === 'ArrowLeft' ? -1 : 1;
      if (!(step === -1 ? props.history?.canBack : props.history?.canForward)) return;
      event.preventDefault();
      props.history?.onGo(step);
    }}>
      <Show when={isMarkdown() && !controlled()}>
        <div class="note-toolbar shrink-0" role="toolbar" aria-label="Note view">
          <Show when={props.history}>{(history) => <HistoryNav {...history()} />}</Show>
          <Breadcrumb root={props.kiln?.split('/').pop() || ''} path={props.kiln && props.path.startsWith(`${props.kiln}/`) ? props.path.slice(props.kiln.length + 1) : props.path.replace(/^\//, '')} />
          <NoteViewSwitch mode={mode()} onChange={setMode} />
        </div>
      </Show>
      <div class="relative min-h-0 flex-1 overflow-hidden">
      <Show
        when={mode() !== 'reading' || !isMarkdown()}
        fallback={
          <MarkdownPreview
            content={props.content}
            path={props.path}
            kiln={props.kiln}
            maxWidth={props.lineWidth}
            scrollToNote={props.scrollToNote}
            onToggleTask={toggleTask}
            onFollowLink={props.onFollowLink}
          />
        }
      >
        <CodeMirrorEditor
          apiRef={props.editorApiRef}
          editorStates={props.editorStates}
          content={props.content}
          path={props.path}
          onChange={props.onChange}
          onSave={props.onSave}
          onFollowLink={props.onFollowLink}
          kiln={props.kiln}
          vimMode={props.vimMode}
          livePreview={isMarkdown() && mode() === 'live'}
          lineWidth={props.lineWidth}
          renderMath={props.renderMath}
          renderDiagrams={props.renderDiagrams}
          hideFrontmatterGap={props.hideFrontmatterGap}
          reflowParagraphs={props.reflowParagraphs}
          onTogglePreview={isMarkdown() ? () => setMode('reading') : undefined}
          scrollToNote={props.scrollToNote}
          scrollToLine={props.scrollToLine}
        />
      </Show>
      </div>
    </div>
  );
};

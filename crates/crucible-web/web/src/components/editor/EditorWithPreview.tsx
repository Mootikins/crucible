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
import { CodeMirrorEditor } from './CodeMirrorEditor';
import { MarkdownPreview } from './MarkdownPreview';
import { Eye, Pencil, Code } from '@/lib/icons';
import { editNote } from '@/lib/offline/sync';
import { applyTaskToggle, taskEditForLine } from '@/lib/task-toggle';
import { notificationActions } from '@/stores/notificationStore';
import { isMarkdownPath } from '@/lib/markdown-path';

type EditorMode = 'live' | 'source' | 'reading';

export const EditorWithPreview: Component<{
  content: string;
  path: string;
  onChange: (content: string) => void;
  onSave?: () => void;
  onFollowLink?: (target: string) => void;
  /**
   * Kiln owning the file in the buffer. Wikilinks resolve here — in the
   * rendered view, and (via `data-kiln`) in the document-level hover
   * popovers — so a buffer from one kiln can never follow a link into
   * another. Absent for files that belong to no kiln.
   */
  kiln?: string;
  /** The disk hash the buffer was read at. A task tick carries it, so a tick
   * on a note that changed elsewhere is refused instead of landing on text
   * the user did not see. */
  baseHash?: string;
  /** A tick that landed changed the note on disk. The answered hash is the
   * buffer's new base, or the next whole save is stale by construction. */
  onBaseChange?: (hash: string) => void;
  vimMode?: boolean;
  /** Mode a markdown file opens in (hover popovers pass the configured
   * hover mode; default live). Non-markdown is always source. */
  initialMode?: EditorMode;
  /** Drive the mode from outside. The compact shell does: its app bar carries
   * the Read/Write control, because these floating buttons are ~26 px and sit
   * over the text. Supplying it also hides them. */
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
  /** Proposed-edit review: `content` is the proposed text shown as an inline
   * diff against this original. Forces the source editor (not the reading view). */
  diffOriginal?: string;
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
      base: props.baseHash ?? '',
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
  const defaultMode = (): EditorMode =>
    isMarkdown() ? (props.initialMode ?? 'live') : 'source';
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

  const modeButton = 'rounded border border-hairline bg-surface-elevated/90 p-1.5 text-muted hover:text-shell-ink hover:border-primary/50 transition-colors';

  return (
    <div class="relative h-full w-full" data-kiln={props.kiln || undefined}>
      <Show when={isMarkdown() && !controlled()}>
        <div class="absolute right-3 top-2 z-10 flex items-center gap-1">
          {/* Live ↔ source: the prose flow vs the mono/raw code flow. */}
          <Show when={mode() !== 'reading'}>
            <button
              type="button"
              data-testid="mode-toggle"
              title={mode() === 'live' ? 'Source mode' : 'Live preview'}
              onClick={() => setMode((m) => (m === 'live' ? 'source' : 'live'))}
              class={modeButton}
            >
              <Show when={mode() === 'live'} fallback={<Pencil class="h-3.5 w-3.5" />}>
                <Code class="h-3.5 w-3.5" />
              </Show>
            </button>
          </Show>
          <button
            type="button"
            data-testid="preview-toggle"
            title={mode() === 'reading' ? 'Edit (Ctrl+Shift+E)' : 'Reading view (Ctrl+Shift+E)'}
            onClick={() => setMode((m) => (m === 'reading' ? 'live' : 'reading'))}
            class={modeButton}
          >
            <Show when={mode() === 'reading'} fallback={<Eye class="h-3.5 w-3.5" />}>
              <Pencil class="h-3.5 w-3.5" />
            </Show>
          </button>
        </div>
      </Show>
      <Show
        when={mode() !== 'reading' || !isMarkdown() || props.diffOriginal != null}
        fallback={
          <MarkdownPreview
            content={props.content}
            path={props.path}
            kiln={props.kiln}
            maxWidth={props.lineWidth}
            scrollToNote={props.scrollToNote}
            onToggleTask={toggleTask}
          />
        }
      >
        <CodeMirrorEditor
          apiRef={props.editorApiRef}
          content={props.content}
          path={props.path}
          diffOriginal={props.diffOriginal}
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
          onTogglePreview={isMarkdown() ? () => setMode('reading') : undefined}
          scrollToNote={props.scrollToNote}
          scrollToLine={props.scrollToLine}
        />
      </Show>
    </div>
  );
};

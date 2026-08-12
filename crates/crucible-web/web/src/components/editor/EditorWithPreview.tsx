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
  vimMode?: boolean;
  /** Mode a markdown file opens in (hover popovers pass the configured
   * hover mode; default live). Non-markdown is always source. */
  initialMode?: EditorMode;
  /** Readable line length in px (0 = full width). */
  lineWidth?: number;
  /** Render `$…$`/`$$…$$` as KaTeX in live preview (default true). */
  renderMath?: boolean;
  /** Render ```mermaid fences as diagrams in live preview (default true). */
  renderDiagrams?: boolean;
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
  const defaultMode = (): EditorMode =>
    isMarkdown() ? (props.initialMode ?? 'live') : 'source';
  const [mode, setMode] = createSignal<EditorMode>(defaultMode());

  // A different file starts back in its default mode — reading is a
  // per-look choice, and markdown always leads with prose.
  createEffect(() => {
    props.path;
    setMode(defaultMode());
  });

  const modeButton = 'rounded border border-hairline bg-surface-elevated/90 p-1.5 text-muted hover:text-shell-ink hover:border-primary/50 transition-colors';

  return (
    <div class="relative h-full w-full" data-kiln={props.kiln || undefined}>
      <Show when={isMarkdown()}>
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
          onTogglePreview={isMarkdown() ? () => setMode('reading') : undefined}
          scrollToNote={props.scrollToNote}
          scrollToLine={props.scrollToLine}
        />
      </Show>
    </div>
  );
};

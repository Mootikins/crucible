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

const isMarkdownPath = (path: string) => /\.(md|markdown)$/i.test(path);

type EditorMode = 'live' | 'source' | 'reading';

export const EditorWithPreview: Component<{
  content: string;
  path: string;
  onChange: (content: string) => void;
  onSave?: () => void;
  onFollowLink?: (target: string) => void;
  vimMode?: boolean;
}> = (props) => {
  const isMarkdown = () => isMarkdownPath(props.path);
  const [mode, setMode] = createSignal<EditorMode>(isMarkdown() ? 'live' : 'source');

  // A different file starts back in its default mode — reading is a
  // per-look choice, and markdown always leads with prose.
  createEffect(() => {
    props.path;
    setMode(isMarkdown() ? 'live' : 'source');
  });

  const modeButton = 'rounded border border-white/10 bg-surface-elevated/90 p-1.5 text-muted hover:text-shell-ink hover:border-primary/50 transition-colors';

  return (
    <div class="relative h-full w-full">
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
        when={mode() !== 'reading' || !isMarkdown()}
        fallback={<MarkdownPreview content={props.content} />}
      >
        <CodeMirrorEditor
          content={props.content}
          path={props.path}
          onChange={props.onChange}
          onSave={props.onSave}
          onFollowLink={props.onFollowLink}
          vimMode={props.vimMode}
          livePreview={isMarkdown() && mode() === 'live'}
          onTogglePreview={isMarkdown() ? () => setMode('reading') : undefined}
        />
      </Show>
    </div>
  );
};

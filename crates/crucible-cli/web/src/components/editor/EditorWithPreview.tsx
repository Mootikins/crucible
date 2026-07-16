/**
 * CodeMirror editor with an Edit ↔ Preview toggle for markdown notes —
 * the "rendered reading view" half of the live-preview story (inline
 * Obsidian-style WYSIWYG remains future work).
 *
 * Toggle affordances: the corner button, or Mod-Shift-E from the editor
 * (plain Ctrl-E belongs to vim's scroll-line when vim mode is on).
 */
import { Component, Show, createSignal, createEffect } from 'solid-js';
import { CodeMirrorEditor } from './CodeMirrorEditor';
import { MarkdownPreview } from './MarkdownPreview';
import { Eye, Pencil } from '@/lib/icons';

const isMarkdownPath = (path: string) => /\.(md|markdown)$/i.test(path);

export const EditorWithPreview: Component<{
  content: string;
  path: string;
  onChange: (content: string) => void;
  onSave?: () => void;
  onFollowLink?: (target: string) => void;
  vimMode?: boolean;
}> = (props) => {
  const [previewing, setPreviewing] = createSignal(false);

  // A different file starts back in edit mode — preview is a per-look choice,
  // not a sticky mode.
  createEffect(() => {
    props.path;
    setPreviewing(false);
  });

  const canPreview = () => isMarkdownPath(props.path);

  return (
    <div class="relative h-full w-full">
      <Show when={canPreview()}>
        <button
          type="button"
          data-testid="preview-toggle"
          title={previewing() ? 'Edit (Ctrl+Shift+E)' : 'Preview (Ctrl+Shift+E)'}
          onClick={() => setPreviewing((p) => !p)}
          class="absolute right-3 top-2 z-10 rounded border border-white/10 bg-surface-elevated/90 p-1.5 text-muted hover:text-shell-ink hover:border-primary/50 transition-colors"
        >
          <Show when={previewing()} fallback={<Eye class="h-3.5 w-3.5" />}>
            <Pencil class="h-3.5 w-3.5" />
          </Show>
        </button>
      </Show>
      <Show
        when={!previewing() || !canPreview()}
        fallback={<MarkdownPreview content={props.content} />}
      >
        <CodeMirrorEditor
          content={props.content}
          path={props.path}
          onChange={props.onChange}
          onSave={props.onSave}
          onFollowLink={props.onFollowLink}
          vimMode={props.vimMode}
          onTogglePreview={canPreview() ? () => setPreviewing(true) : undefined}
        />
      </Show>
    </div>
  );
};

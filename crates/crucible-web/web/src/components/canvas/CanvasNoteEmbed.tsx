import { Component, Show, createEffect, createSignal, onCleanup } from 'solid-js';
import { CodeMirrorEditor } from '../editor/CodeMirrorEditor';
import { getFileContent, saveFileContent } from '@/lib/api';

/**
 * A note card that is a live, editable view of the real file.
 *
 * This is what "including notes" means and why the renderer had to be DOM: an
 * editor cannot be hosted inside a painted 2D canvas. Edits write back to the
 * source note, so a card is a window onto the file rather than a copy of it —
 * matching Obsidian, where dragging a note onto a canvas gives you a live
 * portal.
 *
 * The embed manages its own content and saves. The canvas document only ever
 * stores the *path*, so note edits never touch the `.canvas` file and cannot
 * collide with canvas-level undo. Those are genuinely separate documents with
 * separate histories, and conflating them would make Ctrl+Z ambiguous.
 */

interface CanvasNoteEmbedProps {
  /** Absolute path to the note. */
  absPath: string;
  /** Whether the card is the active selection — only then is it editable. */
  editable: boolean;
  onDirtyChange?: (dirty: boolean) => void;
}

export const CanvasNoteEmbed: Component<CanvasNoteEmbedProps> = (props) => {
  const [content, setContent] = createSignal<string | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [dirty, setDirty] = createSignal(false);

  createEffect(() => {
    const path = props.absPath;
    setError(null);
    setContent(null);
    getFileContent(path)
      .then((text: string) => setContent(text))
      .catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)));
  });

  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => {
    clearTimeout(saveTimer);
    // A card scrolled out of the viewport unmounts (virtualization). Flushing
    // here means panning away from a card mid-edit does not silently discard
    // the edit — without this, virtualization would become data loss.
    if (dirty()) void flush();
  });

  const flush = async () => {
    const text = content();
    if (text === null) return;
    try {
      await saveFileContent(props.absPath, text);
      setDirty(false);
      props.onDirtyChange?.(false);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const onChange = (next: string) => {
    setContent(next);
    setDirty(true);
    props.onDirtyChange?.(true);
    clearTimeout(saveTimer);
    saveTimer = setTimeout(flush, 800);
  };

  return (
    <div class="h-full w-full overflow-auto" data-testid="canvas-note-embed">
      <Show when={error()}>
        <div class="px-3 py-2 text-xs text-error" data-testid="canvas-embed-error">
          {error()}
        </div>
      </Show>

      <Show when={content() !== null} fallback={<div class="px-3 py-2 text-xs text-muted">Loading…</div>}>
        <Show
          when={props.editable}
          fallback={
            // Read-only until the card is selected: a canvas is mostly read at
            // a glance, and mounting a live editor per visible card would cost
            // far more than it buys.
            <pre class="whitespace-pre-wrap px-3 py-2 font-sans text-xs text-shell-ink">
              {content()}
            </pre>
          }
        >
          <CodeMirrorEditor
            content={content()!}
            path={props.absPath}
            onChange={onChange}
            onSave={() => void flush()}
            livePreview
          />
        </Show>
      </Show>
    </div>
  );
};

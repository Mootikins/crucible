import { Component, For, Show, createMemo, createResource, createSignal, onMount } from 'solid-js';
import { listNotes } from '@/lib/api';
import { FileText } from '@/lib/icons';

/**
 * Pick a kiln note to drop onto the canvas as a file card.
 *
 * Obsidian gets this from dragging a note out of the file explorer. That
 * gesture is worth having too, but it cannot be the *only* route: it requires
 * the explorer to be open and visible beside the canvas, which on a narrow
 * window or a tablet it simply is not. A picker makes "reference a note"
 * reachable from the canvas itself.
 *
 * Restricted to the canvas's own kiln, matching the containment rule — a canvas
 * may only reference files inside the kiln that owns it, so offering anything
 * else would be offering a choice the server is going to refuse.
 */
export const NotePicker: Component<{
  kiln: string;
  onPick: (relPath: string) => void;
  onClose: () => void;
}> = (props) => {
  const [query, setQuery] = createSignal('');
  let input: HTMLInputElement | undefined;

  const [notes] = createResource(
    () => props.kiln,
    async (kiln) => {
      try {
        return await listNotes(kiln);
      } catch {
        return [];
      }
    },
  );

  const relative = (path: string) => {
    const root = props.kiln.replace(/\/$/, '');
    return path.startsWith(root) ? path.slice(root.length + 1) : path;
  };

  const matches = createMemo(() => {
    const q = query().toLowerCase().trim();
    const all = (notes() ?? []).map((n) => ({
      rel: relative(n.path),
      title: n.title || relative(n.path),
    }));
    if (!q) return all.slice(0, 50);
    return all.filter((n) => `${n.rel} ${n.title}`.toLowerCase().includes(q)).slice(0, 50);
  });

  onMount(() => input?.focus());

  return (
    <div
      class="absolute inset-0 z-20 flex items-start justify-center bg-black/30 pt-16"
      data-testid="canvas-note-picker"
      onPointerDown={(e) => {
        if (e.target === e.currentTarget) props.onClose();
      }}
    >
      <div
        class="flex max-h-[60%] w-96 flex-col overflow-hidden rounded-lg border border-hairline bg-surface-elevated shadow-lg"
        onPointerDown={(e) => e.stopPropagation()}
      >
        <input
          ref={input}
          class="border-b border-hairline bg-transparent px-3 py-2 text-sm text-shell-ink outline-none"
          placeholder="Reference a note…"
          value={query()}
          data-testid="canvas-note-picker-input"
          onInput={(e) => setQuery(e.currentTarget.value)}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === 'Escape') props.onClose();
            if (e.key === 'Enter' && matches()[0]) props.onPick(matches()[0].rel);
          }}
        />
        <div class="min-h-0 flex-1 overflow-auto py-1">
          <Show
            when={matches().length > 0}
            fallback={<div class="px-3 py-2 text-xs text-muted">No matching notes</div>}
          >
            <For each={matches()}>
              {(note) => (
                <button
                  type="button"
                  class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs text-shell-ink hover:bg-hover-wash"
                  data-testid="canvas-note-picker-item"
                  onClick={() => props.onPick(note.rel)}
                >
                  <FileText class="h-3 w-3 shrink-0 text-muted" />
                  <span class="truncate">{note.title}</span>
                  <span class="ml-auto truncate text-[10px] text-muted-dark">{note.rel}</span>
                </button>
              )}
            </For>
          </Show>
        </div>
      </div>
    </div>
  );
};

import { Component, Show, For, createSignal } from 'solid-js';

interface PrecognitionBadgeProps {
  notesCount: number;
  notes: { name: string; relevance: number }[];
}

/**
 * Compact badge rendered on the user message that triggered Precognition
 * auto-enrichment. Shows the number of notes injected; clickable to expand
 * and reveal the note names + relevance scores.
 */
export const PrecognitionBadge: Component<PrecognitionBadgeProps> = (props) => {
  const [expanded, setExpanded] = createSignal(false);

  return (
    <div
      class="mt-1 inline-flex flex-col rounded border border-neutral-700/50 bg-neutral-900/40 px-2 py-1 text-neutral-300"
      data-testid="precognition-badge"
    >
      <button
        type="button"
        class="flex items-center gap-1.5 text-[11px] text-neutral-400 hover:text-neutral-200"
        onClick={() => setExpanded((v) => !v)}
        title="Click to show injected notes"
        data-testid="precognition-badge-toggle"
      >
        <span aria-hidden>🔮</span>
        <span>
          Enriched with {props.notesCount} {props.notesCount === 1 ? 'note' : 'notes'}
        </span>
        <Show when={props.notes.length > 0}>
          <span class="text-neutral-500">{expanded() ? '▼' : '▶'}</span>
        </Show>
      </button>
      <Show when={expanded() && props.notes.length > 0}>
        <ul
          class="mt-1 flex flex-col gap-0.5"
          data-testid="precognition-badge-notes"
        >
          <For each={props.notes}>
            {(note) => (
              <li class="flex items-center gap-2 text-[11px] font-mono text-neutral-400">
                <span class="text-neutral-200 truncate">{note.name}</span>
                <span class="text-neutral-500">{note.relevance.toFixed(2)}</span>
              </li>
            )}
          </For>
        </ul>
      </Show>
    </div>
  );
};

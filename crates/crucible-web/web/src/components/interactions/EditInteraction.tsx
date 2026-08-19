import { Component, Show, createSignal } from 'solid-js';
import type { CancelledResponse, EditResponse, InteractionOf } from '@/lib/types';
import { btnPrimary, btnNeutral } from '@/lib/button-style';

interface Props {
  request: InteractionOf<'edit'>;
  onRespond: (response: EditResponse | CancelledResponse) => void;
}

/**
 * A plain textarea, deliberately — not CodeMirror.
 *
 * The editor panel is for files the user opened and expects to keep working
 * in. This is a short-lived modal the agent is blocked on, and pulling the
 * editor's extensions, language detection and dirty-state machinery into it
 * would couple a plugin prompt to the document surface for a box someone types
 * three lines into. `format` still selects a monospace face so code does not
 * arrive proportional.
 */
export const EditInteraction: Component<Props> = (props) => {
  const [content, setContent] = createSignal(props.request.content);

  const isProse = () => (props.request.format ?? 'markdown') === 'markdown';
  const unchanged = () => content() === props.request.content;

  return (
    <div class="bg-surface-elevated rounded-lg p-4 mb-4 border border-hairline">
      <Show when={props.request.hint}>
        <p class="text-shell-ink font-medium mb-3">{props.request.hint}</p>
      </Show>

      <textarea
        value={content()}
        onInput={(e) => setContent(e.currentTarget.value)}
        rows={12}
        spellcheck={isProse()}
        class={`w-full px-3 py-2 mb-3 bg-control border border-hairline rounded-md text-shell-ink placeholder-muted-dark focus:outline-none focus:ring-2 focus:ring-primary resize-y ${
          isProse() ? '' : 'font-mono text-sm'
        }`}
      />

      <div class="flex items-center gap-2">
        <button
          onClick={() => props.onRespond({ kind: 'edit', modified: content() })}
          class={btnPrimary}
        >
          Save
        </button>
        <button onClick={() => props.onRespond({ kind: 'cancelled' })} class={btnNeutral}>
          Cancel
        </button>
        <Show when={unchanged()}>
          <span class="text-muted-dark text-xs">unchanged</span>
        </Show>
      </div>
    </div>
  );
};

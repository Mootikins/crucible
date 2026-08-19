import { Component, Show } from 'solid-js';
import type { CancelledResponse, InteractionOf } from '@/lib/types';
import { btnPrimary } from '@/lib/button-style';
import { renderMarkdown, PROSE_CLASS } from '@/lib/markdown';

interface Props {
  request: InteractionOf<'show'>;
  onRespond: (response: CancelledResponse) => void;
}

/**
 * `show` is the one variant that expects no answer — the asker is telling, not
 * asking. It still responds on dismiss, because the daemon side is parked on a
 * oneshot either way and a modal nobody can close is worse than a pointless
 * response.
 */
export const ShowInteraction: Component<Props> = (props) => {
  const isMarkdown = () => (props.request.format ?? 'markdown') === 'markdown';

  return (
    <div class="bg-surface-elevated rounded-lg p-4 mb-4 border border-hairline">
      <Show when={props.request.title}>
        <p class="text-shell-ink font-medium mb-3">{props.request.title}</p>
      </Show>

      <div class="mb-3 max-h-96 overflow-y-auto">
        <Show
          when={isMarkdown()}
          fallback={
            <pre class="text-shell-body text-sm whitespace-pre-wrap break-words font-mono">
              {props.request.content}
            </pre>
          }
        >
          {/* eslint-disable-next-line solid/no-innerhtml */}
          <div class={PROSE_CLASS} innerHTML={renderMarkdown(props.request.content)} />
        </Show>
      </div>

      <button onClick={() => props.onRespond({ kind: 'cancelled' })} class={btnPrimary}>
        Dismiss
      </button>
    </div>
  );
};

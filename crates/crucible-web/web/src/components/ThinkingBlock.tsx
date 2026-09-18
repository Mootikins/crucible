import { Component, Show, createEffect, createSignal, on } from 'solid-js';
import { ChevronRight } from 'lucide-solid';

interface ThinkingBlockProps {
  content: string;
  isStreaming: boolean;
  tokenCount?: number;
}

export const ThinkingBlock: Component<ThinkingBlockProps> = (props) => {
  // The fold follows the stream unless the reader overrides it: a block
  // opens while the model reasons (the text is the point), folds back to the
  // summary line when the reasoning ends, and a click takes over until the
  // NEXT stream starts. `null` means "follow the stream"; a click sets
  // true/false; a stream start clears the override.
  const [override, setOverride] = createSignal<boolean | null>(null);
  const isExpanded = () => override() ?? props.isStreaming;
  // A stream START re-arms the fold: the reader's override answered one
  // stream, not every stream after it.
  createEffect(on(() => props.isStreaming, (streaming) => {
    if (streaming) setOverride(null);
  }));
  const toggle = () => {
    if (props.content.length > 0) {
      setOverride((prev) => !(prev ?? props.isStreaming));
    }
  };

  const headerLabel = () => {
    if (props.isStreaming) {
      return 'Thinking';
    }
    if (props.tokenCount != null && props.tokenCount > 0) {
      return `Thought for ~${props.tokenCount} tokens`;
    }
    return 'Thought';
  };

  return (
    <div class="mb-2">
      {/* Clickable header */}
      <button
        type="button"
        onClick={toggle}
        class="flex items-center gap-1.5 text-xs text-muted hover:text-shell-body transition-colors cursor-pointer select-none group"
      >
        <span
          class="transition-transform duration-300 ease-in-out"
          style={{
            transform: isExpanded() ? 'rotate(90deg)' : 'rotate(0deg)',
          }}
        >
          <ChevronRight size={14} />
        </span>

        <span>{headerLabel()}</span>

        <Show when={props.isStreaming}>
          {/* The same wave as WorkingDots, at the same cadence — thinking and
              answering are one wait, so they must not run on two rhythms. */}
          <span class="ml-1 inline-flex items-center gap-0.5">
            <span class="cru-think-dot h-1 w-1 rounded-full bg-muted" />
            <span
              class="cru-think-dot h-1 w-1 rounded-full bg-muted"
              style={{ 'animation-delay': '160ms' }}
            />
            <span
              class="cru-think-dot h-1 w-1 rounded-full bg-muted"
              style={{ 'animation-delay': '320ms' }}
            />
          </span>
        </Show>
      </button>

      {/* Collapsible content with gridTemplateRows animation. Open while the
          model reasons — the streaming text is visible as it lands — and
          folded to the summary line once it ends. */}
      <div
        class="grid transition-[grid-template-rows] duration-300 ease-in-out"
        style={{
          'grid-template-rows': isExpanded() ? '1fr' : '0fr',
        }}
      >
        <div class="overflow-hidden">
          <div class="mt-2 pl-5 border-l-2 border-hairline">
            <p class="text-xs text-muted-dark italic whitespace-pre-wrap leading-relaxed">
              {props.content}
              {/* The text stream's caret, on the same steps() cadence: the
                  reasoning visibly grows token by token, and the caret is
                  what marks the growing edge. */}
              <Show when={props.isStreaming}>
                <span
                  class="cru-caret ml-0.5 inline-block h-[1.05em] w-[2px] translate-y-[0.15em] bg-muted"
                  data-testid="think-stream-caret"
                />
              </Show>
            </p>
          </div>
        </div>
      </div>
    </div>
  );
};

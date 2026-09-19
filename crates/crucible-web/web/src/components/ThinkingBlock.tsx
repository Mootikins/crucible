import { Component, For, Show, createEffect, createMemo, createSignal, on, onCleanup } from 'solid-js';
import { ChevronRight } from 'lucide-solid';

interface ThinkingBlockProps {
  content: string;
  isStreaming: boolean;
  tokenCount?: number;
}

/** Reveal cadence, in ms per tick. Providers deliver thinking in large
 *  bursts; rendering them whole reads as chunky jumps, so the block drips
 *  the words out on its own clock. */
const TICK_MS = 50;
/** How many trailing words render as individually fading spans. */
const FADE_TAIL = 12;

/** Word tokens with their trailing whitespace attached, so joining them
 *  reproduces the content byte for byte. */
function wordsOf(content: string): string[] {
  return content.match(/\S+\s*/g) ?? [];
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

  // The reveal buffer: `content` is the target (network bursts land whole);
  // `revealed` is how many words the reader has been shown. The ticker
  // advances at a steady cadence with catch-up, so a 40-word burst still
  // drains in a blink instead of pasting itself in — and a completed or
  // historical block reveals everything at once.
  const [revealed, setRevealed] = createSignal(0);
  createEffect(() => {
    if (!props.isStreaming) {
      setRevealed(wordsOf(props.content).length);
      return;
    }
    const id = setInterval(() => {
      const total = wordsOf(props.content).length;
      setRevealed((shown) => {
        if (shown >= total) return shown;
        // One word per tick while close; proportional catch-up when a big
        // burst lands, so the reveal never falls seconds behind the model.
        const backlog = total - shown;
        return Math.min(total, shown + Math.max(1, Math.ceil(backlog / 10)));
      });
    }, TICK_MS);
    onCleanup(() => clearInterval(id));
  });

  const words = createMemo(() => wordsOf(props.content));
  // Everything older than the fade tail is one plain text node — a long
  // reasoning block must not become two thousand permanent spans.
  const settled = createMemo(() => {
    const w = words();
    const shown = Math.min(revealed(), w.length);
    return w.slice(0, Math.max(0, shown - FADE_TAIL)).join('');
  });
  const fading = createMemo(() => {
    const w = words();
    const shown = Math.min(revealed(), w.length);
    return w.slice(Math.max(0, shown - FADE_TAIL), shown);
  });

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
              {settled()}
              {/* The newest words each sit in their own span and fade in over
                  their stay at the growing edge; they graduate into the plain
                  prefix as the reveal advances past them. */}
              <For each={fading()}>
                {(word) => (
                  <span class={props.isStreaming ? 'thinking-word-in' : undefined}>{word}</span>
                )}
              </For>
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

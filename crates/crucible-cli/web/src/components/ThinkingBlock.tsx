import { Component, Show, createSignal } from 'solid-js';
import { ChevronRight } from 'lucide-solid';

interface ThinkingBlockProps {
  content: string;
  isStreaming: boolean;
  tokenCount?: number;
}

export const ThinkingBlock: Component<ThinkingBlockProps> = (props) => {
  const [isExpanded, setIsExpanded] = createSignal(false);

  const toggle = () => {
    if (props.content.length > 0) {
      setIsExpanded((prev) => !prev);
    }
  };

  const headerLabel = () => {
    if (props.isStreaming) {
      return 'Thinking';
    }
    if (props.tokenCount != null && props.tokenCount > 0) {
      return `Thought for ${props.tokenCount} tokens`;
    }
    return 'Thought';
  };

  return (
    <div class="mb-2">
      {/* Clickable header */}
      <button
        type="button"
        onClick={toggle}
        class="flex items-center gap-1.5 text-xs text-neutral-400 hover:text-neutral-300 transition-colors cursor-pointer select-none group"
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
          <span class="inline-flex items-center gap-0.5 ml-1">
            <span class="w-1 h-1 bg-neutral-400 rounded-full animate-pulse" />
            <span
              class="w-1 h-1 bg-neutral-400 rounded-full animate-pulse"
              style={{ 'animation-delay': '150ms' }}
            />
            <span
              class="w-1 h-1 bg-neutral-400 rounded-full animate-pulse"
              style={{ 'animation-delay': '300ms' }}
            />
          </span>
        </Show>
      </button>

      {/* Collapsible content with gridTemplateRows animation */}
      <div
        class="grid transition-[grid-template-rows] duration-300 ease-in-out"
        style={{
          'grid-template-rows': isExpanded() ? '1fr' : '0fr',
        }}
      >
        <div class="overflow-hidden">
          <div class="mt-2 pl-5 border-l-2 border-neutral-700/50">
            <p class="text-xs text-neutral-500 italic whitespace-pre-wrap leading-relaxed">
              {props.content}
            </p>
          </div>
        </div>
      </div>
    </div>
  );
};

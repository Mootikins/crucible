import { Component, Show, createSignal } from 'solid-js';
import type { ToolCallSummary } from '@/lib/types';

interface ToolCardProps {
  tool: ToolCallSummary;
}

export const ToolCard: Component<ToolCardProps> = (props) => {
  const [expanded, setExpanded] = createSignal(false);

  const iconForTool = (title: string): string => {
    const lower = title.toLowerCase();
    if (lower.includes('read') || lower.includes('file')) return '📄';
    if (lower.includes('write') || lower.includes('edit')) return '✏️';
    if (lower.includes('search') || lower.includes('find')) return '🔍';
    if (lower.includes('bash') || lower.includes('shell') || lower.includes('exec')) return '⚡';
    if (lower.includes('web') || lower.includes('fetch') || lower.includes('http')) return '🌐';
    if (lower.includes('note') || lower.includes('memory')) return '📝';
    return '🔧';
  };

  return (
    <div class="border border-neutral-700 rounded-lg bg-neutral-850 overflow-hidden my-2">
      <button
        onClick={() => setExpanded(!expanded())}
        class="w-full flex items-center gap-2 px-3 py-2 hover:bg-neutral-700/50 transition-colors text-left"
      >
        <span class="text-lg">{iconForTool(props.tool.title)}</span>
        <span class="flex-1 text-sm font-medium text-neutral-200 truncate">
          {props.tool.title}
        </span>
        <span class="text-neutral-500 text-xs">
          {expanded() ? '▼' : '▶'}
        </span>
      </button>
      
      <Show when={expanded()}>
        <div class="px-3 py-2 border-t border-neutral-700 bg-neutral-900">
          <div class="text-xs text-neutral-500">ID: {props.tool.id}</div>
        </div>
      </Show>
    </div>
  );
};

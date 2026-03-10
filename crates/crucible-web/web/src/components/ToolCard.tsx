import { Component, Show, createSignal, createMemo, createEffect } from 'solid-js';
import type { ToolCallDisplay } from '@/lib/types';

interface ToolCardProps {
  toolCall: ToolCallDisplay;
}

export const ToolCard: Component<ToolCardProps> = (props) => {
  // Error state auto-expands so users can see what went wrong
  const [expanded, setExpanded] = createSignal(props.toolCall.status === 'error');

  // Auto-expand on error status change
  createEffect(() => {
    if (props.toolCall.status === 'error') {
      setExpanded(true);
    }
  });

  const iconForTool = (name: string): string => {
    const lower = name.toLowerCase();
    if (lower.includes('read') || lower.includes('file')) return '📄';
    if (lower.includes('write') || lower.includes('edit')) return '✏️';
    if (lower.includes('search') || lower.includes('find')) return '🔍';
    if (lower.includes('bash') || lower.includes('shell') || lower.includes('exec')) return '⚡';
    if (lower.includes('web') || lower.includes('fetch') || lower.includes('http')) return '🌐';
    if (lower.includes('note') || lower.includes('memory')) return '📝';
    return '🔧';
  };

  const statusIcon = () => {
    switch (props.toolCall.status) {
      case 'running':
        return (
          <span class="inline-flex items-center text-blue-400" title="Running">
            <svg class="w-4 h-4 animate-spin" viewBox="0 0 24 24" fill="none">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="3" />
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
            </svg>
          </span>
        );
      case 'complete':
        return <span class="text-emerald-400 text-sm font-bold" title="Complete">✓</span>;
      case 'error':
        return <span class="text-red-400 text-sm font-bold" title="Error">✗</span>;
    }
  };

  const statusBorderColor = () => {
    switch (props.toolCall.status) {
      case 'running': return 'border-blue-500/40';
      case 'complete': return 'border-emerald-500/30';
      case 'error': return 'border-red-500/40';
    }
  };

  const statusBgColor = () => {
    switch (props.toolCall.status) {
      case 'running': return 'bg-blue-950/20';
      case 'complete': return 'bg-neutral-850';
      case 'error': return 'bg-red-950/20';
    }
  };

  const formattedArgs = createMemo(() => {
    const args = props.toolCall.args;
    if (!args || args === '' || args === '""') return null;
    try {
      const parsed = JSON.parse(args);
      return JSON.stringify(parsed, null, 2);
    } catch {
      return args;
    }
  });

  return (
    <div class={`border ${statusBorderColor()} rounded-lg ${statusBgColor()} overflow-hidden my-2`}>
      <button
        onClick={() => setExpanded(!expanded())}
        class="w-full flex items-center gap-2 px-3 py-2 hover:bg-neutral-700/30 transition-colors text-left"
      >
        <span class="text-base leading-none">{iconForTool(props.toolCall.name)}</span>
        <span class="flex-1 text-sm font-medium text-neutral-200 truncate font-mono">
          {props.toolCall.name}
        </span>
        <span class="flex-shrink-0">{statusIcon()}</span>
        <span class="text-neutral-500 text-xs ml-1">
          {expanded() ? '▼' : '▶'}
        </span>
      </button>

      <Show when={expanded()}>
        <div class="border-t border-neutral-700/50">
          {/* Args section */}
          <Show when={formattedArgs()}>
            <div class="px-3 py-2 bg-neutral-900/50">
              <div class="text-[10px] uppercase tracking-wider text-neutral-500 mb-1 font-semibold">Arguments</div>
              <pre class="text-xs text-neutral-300 font-mono whitespace-pre-wrap break-all overflow-x-auto max-h-48 overflow-y-auto">
                {formattedArgs()}
              </pre>
            </div>
          </Show>

          {/* Result section */}
          <Show when={props.toolCall.result}>
            <div class={`px-3 py-2 ${formattedArgs() ? 'border-t border-neutral-700/30' : ''} bg-neutral-900/50`}>
              <div class="text-[10px] uppercase tracking-wider text-neutral-500 mb-1 font-semibold">
                {props.toolCall.status === 'error' ? 'Error' : 'Result'}
              </div>
              <pre class={`text-xs font-mono whitespace-pre-wrap break-all overflow-x-auto max-h-64 overflow-y-auto ${
                props.toolCall.status === 'error' ? 'text-red-300' : 'text-neutral-300'
              }`}>
                {props.toolCall.result}
              </pre>
            </div>
          </Show>

          {/* Running with no result yet — show waiting indicator */}
          <Show when={props.toolCall.status === 'running' && !props.toolCall.result}>
            <div class="px-3 py-2 bg-neutral-900/50">
              <span class="inline-flex items-center gap-1.5 text-xs text-neutral-500">
                <span class="w-1.5 h-1.5 bg-blue-400 rounded-full animate-pulse" />
                Executing…
              </span>
            </div>
          </Show>

          {/* ID for debugging */}
          <div class="px-3 py-1.5 text-[10px] text-neutral-600 border-t border-neutral-800/50">
            ID: {props.toolCall.callId ?? props.toolCall.id}
          </div>
        </div>
      </Show>
    </div>
  );
};

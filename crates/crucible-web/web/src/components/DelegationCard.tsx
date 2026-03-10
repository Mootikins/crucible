import { Component, Show, createSignal, createEffect } from 'solid-js';
import { ArrowRightLeft, Check, X } from 'lucide-solid';
import type { SubagentEvent } from '@/lib/types';

interface DelegationCardProps {
  event: SubagentEvent;
}

export const DelegationCard: Component<DelegationCardProps> = (props) => {
  const [expanded, setExpanded] = createSignal(props.event.status === 'failed');

  createEffect(() => {
    if (props.event.status === 'failed') {
      setExpanded(true);
    }
  });

  const agentName = () => props.event.targetAgent || 'Unknown agent';

  const statusIcon = () => {
    switch (props.event.status) {
      case 'spawned':
        return (
          <span class="inline-flex items-center text-violet-400" title="Running">
            <svg class="w-3.5 h-3.5 animate-spin" viewBox="0 0 24 24" fill="none">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="3" />
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
            </svg>
          </span>
        );
      case 'completed':
        return <Check class="w-3.5 h-3.5 text-emerald-400" />;
      case 'failed':
        return <X class="w-3.5 h-3.5 text-red-400" />;
    }
  };

  const borderColor = () => {
    switch (props.event.status) {
      case 'spawned': return 'border-violet-500/40';
      case 'completed': return 'border-violet-500/25';
      case 'failed': return 'border-red-500/40';
    }
  };

  const bgColor = () => {
    switch (props.event.status) {
      case 'spawned': return 'bg-violet-950/15';
      case 'completed': return 'bg-neutral-850';
      case 'failed': return 'bg-red-950/20';
    }
  };

  const accentColor = () => {
    switch (props.event.status) {
      case 'spawned': return 'text-violet-400';
      case 'completed': return 'text-violet-300';
      case 'failed': return 'text-red-400';
    }
  };

  const statusLabel = () => {
    switch (props.event.status) {
      case 'spawned': return 'Delegating…';
      case 'completed': return 'Complete';
      case 'failed': return 'Failed';
    }
  };

  const promptPreview = () => {
    const p = props.event.prompt;
    if (!p) return `Delegated to ${agentName()}`;
    return p.length > 70 ? p.slice(0, 70) + '…' : p;
  };

  return (
    <div class={`border ${borderColor()} rounded-lg ${bgColor()} overflow-hidden my-2`}>
      <button
        onClick={() => setExpanded(!expanded())}
        class="w-full flex items-center gap-2 px-3 py-2 hover:bg-neutral-700/20 transition-colors text-left group"
      >
        <ArrowRightLeft class={`w-4 h-4 flex-shrink-0 ${accentColor()}`} />
        <span class="flex-1 text-sm text-neutral-300 truncate">
          <Show
            when={props.event.status !== 'spawned'}
            fallback={
              <>
                <span class="text-neutral-400">{promptPreview()}</span>
                <span class="text-neutral-500 mx-1.5">→</span>
                <span class={`text-xs font-medium ${accentColor()}`}>{agentName()}</span>
              </>
            }
          >
            <span class="font-medium">Delegated to {agentName()}</span>
            <span class="text-neutral-500 mx-1.5">·</span>
            <span class={`text-xs ${accentColor()}`}>{statusLabel()}</span>
          </Show>
        </span>
        <span class="flex-shrink-0">{statusIcon()}</span>
        <span class="text-neutral-600 text-xs ml-0.5 group-hover:text-neutral-400 transition-colors">
          {expanded() ? '▼' : '▶'}
        </span>
      </button>

      <Show when={expanded()}>
        <div class="border-t border-neutral-700/50">
          <Show when={props.event.prompt}>
            <div class="px-3 py-2 bg-neutral-900/50">
              <div class="text-[10px] uppercase tracking-wider text-neutral-500 mb-1 font-semibold">Prompt</div>
              <p class="text-xs text-neutral-300 whitespace-pre-wrap break-words max-h-32 overflow-y-auto">
                {props.event.prompt}
              </p>
            </div>
          </Show>

          <Show when={props.event.status === 'completed' && props.event.summary}>
            <div class={`px-3 py-2 bg-neutral-900/50 ${props.event.prompt ? 'border-t border-neutral-700/30' : ''}`}>
              <div class="text-[10px] uppercase tracking-wider text-neutral-500 mb-1 font-semibold">Summary</div>
              <p class="text-xs text-neutral-300 whitespace-pre-wrap break-words max-h-48 overflow-y-auto">
                {props.event.summary}
              </p>
            </div>
          </Show>

          <Show when={props.event.status === 'failed' && props.event.error}>
            <div class={`px-3 py-2 bg-red-950/20 ${props.event.prompt ? 'border-t border-neutral-700/30' : ''}`}>
              <div class="text-[10px] uppercase tracking-wider text-red-400/70 mb-1 font-semibold">Error</div>
              <pre class="text-xs text-red-300 font-mono whitespace-pre-wrap break-all max-h-48 overflow-y-auto">
                {props.event.error}
              </pre>
            </div>
          </Show>

          <Show when={props.event.status === 'spawned'}>
            <div class="px-3 py-2 bg-neutral-900/50">
              <span class="inline-flex items-center gap-1.5 text-xs text-neutral-500">
                <span class="w-1.5 h-1.5 bg-violet-400 rounded-full animate-pulse" />
                Delegating to {agentName()}…
              </span>
            </div>
          </Show>

          <div class="px-3 py-1.5 text-[10px] text-neutral-600 border-t border-neutral-800/50">
            ID: {props.event.id}
          </div>
        </div>
      </Show>
    </div>
  );
};

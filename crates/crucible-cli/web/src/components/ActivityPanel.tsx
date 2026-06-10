import { Component, For, Show, createSignal } from 'solid-js';
import { Bot, ArrowRightLeft, Check, X, Activity } from 'lucide-solid';
import { useChatSafe } from '@/contexts/ChatContext';
import { PanelShell } from './PanelShell';
import { PanelHeader } from './PanelHeader';
import type { SubagentEvent } from '@/lib/types';


const TaskItem: Component<{ event: SubagentEvent }> = (props) => {
  const [expanded, setExpanded] = createSignal(false);

  const isDelegation = () => !!props.event.targetAgent;
  const agentName = () => props.event.targetAgent || 'Unknown agent';

  const statusIcon = () => {
    switch (props.event.status) {
      case 'spawned':
        return (
          <span class="inline-flex items-center" title="Running">
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

  const statusColor = () => {
    switch (props.event.status) {
      case 'spawned': return isDelegation() ? 'text-violet-400' : 'text-sky-400';
      case 'completed': return 'text-emerald-400';
      case 'failed': return 'text-red-400';
    }
  };

  const borderColor = () => {
    switch (props.event.status) {
      case 'spawned': return isDelegation() ? 'border-violet-500/30' : 'border-sky-500/30';
      case 'completed': return 'border-emerald-500/20';
      case 'failed': return 'border-red-500/30';
    }
  };

  const bgColor = () => {
    switch (props.event.status) {
      case 'spawned': return isDelegation() ? 'bg-violet-950/10' : 'bg-sky-950/10';
      case 'completed': return 'bg-neutral-800/30';
      case 'failed': return 'bg-red-950/15';
    }
  };

  const label = () => {
    if (isDelegation()) return `Delegation to ${agentName()}`;
    return 'Subagent';
  };

  const statusLabel = () => {
    switch (props.event.status) {
      case 'spawned': return 'Running';
      case 'completed': return 'Complete';
      case 'failed': return 'Failed';
    }
  };

  const promptPreview = () => {
    const p = props.event.prompt;
    if (!p) return isDelegation() ? `Task for ${agentName()}` : 'Background task';
    return p.length > 60 ? p.slice(0, 60) + '…' : p;
  };

  return (
    <div class={`border ${borderColor()} rounded-lg ${bgColor()} overflow-hidden`}>
      <button
        onClick={() => setExpanded(!expanded())}
        class="w-full flex items-center gap-2 px-3 py-2 hover:bg-white/[0.03] transition-colors text-left group"
      >
        {/* Icon */}
        <Show when={isDelegation()} fallback={<Bot class={`w-3.5 h-3.5 flex-shrink-0 ${statusColor()}`} />}>
          <ArrowRightLeft class={`w-3.5 h-3.5 flex-shrink-0 ${statusColor()}`} />
        </Show>

        {/* Label + status */}
        <div class="flex-1 min-w-0">
          <div class="flex items-center gap-1.5">
            <span class="text-xs font-medium text-neutral-300 truncate">{label()}</span>
            <span class={`text-[10px] ${statusColor()}`}>{statusLabel()}</span>
          </div>
          <div class="text-[11px] text-neutral-500 truncate mt-0.5">{promptPreview()}</div>
        </div>

        {/* Status icon */}
        <span class={`flex-shrink-0 ${statusColor()}`}>{statusIcon()}</span>

        {/* Expand chevron */}
        <span class="text-neutral-600 text-[10px] group-hover:text-neutral-400 transition-colors">
          {expanded() ? '▼' : '▶'}
        </span>
      </button>

      {/* Expanded detail */}
      <Show when={expanded()}>
        <div class="border-t border-neutral-700/40">
          {/* Prompt */}
          <Show when={props.event.prompt}>
            <div class="px-3 py-2 bg-neutral-900/40">
              <div class="text-[10px] uppercase tracking-wider text-neutral-500 mb-1 font-semibold">Prompt</div>
              <p class="text-xs text-neutral-300 whitespace-pre-wrap break-words max-h-40 overflow-y-auto leading-relaxed">
                {props.event.prompt}
              </p>
            </div>
          </Show>

          {/* Summary (completed) */}
          <Show when={props.event.status === 'completed' && props.event.summary}>
            <div class={`px-3 py-2 bg-neutral-900/40 ${props.event.prompt ? 'border-t border-neutral-700/25' : ''}`}>
              <div class="text-[10px] uppercase tracking-wider text-emerald-500/70 mb-1 font-semibold">Result</div>
              <p class="text-xs text-neutral-300 whitespace-pre-wrap break-words max-h-56 overflow-y-auto leading-relaxed">
                {props.event.summary}
              </p>
            </div>
          </Show>

          {/* Error (failed) */}
          <Show when={props.event.status === 'failed' && props.event.error}>
            <div class={`px-3 py-2 bg-red-950/15 ${props.event.prompt ? 'border-t border-neutral-700/25' : ''}`}>
              <div class="text-[10px] uppercase tracking-wider text-red-400/70 mb-1 font-semibold">Error</div>
              <pre class="text-xs text-red-300 font-mono whitespace-pre-wrap break-all max-h-56 overflow-y-auto">
                {props.event.error}
              </pre>
            </div>
          </Show>

          {/* Running indicator */}
          <Show when={props.event.status === 'spawned'}>
            <div class="px-3 py-2 bg-neutral-900/40">
              <span class="inline-flex items-center gap-1.5 text-xs text-neutral-500">
                <span class={`w-1.5 h-1.5 rounded-full animate-pulse ${isDelegation() ? 'bg-violet-400' : 'bg-sky-400'}`} />
                Processing…
              </span>
            </div>
          </Show>

          {/* ID footer */}
          <div class="px-3 py-1 text-[10px] text-neutral-600 border-t border-neutral-800/40 font-mono">
            {props.event.id}
          </div>
        </div>
      </Show>
    </div>
  );
};


const TaskSummary: Component<{ events: SubagentEvent[] }> = (props) => {
  const active = () => props.events.filter((e) => e.status === 'spawned').length;
  const completed = () => props.events.filter((e) => e.status === 'completed').length;
  const failed = () => props.events.filter((e) => e.status === 'failed').length;

  return (
    <div class="flex items-center gap-3 px-3 py-2 border-b border-neutral-800 text-[11px]">
      <Show when={active() > 0}>
        <span class="flex items-center gap-1 text-sky-400">
          <span class="w-1.5 h-1.5 bg-sky-400 rounded-full animate-pulse" />
          {active()} active
        </span>
      </Show>
      <Show when={completed() > 0}>
        <span class="flex items-center gap-1 text-emerald-400">
          <Check class="w-3 h-3" />
          {completed()}
        </span>
      </Show>
      <Show when={failed() > 0}>
        <span class="flex items-center gap-1 text-red-400">
          <X class="w-3 h-3" />
          {failed()}
        </span>
      </Show>
      <Show when={active() === 0 && completed() === 0 && failed() === 0}>
        <span class="text-neutral-500">No tasks</span>
      </Show>
    </div>
  );
};

export const ActivityPanel: Component = () => {
  const { subagentEvents } = useChatSafe();

  const events = () => subagentEvents();


  const sortedEvents = () => {
    const order: Record<string, number> = { spawned: 0, failed: 1, completed: 2 };
    return [...events()].sort((a, b) => (order[a.status] ?? 3) - (order[b.status] ?? 3));
  };

  return (
    <PanelShell>
      {/* Header */}
      <PanelHeader title="Activity">
        <div class="flex items-center gap-2">
          <Activity class="w-4 h-4 text-neutral-400" />
        </div>
      </PanelHeader>

      {/* Summary bar */}
      {/* Summary bar */}
      <Show when={events().length > 0}>
        <TaskSummary events={events()} />
      </Show>

      {/* Task list */}
      <div class="flex-1 overflow-y-auto">
        <Show
          when={events().length > 0}
          fallback={
            <div class="flex flex-col items-center justify-center h-full px-4 text-center">
              <Activity class="w-8 h-8 text-neutral-700 mb-3" />
              <p class="text-sm text-neutral-500">No background tasks</p>
              <p class="text-xs text-neutral-600 mt-1">
                Subagent and delegation tasks will appear here
              </p>
            </div>
          }
        >
          <div class="p-2 space-y-1.5">
            <For each={sortedEvents()}>
              {(event) => <TaskItem event={event} />}
            </For>
          </div>
        </Show>
      </div>
    </PanelShell>
  );
};

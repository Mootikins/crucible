import { For, Show, type Component } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import type { ToolCallDisplay } from '@/lib/types';
import { fileOpenOptionsForEvent, openFileInEditor } from '@/lib/file-actions';
import { ChevronRight, FileText, Globe, Pencil, Search, Wrench, Zap } from '@/lib/icons';

// Presentation of the daemon's canonical kind, never classification by tool name.
const kinds: Record<string, { icon: Component<{ class?: string }>; past: string; now: string }> = {
  command: { icon: Zap, past: 'Ran', now: 'Run' },
  file_read: { icon: FileText, past: 'Read', now: 'Read' },
  file_edit: { icon: Pencil, past: 'Edited', now: 'Edit' },
  search: { icon: Search, past: 'Searched for', now: 'Search for' },
  fetch: { icon: Globe, past: 'Fetched', now: 'Fetch' },
};

export const ToolCallRow: Component<{
  toolCall: ToolCallDisplay;
  expanded: boolean;
  onToggle: () => void;
}> = (props) => {
  const display = () => props.toolCall.display;
  const presentation = () => kinds[display()?.kind ?? ''];
  const name = () => display()?.tool || props.toolCall.name;
  const label = () => presentation()?.[props.toolCall.status === 'complete' ? 'past' : 'now'] ?? name();
  const line = () => display()?.render?.line?.split('\n')[0] ?? '';
  const paths = () => display()?.paths ?? [];
  return (
    <div class="tool-call-row" data-expanded={props.expanded || undefined}>
      <button type="button" class="tool-call-toggle" title={name()}
        aria-expanded={props.expanded} onClick={props.onToggle}>
        <Dynamic component={presentation()?.icon ?? Wrench} class="w-3.5 h-3.5 shrink-0" />
        <span>{label()}</span>
        <Show when={!paths().length}><span class="tool-call-target" title={line()}>{line()}</span></Show>
      </button>
      <For each={paths()}>{path => (
        <button type="button" class="tool-call-target tool-call-file" title={`Open ${path}`}
          aria-label={`Open ${path}`} onClick={event => void openFileInEditor(path, undefined, fileOpenOptionsForEvent(event))}>
          {path.split('/').pop()}
        </button>
      )}</For>
      <Show when={display()?.render?.summary}>
        <span class="tool-call-result" data-testid="tool-result-summary">→ {display()!.render!.summary}</span>
      </Show>
      <Show when={props.toolCall.autoApproved}>
        <span class="tool-call-status" data-testid="tool-auto-approved"
          title={`Permission granted without asking (${props.toolCall.autoApproved}).`}>Auto</span>
      </Show>
      <Show when={props.toolCall.terminate}><span class="tool-call-status" title="This tool ended the agent turn early.">Terminated</span></Show>
      <Show when={props.toolCall.status === 'running'}><span class="tool-call-status animate-pulse" title="Running">Running…</span></Show>
      <Show when={props.toolCall.status === 'error'}><span class="tool-call-status text-error" title="Error">Failed</span></Show>
      <Show when={props.toolCall.status === 'complete'}><span class="sr-only" title="Complete">Complete</span></Show>
      <button type="button" class="tool-call-chevron" aria-label={`${props.expanded ? 'Collapse' : 'Expand'} ${name()}`}
        aria-expanded={props.expanded} onClick={props.onToggle}>
        <ChevronRight class={`w-3 h-3 ${props.expanded ? 'rotate-90' : ''}`} />
      </button>
    </div>
  );
};

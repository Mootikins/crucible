import { Component, Show, createSignal, createMemo, createEffect } from 'solid-js';
import type { ToolCallDisplay } from '@/lib/types';
import { DiffViewer } from './DiffViewer';
import { MultiEditDiff } from './MultiEditDiff';
import { extractDiffFromToolCall } from '@/lib/tool-diffs';

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
          <span class="inline-flex items-center text-primary" title="Running">
            <svg class="w-4 h-4 animate-spin" viewBox="0 0 24 24" fill="none">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="3" />
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
            </svg>
          </span>
        );
      case 'complete':
        return <span class="text-ok text-sm font-bold" title="Complete">✓</span>;
      case 'error':
        return <span class="text-error text-sm font-bold" title="Error">✗</span>;
    }
  };

  const statusBorderColor = () => {
    switch (props.toolCall.status) {
      case 'running': return 'border-primary/40';
      case 'complete': return 'border-ok/30';
      case 'error': return 'border-error/40';
    }
  };

  const statusBgColor = () => {
    switch (props.toolCall.status) {
      case 'running': return 'bg-primary/10';
      case 'complete': return 'bg-surface-elevated';
      case 'error': return 'bg-error/10';
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

  // One-line header summary (the bash command, file path, query, …) so a
  // collapsed row still says what the tool did — like other agent UIs.
  const argSummary = createMemo(() => {
    const args = props.toolCall.args;
    if (!args || args === '' || args === '""') return null;
    try {
      const parsed: unknown = JSON.parse(args);
      if (typeof parsed === 'string') return parsed || null;
      if (parsed && typeof parsed === 'object') {
        const record = parsed as Record<string, unknown>;
        for (const key of ['command', 'file_path', 'path', 'pattern', 'query', 'url', 'name', 'note']) {
          if (typeof record[key] === 'string' && record[key]) return record[key] as string;
        }
        const first = Object.values(record).find((v) => typeof v === 'string' && v);
        return (first as string | undefined) ?? null;
      }
      return null;
    } catch {
      return null;
    }
  });

  const diff = createMemo(() => extractDiffFromToolCall(props.toolCall));

  // Results are often serialized JSON — pretty-print them instead of showing
  // one raw line of bytes. Nested JSON-in-strings (MCP text payloads) gets
  // one unwrap pass; anything unparseable renders verbatim.
  const formattedResult = createMemo(() => {
    const raw = props.toolCall.result;
    if (!raw) return raw;
    try {
      let parsed: unknown = JSON.parse(raw);
      if (typeof parsed === 'string') {
        try {
          parsed = JSON.parse(parsed) as unknown;
        } catch {
          return parsed as string;
        }
      }
      return JSON.stringify(parsed, null, 2);
    } catch {
      return raw;
    }
  });

  return (
    <div class={`border ${statusBorderColor()} rounded-lg ${statusBgColor()} overflow-hidden my-2`}>
      <button
        onClick={() => setExpanded(!expanded())}
        class="w-full flex items-center gap-2 px-3 py-2 hover:bg-hover-wash transition-colors text-left"
      >
        <span class="text-base leading-none">{iconForTool(props.toolCall.name)}</span>
        <span class="flex-shrink-0 max-w-[40%] text-sm font-medium text-shell-ink truncate font-mono">
          {props.toolCall.name}
        </span>
        <span class="flex-1 min-w-0 text-xs text-muted-dark truncate font-mono">
          {argSummary() ?? ''}
        </span>
        <Show when={props.toolCall.terminate}>
          <span
            class="flex-shrink-0 text-[10px] uppercase tracking-wider px-1.5 py-0.5 rounded bg-attention/15 text-attention border border-attention/50 font-semibold"
            title="This tool ended the agent turn early."
          >
            Terminated
          </span>
        </Show>
        <span class="flex-shrink-0">{statusIcon()}</span>
        <span class="text-muted-dark text-xs ml-1">
          {expanded() ? '▼' : '▶'}
        </span>
      </button>

      <Show when={expanded()}>
        <div class="border-t border-hairline">
          {/* Args section — suppressed when a diff renders, since the diff header
              shows the file path and the diff body shows the old/new content. */}
          <Show when={formattedArgs() && !diff()}>
            <div class="px-3 py-2 bg-surface-base">
              <div class="text-[10px] uppercase tracking-wider text-muted-dark mb-1 font-semibold">Arguments</div>
              <pre class="text-xs text-shell-body font-mono whitespace-pre-wrap break-all overflow-x-auto max-h-48 overflow-y-auto">
                {formattedArgs()}
              </pre>
            </div>
          </Show>

          {/* Error result section — rendered BEFORE the diff so users see why a
              tool failed before scrolling past the failed-attempt diff. */}
          <Show when={props.toolCall.result && props.toolCall.status === 'error'}>
            <div class={`px-3 py-2 ${formattedArgs() && !diff() ? 'border-t border-hairline' : ''} bg-surface-base`}>
              <div class="text-[10px] uppercase tracking-wider text-muted-dark mb-1 font-semibold">
                Error
              </div>
              <pre class="text-xs font-mono whitespace-pre-wrap break-all overflow-x-auto max-h-64 overflow-y-auto text-error">
                {props.toolCall.result}
              </pre>
            </div>
          </Show>

          {/* Diff rendering for Edit/Write/MultiEdit when args parse cleanly */}
          <Show when={diff()}>
            {(d) => (
              <div class={`px-3 py-2 ${props.toolCall.status === 'error' && props.toolCall.result ? 'border-t border-hairline' : ''} bg-surface-base`}>
                <Show
                  when={d().kind === 'single'}
                  fallback={
                    <MultiEditDiff
                      fileName={(d() as { kind: 'multi'; fileName: string; edits: { oldContent: string; newContent: string }[] }).fileName}
                      edits={(d() as { kind: 'multi'; fileName: string; edits: { oldContent: string; newContent: string }[] }).edits}
                    />
                  }
                >
                  <DiffViewer
                    fileName={(d() as { kind: 'single'; fileName: string; oldContent: string; newContent: string }).fileName}
                    oldContent={(d() as { kind: 'single'; fileName: string; oldContent: string; newContent: string }).oldContent}
                    newContent={(d() as { kind: 'single'; fileName: string; oldContent: string; newContent: string }).newContent}
                  />
                </Show>
              </div>
            )}
          </Show>

          {/* Plain-text result section (kept for non-diff tools on success). */}
          <Show when={props.toolCall.result && !diff() && props.toolCall.status !== 'error'}>
            <div class={`px-3 py-2 ${formattedArgs() ? 'border-t border-hairline' : ''} bg-surface-base`}>
              <div class="text-[10px] uppercase tracking-wider text-muted-dark mb-1 font-semibold">
                Result
              </div>
              <pre class="text-xs font-mono whitespace-pre-wrap break-all overflow-x-auto max-h-64 overflow-y-auto text-shell-body">
                {formattedResult()}
              </pre>
            </div>
          </Show>

          {/* Running with no result yet — show waiting indicator */}
          <Show when={props.toolCall.status === 'running' && !props.toolCall.result}>
            <div class="px-3 py-2 bg-surface-base">
              <span class="inline-flex items-center gap-1.5 text-xs text-muted-dark">
                <span class="w-1.5 h-1.5 bg-primary rounded-full animate-pulse" />
                Executing…
              </span>
            </div>
          </Show>

          {/* ID for debugging */}
          <div class="px-3 py-1.5 text-[10px] text-muted-dark border-t border-hairline">
            ID: {props.toolCall.callId ?? props.toolCall.id}
          </div>
        </div>
      </Show>
    </div>
  );
};

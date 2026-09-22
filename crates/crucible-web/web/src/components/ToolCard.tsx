import { Component, Show, createSignal, createMemo, createEffect } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import type { ToolCallDisplay } from '@/lib/types';
import { DiffViewer } from './DiffViewer';
import { MultiEditDiff } from './MultiEditDiff';
import { toolDiffsFromWire } from '@/lib/tool-diffs';
import { openDiff } from '@/lib/panel-actions';
import { useChatSafe } from '@/contexts/ChatContext';
import { deepPrettyPrintJson } from '@/lib/pretty-print';
import { unwrapMcpEnvelope } from '@/lib/mcp-envelope';
import {
  ChevronRight,
  FileOutput,
  FileText,
  Globe,
  Pencil,
  Search,
  StickyNote,
  Wrench,
  Zap,
} from '@/lib/icons';

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

  const iconForTool = (name: string): Component<{ class?: string }> => {
    const lower = name.toLowerCase();
    if (lower.includes('read') || lower.includes('file')) return FileText;
    if (lower.includes('write') || lower.includes('edit')) return Pencil;
    if (lower.includes('search') || lower.includes('find')) return Search;
    if (lower.includes('bash') || lower.includes('shell') || lower.includes('exec')) return Zap;
    if (lower.includes('web') || lower.includes('fetch') || lower.includes('http')) return Globe;
    if (lower.includes('note') || lower.includes('memory')) return StickyNote;
    return Wrench;
  };

  const statusIcon = () => {
    switch (props.toolCall.status) {
      case 'running':
        return (
          <span class="inline-flex items-center text-primary" title="Running">
            <svg class="w-3.5 h-3.5 animate-spin" viewBox="0 0 24 24" fill="none">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="3" />
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
            </svg>
          </span>
        );
      case 'complete':
        return <span class="text-ok text-floor font-semibold" title="Complete">✓</span>;
      case 'error':
        return <span class="text-error text-floor font-semibold" title="Error">✗</span>;
    }
  };

  // Completed rows stay flat (transparent) so a run of them reads as one tight
  // stack on the group's surface, not a column of raised cards. Only the
  // meaningful in-progress/failed states carry a wash.
  const statusBgColor = () => {
    switch (props.toolCall.status) {
      case 'running': return 'bg-primary/10';
      case 'complete': return 'bg-transparent';
      case 'error': return 'bg-error/10';
    }
  };

  // What this call is about comes from the daemon (`toolCall.display`) — one
  // projection shared with the TUI and with the daemon's own deny messages.
  // No local fallback: this page used to keep its own key-priority list, and
  // the two answers could disagree. A recording that predates the field shows
  // no summary line; the expanded card still renders the full args.
  const display = createMemo(() => props.toolCall.display);

  const bashCommand = createMemo(() =>
    display()?.kind === 'command' ? (display()!.primary ?? null) : null,
  );

  const formattedArgs = createMemo(() => {
    const args = props.toolCall.args;
    if (!args || args === '' || args === '""') return null;
    try {
      const parsed = JSON.parse(args);
      // The command renders in its own block above; showing it again here
      // would just be the escaped copy we were trying to get rid of. Any
      // other argument (timeout, cwd, …) still deserves display.
      if (bashCommand() !== null && parsed && typeof parsed === 'object') {
        const { command: _command, ...rest } = parsed as Record<string, unknown>;
        if (Object.keys(rest).length === 0) return null;
        return JSON.stringify(deepPrettyPrintJson(rest), null, 2);
      }
      return JSON.stringify(deepPrettyPrintJson(parsed), null, 2);
    } catch {
      return args;
    }
  });

  // One-line header summary so a collapsed row still says what the tool did.
  // Same source as the Command block below, so the two can no longer disagree
  // about which argument matters.
  const argSummary = createMemo(() => display()?.primary?.split('\n')[0] ?? null);

  const diffs = createMemo(() => toolDiffsFromWire(props.toolCall?.diffs));

  // The session record of the chat holds every change of the session, this
  // call's change too. The diff pane opens on the record, focused on the file.
  // A card outside a chat names no session, so it offers no pane.
  const chat = useChatSafe();
  const sessionId = () => chat.sessionId?.();
  const openInDiff = (fileName: string) => {
    const session = sessionId();
    if (session) openDiff({ kind: 'session_record', session }, { path: fileName });
  };

  // Results are often serialized JSON — pretty-print them instead of showing
  // one raw line of bytes. The unwrapping is intentionally aggressive:
  //   1. MCP tool results arrive wrapped in `{content:[{type:'text',text}]}`
  //      envelopes whose `text` payload is itself JSON. Unwrap envelopes
  //      recursively at any depth.
  //   2. Many providers stringify JSON inside JSON (string-valued fields
  //      carrying escaped JSON). Re-parse those strings until parsing stops
  //      making progress — single-pass leaves double-encoded MCP payloads as
  //      "wrapper pretty + inner one-line blob".
  //   3. Walk the final tree and replace every JSON-bearing string with its
  //      pretty-printed form so nested objects/arrays breathe.
  // Anything unparseable renders verbatim.
  const formattedResult = createMemo(() => {
    const raw = props.toolCall.result;
    if (!raw) return raw;
    try {
      const parsed = deepPrettyPrintJson(JSON.parse(raw), unwrapMcpEnvelope);
      if (typeof parsed === 'string') return parsed;
      return JSON.stringify(parsed, null, 2);
    } catch {
      return raw;
    }
  });

  return (
    <div
      class={`${statusBgColor()} overflow-hidden`}
      // The daemon's call id, which the ledger keys intervals on. A card
      // without one carries no id.
      data-tool-call-id={props.toolCall.callId ?? undefined}
    >
      <button
        onClick={() => setExpanded(!expanded())}
        aria-expanded={expanded()}
        class="w-full flex items-center gap-2 px-2.5 py-1.5 hover:bg-hover-wash transition-colors text-left"
      >
        <Dynamic
          component={iconForTool(props.toolCall.name)}
          class="w-3.5 h-3.5 flex-shrink-0 text-muted"
        />
        <span class="flex-shrink-0 max-w-[45%] text-xs font-medium text-shell-ink truncate font-mono">
          {props.toolCall.name}
        </span>
        <span class="flex-1 min-w-0 text-floor text-muted-dark truncate font-mono">
          {argSummary() ?? ''}
        </span>
        <Show when={props.toolCall.autoApproved}>
          <span
            class="flex-shrink-0 text-floor uppercase tracking-wider px-1.5 py-0.5 rounded bg-precog/15 text-precog border border-precog/50 font-semibold"
            data-testid="tool-auto-approved"
            title={`Permission granted without asking (${props.toolCall.autoApproved}).`}
          >
            Auto
          </span>
        </Show>
        <Show when={props.toolCall.terminate}>
          <span
            class="flex-shrink-0 text-floor uppercase tracking-wider px-1.5 py-0.5 rounded bg-attention/15 text-attention border border-attention/50 font-semibold"
            title="This tool ended the agent turn early."
          >
            Terminated
          </span>
        </Show>
        <span class="flex-shrink-0">{statusIcon()}</span>
        <ChevronRight
          class={`w-3 h-3 flex-shrink-0 text-muted-dark transition-transform ${expanded() ? 'rotate-90' : ''}`}
        />
      </button>

      <Show when={expanded()}>
        <div class="border-t border-hairline">
          {/* A shell call reads as a command, so render it as one: prompt
              marker, real newlines, no JSON envelope. */}
          <Show when={bashCommand()}>
            <div class="px-3 py-2 bg-surface-base">
              <div class="text-floor uppercase tracking-wider text-muted-dark mb-1 font-semibold">
                Command
              </div>
              <div class="flex gap-2">
                <span class="select-none text-primary text-xs font-mono leading-5" aria-hidden="true">
                  $
                </span>
                <pre
                  data-testid="bash-command"
                  class="flex-1 min-w-0 text-xs text-shell-ink font-mono leading-5 whitespace-pre-wrap break-words overflow-x-auto max-h-48 overflow-y-auto"
                >
                  {bashCommand()}
                </pre>
              </div>
            </div>
          </Show>

          {/* Args section — suppressed when a diff renders, since the diff header
              shows the file path and the diff body shows the old/new content. */}
          <Show when={formattedArgs() && diffs().length === 0}>
            <div class={`px-3 py-2 bg-surface-base ${bashCommand() ? 'border-t border-hairline' : ''}`}>
              <div class="text-floor uppercase tracking-wider text-muted-dark mb-1 font-semibold">Arguments</div>
              <pre
                data-testid="tool-args"
                class="text-xs text-shell-body font-mono whitespace-pre-wrap break-words overflow-x-auto max-h-48 overflow-y-auto"
              >
                {formattedArgs()}
              </pre>
            </div>
          </Show>

          {/* Error result section — rendered BEFORE the diff so users see why a
              tool failed before scrolling past the failed-attempt diff. Uses
              formattedResult() so JSON-bearing error payloads get the same
              pretty-printing as successful results. */}
          <Show when={props.toolCall.result && props.toolCall.status === 'error'}>
            <div class={`px-3 py-2 ${formattedArgs() && diffs().length === 0 ? 'border-t border-hairline' : ''} bg-surface-base`}>
              <div class="text-floor uppercase tracking-wider text-muted-dark mb-1 font-semibold">
                Error
              </div>
              <pre class="text-xs font-mono whitespace-pre-wrap break-words overflow-x-auto max-h-64 overflow-y-auto text-error">
                {formattedResult()}
              </pre>
            </div>
          </Show>

          {/* Diff rendering — the daemon's FileDiff projection, one block per
              file (an ACP call may touch several). */}
          <Show when={diffs().length > 0}>
            <div class={`px-3 py-2 ${props.toolCall.status === 'error' && props.toolCall.result ? 'border-t border-hairline' : ''} bg-surface-base`}>
              {diffs().map((d) => (
                <div class="mb-1.5 last:mb-0">
                  {/* Review this change in the session record — one
                      control per file. */}
                  <Show when={sessionId()}>
                    <div class="flex items-center justify-end mb-1">
                      <button
                        type="button"
                        onClick={() => openInDiff(d.fileName)}
                        data-testid="tool-open-diff"
                        class="inline-flex items-center gap-1 rounded-md border border-hairline px-2 py-1 text-floor text-muted-dark hover:text-shell-ink hover:bg-hover-wash"
                        title="Open the session record in the diff pane, at this file"
                      >
                        <FileOutput class="w-3.5 h-3.5" /> Open diff
                      </button>
                    </div>
                  </Show>
                  {d.kind === 'single'
                    ? (
                      <DiffViewer
                        fileName={d.fileName}
                        oldContent={d.oldContent}
                        newContent={d.newContent}
                      />
                    )
                    : <MultiEditDiff fileName={d.fileName} edits={d.edits} />}
                </div>
              ))}
            </div>
          </Show>

          {/* Plain-text result section (kept for non-diff tools on success). */}
          <Show when={props.toolCall.result && diffs().length === 0 && props.toolCall.status !== 'error'}>
            <div class={`px-3 py-2 ${formattedArgs() ? 'border-t border-hairline' : ''} bg-surface-base`}>
              <div class="text-floor uppercase tracking-wider text-muted-dark mb-1 font-semibold">
                Result
              </div>
              <pre class="text-xs font-mono whitespace-pre-wrap break-words overflow-x-auto max-h-64 overflow-y-auto text-shell-body">
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
          <div class="px-3 py-1.5 text-floor text-muted-dark border-t border-hairline">
            ID: {props.toolCall.callId ?? props.toolCall.id}
          </div>
        </div>
      </Show>
    </div>
  );
};

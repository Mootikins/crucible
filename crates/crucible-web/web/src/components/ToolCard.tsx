import { Component, For, Show, createSignal, createMemo, createEffect } from 'solid-js';
import { Dynamic } from 'solid-js/web';
import type { ToolCallDisplay } from '@/lib/types';
import { DiffViewer } from './DiffViewer';
import { MultiEditDiff } from './MultiEditDiff';
import { extractDiffFromToolCall, applyToolDiff } from '@/lib/tool-diffs';
import { openFileWithDiff } from '@/lib/file-actions';
import { getFileContent } from '@/lib/api';
import { deepPrettyPrintJson } from '@/lib/pretty-print';
import { unwrapMcpEnvelope } from '@/lib/mcp-envelope';
import { notificationActions } from '@/stores/notificationStore';
import { useChatSafe } from '@/contexts/ChatContext';
import {
  indexToolCall,
  reviewActions,
  reviewStore,
  revealedToolCall,
} from '@/lib/review-store';
import { isExternal } from '@/lib/review-types';
import {
  Check,
  ChevronRight,
  FileOutput,
  FileText,
  Globe,
  Pencil,
  Search,
  StickyNote,
  Undo2,
  Wrench,
  Zap,
} from '@/lib/icons';

interface ToolCardProps {
  toolCall: ToolCallDisplay;
}

/** Mirrors `SHELL_TOOLS` in `crucible_core::types::tool_display`. Only used by
 *  the fallback path, for events that predate the daemon-side projection. */
const SHELL_TOOLS = ['bash', 'shell', 'sh', 'zsh', 'exec', 'run_command', 'terminal'];

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
        return <span class="text-ok text-[11px] font-semibold" title="Complete">✓</span>;
      case 'error':
        return <span class="text-error text-[11px] font-semibold" title="Error">✗</span>;
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
  // projection shared with the TUI and with the daemon's own deny messages,
  // instead of this component keeping its own key-priority list.
  //
  // The local fallback is not redundancy for its own sake: replayed
  // transcripts and older daemons carry no `display`, and a card that renders
  // nothing for them would be a regression. It mirrors the Rust rule and is
  // the ONLY place this heuristic still lives in the web.
  const display = createMemo(() => {
    if (props.toolCall.display) return props.toolCall.display;

    const name = props.toolCall.name.toLowerCase();
    const isShell = SHELL_TOOLS.some((t) => name === t || name.endsWith(`__${t}`));
    const args = props.toolCall.args;
    if (!args) return undefined;
    try {
      const parsed: unknown = JSON.parse(args);
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return undefined;
      const record = parsed as Record<string, unknown>;
      if (isShell && typeof record.command === 'string' && record.command) {
        return { kind: 'command' as const, primary: record.command };
      }
      for (const key of ['file_path', 'path', 'file', 'note', 'name']) {
        if (typeof record[key] === 'string' && record[key]) {
          return { kind: 'path' as const, primary: record[key] as string };
        }
      }
      for (const key of ['pattern', 'query', 'url']) {
        if (typeof record[key] === 'string' && record[key]) {
          return { kind: 'query' as const, primary: record[key] as string };
        }
      }
      const first = Object.values(record).find((v) => typeof v === 'string' && v);
      return first ? { kind: 'other' as const, primary: first as string } : undefined;
    } catch {
      return undefined;
    }
  });

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

  const diff = createMemo(() => extractDiffFromToolCall(props.toolCall));

  // ===== Review attribution ===================================================
  //
  // This card is an INDEX ENTRY into the composed diff: it says what the call
  // did, and offers accept/reject only for the parts of it that are still
  // there. The action unit is the composed hunk, never the call's own
  // contribution — rejecting an early edit after later edits touched the same
  // lines is a three-way merge that fails exactly when it matters.
  //
  // A passive consumer: it reads the review store but never retains it. A
  // transcript holds dozens of cards and each would open the same subscription
  // (refcounted to one, but re-entered on every mount/unmount as the list
  // virtualizes). `SessionStatusChips` — mounted once per chat, beside the
  // composer — is what keeps the session's review state live.
  const { sessionId } = useChatSafe();

  // `callId` is the daemon's ChatToolCall id, which is what the ledger keys
  // intervals on. `toolCall.id` is a transcript id and would mis-link, so a
  // card without a callId simply carries no attribution.
  const callId = () => props.toolCall.callId ?? null;

  createEffect(() => {
    const id = callId();
    // The gutter and the Changes panel render outside ChatProvider and have
    // only ids; the transcript is where the tool's NAME lives, so publish it.
    if (id) indexToolCall(id, props.toolCall.name);
  });

  /** Composed hunks this call is still responsible for. */
  const liveHunks = createMemo(() => {
    const id = callId();
    return id ? reviewStore.hunksForToolCall(sessionId(), id) : [];
  });

  /**
   * The call wrote something, the ledger has been read, and none of it
   * survives in the composed diff — a later edit overwrote it.
   *
   * Gated on `loaded` so an unanswered (or failed) list never claims work was
   * thrown away, and on `diff()` so a call that never proposed an edit is not
   * described as superseded for having produced no hunks.
   */
  const superseded = createMemo(
    () =>
      !!callId() &&
      !!diff() &&
      reviewStore.session(sessionId()).loaded &&
      liveHunks().length === 0,
  );

  /** A rejected mutation is worth saying out loud — it means the disk did not
   * change, and a silent no-op here reads as a dropped click. */
  const review = (op: Promise<void>) =>
    void op.catch((e: Error) => notificationActions.addNotification('error', e.message));

  // Open the edited file in the real editor with this change overlaid as an
  // inline diff: fetch the current content, apply the tool's edit to get the
  // proposed content, and hand both to openFileWithDiff (opens or focuses the
  // tab).
  const [opening, setOpening] = createSignal(false);
  const openInEditor = async () => {
    const d = diff();
    if (!d || opening()) return;
    setOpening(true);
    try {
      // A Write proposes the whole file, so an unreadable path just means a
      // new file — diff against empty. An Edit NEEDS the real baseline: with
      // an empty one its old_string can't match, and the "proposed" content
      // would be an empty document, i.e. a delete-everything diff the user
      // could save. Refuse instead.
      const wholeFile = d.kind === 'single' && d.oldContent === '';
      let original: string;
      try {
        original = await getFileContent(d.fileName);
      } catch {
        if (!wholeFile) {
          notificationActions.addNotification(
            'warning',
            `Can't open the diff — ${d.fileName} could not be read`,
          );
          return;
        }
        original = '';
      }
      const proposed = applyToolDiff(original, d);
      openFileWithDiff(d.fileName, original, proposed, d.fileName.split('/').pop());
    } finally {
      setOpening(false);
    }
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
      class={`${statusBgColor()} overflow-hidden ${
        revealedToolCall() === callId() ? 'ring-1 ring-attention/60' : ''
      }`}
      // The gutter chip in the editor scrolls the transcript here by id. This
      // attribute IS that contract — the editor lives outside ChatProvider and
      // cannot reach the transcript any other way.
      data-tool-call-id={callId() ?? undefined}
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
        <span class="flex-1 min-w-0 text-[11px] text-muted-dark truncate font-mono">
          {argSummary() ?? ''}
        </span>
        <Show when={props.toolCall.autoApproved}>
          <span
            class="flex-shrink-0 text-[10px] uppercase tracking-wider px-1.5 py-0.5 rounded bg-precog/15 text-precog border border-precog/50 font-semibold"
            data-testid="tool-auto-approved"
            title={`Permission granted without asking (${props.toolCall.autoApproved}).`}
          >
            Auto
          </span>
        </Show>
        {/* Nothing this call wrote is still in the composed diff. Its
            accept/reject buttons vanish with it — the action unit is gone, so
            offering the action would be offering a no-op. */}
        <Show when={superseded()}>
          <span
            class="flex-shrink-0 text-[10px] uppercase tracking-wider px-1.5 py-0.5 rounded bg-attention/15 text-attention border border-attention/50 font-semibold"
            data-testid="tool-superseded"
            title="A later edit replaced everything this call wrote — there is nothing left to review."
          >
            Superseded
          </span>
        </Show>
        <Show when={props.toolCall.terminate}>
          <span
            class="flex-shrink-0 text-[10px] uppercase tracking-wider px-1.5 py-0.5 rounded bg-attention/15 text-attention border border-attention/50 font-semibold"
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
              <div class="text-[10px] uppercase tracking-wider text-muted-dark mb-1 font-semibold">
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
          <Show when={formattedArgs() && !diff()}>
            <div class={`px-3 py-2 bg-surface-base ${bashCommand() ? 'border-t border-hairline' : ''}`}>
              <div class="text-[10px] uppercase tracking-wider text-muted-dark mb-1 font-semibold">Arguments</div>
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
            <div class={`px-3 py-2 ${formattedArgs() && !diff() ? 'border-t border-hairline' : ''} bg-surface-base`}>
              <div class="text-[10px] uppercase tracking-wider text-muted-dark mb-1 font-semibold">
                Error
              </div>
              <pre class="text-xs font-mono whitespace-pre-wrap break-words overflow-x-auto max-h-64 overflow-y-auto text-error">
                {formattedResult()}
              </pre>
            </div>
          </Show>

          {/* Diff rendering for Edit/Write/MultiEdit when args parse cleanly */}
          <Show when={diff()}>
            {(d) => (
              <div class={`px-3 py-2 ${props.toolCall.status === 'error' && props.toolCall.result ? 'border-t border-hairline' : ''} bg-surface-base`}>
                {/* Review this change in the real editor (inline diff overlay). */}
                <div class="flex items-center justify-end gap-1.5 mb-1.5 flex-wrap">
                  {/* One control pair per composed hunk this call still owns.
                      External hunks cannot appear here by construction (they
                      have no tool call), so there is no reject to suppress. */}
                  <div class="flex items-center gap-1.5 flex-wrap mr-auto">
                  <For each={liveHunks().filter((h) => !isExternal(h))}>
                    {(hunk) => (
                      <span
                        class="inline-flex items-center gap-1 rounded-md border border-hairline px-1.5 py-0.5"
                        data-testid={`tool-hunk-${hunk.id}`}
                      >
                        <span class="text-[10px] font-mono text-muted-dark">
                          L{hunk.current_range.start}
                        </span>
                        <Show when={hunk.state !== 'accepted'}>
                          <button
                            type="button"
                            title="Accept this hunk"
                            data-testid={`tool-accept-${hunk.id}`}
                            onClick={() =>
                              review(reviewActions.setState(sessionId()!, hunk.id, 'accepted'))
                            }
                            class="rounded p-0.5 text-muted-dark hover:text-ok hover:bg-hover-wash"
                          >
                            <Check class="w-3 h-3" />
                          </button>
                        </Show>
                        <button
                          type="button"
                          title="Reject — reverts it on disk and tells the agent"
                          data-testid={`tool-reject-${hunk.id}`}
                          onClick={() => review(reviewActions.reject(sessionId()!, hunk.id))}
                          class="rounded p-0.5 text-muted-dark hover:text-error hover:bg-hover-wash"
                        >
                          <Undo2 class="w-3 h-3" />
                        </button>
                      </span>
                    )}
                  </For>
                  </div>
                  <button
                    type="button"
                    onClick={openInEditor}
                    disabled={opening()}
                    data-testid="tool-open-in-editor"
                    class="inline-flex items-center gap-1 rounded-md border border-hairline px-2 py-1 text-[11px] text-muted-dark hover:text-shell-ink hover:bg-hover-wash disabled:opacity-50"
                    title="Open the file in the editor with this change shown as an inline diff"
                  >
                    <FileOutput class="w-3.5 h-3.5" /> Open in editor
                  </button>
                </div>
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
          <div class="px-3 py-1.5 text-[10px] text-muted-dark border-t border-hairline">
            ID: {props.toolCall.callId ?? props.toolCall.id}
          </div>
        </div>
      </Show>
    </div>
  );
};

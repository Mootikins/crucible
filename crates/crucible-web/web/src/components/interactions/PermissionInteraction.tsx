import { Component, Show, createSignal, createResource } from 'solid-js';
import type { InteractionOf, PermResponse, PermissionScope } from '@/lib/types';
import { getFileContent } from '@/lib/api';
import { DiffViewer } from '@/components/DiffViewer';
import { btnPrimary, btnDanger, btnNeutral } from '@/lib/button-style';
import { deepPrettyPrintJson } from '@/lib/pretty-print';

interface Props {
  request: InteractionOf<'permission'>;
  onRespond: (response: PermResponse) => void;
}

// Tinted chip per action kind — the loud full-saturation bar (bg-attention
// slab) clashed with the UI's tinted vocabulary. A 15% fill + 50% border
// reads as the same family as btnPrimary/btnDanger while still flagging the
// action type at a glance.
const ACTION_LABELS: Record<string, { label: string; chip: string }> = {
  bash: { label: 'Execute', chip: 'bg-attention/15 text-attention border border-attention/50' },
  read: { label: 'Read', chip: 'bg-primary/15 text-primary border border-primary/50' },
  write: { label: 'Write', chip: 'bg-attention/15 text-attention border border-attention/50' },
  tool: { label: 'Tool', chip: 'bg-precog/15 text-precog border border-precog/50' },
};

/** Extract file path from a write permission request's tokens */
function extractFilePath(request: InteractionOf<'permission'>): string | null {
  if (request.action_type !== 'write') return null;
  // tokens[0] is typically the file path for write operations
  return request.tokens[0] ?? null;
}

/**
 * Full tool arguments as display pairs. A user must be able to see everything
 * they are approving — structured args (queries, URLs, nested objects) were
 * previously invisible unless mirrored into `tokens` (TUI parity:
 * `perm.full_commands`). No truncation; long values wrap.
 */
function toolArgPairs(request: InteractionOf<'permission'>): [string, string][] {
  if (request.action_type !== 'tool') return [];
  if (!request.tool_args || typeof request.tool_args !== 'object') return [];
  return Object.entries(request.tool_args as Record<string, unknown>).map(([k, v]) => [
    k,
    prettyPrintMaybeJson(v),
  ]);
}

/**
 * Render one value for the approval dialog. Delegates the decoding to the
 * shared `deepPrettyPrintJson` — same unwrapping as tool results, and
 * depth-capped, unlike a local re-parse loop. Expanding JSON nested inside
 * string fields matters most here: the user is approving whatever this box
 * shows, so an escaped one-line blob hides part of the request.
 */
function prettyPrintMaybeJson(raw: unknown): string {
  const decoded = deepPrettyPrintJson(raw);
  return typeof decoded === 'string' ? decoded : JSON.stringify(decoded, null, 2);
}

/** Extract new content from tool_args if available */
function extractNewContent(request: InteractionOf<'permission'>): string | null {
  if (!request.tool_args || typeof request.tool_args !== 'object') return null;
  const args = request.tool_args as Record<string, unknown>;
  // Common field names for file content in tool args
  if (typeof args.content === 'string') return args.content;
  if (typeof args.new_content === 'string') return args.new_content;
  if (typeof args.text === 'string') return args.text;
  return null;
}

export const PermissionInteraction: Component<Props> = (props) => {
  const [scope, setScope] = createSignal<PermissionScope>('once');
  const [showScopes, setShowScopes] = createSignal(false);
  const [showDiff, setShowDiff] = createSignal(true);

  const actionInfo = () => ACTION_LABELS[props.request.action_type] || ACTION_LABELS.tool;

  // A tool request used to render a generic "Tool" chip AND a "Tool: <name>"
  // line beneath it — the word twice, the name once, and two lines spent on
  // one fact. The chip carries the tool's own name instead: it is the thing
  // that identifies the request, and "Permission Required" beside it already
  // says what kind of card this is. Non-tool actions keep their verb chip
  // ("Execute", "Read", "Write"), which is their identifying label.
  const isNamedTool = () =>
    props.request.action_type === 'tool' && !!props.request.tool_name;
  const chipLabel = () => (isNamedTool() ? props.request.tool_name! : actionInfo().label);
  // Two values, deliberately separate: the daemon's permission engine
  // pattern-matches against the EXACT token shape (`tokens.join(' ')`), so
  // the response payload must carry the raw pattern. The display can still
  // pretty-print the same string for human legibility — but the formatted
  // version must never reach `PermResponse.pattern`, or "Allow for session"
  // silently fails to grant (the persisted pattern won't match future
  // requests).
  const commandPattern = () => props.request.tokens.join(' ');
  const commandDisplay = () => prettyPrintMaybeJson(commandPattern());

  const filePath = () => extractFilePath(props.request);
  const newContent = () => extractNewContent(props.request);

  // Fetch old content when we have a write action with a file path
  const [oldContent] = createResource(
    () => {
      const path = filePath();
      const content = newContent();
      if (path && content !== null) return path;
      return false;
    },
    async (path) => {
      if (typeof path !== 'string') return '';
      try {
        return await getFileContent(path);
      } catch {
        // File may not exist yet (new file creation) — treat as empty
        return '';
      }
    },
  );

  const hasDiff = () => {
    return props.request.action_type === 'write' && newContent() !== null && oldContent() !== undefined;
  };

  const handleAllow = () => {
    props.onRespond({
      kind: 'permission',
      allowed: true,
      pattern: commandPattern(),
      scope: scope(),
    });
  };

  const handleDeny = () => {
    props.onRespond({
      kind: 'permission',
      allowed: false,
      scope: 'once',
    });
  };

  return (
    <div class="bg-surface-elevated rounded-lg p-4 mb-4 border border-hairline">
      <div class="flex items-center gap-2 mb-3">
        <span
          class={`px-2 py-0.5 text-[11px] font-medium rounded-md ${actionInfo().chip} ${isNamedTool() ? 'font-mono' : ''}`}
          data-testid="perm-action-chip"
        >
          {chipLabel()}
        </span>
        <span class="text-[11px] uppercase tracking-wider text-muted-dark font-semibold">
          Permission Required
        </span>
      </div>

      {/* Full tool arguments — everything being approved must be visible */}
      <Show when={toolArgPairs(props.request).length > 0}>
        <div
          class="bg-surface-base rounded-md p-3 mb-4 font-mono text-xs text-shell-ink"
          data-testid="perm-tool-args"
        >
          {toolArgPairs(props.request).map(([key, value]) => (
            <div class="whitespace-pre-wrap break-all">
              <span class="text-muted">{key}=</span>
              {value}
            </div>
          ))}
        </div>
      </Show>

      {/* File path display for write actions */}
      <Show when={props.request.action_type === 'write' && filePath()}>
        <p class="text-shell-body mb-2 text-sm">
          File: <span class="text-shell-ink font-mono">{filePath()}</span>
        </p>
      </Show>

      {/* Diff preview for file write permissions */}
      <Show when={hasDiff() && !oldContent.loading}>
        <div class="mb-4">
          <button
            onClick={() => setShowDiff(!showDiff())}
            class="flex items-center gap-1 text-xs text-muted hover:text-shell-ink mb-2 transition-colors"
          >
            <span
              class="inline-block transition-transform duration-200"
              classList={{ 'rotate-90': showDiff() }}
            >
              ▶
            </span>
            {showDiff() ? 'Hide changes' : 'Show changes'}
          </button>
          <Show when={showDiff()}>
            <DiffViewer
              oldContent={oldContent() ?? ''}
              newContent={newContent()!}
              fileName={filePath() ?? undefined}
            />
          </Show>
        </div>
      </Show>

      {/* Loading state while fetching old content */}
      <Show when={hasDiff() && oldContent.loading}>
        <div class="mb-4 text-xs text-muted-dark flex items-center gap-2">
          <span class="inline-block w-3 h-3 border border-muted-dark border-t-transparent rounded-full animate-spin" />
          Loading file for diff...
        </div>
      </Show>

      {/* Fallback: show raw command text when neither a diff nor the
          tool-args block already covers the request */}
      <Show when={!hasDiff() && (commandDisplay() !== '' || toolArgPairs(props.request).length === 0)}>
        <div class="bg-surface-base rounded-md p-3 mb-4 font-mono text-xs text-shell-ink overflow-x-auto">
          {commandDisplay() || '(no arguments)'}
        </div>
      </Show>

      <div class="flex items-center gap-2 flex-wrap">
        <button onClick={handleAllow} class={btnPrimary}>
          Allow
        </button>
        <button onClick={handleDeny} class={btnDanger}>
          Deny
        </button>

        <button
          onClick={() => setShowScopes(!showScopes())}
          class={btnNeutral}
        >
          {showScopes() ? 'Hide options' : 'More options...'}
        </button>
      </div>

      <Show when={showScopes()}>
        <div class="mt-3 pt-3 border-t border-hairline">
          <p class="text-muted text-xs mb-2">Allow for:</p>
          <div class="flex gap-1.5 flex-wrap">
            {(['once', 'session', 'project', 'user'] as PermissionScope[]).map((s) => (
              <button
                onClick={() => setScope(s)}
                classList={{
                  'px-2.5 py-1 text-[11px] rounded-md border transition-colors font-medium': true,
                  'bg-primary/15 text-primary border-primary/40': scope() === s,
                  'bg-surface-elevated text-shell-body border-hairline hover:bg-hover-wash': scope() !== s,
                }}
              >
                {s.charAt(0).toUpperCase() + s.slice(1)}
              </button>
            ))}
          </div>
        </div>
      </Show>
    </div>
  );
};

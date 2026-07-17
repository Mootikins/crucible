import { Component, Show, createSignal, createResource } from 'solid-js';
import type { PermRequest, PermResponse, PermissionScope } from '@/lib/types';
import { getFileContent } from '@/lib/api';
import { DiffViewer } from '@/components/DiffViewer';

interface Props {
  request: PermRequest;
  onRespond: (response: PermResponse) => void;
}

const ACTION_LABELS: Record<string, { label: string; color: string }> = {
  bash: { label: 'Execute', color: 'bg-orange-600' },
  read: { label: 'Read', color: 'bg-primary' },
  write: { label: 'Write', color: 'bg-attention' },
  tool: { label: 'Tool', color: 'bg-precog' },
};

/** Extract file path from a write permission request's tokens */
function extractFilePath(request: PermRequest): string | null {
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
function toolArgPairs(request: PermRequest): [string, string][] {
  if (request.action_type !== 'tool') return [];
  if (!request.tool_args || typeof request.tool_args !== 'object') return [];
  return Object.entries(request.tool_args as Record<string, unknown>).map(([k, v]) => [
    k,
    typeof v === 'string' ? v : JSON.stringify(v),
  ]);
}

/** Extract new content from tool_args if available */
function extractNewContent(request: PermRequest): string | null {
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
  const commandText = () => props.request.tokens.join(' ');

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
      allowed: true,
      pattern: commandText(),
      scope: scope(),
    });
  };

  const handleDeny = () => {
    props.onRespond({
      allowed: false,
      scope: 'once',
    });
  };

  return (
    <div class="bg-surface-elevated rounded-lg p-4 mb-4 border border-attention/50">
      <div class="flex items-center gap-2 mb-3">
        <span class={`px-2 py-1 text-xs font-medium text-white rounded ${actionInfo().color}`}>
          {actionInfo().label}
        </span>
        <span class="text-muted text-sm">Permission Required</span>
      </div>

      <Show when={props.request.action_type === 'tool' && props.request.tool_name}>
        <p class="text-shell-body mb-2">
          Tool: <span class="text-shell-ink font-mono">{props.request.tool_name}</span>
        </p>
      </Show>

      {/* Full tool arguments — everything being approved must be visible */}
      <Show when={toolArgPairs(props.request).length > 0}>
        <div
          class="bg-surface-base rounded-md p-3 mb-4 font-mono text-sm text-shell-ink"
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
      <Show when={!hasDiff() && (commandText() !== '' || toolArgPairs(props.request).length === 0)}>
        <div class="bg-surface-base rounded-md p-3 mb-4 font-mono text-sm text-shell-ink overflow-x-auto">
          {commandText() || '(no arguments)'}
        </div>
      </Show>

      <div class="flex items-center gap-2 flex-wrap">
        <button
          onClick={handleAllow}
          class="px-4 py-2 bg-ok text-white rounded-md hover:bg-ok transition-colors font-medium"
        >
          Allow
        </button>
        <button
          onClick={handleDeny}
          class="px-4 py-2 bg-error text-white rounded-md hover:bg-error-dark transition-colors font-medium"
        >
          Deny
        </button>

        <button
          onClick={() => setShowScopes(!showScopes())}
          class="px-3 py-2 text-muted hover:text-shell-ink text-sm transition-colors"
        >
          {showScopes() ? 'Hide options' : 'More options...'}
        </button>
      </div>

      <Show when={showScopes()}>
        <div class="mt-3 pt-3 border-t border-hairline">
          <p class="text-muted text-sm mb-2">Allow for:</p>
          <div class="flex gap-2 flex-wrap">
            {(['once', 'session', 'project', 'user'] as PermissionScope[]).map((s) => (
              <button
                onClick={() => setScope(s)}
                class={`px-3 py-1 text-sm rounded-md transition-colors ${
                  scope() === s
                    ? 'bg-primary text-white'
                    : 'bg-control text-shell-body hover:bg-hover-wash'
                }`}
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

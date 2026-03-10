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
  read: { label: 'Read', color: 'bg-blue-600' },
  write: { label: 'Write', color: 'bg-yellow-600' },
  tool: { label: 'Tool', color: 'bg-purple-600' },
};

/** Extract file path from a write permission request's tokens */
function extractFilePath(request: PermRequest): string | null {
  if (request.action_type !== 'write') return null;
  // tokens[0] is typically the file path for write operations
  return request.tokens[0] ?? null;
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
    <div class="bg-neutral-800 rounded-lg p-4 mb-4 border border-amber-600/50">
      <div class="flex items-center gap-2 mb-3">
        <span class={`px-2 py-1 text-xs font-medium text-white rounded ${actionInfo().color}`}>
          {actionInfo().label}
        </span>
        <span class="text-neutral-400 text-sm">Permission Required</span>
      </div>

      <Show when={props.request.action_type === 'tool' && props.request.tool_name}>
        <p class="text-neutral-300 mb-2">
          Tool: <span class="text-neutral-100 font-mono">{props.request.tool_name}</span>
        </p>
      </Show>

      {/* File path display for write actions */}
      <Show when={props.request.action_type === 'write' && filePath()}>
        <p class="text-neutral-300 mb-2 text-sm">
          File: <span class="text-neutral-100 font-mono">{filePath()}</span>
        </p>
      </Show>

      {/* Diff preview for file write permissions */}
      <Show when={hasDiff() && !oldContent.loading}>
        <div class="mb-4">
          <button
            onClick={() => setShowDiff(!showDiff())}
            class="flex items-center gap-1 text-xs text-zinc-400 hover:text-zinc-200 mb-2 transition-colors"
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
        <div class="mb-4 text-xs text-zinc-500 flex items-center gap-2">
          <span class="inline-block w-3 h-3 border border-zinc-500 border-t-transparent rounded-full animate-spin" />
          Loading file for diff...
        </div>
      </Show>

      {/* Fallback: show raw command text when no diff available */}
      <Show when={!hasDiff()}>
        <div class="bg-neutral-900 rounded-md p-3 mb-4 font-mono text-sm text-neutral-100 overflow-x-auto">
          {commandText() || '(no arguments)'}
        </div>
      </Show>

      <div class="flex items-center gap-2 flex-wrap">
        <button
          onClick={handleAllow}
          class="px-4 py-2 bg-green-600 text-white rounded-md hover:bg-green-700 transition-colors font-medium"
        >
          Allow
        </button>
        <button
          onClick={handleDeny}
          class="px-4 py-2 bg-red-600 text-white rounded-md hover:bg-red-700 transition-colors font-medium"
        >
          Deny
        </button>

        <button
          onClick={() => setShowScopes(!showScopes())}
          class="px-3 py-2 text-neutral-400 hover:text-neutral-200 text-sm transition-colors"
        >
          {showScopes() ? 'Hide options' : 'More options...'}
        </button>
      </div>

      <Show when={showScopes()}>
        <div class="mt-3 pt-3 border-t border-neutral-700">
          <p class="text-neutral-400 text-sm mb-2">Allow for:</p>
          <div class="flex gap-2 flex-wrap">
            {(['once', 'session', 'project', 'user'] as PermissionScope[]).map((s) => (
              <button
                onClick={() => setScope(s)}
                class={`px-3 py-1 text-sm rounded-md transition-colors ${
                  scope() === s
                    ? 'bg-blue-600 text-white'
                    : 'bg-neutral-700 text-neutral-300 hover:bg-neutral-600'
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

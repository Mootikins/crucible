import { Component, Show, createSignal } from 'solid-js';
import type { PermRequest, PermResponse, PermissionScope } from '@/lib/types';

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

export const PermissionInteraction: Component<Props> = (props) => {
  const [scope, setScope] = createSignal<PermissionScope>('once');
  const [showScopes, setShowScopes] = createSignal(false);

  const actionInfo = () => ACTION_LABELS[props.request.action_type] || ACTION_LABELS.tool;
  const commandText = () => props.request.tokens.join(' ');

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

      <div class="bg-neutral-900 rounded-md p-3 mb-4 font-mono text-sm text-neutral-100 overflow-x-auto">
        {commandText() || '(no arguments)'}
      </div>

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

import type { Component } from 'solid-js';
import { Plus } from 'lucide-solid';

interface EmptyStateProps {
  onAction?: () => void;
  actionLabel?: string;
}

/**
 * Empty state component shown when the center pane has no tabs.
 * Provides clear guidance and an action button to recover.
 */
export const EmptyState: Component<EmptyStateProps> = (props) => {
  return (
    <div class="flex-1 flex flex-col items-center justify-center bg-zinc-900/50 gap-6 p-8">
      <div class="text-center space-y-2">
        <h2 class="text-lg font-semibold text-zinc-100">No session open</h2>
        <p class="text-sm text-zinc-400">
          Select a session from the left panel or create a new one to get started.
        </p>
      </div>

      {props.onAction && (
        <button
          onClick={props.onAction}
          class="flex items-center gap-2 px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white text-sm font-medium rounded-lg transition-colors"
        >
          <Plus class="w-4 h-4" />
          {props.actionLabel || 'New Session'}
        </button>
      )}
    </div>
  );
};

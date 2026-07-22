import type { Component } from 'solid-js';
import { Plus } from '@/lib/icons';

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
    <div class="flex-1 flex flex-col items-center justify-center bg-shell-bg gap-6 p-8">
      {/* The center is the EDITING surface (sessions dock right, WS-220) —
          point at opening content, not at sessions. */}
      <div class="text-center space-y-2">
        <h2 class="text-lg font-semibold text-shell-ink">Nothing open</h2>
        <p class="text-sm text-muted">
          Open a file from the Files panel or with <kbd class="px-1 rounded bg-surface-elevated text-shell-body">Ctrl+P</kbd>,
          or start a new session.
        </p>
      </div>

      {props.onAction && (
        <button
          onClick={props.onAction}
          data-testid="empty-state-action"
          class="flex items-center gap-2 px-4 py-2 bg-primary hover:bg-primary-hover text-white text-sm font-medium rounded-lg transition-colors"
        >
          <Plus class="w-4 h-4" />
          {props.actionLabel || 'New Session'}
        </button>
      )}
    </div>
  );
};

import { Component, Show, createSignal } from 'solid-js';
import { LayoutDashboard } from '@/lib/icons';
import { SectionHeader } from './primitives';
import { windowActions } from '@/stores/windowStore';
import { useResetLayout } from '@/lib/query/layout';
import { notificationActions } from '@/stores/notificationStore';

/**
 * The workspace itself: the arrangement of panes, tabs and edge panels.
 *
 * It is settings rather than a command because it is destructive and rare —
 * the palette is for things you do, and this is a thing you undo. It also has
 * nowhere else to live: a layout you have broken badly enough to want reset is
 * one where finding any other control is the problem.
 */
export const WorkspaceSettingsSection: Component = () => {
  const resetMutation = useResetLayout();
  const [busy, setBusy] = createSignal(false);
  const [confirming, setConfirming] = createSignal(false);

  const reset = async () => {
    setBusy(true);
    try {
      // Server copy FIRST. The store write below triggers the layout
      // auto-save, so deleting afterwards would race it and could leave the
      // old layout on disk to come back on the next load.
      await resetMutation.mutateAsync();
      windowActions.resetLayoutToDefaults();
      notificationActions.addNotification('info', 'Pane layout reset to defaults');
    } catch (err) {
      notificationActions.addNotification(
        'error',
        err instanceof Error ? err.message : 'Could not reset the layout',
      );
    } finally {
      setBusy(false);
      setConfirming(false);
    }
  };

  return (
    <>
      <SectionHeader title="Workspace" icon={LayoutDashboard} />
      <tr class="border-b border-hairline">
        <td class="py-3 align-top">
          <div class="text-sm text-shell-body">Reset pane layout</div>
          <div class="mt-0.5 max-w-[34rem] text-floor leading-4 text-muted-dark">
            Return the panes, tabs and edge panels to the arrangement a fresh
            install ships with. Sessions, notes and settings are untouched —
            this moves windows, nothing else.
          </div>
        </td>
        <td class="py-3 text-right align-top">
          {/* Two steps, because it is not undoable and the first click is easy
              to make by accident while reading the row above it. */}
          <Show
            when={confirming()}
            fallback={
              <button
                type="button"
                class="focus-ring rounded border border-hairline-strong px-2.5 py-1 text-xs text-shell-body transition-colors hover:border-muted-dark hover:text-shell-ink"
                onClick={() => setConfirming(true)}
                data-testid="settings-reset-layout"
              >
                Reset layout
              </button>
            }
          >
            <span class="inline-flex items-center gap-1.5">
              <button
                type="button"
                class="focus-ring rounded px-2.5 py-1 text-xs text-muted transition-colors hover:text-shell-ink"
                onClick={() => setConfirming(false)}
                disabled={busy()}
              >
                Cancel
              </button>
              <button
                type="button"
                class="focus-ring rounded bg-error px-2.5 py-1 text-xs text-white transition-colors hover:bg-error-dark disabled:opacity-50"
                onClick={() => void reset()}
                disabled={busy()}
                data-testid="settings-reset-layout-confirm"
              >
                {busy() ? 'Resetting…' : 'Reset layout'}
              </button>
            </span>
          </Show>
        </td>
      </tr>
    </>
  );
};

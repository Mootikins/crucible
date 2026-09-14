/**
 * Conflicts — every note write waiting on a person to choose.
 *
 * A stale write the daemon could neither take nor merge stays in the outbox
 * with the merged text and its regions (`lib/offline/outbox.ts`). This panel is
 * the index into them: a list while nothing is chosen, and `ConflictView` for
 * the one a surface selected. It replaces the conflict copy, whose whole defect
 * was that nothing listed it — a second note under a dated name that a user met
 * by accident, weeks later, or never.
 *
 * The badge, the phone's More sheet and the Changes panel all open this one
 * surface through `openConflict`, so a phone and a desktop settle a conflict
 * the same way.
 */
import { Component, For, Show, onMount } from 'solid-js';
import { PanelShell } from './PanelShell';
import { PanelHeader } from './PanelHeader';
import { ConflictView } from './ConflictView';
import { conflictActions, conflictStore } from '@/lib/conflicts';
import { notificationActions } from '@/stores/notificationStore';
import { hit } from '@/lib/touch';

/** The note's own name; the full path is the row's title. */
const nameOf = (path: string) => path.split('/').pop() ?? path;

export const ConflictsPanel: Component = () => {
  onMount(() => {
    void conflictActions
      .refresh()
      .catch((e: Error) => notificationActions.addNotification('error', e.message));
  });

  const rows = () => conflictStore.list();
  // The selected path may name a conflict that has since been settled — from
  // another tab, or by a drain — so the row is read, not the path alone.
  const current = () => {
    const path = conflictStore.selected();
    return path ? conflictStore.get(path) : null;
  };
  const back = () => conflictActions.select(null);

  return (
    <PanelShell>
      <Show
        when={current()}
        fallback={
          <>
            <PanelHeader title="Conflicts" class="shrink-0">
              <p class="mt-1 text-floor text-muted-dark">
                Writing the daemon could neither take nor merge. Nothing is lost and nothing is
                written until you choose.
              </p>
            </PanelHeader>
            <div class="flex-1 overflow-y-auto">
              <Show
                when={rows().length > 0}
                fallback={
                  <p class="p-3 text-xs text-muted-dark" data-testid="conflicts-empty">
                    No note is waiting on a choice.
                  </p>
                }
              >
                <For each={rows()}>
                  {(row) => (
                    <div
                      class="flex items-center gap-2 border-b border-hairline px-3 py-1.5"
                      data-testid={`conflict-row-${row.path}`}
                    >
                      <div class="min-w-0 flex-1">
                        <div class="truncate text-xs font-mono text-shell-ink" title={row.path}>
                          {nameOf(row.path)}
                        </div>
                        <div class="text-floor text-muted-dark">
                          {row.regions.length} {row.regions.length === 1 ? 'region' : 'regions'} to
                          settle
                        </div>
                      </div>
                      <button
                        type="button"
                        title={`Settle ${row.path}`}
                        data-testid={`conflict-open-${row.path}`}
                        onClick={() => conflictActions.select(row.path)}
                        class={`shrink-0 rounded border border-hairline px-2 py-0.5 text-floor text-shell-ink hover:bg-hover-wash ${hit()}`}
                      >
                        Open
                      </button>
                    </div>
                  )}
                </For>
              </Show>
            </div>
          </>
        }
      >
        <ConflictView path={conflictStore.selected()!} onResolved={back} onClose={back} />
      </Show>
    </PanelShell>
  );
};

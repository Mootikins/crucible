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

export interface OutboxConflictViewProps {
  /** The note whose outbox conflict this settles. */
  path: string;
  /** Told when the resolution is safe with the daemon, or queued for it. */
  onResolved?: () => void;
  /** Told when the person leaves. */
  onClose?: () => void;
}

/**
 * `ConflictView` over the outbox conflict of one note.
 *
 * The outbox holds the conflict (`lib/offline/outbox.ts`), so this reads it
 * and gives its merged text and regions to the view. Save goes through
 * `conflictActions.resolve`, the one door that knows the hash to write against.
 */
export const OutboxConflictView: Component<OutboxConflictViewProps> = (props) => {
  onMount(() => {
    void conflictActions
      .refresh()
      .catch((e: Error) => notificationActions.addNotification('error', e.message));
  });

  const conflict = () => conflictStore.get(props.path);

  const save = async (text: string): Promise<void> => {
    const name = nameOf(props.path);
    const outcome = await conflictActions.resolve(props.path, text);
    if (outcome.queued) {
      notificationActions.addNotification(
        'info',
        `${name} is settled. It goes out when the daemon answers.`,
      );
      props.onResolved?.();
      return;
    }
    if (outcome.stale) {
      // The note moved AGAIN between the merge and the choice. Nothing is
      // settled, so the conflict stays where it is. The refresh gives the view
      // the new conflict, with its new hash.
      notificationActions.addNotification(
        'warning',
        `${name} changed again while you were choosing. Open it again to settle the new difference.`,
      );
      return;
    }
    notificationActions.addNotification('info', `Resolved ${name}.`);
    props.onResolved?.();
  };

  return (
    <Show
      when={conflict()}
      fallback={
        <p class="p-3 text-xs text-muted-dark" data-testid="conflict-missing">
          Nothing waits for this note.
        </p>
      }
    >
      {(row) => (
        <ConflictView
          path={row().path}
          mergedContent={row().mergedContent}
          regions={row().regions}
          baseHash={row().currentHash}
          onSave={save}
          onClose={() => props.onClose?.()}
        />
      )}
    </Show>
  );
};

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
                      class="flex items-center gap-2  px-3 py-1.5"
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
                        class={`shrink-0 rounded  px-2 py-0.5 text-floor text-shell-ink panel-hover hover:bg-hover-wash ${hit()}`}
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
        <OutboxConflictView path={conflictStore.selected()!} onResolved={back} onClose={back} />
      </Show>
    </PanelShell>
  );
};

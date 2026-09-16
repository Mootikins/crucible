import { Component, For, Show, createSignal, onMount } from 'solid-js';
import { useKilns } from '@/lib/query/kilns';
import { kilnLabel } from '@/lib/kiln-label';
import { kept, keptActions, keptMode, type OfflineMode } from '@/lib/offline/kept';
import { cacheKiln, dropKiln, kilnSize, pendingCount, syncNow } from '@/lib/offline/sync';
import { SectionHeader, SettingRow } from '@/components/settings/primitives';
import { notificationActions } from '@/stores/notificationStore';
import { Database } from '@/lib/icons';
import type { KilnListEntry } from '@/lib/types';

/** Bytes, as a person reads them. */
function readableSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/**
 * Which kilns this device keeps, and how much of each.
 *
 * The source of truth for the choice — the files drawer's picker writes the
 * same flag. A kiln is kept WHOLE, so the only question is whether its
 * attachments come too: they are the whole storage budget, and a phone should
 * not spend it without being asked.
 */
export const OfflineSettingsSection: Component = () => {
  const kilnsQuery = useKilns();
  const kilns = (): KilnListEntry[] => kilnsQuery.data ?? [];
  const [sizes, setSizes] = createSignal<Record<string, { notes: number; attachments: number }>>({});
  const [busy, setBusy] = createSignal<Record<string, string>>({});
  const [queued, setQueued] = createSignal(0);

  const refreshSizes = async () => {
    const next: Record<string, { notes: number; attachments: number }> = {};
    for (const path of Object.keys(kept())) next[path] = await kilnSize(path);
    setSizes(next);
    setQueued(await pendingCount());
  };

  onMount(() => {
    void refreshSizes();
  });

  const setMode = async (kilnPath: string, mode: OfflineMode | null) => {
    if (!mode) {
      keptActions.forget(kilnPath);
      await dropKiln(kilnPath);
      await refreshSizes();
      return;
    }
    keptActions.keep(kilnPath, mode);
    setBusy({ ...busy(), [kilnPath]: 'Fetching…' });
    try {
      const result = await cacheKiln(kilnPath, (done, total) =>
        setBusy({ ...busy(), [kilnPath]: `${done} of ${total}` }),
      );
      if (result.failed.length > 0) {
        notificationActions.addNotification(
          'warning',
          `${kilnLabel(kilnPath)}: ${result.failed.length} file(s) could not be fetched`,
        );
      }
    } catch {
      notificationActions.addNotification(
        'error',
        `Could not keep ${kilnLabel(kilnPath)} offline`,
      );
      keptActions.forget(kilnPath);
    } finally {
      const { [kilnPath]: _done, ...rest } = busy();
      void _done;
      setBusy(rest);
      await refreshSizes();
    }
  };

  return (
    <>
      <SectionHeader title="Offline" icon={Database} />

      <SettingRow
        label="Unsent edits"
        description="Notes edited on this device that the daemon has not received. They are sent when it answers again, and never discarded."
      >
        <div class="flex items-center gap-2">
          <span class="text-reading text-shell-ink" data-testid="offline-queued">
            {queued()}
          </span>
          <Show when={queued() > 0}>
            <button
              type="button"
              class="h-11 px-3 rounded text-xs text-shell-ink hover:bg-hover-wash transition-colors focus-ring"
              onClick={() => void syncNow().then(refreshSizes)}
            >
              Send now
            </button>
          </Show>
        </div>
      </SettingRow>

      <For each={kilns()}>
        {(kiln) => {
          const path = () => kiln.path;
          const mode = () => keptMode(path());
          const size = () => sizes()[path()];
          return (
            <SettingRow
              label={kiln.name?.trim() || kilnLabel(path())}
              description={
                busy()[path()] ??
                (mode()
                  ? `Kept offline · ${readableSize((size()?.notes ?? 0) + (size()?.attachments ?? 0))}` +
                    (mode() === 'everything'
                      ? ` (${readableSize(size()?.attachments ?? 0)} of it attachments)`
                      : '')
                  : 'Not kept on this device.')
              }
            >
              <select
                class="h-11 rounded border border-hairline bg-surface-base px-2 text-xs text-shell-ink focus-ring"
                data-testid={`offline-mode-${kiln.name ?? path()}`}
                value={mode() ?? 'off'}
                disabled={!!busy()[path()]}
                onChange={(e) => {
                  const next = e.currentTarget.value;
                  void setMode(path(), next === 'off' ? null : (next as OfflineMode));
                }}
              >
                <option value="off">Don't keep</option>
                <option value="notes">Notes only</option>
                <option value="everything">Everything, with images</option>
              </select>
            </SettingRow>
          );
        }}
      </For>
    </>
  );
};

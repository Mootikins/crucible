import { Component, Show, createSignal, onCleanup, onMount } from 'solid-js';
import { Cloud } from '@/lib/icons';
import { queuedCount } from '@/lib/offline/outbox';
import { isOnline, offlineStore, syncNow, warmIdentity } from '@/lib/offline/sync';

/**
 * Whether this device can reach the daemon, and how much writing it owes it.
 *
 * A queued write that looks saved and is not is the failure the whole offline
 * design exists to prevent, so the count is on the app bar rather than behind
 * a settings screen.
 */
export const OfflineBadge: Component = () => {
  const [online, setOnline] = createSignal(isOnline());
  const [queued, setQueued] = createSignal(0);

  const refresh = async () => {
    try {
      setQueued(await queuedCount(offlineStore()));
    } catch {
      /* no store yet: nothing is queued */
    }
  };

  onMount(() => {
    void refresh();
    // Learn the daemon's identity while it answers, so a write queued after
    // the network drops carries a stamp a later drain will accept.
    if (isOnline()) void warmIdentity().catch(() => undefined);
    const goOnline = () => {
      setOnline(true);
      void warmIdentity().catch(() => undefined);
      // Reconnecting is exactly when the queue should empty.
      void syncNow().then(refresh).catch(() => undefined);
    };
    const goOffline = () => setOnline(false);
    window.addEventListener('online', goOnline);
    window.addEventListener('offline', goOffline);
    const poll = window.setInterval(refresh, 5000);
    onCleanup(() => {
      window.removeEventListener('online', goOnline);
      window.removeEventListener('offline', goOffline);
      window.clearInterval(poll);
    });
  });

  return (
    <Show when={!online() || queued() > 0}>
      <button
        type="button"
        data-testid="offline-badge"
        aria-label={
          online()
            ? `${queued()} unsent edit${queued() === 1 ? '' : 's'}`
            : `Offline, ${queued()} unsent edit${queued() === 1 ? '' : 's'}`
        }
        class="h-11 px-2 flex items-center gap-1 shrink-0 rounded text-muted-dark hover:text-shell-ink hover:bg-hover-wash transition-colors focus-ring"
        onClick={() => void syncNow().then(refresh)}
      >
        <Show when={!online()}>
          <Cloud class="w-4 h-4" />
        </Show>
        <Show when={queued() > 0}>
          <span class="text-floor tabular-nums">{queued()}</span>
        </Show>
      </button>
    </Show>
  );
};

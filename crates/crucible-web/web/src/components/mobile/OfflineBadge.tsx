import { Component, Show, createSignal, onCleanup, onMount } from 'solid-js';
import { AlertTriangle, Cloud } from '@/lib/icons';
import { isOnline, pendingCount, syncNow, warmIdentity } from '@/lib/offline/sync';
import { conflictActions, conflictStore, openConflict } from '@/lib/conflicts';

/**
 * Whether this device can reach the daemon, and how much writing it owes it.
 *
 * A queued write that looks saved and is not is the failure the whole offline
 * design exists to prevent, so the count is on the app bar rather than behind
 * a settings screen.
 *
 * Two counts, never one sum. A queued write is owed to the NETWORK and leaves
 * on the next drain; a conflict is owed to a PERSON and no amount of sending
 * will clear it. Added together they would tell a user to keep pressing a
 * button that cannot help.
 */
export const OfflineBadge: Component = () => {
  const [online, setOnline] = createSignal(isOnline());
  const [queued, setQueued] = createSignal(0);
  const conflicts = () => conflictStore.count();

  const refresh = async () => {
    try {
      setQueued(await pendingCount());
    } catch {
      /* no store yet: nothing is queued */
    }
    try {
      await conflictActions.refresh();
    } catch {
      /* no store yet: nothing waits */
    }
  };

  const label = () =>
    [
      online() ? null : 'Offline',
      conflicts() > 0 ? `${conflicts()} conflict${conflicts() === 1 ? '' : 's'}` : null,
      `${queued()} unsent edit${queued() === 1 ? '' : 's'}`,
    ]
      .filter(Boolean)
      .join(', ');

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
    <Show when={!online() || queued() > 0 || conflicts() > 0}>
      <button
        type="button"
        data-testid="offline-badge"
        aria-label={label()}
        class="h-11 px-2 flex items-center gap-1 shrink-0 rounded text-muted-dark hover:text-shell-ink hover:bg-hover-wash transition-colors focus-ring"
        // Sending is the right answer to a queue and the wrong one to a
        // conflict: the daemon already refused that write, and only a person
        // can settle it. So a waiting conflict takes the tap.
        onClick={() => (conflicts() > 0 ? openConflict() : void syncNow().then(refresh))}
      >
        <Show when={!online()}>
          <Cloud class="w-4 h-4" />
        </Show>
        <Show when={conflicts() > 0}>
          <AlertTriangle class="w-4 h-4 text-attention" />
          <span
            class="text-floor tabular-nums text-attention"
            data-testid="offline-badge-conflicts"
          >
            {conflicts()}
          </span>
        </Show>
        <Show when={queued() > 0}>
          <span class="text-floor tabular-nums">{queued()}</span>
        </Show>
      </button>
    </Show>
  );
};

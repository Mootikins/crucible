import { Component, For, Show, createEffect, createSignal, on } from 'solid-js';
import { useSessionSafe } from '@/contexts/SessionContext';
import { getSessionStatus, type SessionStatusSlot } from '@/lib/api';

/**
 * Read-only status strip for the current session: whatever keyed slots the
 * daemon's plugins published, rendered as chips.
 *
 * Deliberately knows nothing about any particular plugin. A slot arrives as
 * `{key, plugin, text, level}`; this renders `text`, attributes it to `plugin`,
 * and picks a tone from `level`. There is no branch on `key` and no list of
 * known plugins — a plugin shipped tomorrow gets a chip here for free, which is
 * the whole point of the channel. Adding an `if (key === …)` would quietly
 * revoke that.
 *
 * `level` is matched loosely with a fallback because it is the plugin's word,
 * not an enum this file owns.
 */
const TONES: Record<string, string> = {
  warn: 'border-attention/40 bg-attention/10 text-attention',
  warning: 'border-attention/40 bg-attention/10 text-attention',
  error: 'border-error/40 bg-error/10 text-error',
};
const DEFAULT_TONE = 'border-hairline bg-surface-elevated text-muted';

export const SessionStatusChips: Component = () => {
  const { currentSession } = useSessionSafe();
  const [slots, setSlots] = createSignal<SessionStatusSlot[]>([]);

  // No SSE event carries plugin status, so the fetch hangs off the session id.
  // Scoped to the ACTIVE session rather than polling every open one: this is a
  // per-session daemon round trip.
  createEffect(
    on(
      () => currentSession()?.id,
      (id) => {
        // Clear first: the previous session's chips must never linger over a
        // new one while its fetch is in flight.
        setSlots([]);
        if (!id) return;
        getSessionStatus(id)
          .then((next) => currentSession()?.id === id && setSlots(next))
          // A failed status fetch is "no chips", never a notification. The
          // request fails on every daemon reconnect, and a session with
          // nothing to say is the normal case anyway.
          .catch(() => currentSession()?.id === id && setSlots([]));
      },
    ),
  );

  return (
    <Show when={slots().length > 0}>
      <div class="flex items-center gap-1 flex-wrap" data-testid="session-status">
        <For each={slots()}>
          {(slot) => (
            <span
              class={`inline-flex items-center px-2 py-0.5 rounded-md border text-[11px] ${
                TONES[slot.level] ?? DEFAULT_TONE
              }`}
              title={`${slot.text} — ${slot.plugin}`}
              data-testid={`session-status-${slot.key}`}
            >
              {slot.text}
            </span>
          )}
        </For>
      </div>
    </Show>
  );
};

import { Component, For, Show, createSignal, onMount } from 'solid-js';
import { getBus } from '@/lib/bus';
import { useSessionSafe } from '@/contexts/SessionContext';
import { useSessionModes } from '@/lib/query/modes';
import { useSessionStatus } from '@/lib/query/session-config';
import type { ModeDescriptor } from '@/lib/types';

/**
 * Read-only status strip for the current session: whatever keyed slots the
 * daemon's plugins published, rendered as chips, plus the one thing about a
 * session that is not any plugin's to say: what a note write in the current
 * mode does.
 *
 * The plugin half deliberately knows nothing about any particular plugin. A
 * slot arrives with a named color group; this renders `text`, attributes it
 * to `plugin`, and lets CSS resolve the group. There is no branch on `key`
 * and no list of known plugins — a plugin shipped tomorrow gets a chip here
 * for free, which is the whole point of the channel. Adding an
 * `if (key === …)` would quietly revoke that.
 *
 * A pre-color-group daemon has only `level`; keep its semantic fallback until
 * the daemon and web bundle have both been updated.
 *
 * A slot's `progress` is a fraction, the literal string `"indeterminate"`, or
 * `null` when the slot describes a state rather than work — `null` must stay
 * distinguishable from a bar pinned at zero, which would read as stalled.
 * This renders a fraction as a percentage and leaves a state slot untouched;
 * an indeterminate slot gets an ellipsis rather than a fabricated number.
 */
function progressSuffix(progress: unknown): string | null {
  if (typeof progress === 'number') {
    return ` ${Math.round(progress * 100)}%`;
  }
  if (progress === 'indeterminate') {
    return ' …';
  }
  return null;
}

const legacyGroup = (level: string): string => {
  if (level === 'warn' || level === 'warning') return 'warn';
  if (level === 'error' || level === 'danger') return 'danger';
  if (level === 'ok' || level === 'success') return 'ok';
  return 'info';
};

export const SessionStatusChips: Component = () => {
  const [menuOpen, setMenuOpen] = createSignal(false);
  onMount(() => getBus().on('openPluginApproval', () => setMenuOpen(true)));
  const { currentSession } = useSessionSafe();

  const sessionId = () => currentSession()?.session_id;
  // The chat pane reads this same list for its mode control. This component
  // used to fetch a second copy of it on every mount, and the two disagreed
  // the moment one of them failed.
  const modes = useSessionModes(() => sessionId() ?? null);

  // No SSE event carries plugin status, so the read hangs off the session id.
  // Scoped to the ACTIVE session rather than polling every open one: this is a
  // per-session daemon round trip. The key carries that id, so the previous
  // session's chips cannot linger over a new one while its read is in flight,
  // and a refused read is no chips rather than a notification.
  const status = useSessionStatus(() => sessionId() ?? null);
  const slots = () => [...(status.data ?? [])].sort((a, b) => (a.priority ?? 128) - (b.priority ?? 128));

  // The EFFECTIVE write mode, from the mode descriptor of the daemon.
  //
  // This file never derives it from the mode id. The daemon degrades the
  // configured value by what the agent can hold back: an ACP agent runs its
  // tools in its own process, so its `propose` mode comes back as `apply`.
  //
  // It reads the query and holds no copy, so a session with no answer yet (a
  // new one, or one whose list failed) reports no write mode, and not the
  // write mode of the session before it.
  const writes = (): ModeDescriptor['writes'] | null => {
    const listed = modes.data;
    if (!listed) return null;
    const current = listed.modes.find((mode) => mode.id === listed.current_mode_id);
    return current?.writes ?? null;
  };

  // The write mode is not a chip, because the mode control already says
  // "proposes". It stays on the wrapper as data for tests and plugins.
  const anything = () => slots().length > 0 || writes() !== null || menuOpen();
  const controlPlugins = () => [...new Set(slots().filter((slot) => slot.action === 'plugin_approval').map((slot) => slot.plugin))];

  return (
    <Show when={anything()}>
      <div class="contents" data-testid="session-status" data-writes={writes() ?? undefined}>
        <For each={slots()}>
          {(slot) => (
            <button
              type="button"
              class="session-status-color inline-flex items-center px-2 py-0.5 rounded-md border text-floor"
              data-status-color={slot.color_group ?? legacyGroup(slot.level)}
              data-pinned={slot.pinned ?? false}
              data-action={slot.action ?? undefined}
              title={`${slot.text} — ${slot.plugin}`}
              data-testid={`session-status-${slot.key}`}
              onClick={() => setMenuOpen(true)}
            >
              {slot.text}
              {progressSuffix(slot.progress)}
            </button>
          )}
        </For>
        <Show when={menuOpen()}>
          <div role="dialog" aria-label="Plugin approval" class="fixed bottom-16 right-6 z-50 rounded-lg border border-edge bg-surface p-3 shadow-xl" onKeyDown={(event) => { if (event.key === 'Escape') setMenuOpen(false); }}>
            <div class="flex items-center justify-between gap-4"><strong>Plugin approval</strong><button type="button" aria-label="Close plugin approval" onClick={() => setMenuOpen(false)}>×</button></div>
            {/* TODO(plugin-turns): bind these choices to the session knob when that branch lands. */}
            <For each={controlPlugins()}>{(plugin) => <div class="mt-2"><div>{plugin}</div><div class="text-floor-muted">inherit · ask · stop</div></div>}</For>
            <Show when={controlPlugins().length === 0}><p class="text-floor-muted">No plugin approval controls are active.</p></Show>
          </div>
        </Show>
      </div>
    </Show>
  );
};

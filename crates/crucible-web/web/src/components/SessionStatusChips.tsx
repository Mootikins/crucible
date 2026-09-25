import { Component, For, Show, createSignal, onCleanup, onMount } from 'solid-js';
import { Portal } from 'solid-js/web';
import { getBus } from '@/lib/bus';
import { useSessionSafe } from '@/contexts/SessionContext';
import { useSessionModes } from '@/lib/query/modes';
import {
  usePluginApprovals,
  useSessionStatus,
  useSetPluginApproval,
  type PluginApproval,
} from '@/lib/query/session-config';
import type { ModeDescriptor } from '@/lib/types';

/**
 * Read-only status strip for the current session: whatever keyed slots the
 * daemon's plugins published, rendered as chips, plus the one thing about a
 * session that is not any plugin's to say: what a note write in the current
 * mode does.
 *
 * The plugin half deliberately knows nothing about any particular plugin. A
 * slot arrives with a named color group; this renders `text`, attributes it
 * to `plugin`, and lets CSS resolve the group. There is no branch on `id`
 * and no list of known plugins — a plugin shipped tomorrow gets a chip here
 * for free, which is the whole point of the channel. Adding an
 * `if (id === …)` would quietly revoke that.
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

/**
 * The three values of the approval knob, in menu order. The type check below
 * fails the build when the daemon's `PluginApproval` gains a value that this
 * list does not name.
 */
const APPROVALS = ['inherit', 'ask', 'stop'] as const satisfies readonly PluginApproval[];
type Unlisted = Exclude<PluginApproval, (typeof APPROVALS)[number]>;
const approvalsComplete: [Unlisted] extends [never] ? true : never = true;
void approvalsComplete;

export const SessionStatusChips: Component = () => {
  const { currentSession } = useSessionSafe();
  const sessionId = () => currentSession()?.session_id;
  const modes = useSessionModes(() => sessionId() ?? null);
  const status = useSessionStatus(() => sessionId() ?? null);
  const slots = () => [...(status.data ?? [])].sort((a, b) => a.priority - b.priority);
  type Slot = ReturnType<typeof slots>[number];
  const writes = (): ModeDescriptor['writes'] | null => {
    const listed = modes.data;
    return listed?.modes.find((mode) => mode.id === listed.current_mode_id)?.writes ?? null;
  };
  const [menuOpen, setMenuOpen] = createSignal(false);
  const [approvalOpen, setApprovalOpen] = createSignal(false);
  // The engine control of decision 10: each plugin that starts turns, with
  // the value that the session holds. Read from the daemon when the menu
  // opens; `plugin_approval_changed` refreshes it while it is open.
  const approvals = usePluginApprovals(() => (approvalOpen() ? sessionId() ?? null : null));
  const setApproval = useSetPluginApproval();
  const [approvalError, setApprovalError] = createSignal<string | null>(null);
  const chooseApproval = async (plugin: string, approval: PluginApproval) => {
    const id = sessionId();
    if (!id) return;
    setApprovalError(null);
    try {
      await setApproval.mutateAsync({ id, plugin, approval });
    } catch (err) {
      setApprovalError(err instanceof Error ? err.message : 'Failed to set plugin approval');
    }
  };
  const [detail, setDetail] = createSignal<string | null>(null);
  const [preview, setPreview] = createSignal(false);
  const [maxWidth, setMaxWidth] = createSignal(420);
  const [position, setPosition] = createSignal({ right: 16, bottom: 64 });
  let area: HTMLDivElement | undefined;
  let strip: HTMLDivElement | undefined;
  let menu: HTMLDivElement | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let scrollTimer: ReturnType<typeof setTimeout> | undefined;
  let touching = false;
  let pointerType = '';

  const scrollRight = () => { if (strip) strip.scrollLeft = strip.scrollWidth; };
  const scrollAfterExpand = () => {
    if (scrollTimer) clearTimeout(scrollTimer);
    scrollTimer = setTimeout(scrollRight, 310);
  };
  const collapseLater = () => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      if (touching || menuOpen() || approvalOpen() || detail()) { collapseLater(); return; }
      setPreview(false);
      (document.activeElement as HTMLElement | null)?.blur();
    }, 3000);
  };
  const expand = () => {
    setPreview(true);
    scrollAfterExpand();
    collapseLater();
  };
  const openMenu = () => {
    const rect = area?.getBoundingClientRect();
    if (rect) setPosition({ right: Math.max(8, innerWidth - rect.right), bottom: Math.max(8, innerHeight - rect.top + 6) });
    setMenuOpen(true);
    queueMicrotask(() => menu?.querySelector<HTMLButtonElement>('[role="menuitem"]')?.focus());
  };
  const choose = (slot: Slot) => {
    setMenuOpen(false);
    if (slot.action === 'plugin_approval') setApprovalOpen(true);
    else setDetail(`${slot.text} — ${slot.plugin}`);
  };
  const dot = (slot: Slot) => (
    <button type="button" class="session-status-color status-dot-button"
      data-status-color={slot.color_group}
      data-pinned={slot.pinned} data-action={slot.action ?? undefined}
      data-testid={`session-status-${slot.id}`}
      title={`${slot.text} — ${slot.plugin}`}
      aria-label={`${slot.text} — ${slot.plugin}; open session status`}
      aria-haspopup="menu" aria-expanded={menuOpen()}
      onClick={(event) => {
        if (pointerType === 'touch' && event.detail > 0) {
          if (!preview()) { expand(); return; }
          const rect = area?.getBoundingClientRect();
          if (rect) setPosition({ right: Math.max(8, innerWidth - rect.right), bottom: Math.max(8, innerHeight - rect.top + 6) });
          choose(slot);
          return;
        }
        openMenu();
      }}>
      <span class="status-dot" data-testid="status-dot" aria-hidden="true" />
      <span class="status-dot-name">{slot.text}{progressSuffix(slot.progress)}</span>
    </button>
  );

  onMount(() => {
    const off = getBus().on('openPluginApproval', () => setApprovalOpen(true));
    const row = area?.parentElement?.parentElement;
    const resize = () => setMaxWidth(Math.min(420, Math.max(80, (row?.clientWidth ?? 800) * .48)));
    resize();
    let frame = 0;
    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(() => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(resize);
    });
    if (row) observer?.observe(row);
    const outside = (event: PointerEvent) => {
      if (menuOpen() && !area?.contains(event.target as Node) && !menu?.contains(event.target as Node)) setMenuOpen(false);
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      setMenuOpen(false); setApprovalOpen(false); setDetail(null);
      area?.querySelector<HTMLButtonElement>('button')?.focus();
    };
    document.addEventListener('pointerdown', outside);
    document.addEventListener('keydown', escape);
    onCleanup(() => {
      off(); observer?.disconnect(); cancelAnimationFrame(frame);
      document.removeEventListener('pointerdown', outside);
      document.removeEventListener('keydown', escape);
      if (timer) clearTimeout(timer);
      if (scrollTimer) clearTimeout(scrollTimer);
    });
  });

  return <Show when={slots().length || writes() !== null || approvalOpen()}>
    <div ref={area} class="session-status-area" classList={{ 'is-preview': preview() }}
      data-testid="session-status" data-writes={writes() ?? undefined}
      style={{ 'max-width': `${maxWidth()}px` }}
      onPointerEnter={scrollAfterExpand} onFocusIn={scrollAfterExpand}
      onPointerDown={(e) => { pointerType = e.pointerType; touching = e.pointerType === 'touch'; }}
      onPointerUp={() => { touching = false; if (preview()) collapseLater(); }}
      onPointerCancel={() => { touching = false; }}>
      <div ref={strip} class="session-status-strip" data-testid="session-status-strip"
        role="group" aria-label="Informational status; scroll horizontally for more">
        <For each={slots().filter((s) => !s.pinned)}>{dot}</For>
      </div>
      <div class="session-status-pinned"><For each={slots().filter((s) => s.pinned)}>{dot}</For></div>
    </div>
    <Show when={menuOpen()}><Portal>
      <div ref={menu} role="menu" aria-label="Session status" class="status-menu"
        style={{ right: `${position().right}px`, bottom: `${position().bottom}px` }}
        onKeyDown={(e) => {
          if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
          e.preventDefault();
          const items = [...menu!.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')];
          const next = (items.indexOf(document.activeElement as HTMLButtonElement) + (e.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length;
          items[next]?.focus();
        }}>
        <div class="status-menu-title">Session status</div>
        <For each={slots()}>{(slot) => <button type="button" role="menuitem" class="status-menu-item" onClick={() => choose(slot)}>
          <span class="session-status-color status-dot" data-status-color={slot.color_group} aria-hidden="true" />
          <span>{slot.text}</span><small>{slot.plugin}</small>
        </button>}</For>
      </div>
    </Portal></Show>
    <Show when={approvalOpen()}><Portal><div role="dialog" aria-label="Plugin approval" class="status-menu status-dialog"
      style={{ right: `${position().right}px`, bottom: `${position().bottom}px` }}>
      <div class="flex items-center justify-between gap-4"><strong>Plugin approval</strong><button type="button" aria-label="Close plugin approval" onClick={() => setApprovalOpen(false)}>×</button></div>
      <For each={Object.entries(approvals.data ?? {})}>{([plugin, current]) =>
        <div role="radiogroup" aria-label={plugin} class="status-approval-row">
          <span class="status-approval-plugin">{plugin}</span>
          <For each={APPROVALS}>{(value) =>
            <button type="button" role="radio" aria-label={`${plugin}: ${value}`}
              aria-checked={current === value} class="status-approval-value"
              onClick={() => void chooseApproval(plugin, value)}>{value}</button>
          }</For>
        </div>
      }</For>
      <Show when={approvals.isSuccess && Object.keys(approvals.data ?? {}).length === 0}>
        <p class="text-floor-muted">No plugin starts turns in this session.</p>
      </Show>
      <Show when={approvalError() ?? (approvals.error?.message ?? null)}>{(message) =>
        <p role="alert" class="text-error">{message()}</p>
      }</Show>
    </div></Portal></Show>
    <Show when={detail()}><Portal><div role="dialog" aria-label="Status detail" class="status-menu status-dialog"
      style={{ right: `${position().right}px`, bottom: `${position().bottom}px` }}>
      <div class="flex items-center justify-between gap-4"><strong>{detail()}</strong><button type="button" aria-label="Close status detail" onClick={() => setDetail(null)}>×</button></div>
    </div></Portal></Show>
  </Show>;
};

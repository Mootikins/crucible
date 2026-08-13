import { Component, For, Show, createEffect, createSignal, onCleanup } from 'solid-js';
import { Portal } from 'solid-js/web';
import { notificationStore, notificationActions } from '@/stores/notificationStore';
import { placeFlyout } from '@/lib/popup-placement';
import type { Notification, NotificationType } from '@/lib/types';

/** Fixed panel width — no measuring pass, so placement is a single sync call. */
const PANEL_WIDTH = 320;
/** Distance from the bell, and the minimum from a viewport edge. */
const GAP = 8;
const EDGE_MARGIN = 8;

// ── Time grouping helpers ───────────────────────────────────────────────

interface NotificationGroup {
  label: string;
  items: Notification[];
}

function getTimeGroup(timestamp: number, now: number): string {
  const diff = now - timestamp;
  const DAY = 86_400_000;
  if (diff < DAY) return 'Today';
  if (diff < 2 * DAY) return 'Yesterday';
  return 'Older';
}

function groupNotifications(notifications: Notification[]): NotificationGroup[] {
  const now = Date.now();
  const groups = new Map<string, Notification[]>();
  const order = ['Today', 'Yesterday', 'Older'];

  for (const n of notifications) {
    const label = getTimeGroup(n.timestamp, now);
    const existing = groups.get(label);
    if (existing) {
      existing.push(n);
    } else {
      groups.set(label, [n]);
    }
  }

  // Sort within each group by timestamp descending (newest first)
  const result: NotificationGroup[] = [];
  for (const label of order) {
    const items = groups.get(label);
    if (items && items.length > 0) {
      items.sort((a, b) => b.timestamp - a.timestamp);
      result.push({ label, items });
    }
  }
  return result;
}

// ── Notification type styling ───────────────────────────────────────────

const TYPE_CONFIG: Record<NotificationType, { icon: string; color: string; bg: string }> = {
  info: { icon: 'ℹ', color: 'text-primary', bg: 'bg-primary/10' },
  success: { icon: '✓', color: 'text-ok', bg: 'bg-ok/10' },
  warning: { icon: '⚠', color: 'text-attention', bg: 'bg-attention/10' },
  error: { icon: '✕', color: 'text-error', bg: 'bg-error/10' },
};

function formatTime(timestamp: number): string {
  const d = new Date(timestamp);
  const h = d.getHours().toString().padStart(2, '0');
  const m = d.getMinutes().toString().padStart(2, '0');
  return `${h}:${m}`;
}

// ── Notification Item ───────────────────────────────────────────────────

const NotificationItem: Component<{ notification: Notification }> = (props) => {
  const cfg = () => TYPE_CONFIG[props.notification.type];

  return (
    <div
      class={`flex items-start gap-2.5 px-3 py-2 rounded-md transition-colors ${cfg().bg} hover:bg-hover-wash`}
      classList={{ 'opacity-50': props.notification.dismissed }}
    >
      <span class={`text-sm flex-shrink-0 mt-0.5 ${cfg().color}`}>
        {cfg().icon}
      </span>
      <div class="flex-1 min-w-0">
        <p class="text-xs text-shell-ink leading-snug break-words">
          {props.notification.message}
        </p>
        <Show when={props.notification.action && !props.notification.dismissed}>
          <button
            type="button"
            onClick={() => {
              props.notification.action!.run();
              notificationActions.dismiss(props.notification.id);
            }}
            class="mt-1 px-2 py-0.5 rounded border border-hairline-strong bg-control text-shell-ink text-[11px] font-medium hover:bg-hover-wash transition-colors"
          >
            {props.notification.action!.label}
          </button>
        </Show>
        <span class="text-[10px] text-muted-dark mt-0.5 block">
          {formatTime(props.notification.timestamp)}
        </span>
      </div>
      <Show when={!props.notification.dismissed}>
        <button
          type="button"
          onClick={() => notificationActions.dismiss(props.notification.id)}
          class="flex-shrink-0 text-muted-dark hover:text-shell-body transition-colors p-0.5"
          aria-label="Dismiss"
        >
          <svg class="w-3 h-3" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M4 4l8 8M12 4l-8 8" />
          </svg>
        </button>
      </Show>
    </div>
  );
};

// ── Notification Center Drawer ──────────────────────────────────────────

/**
 * Notification popout, anchored to the bell wherever it sits.
 *
 * Portaled and positioned in VIEWPORT coordinates rather than `absolute` inside
 * the trigger's parent, because the bell now lives on the right ribbon: every
 * ancestor there is `overflow-hidden` (the EdgePanel root, its slide clip
 * frame, and the WindowManager row), so an absolutely-positioned panel is
 * clipped to nothing no matter which side it opens toward. `placeFlyout` also
 * flips it leftward, since a 320px panel cannot fit to the right of a 40px rail.
 *
 * No backdrop; it dismisses on outside click or Escape.
 */
export const NotificationCenter: Component<{
  open: boolean;
  onClose: () => void;
  /** The bell, for placement and for the outside-click test. */
  anchor?: HTMLElement;
}> = (props) => {
  const [visible, setVisible] = createSignal(false);
  const [pos, setPos] = createSignal({ left: 0, bottom: 0, width: PANEL_WIDTH, maxHeight: 480 });
  let panelRef: HTMLDivElement | undefined;

  const place = () => {
    const anchor = props.anchor;
    if (!anchor) return;
    const rect = anchor.getBoundingClientRect();
    const viewport = { width: window.innerWidth, height: window.innerHeight };
    // Horizontal placement is `placeFlyout`'s: it flips leftward when the right
    // cannot hold 320px, which a 40px rail never can.
    const { left } = placeFlyout(rect, viewport, { width: PANEL_WIDTH, gap: GAP });
    // Vertical placement is NOT: `placeFlyout` aligns a submenu's TOP with its
    // row and clamps with `maxHeight`, so a panel shorter than its cap floats
    // high — measured 265px above a bell at the bottom of the rail. Anchoring
    // the BOTTOM instead pins it just above the bell whatever its content
    // height, with no measure-then-reposition pass.
    setPos({
      left,
      bottom: Math.max(EDGE_MARGIN, viewport.height - rect.top + GAP),
      width: PANEL_WIDTH,
      // Never taller than the space between the top margin and the bell.
      maxHeight: Math.max(120, Math.min(480, rect.top - GAP - EDGE_MARGIN)),
    });
  };

  // Animate in/out
  createEffect(() => {
    if (props.open) {
      // Mark all as read when the popout opens
      notificationActions.markAllRead();
      place();
      requestAnimationFrame(() => setVisible(true));
    } else {
      setVisible(false);
    }
  });

  // Close on Escape or outside click
  createEffect(() => {
    if (!props.open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        props.onClose();
      }
    };
    const onPointerDown = (e: MouseEvent) => {
      // Two elements, deliberately. Portaled, `panelRef.parentElement` is the
      // portal container rather than the bell's parent, so the old single
      // containment test read a click on the bell as OUTSIDE — which closed the
      // popout just as the bell's own handler reopened it.
      const target = e.target as Node;
      const insidePanel = panelRef?.contains(target) ?? false;
      const onAnchor = props.anchor?.contains(target) ?? false;
      if (!insidePanel && !onAnchor) props.onClose();
    };
    const onViewportChange = () => place();
    document.addEventListener('keydown', onKey, true);
    document.addEventListener('mousedown', onPointerDown);
    window.addEventListener('resize', onViewportChange);
    onCleanup(() => {
      document.removeEventListener('keydown', onKey, true);
      document.removeEventListener('mousedown', onPointerDown);
      window.removeEventListener('resize', onViewportChange);
    });
  });

  const allNotifications = () => [...notificationStore.notifications];
  const grouped = () => groupNotifications(allNotifications());
  const hasNotifications = () => allNotifications().length > 0;

  const handleClearAll = () => {
    notificationActions.clearAll();
  };

  return (
    <Show when={props.open}>
      <Portal>
        <div
          ref={panelRef}
          data-testid="notification-popout"
          style={{
            left: `${pos().left}px`,
            bottom: `${pos().bottom}px`,
            width: `${pos().width}px`,
            'max-height': `${pos().maxHeight}px`,
          }}
          class={`
          fixed z-50
          rounded-lg border border-hairline-strong bg-surface-overlay
          shadow-2xl shadow-black/50
          flex flex-col overflow-hidden
          origin-bottom-right
          transition-[opacity,scale,translate] duration-200 ease-out
          ${visible() ? 'opacity-100 scale-100 translate-y-0' : 'opacity-0 scale-95 translate-y-1'}
        `}
        >
          {/* Header */}
          <div class="flex items-center justify-between px-4 py-3 border-b border-hairline">
            <div class="flex items-center gap-2">
              <span class="text-sm font-medium text-shell-ink">Notifications</span>
              <Show when={allNotifications().length > 0}>
                <span class="text-[10px] px-1.5 py-0.5 rounded-full bg-surface-elevated text-muted tabular-nums">
                  {allNotifications().length}
                </span>
              </Show>
            </div>
            <div class="flex items-center gap-1">
              <Show when={hasNotifications()}>
                <button
                  type="button"
                  onClick={handleClearAll}
                  class="text-[11px] px-2 py-1 rounded text-muted hover:text-shell-ink hover:bg-hover-wash transition-colors"
                >
                  Clear All
                </button>
              </Show>
              <button
                type="button"
                onClick={() => props.onClose()}
                class="p-1 text-muted-dark hover:text-shell-ink hover:bg-hover-wash rounded transition-colors"
                aria-label="Close notifications"
              >
                <svg class="w-4 h-4" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="M4 4l8 8M12 4l-8 8" />
                </svg>
              </button>
            </div>
          </div>

          {/* Content */}
          <div class="flex-1 overflow-y-auto">
            <Show
              when={hasNotifications()}
              fallback={
                <div class="flex flex-col items-center justify-center py-10 text-muted-dark">
                  <span class="text-2xl mb-2">🔔</span>
                  <span class="text-xs">No notifications</span>
                </div>
              }
            >
              <div class="py-2">
                <For each={grouped()}>
                  {(group) => (
                    <div class="mb-1">
                      <div class="px-4 py-1.5">
                        <span class="text-[10px] font-semibold uppercase tracking-widest text-muted-dark">
                          {group.label}
                        </span>
                      </div>
                      <div class="px-2 space-y-0.5">
                        <For each={group.items}>
                          {(notif) => <NotificationItem notification={notif} />}
                        </For>
                      </div>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          </div>
        </div>
      </Portal>
    </Show>
  );
};

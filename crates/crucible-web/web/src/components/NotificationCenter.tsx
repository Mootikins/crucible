import { Component, For, Show, createEffect, createSignal, onCleanup } from 'solid-js';
import { notificationStore, notificationActions } from '@/stores/notificationStore';
import type { Notification, NotificationType } from '@/lib/types';

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
 * Notification popout — anchors to the corner bell (Adobe-style flyout),
 * expects a `position: relative` parent to hang from. No backdrop; it
 * dismisses on outside click or Escape.
 */
export const NotificationCenter: Component<{ open: boolean; onClose: () => void }> = (props) => {
  const [visible, setVisible] = createSignal(false);
  let panelRef: HTMLDivElement | undefined;

  // Animate in/out
  createEffect(() => {
    if (props.open) {
      // Mark all as read when the popout opens
      notificationActions.markAllRead();
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
      // The anchor parent contains both the bell and this panel — clicks
      // inside either keep the popout open (the bell's own handler toggles).
      if (panelRef && !panelRef.parentElement?.contains(e.target as Node)) {
        props.onClose();
      }
    };
    document.addEventListener('keydown', onKey, true);
    document.addEventListener('mousedown', onPointerDown);
    onCleanup(() => {
      document.removeEventListener('keydown', onKey, true);
      document.removeEventListener('mousedown', onPointerDown);
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
      {/* Popout above the bell, growing up-left from the corner. */}
      <div
        ref={panelRef}
        class={`
          absolute bottom-full right-0 mb-2 z-50
          w-80 max-w-[85vw] max-h-[min(480px,70vh)]
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
    </Show>
  );
};

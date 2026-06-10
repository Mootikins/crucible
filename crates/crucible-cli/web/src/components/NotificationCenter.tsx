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
  info: { icon: 'ℹ', color: 'text-blue-400', bg: 'bg-blue-500/10' },
  success: { icon: '✓', color: 'text-emerald-400', bg: 'bg-emerald-500/10' },
  warning: { icon: '⚠', color: 'text-amber-400', bg: 'bg-amber-500/10' },
  error: { icon: '✕', color: 'text-red-400', bg: 'bg-red-500/10' },
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
      class={`flex items-start gap-2.5 px-3 py-2 rounded-md transition-colors ${cfg().bg} hover:bg-white/5`}
      classList={{ 'opacity-50': props.notification.dismissed }}
    >
      <span class={`text-sm flex-shrink-0 mt-0.5 ${cfg().color}`}>
        {cfg().icon}
      </span>
      <div class="flex-1 min-w-0">
        <p class="text-xs text-neutral-200 leading-snug break-words">
          {props.notification.message}
        </p>
        <span class="text-[10px] text-neutral-500 mt-0.5 block">
          {formatTime(props.notification.timestamp)}
        </span>
      </div>
      <Show when={!props.notification.dismissed}>
        <button
          type="button"
          onClick={() => notificationActions.dismiss(props.notification.id)}
          class="flex-shrink-0 text-neutral-600 hover:text-neutral-300 transition-colors p-0.5"
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

export const NotificationCenter: Component<{ open: boolean; onClose: () => void }> = (props) => {
  const [visible, setVisible] = createSignal(false);
  let backdropRef: HTMLDivElement | undefined;

  // Animate in/out
  createEffect(() => {
    if (props.open) {
      // Mark all as read when drawer opens
      notificationActions.markAllRead();
      requestAnimationFrame(() => setVisible(true));
    } else {
      setVisible(false);
    }
  });

  // Close on Escape
  createEffect(() => {
    if (!props.open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        props.onClose();
      }
    };
    document.addEventListener('keydown', onKey, true);
    onCleanup(() => document.removeEventListener('keydown', onKey, true));
  });

  const allNotifications = () => [...notificationStore.notifications];
  const grouped = () => groupNotifications(allNotifications());
  const hasNotifications = () => allNotifications().length > 0;

  const handleBackdropClick = (e: MouseEvent) => {
    if (e.target === backdropRef) {
      props.onClose();
    }
  };

  const handleClearAll = () => {
    notificationActions.clearAll();
  };

  return (
    <Show when={props.open}>
      {/* Backdrop */}
      <div
        ref={backdropRef}
        class="fixed inset-0 z-50 bg-black/30 backdrop-blur-[2px] transition-opacity duration-200"
        classList={{
          'opacity-100': visible(),
          'opacity-0': !visible(),
        }}
        onClick={handleBackdropClick}
      >
        {/* Drawer */}
        <div
          class={`
            absolute top-0 right-0 h-full w-80 max-w-[90vw]
            bg-zinc-900 border-l border-zinc-700/60
            shadow-2xl shadow-black/50
            flex flex-col
            transition-transform duration-300 ease-out
            ${visible() ? 'translate-x-0' : 'translate-x-full'}
          `}
        >
          {/* Header */}
          <div class="flex items-center justify-between px-4 py-3 border-b border-zinc-800">
            <div class="flex items-center gap-2">
              <span class="text-sm font-medium text-neutral-200">Notifications</span>
              <Show when={allNotifications().length > 0}>
                <span class="text-[10px] px-1.5 py-0.5 rounded-full bg-zinc-800 text-neutral-400 tabular-nums">
                  {allNotifications().length}
                </span>
              </Show>
            </div>
            <div class="flex items-center gap-1">
              <Show when={hasNotifications()}>
                <button
                  type="button"
                  onClick={handleClearAll}
                  class="text-[11px] px-2 py-1 rounded text-neutral-400 hover:text-neutral-200 hover:bg-zinc-800 transition-colors"
                >
                  Clear All
                </button>
              </Show>
              <button
                type="button"
                onClick={() => props.onClose()}
                class="p-1 text-neutral-500 hover:text-neutral-200 hover:bg-zinc-800 rounded transition-colors"
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
                <div class="flex flex-col items-center justify-center h-full text-neutral-500">
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
                        <span class="text-[10px] font-semibold uppercase tracking-widest text-neutral-500">
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
      </div>
    </Show>
  );
};

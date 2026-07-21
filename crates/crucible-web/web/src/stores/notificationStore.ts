import { createSignal } from 'solid-js';
import { createStore, produce } from 'solid-js/store';
import type { Notification, NotificationType } from '@/lib/types';
import { statusBarActions } from '@/stores/statusBarStore';

// ── Global notification state ────────────────────────────────────────────
// Module-level store following statusBarStore pattern.
// Toasts are ephemeral UI; the full notification list is available
// for Task 18's Notification Center to consume.

const [notifications, setNotifications] = createStore<Notification[]>([]);
const [notificationCount, setNotificationCount] = createSignal(0);

// Track active auto-dismiss timers so we can cancel on manual dismiss
const dismissTimers = new Map<string, ReturnType<typeof setTimeout>>();

const AUTO_DISMISS_MS = 5000;

function recalcCount() {
  // The badge counts UNREAD notifications (still listed, not yet seen).
  const count = notifications.filter((n) => !n.dismissed && !n.read).length;
  setNotificationCount(count);
  statusBarActions.setNotificationCount(count);
}

let nextId = 0;

function addNotification(
  type: NotificationType,
  message: string,
  action?: Notification['action'],
): string {
  const id = `notif-${Date.now()}-${nextId++}`;
  const notification: Notification = {
    id,
    type,
    message,
    timestamp: Date.now(),
    dismissed: false,
    read: false,
    action,
  };

  setNotifications(produce((list) => list.push(notification)));
  recalcCount();

  // Auto-dismiss info and success after 5s — but never actionable
  // notifications: the whole point is that the user gets to act on them.
  if (!action && (type === 'info' || type === 'success')) {
    const timer = setTimeout(() => {
      dismiss(id);
      dismissTimers.delete(id);
    }, AUTO_DISMISS_MS);
    dismissTimers.set(id, timer);
  }

  return id;
}

function dismiss(id: string) {
  // Cancel any pending auto-dismiss timer
  const timer = dismissTimers.get(id);
  if (timer) {
    clearTimeout(timer);
    dismissTimers.delete(id);
  }

  setNotifications(
    (n) => n.id === id,
    'dismissed',
    true,
  );
  recalcCount();
}

function clearAll() {
  // Cancel all pending timers
  for (const timer of dismissTimers.values()) {
    clearTimeout(timer);
  }
  dismissTimers.clear();

  setNotifications(
    produce((list) => {
      for (const n of list) {
        n.dismissed = true;
      }
    }),
  );
  recalcCount();
}

function markAllRead() {
  // Opening the Notification Center marks everything READ (zeroes the badge)
  // but keeps entries visible — it must NOT dismiss them like clearAll does.
  // Auto-dismiss timers are left running so info/success toasts still fade.
  setNotifications(
    produce((list) => {
      for (const n of list) {
        n.read = true;
      }
    }),
  );
  recalcCount();
}

export const notificationStore = {
  notifications,
  notificationCount,
} as const;

export const notificationActions = {
  addNotification,
  dismiss,
  clearAll,
  markAllRead,
} as const;

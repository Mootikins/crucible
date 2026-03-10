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
  const count = notifications.filter((n) => !n.dismissed).length;
  setNotificationCount(count);
  statusBarActions.setNotificationCount(count);
}

let nextId = 0;

function addNotification(type: NotificationType, message: string): string {
  const id = `notif-${Date.now()}-${nextId++}`;
  const notification: Notification = {
    id,
    type,
    message,
    timestamp: Date.now(),
    dismissed: false,
  };

  setNotifications(produce((list) => list.push(notification)));
  recalcCount();

  // Auto-dismiss info and success after 5s
  if (type === 'info' || type === 'success') {
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

export const notificationStore = {
  notifications,
  notificationCount,
} as const;

export const notificationActions = {
  addNotification,
  dismiss,
  clearAll,
} as const;

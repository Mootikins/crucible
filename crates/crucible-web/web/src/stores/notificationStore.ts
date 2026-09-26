import { createSignal } from 'solid-js';
import { createStore, produce } from 'solid-js/store';
import type { Notification, NotificationOrigin, NotificationType } from '@/lib/types';
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
// Warnings/errors get a longer dwell but still leave the screen — they pile
// up bottom-right forever otherwise. The Notification Center retains them.
const AUTO_DISMISS_ALERT_MS = 12000;

function recalcCount() {
  // The badge counts UNREAD notifications (still listed, not yet seen).
  const count = notifications.filter((n) => !n.dismissed && !n.read).length;
  setNotificationCount(count);
  statusBarActions.setNotificationCount(count);
}

let nextId = 0;

// One sentence, once. A failed session start fans out into several calls
// (the folder listing, the model list, the mode list) that can all refuse
// with the same reason, and a retry loop repeats one; the user needs the
// sentence, not a column of it. Same type and same text inside this window
// return the notification already on screen.
const DEDUPE_WINDOW_MS = 5000;

function addNotification(
  type: NotificationType,
  message: string,
  action?: Notification['action'],
  origin?: NotificationOrigin,
): string {
  const now = Date.now();
  // An owned entry is the same entry for as long as it is open, whatever its
  // text, and two owned entries with one text are still two.
  if (origin) {
    const same = notifications.find((n) => !n.dismissed && n.origin?.key === origin.key);
    if (same) return same.id;
  } else if (!action) {
    const repeat = notifications.find(
      (n) => !n.dismissed && n.type === type && n.message === message && now - n.timestamp < DEDUPE_WINDOW_MS,
    );
    if (repeat) return repeat.id;
  }
  const id = `notif-${now}-${nextId++}`;
  const notification: Notification = {
    id,
    type,
    message,
    timestamp: now,
    dismissed: false,
    read: false,
    action,
    origin,
  };

  setNotifications(produce((list) => list.push(notification)));
  recalcCount();

  // Every toast auto-dismisses (info/success quickly, warning/error after a
  // longer dwell) EXCEPT actionable notifications: the whole point of those
  // is that the user gets to act on them.
  if (!action) {
    const dwell = type === 'info' || type === 'success' ? AUTO_DISMISS_MS : AUTO_DISMISS_ALERT_MS;
    // A timeout takes the toast off the screen. It is not the user's
    // close, so the owner of the entry does not hear about it.
    const timer = setTimeout(() => {
      hide(id);
      dismissTimers.delete(id);
    }, dwell);
    dismissTimers.set(id, timer);
  }

  return id;
}

/** The user closed the entry. Its owner hears about it once. */
function dismiss(id: string) {
  const entry = notifications.find((n) => n.id === id);
  if (entry && !entry.dismissed) entry.origin?.close();
  hide(id);
}

/** The owner closed the entry. Take it off the screen; tell no one. */
function dropOrigin(key: string) {
  for (const n of notifications) {
    if (!n.dismissed && n.origin?.key === key) hide(n.id);
  }
}

/** Take the entry off the screen, without a word to its owner. */
function hide(id: string) {
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

  // The user closed every open entry, so every owner hears about its own.
  for (const n of notifications) {
    if (!n.dismissed) n.origin?.close();
  }
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
  dropOrigin,
  clearAll,
  markAllRead,
} as const;

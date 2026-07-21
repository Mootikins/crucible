import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { notificationStore, notificationActions } from '@/stores/notificationStore';

describe('actionable notifications', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    notificationActions.clearAll();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('info notifications with an action never auto-dismiss', () => {
    const id = notificationActions.addNotification('info', 'update available', {
      label: 'Reload & update',
      run: () => {},
    });
    // Plain info auto-dismisses at 5s; actionable must survive well past it.
    vi.advanceTimersByTime(60_000);
    const notif = notificationStore.notifications.find((n) => n.id === id);
    expect(notif?.dismissed).toBe(false);
    expect(notif?.action?.label).toBe('Reload & update');
  });

  it('plain info notifications still auto-dismiss after 5s', () => {
    const id = notificationActions.addNotification('info', 'saved');
    vi.advanceTimersByTime(5_100);
    expect(notificationStore.notifications.find((n) => n.id === id)?.dismissed).toBe(true);
  });
});

describe('notificationStore mark-read vs dismiss', () => {
  beforeEach(() => {
    // Dismiss any leftovers from other tests sharing the module-level store.
    notificationActions.clearAll();
  });

  it('markAllRead zeroes the badge but keeps entries visible', () => {
    notificationActions.addNotification('warning', 'a');
    notificationActions.addNotification('warning', 'b');
    expect(notificationStore.notificationCount()).toBe(2);

    notificationActions.markAllRead();

    // Badge cleared…
    expect(notificationStore.notificationCount()).toBe(0);
    // …but the notifications are still listed (not dismissed).
    const visible = notificationStore.notifications.filter((n) => !n.dismissed);
    expect(visible).toHaveLength(2);
    expect(visible.every((n) => n.read)).toBe(true);
  });

  it('clearAll dismisses entries (removes them from the visible list)', () => {
    notificationActions.addNotification('warning', 'a');
    notificationActions.clearAll();

    expect(notificationStore.notificationCount()).toBe(0);
    expect(notificationStore.notifications.filter((n) => !n.dismissed)).toHaveLength(0);
  });
});

import { describe, it, expect, beforeEach } from 'vitest';
import { notificationStore, notificationActions } from '@/stores/notificationStore';

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

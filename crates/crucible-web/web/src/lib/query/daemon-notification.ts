import type { ChatEvent } from '@/lib/types';
import { rpc } from '@/lib/api-client';
import { notificationActions } from '@/stores/notificationStore';

/** A daemon notification as the wire carries it. */
export type DaemonNotification =
  { id?: string; kind?: unknown; message?: string } | null | undefined;

/**
 * The sessions that show each open daemon notification, by notification id.
 * A global notification reaches every session that the browser shows, and
 * the browser shows it once, so a close must reach each of those sessions.
 *
 * An entry stays while the daemon holds the notification open. The timer of
 * a toast hides the toast and leaves the entry, so a later snapshot of the
 * same notification (a reconnect or a gap) does not show the toast again.
 */
const shownIn = new Map<string, Set<string>>();

/**
 * Show a daemon notification of the session `sessionId` as a toast. A
 * warning stays a warning. When the user closes it, the daemon closes it
 * for each session that showed it.
 */
export function showDaemonNotification(n: DaemonNotification, sessionId: string): void {
  if (!n?.message) return;
  const type = n.kind === 'warning' ? 'warning' : 'info';
  const id = n.id;
  if (!id) {
    notificationActions.addNotification(type, n.message);
    return;
  }
  const sessions = shownIn.get(id);
  if (sessions) {
    // The browser shows this notification already, or showed it until its
    // timer hid it. The session joins the set that a close must reach.
    sessions.add(sessionId);
    return;
  }
  shownIn.set(id, new Set([sessionId]));
  notificationActions.addNotification(type, n.message, undefined, {
    key: id,
    close: () => closeInDaemon(id),
  });
}

/**
 * The daemon closed the notification `id` for the session `sessionId`: a
 * client of that session closed it. The toast leaves when no session that
 * showed it still has it.
 */
export function dropDaemonNotification(id: unknown, sessionId: string): void {
  if (typeof id !== 'string') return;
  const sessions = shownIn.get(id);
  sessions?.delete(sessionId);
  if (sessions && sessions.size > 0) return;
  shownIn.delete(id);
  notificationActions.dropOrigin(id);
}

function closeInDaemon(id: string): void {
  const sessions = shownIn.get(id) ?? new Set<string>();
  shownIn.delete(id);
  for (const sessionId of sessions) {
    rpc('session.dismiss_notification', { session_id: sessionId, notification_id: id }).catch(
      (e: unknown) =>
        notificationActions.addNotification(
          'warning',
          `The daemon could not close the notification: ${e instanceof Error ? e.message : String(e)}`,
        ),
    );
  }
}

/** Forget every notification. Tests share one module, so each starts empty. */
export function resetDaemonNotificationsForTests(): void {
  shownIn.clear();
}

/** One shared stream owns the snapshot and live overlay for its session. */
export function sessionNotifications(sessionId: string) {
  let generation = 0;
  let pending: Map<string, DaemonNotification> | undefined;

  function refresh(): void {
    const current = ++generation;
    const changes = new Map<string, DaemonNotification>();
    pending = changes;
    void rpc('session.list_notifications', { session_id: sessionId })
      .then((r) => r.notifications)
      .then((list) => {
        if (current !== generation) return;
        const snapshot = new Map<string, DaemonNotification>();
        for (const n of [...list].reverse() as DaemonNotification[]) {
          if (n?.id) snapshot.set(n.id, n);
        }
        for (const [id, n] of changes) {
          if (n) snapshot.set(id, n);
          else snapshot.delete(id);
        }
        // A reconnect can have missed a dismissal entirely.
        for (const [id, sessions] of shownIn) {
          if (sessions.has(sessionId) && !snapshot.has(id)) dropDaemonNotification(id, sessionId);
        }
        for (const n of snapshot.values()) showDaemonNotification(n, sessionId);
      })
      .catch((e: unknown) => {
        if (current !== generation) return;
        notificationActions.addNotification(
          'warning',
          `The notifications of the session are not available: ${e instanceof Error ? e.message : String(e)}`,
        );
      })
      .finally(() => {
        if (current === generation) pending = undefined;
      });
  }

  return {
    /**
     * Reads one event of the session stream. An open of the stream and a gap
     * read the snapshot again, because either one can hide a change.
     */
    event(event: ChatEvent): void {
      if ('type' in event) {
        if (event.type === 'connection' && event.status === 'connected') refresh();
        return;
      }
      if (event.event === 'stream_gap') {
        refresh();
      } else if (event.event === 'notification_added') {
        const n = event.data.notification;
        if (n?.id) pending?.set(n.id, n);
        showDaemonNotification(n, sessionId);
      } else if (event.event === 'notification_dismissed') {
        const id = event.data.notification_id;
        if (typeof id === 'string') pending?.set(id, null);
        dropDaemonNotification(id, sessionId);
      }
    },
    dispose(): void {
      ++generation;
      pending = undefined;
      for (const [id, sessions] of shownIn) {
        if (sessions.has(sessionId)) dropDaemonNotification(id, sessionId);
      }
    },
  };
}

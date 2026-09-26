import { dismissSessionNotification } from '@/lib/api';
import { notificationActions } from '@/stores/notificationStore';

/** A daemon notification as the wire carries it. */
export type DaemonNotification = { id?: string; kind?: unknown; message?: string } | null | undefined;

/**
 * The sessions that show each open daemon notification, by notification id.
 * A global notification reaches every session that the browser shows, and
 * the browser shows it once, so a close must reach each of those sessions.
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
  let sessions = shownIn.get(id);
  if (!sessions) {
    sessions = new Set();
    shownIn.set(id, sessions);
  }
  sessions.add(sessionId);
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
    dismissSessionNotification(sessionId, id).catch((e: unknown) =>
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

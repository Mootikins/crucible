import { notificationActions } from '@/stores/notificationStore';

/** A daemon notification as the wire carries it. */
export type DaemonNotification = { kind?: unknown; message?: string } | null | undefined;

/** Show a daemon notification as a toast. A warning stays a warning. */
export function showDaemonNotification(n: DaemonNotification): void {
  if (n?.message) notificationActions.addNotification(n.kind === 'warning' ? 'warning' : 'info', n.message);
}

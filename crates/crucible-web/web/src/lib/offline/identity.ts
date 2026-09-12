import { getConfig } from '@/lib/api';

/**
 * Which daemon this device is talking to.
 *
 * A key names a DAEMON, not a person, so a phone that reconnects to a
 * different one — another machine, a restored backup, a different instance on
 * the same address — would replay a user's queued edits into that daemon's
 * kiln. Everything stored offline is stamped with this, and the outbox refuses
 * to drain into an identity it did not come from.
 *
 * No daemon id crosses the wire, so the identity is the origin plus the
 * daemon's own config root — the closest thing to "which installation is
 * this" that `GET /api/config` offers.
 */
export async function daemonIdentity(): Promise<string> {
  const origin = typeof location === 'undefined' ? '' : location.origin;
  try {
    const config = await getConfig();
    return `${origin}|${config?.config_root ?? config?.kiln_path ?? ''}`;
  } catch {
    // Offline: the identity is unknown, and an unknown identity never matches
    // a stored one, so nothing drains until the daemon answers again.
    return '';
  }
}

/** Whether a stored entry belongs to the daemon now answering. */
export function sameDaemon(stored: string, current: string): boolean {
  return stored !== '' && current !== '' && stored === current;
}

import { getConfig } from '@/lib/api';
import type { OfflineStore } from '@/lib/offline/store';

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

/** Where the last-seen identity is remembered, in the store's `meta` table. */
export const IDENTITY_KEY = 'daemon-identity';

/**
 * The identity now, remembering it for when the daemon cannot be asked.
 *
 * Pass the store. Without it there is no memory, so an offline call answers
 * the empty string and whatever it stamps can never drain.
 */
export async function daemonIdentity(store?: OfflineStore): Promise<string> {
  const origin = typeof location === 'undefined' ? '' : location.origin;
  try {
    const config = await getConfig();
    const identity = `${origin}|${config?.config_root ?? config?.kiln_path ?? ''}`;
    // Remembered while the daemon can still be asked, because the one moment
    // this is needed — stamping a write queued OFFLINE — is the one moment it
    // cannot be fetched.
    if (store) await store.put('meta', IDENTITY_KEY, identity).catch(() => undefined);
    return identity;
  } catch {
    // Offline. The daemon last seen IS the one this device has been editing
    // against, so it is the right stamp for a write queued now. The guard
    // still holds: a drain computes the identity while ONLINE, so reconnecting
    // to a different daemon produces a different string and refuses.
    //
    // Without this the stamp was always empty, `sameDaemon` refused an empty
    // string, and every write queued offline was skipped by every future
    // drain — stranded on the device with no way out.
    if (store) {
      try {
        return (await store.get<string>('meta', IDENTITY_KEY)) ?? '';
      } catch {
        /* no store on this browser */
      }
    }
    return '';
  }
}

/** Whether a stored entry belongs to the daemon now answering. */
export function sameDaemon(stored: string, current: string): boolean {
  return stored !== '' && current !== '' && stored === current;
}

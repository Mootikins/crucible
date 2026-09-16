/**
 * The route of the surface stream: one event, one cache write.
 *
 * A surface event carries an identity and a version, never the rows. That is
 * what keeps an unbounded panel off an event channel, and it is also why the
 * route asks rather than patches: the frame does not hold what the list needs.
 *
 * A withdrawal is the one exception the daemon marks for us. The surface is
 * gone, so there is nothing to fetch and a refetch would spend a round trip to
 * be told what the event already said. The route drops the row instead.
 *
 * The route runs once per event inside the shared root of `lib/query/sse.ts`,
 * so two panels on one browser invalidate one key once.
 */
import type { Surface, SurfaceChangedEvent } from '@/lib/api';
import { keys } from '../keys';
import { setSurfaceEventRoute, type SseRouteContext } from '../sse';

/** Turns one surface event into the cache write it owes every panel. */
export function routeSurfaceEvent(event: SurfaceChangedEvent, { client }: SseRouteContext): void {
  if (event.withdrawn) {
    client.setQueryData<Surface[]>(keys.surfaces(), (held) => {
      // Nothing read the list, so there is nothing to keep current. Minting one
      // from a withdrawal would answer the next reader an empty roster and call
      // it whole.
      if (!held) return held;
      // Both halves of the identity, because two plugins may declare one name
      // and the event withdraws one of them. A filter on the name alone would
      // take the other plugin's panel away with it.
      return held.filter((s) => s.name !== event.name || s.plugin !== event.plugin);
    });
    return;
  }

  void client.invalidateQueries({ queryKey: keys.surfaces() });
}

/**
 * Names this module the route of the surface stream.
 *
 * The app calls it once, at start (`src/index.tsx`), because a route installed
 * by the first panel to import a module would leave the cache stale for
 * whichever panel opened before that import ran. A test calls it too, after
 * `resetSseForTests` forgets it.
 */
export function installSurfaceEventRoute(): void {
  setSurfaceEventRoute(routeSurfaceEvent);
}

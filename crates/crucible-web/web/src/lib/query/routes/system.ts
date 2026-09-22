/**
 * The route of the system stream: one event, the cache writes it owes.
 *
 * A proposal belongs to no user session, so the daemon sends
 * `proposal_changed` on its system session, and `/api/events/system` forwards
 * it. The frame names the proposal and carries no value. Thus the route
 * invalidates the proposal, its diffset and the Inbox list, and each reader
 * asks the daemon again.
 *
 * The list is in the set because a new proposal, a decision and a supersede
 * each change which proposals the Inbox shows.
 *
 * The plugin blocks read `publication_changed` through `/api/plugins/events`
 * and `routes/plugins.ts`. This route does not repeat that write.
 */
import { diffsetKey } from '@/lib/diffset';
import type { SystemEvent } from '@/lib/api';
import { keys } from '../keys';
import { setSystemEventRoute, type SseRouteContext } from '../sse';

/** Turns one system event into its cache writes. */
function routeSystemEvent(event: SystemEvent, { client }: SseRouteContext): void {
  switch (event.event) {
    case 'proposal_changed':
      if (!event.id) return;
      void client.invalidateQueries({ queryKey: keys.proposal(event.id) });
      void client.invalidateQueries({ queryKey: keys.proposals() });
      void client.invalidateQueries({
        queryKey: keys.diffset(diffsetKey({ kind: 'proposal', id: event.id })),
      });
      return;
    case 'publication_changed':
      return;
  }
}

/**
 * Names this module the route of the system stream.
 *
 * The app calls it once, at start (`src/index.tsx`). A test calls it too,
 * after `resetSseForTests` forgets it.
 */
export function installSystemEventRoute(): void {
  setSystemEventRoute(routeSystemEvent);
}

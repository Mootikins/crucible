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
 * Plugin blocks share this stream. Publications invalidate only the named
 * plugin and key, leaving other blocks alone.
 */
import { invalidateProposal } from '../proposal-cache';
import type { SystemEvent } from '@/lib/api';
import { keys, systemReconcileTargets } from '../keys';
import { refreshQueries } from '../recovery';
import { setEventRoute, type SseRouteContext } from '../sse';

/** Turns one system event into its cache writes. */
function routeSystemEvent(event: SystemEvent, { client }: SseRouteContext): void {
  switch (event.event) {
    case 'proposal_changed':
      if (!event.id) return;
      void invalidateProposal(client, event.id);
      return;
    case 'publication_changed':
      if (!event.plugin || !event.key) return;
      void client.invalidateQueries({ queryKey: keys.pluginPublications(event.plugin, event.key) });
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
  setEventRoute('system', routeSystemEvent, ({ client }) => {
    // A first open closes the snapshot/subscription window; a later open or
    // gap recovers missed changes even if no further event follows.
    void refreshQueries(client, systemReconcileTargets());
  });
}

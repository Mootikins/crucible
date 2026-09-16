/**
 * The route of the plugin stream: one event, one cache write.
 *
 * `cru.plugin.publish` fires `publication_changed` on the daemon's system
 * channel and `/api/plugins/events` forwards it. The frame names the plugin and
 * the key and carries no value, so the route asks for that one publication
 * again rather than patching it. That push is what makes a plugin block
 * different from a poll: without it a board would re-read on a timer and still
 * show a stale value between ticks.
 *
 * The key names the plugin AND the key, because a document may hold four blocks
 * of one plugin. An invalidation of the plugin alone would throw away three
 * values the event said nothing about.
 *
 * The route runs once per event inside the shared root of `lib/query/sse.ts`,
 * so four blocks on one key invalidate it once.
 */
import { keys } from '../keys';
import { setPluginEventRoute, type PluginPublicationEvent, type SseRouteContext } from '../sse';

/** Turns one publication event into the cache write it owes every block. */
function routePluginEvent(
  event: PluginPublicationEvent,
  { client }: SseRouteContext,
): void {
  // Half an identity names the whole family of a plugin, or every plugin's copy
  // of one key. Either would refetch blocks the daemon said nothing about.
  if (!event.plugin || !event.key) return;

  void client.invalidateQueries({ queryKey: keys.pluginPublications(event.plugin, event.key) });
}

/**
 * Names this module the route of the plugin stream.
 *
 * The app calls it once, at start (`src/index.tsx`), because a route installed
 * by the first block to import a module would leave the cache stale for
 * whichever block drew before that import ran. A test calls it too, after
 * `resetSseForTests` forgets it.
 */
export function installPluginEventRoute(): void {
  setPluginEventRoute(routePluginEvent);
}

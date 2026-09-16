import { createResource, onCleanup, type Resource } from 'solid-js';
import { getPluginPublications } from '@/lib/api';
import { pluginEvents } from '@/lib/query/sse';

/**
 * One plugin's published value for `key`, kept live.
 *
 * Read once, then re-read whenever the daemon says that key changed. The push
 * is what makes this different from polling: `cru.plugin.publish` fires a
 * `publication_changed` event on the daemon's system channel, and
 * `/api/plugins/events` forwards it. Without that a board would re-fetch on a
 * timer and still show a stale value between ticks.
 *
 * The source is shared through `pluginEvents()`, the root of
 * `lib/query/sse.ts`. This module used to hold its own `EventSource` and its
 * own refcount, which gave a document with four blocks one stream and the rest
 * of the app a second one for the same URL. The root counts subscribers the
 * same way, and now every reader of the stream is inside that count.
 */
export function usePublication<T>(plugin: string, key: string): Resource<T | undefined> {
  const [value, { refetch }] = createResource<T | undefined>(async () => {
    // Narrowed to this key: the route filters daemon-side, so a document with
    // four blocks in it fetches four small answers rather than four copies of
    // every plugin's data.
    // Declared as `plugin`, not as the app: this is a block drawing one
    // plugin's data, and the route narrows a plugin caller to its own rows.
    //
    // `plugin` reaches here from `BlockProps.plugin`, which is the first line
    // of the ```plugin fence — so a NOTE AUTHOR picked this string. The
    // identity is caller-supplied one layer above the header, and no header
    // fixes that; only isolating blocks does. See `routes/plugin_caller.rs`.
    const all = await getPluginPublications(key, plugin);
    return all[key]?.[plugin] as T | undefined;
  });

  const stop = pluginEvents().subscribe((changedPlugin, changedKey) => {
    if (changedPlugin === plugin && changedKey === key) void refetch();
  });
  onCleanup(stop);

  return value;
}

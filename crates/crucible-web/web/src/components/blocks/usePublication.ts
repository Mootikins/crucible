import { createMemo, onCleanup, type Accessor } from 'solid-js';
import { usePluginPublications } from '@/lib/query/plugins';
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
 * The block no longer re-reads for itself. The value is a cache entry keyed by
 * the plugin and the key, and the stream's route invalidates exactly that pair
 * (`lib/query/routes/plugins.ts`), so four blocks of one plugin get four small
 * answers and an event about one of them refreshes one of them. A second block
 * on the same pair joins the first one's entry instead of asking again.
 */
export function usePublication<T>(plugin: string, key: string): Accessor<T | undefined> {
  // Narrowed to this key AND to this plugin: the route filters daemon-side, so
  // a document with four blocks in it fetches four small answers rather than
  // four copies of every plugin's data.
  //
  // Declared as `plugin`, not as the app: this is a block drawing one plugin's
  // data, and the route narrows a plugin caller to its own rows.
  //
  // `plugin` reaches here from `BlockProps.plugin`, which is the first line of
  // the ```plugin fence — so a NOTE AUTHOR picked this string. The identity is
  // caller-supplied one layer above the header, and no header fixes that; only
  // isolating blocks does. See `routes/plugin_caller.rs`.
  const published = usePluginPublications(plugin, key);

  // The block holds the stream open while it is on screen, and does nothing
  // with the frame: the cache write belongs to the route, which runs inside the
  // shared root. The subscription is still required — the root counts its
  // subscribers, and with none it closes the `EventSource` and no block hears
  // anything.
  const stop = pluginEvents().subscribe(() => {});
  onCleanup(stop);

  return createMemo(() => published.data?.[key]?.[plugin] as T | undefined);
}

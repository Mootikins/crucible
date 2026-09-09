import { createResource, onCleanup, type Resource } from 'solid-js';
import { getPluginPublications } from '@/lib/api';

/**
 * One plugin's published value for `key`, kept live.
 *
 * Read once, then re-read whenever the daemon says that key changed. The push
 * is what makes this different from polling: `cru.plugin.publish` fires a
 * `publication_changed` event on the daemon's system channel, and
 * `/api/plugins/events` forwards it. Without that a board would re-fetch on a
 * timer and still show a stale value between ticks.
 *
 * The EventSource is shared across every caller, because a document with four
 * plugin blocks in it should open one stream, not four.
 */
let shared: EventSource | undefined;
let refCount = 0;
const listeners = new Set<(plugin: string, key: string) => void>();

function subscribe(fn: (plugin: string, key: string) => void): () => void {
  listeners.add(fn);
  refCount += 1;
  if (!shared) {
    shared = new EventSource('/api/plugins/events');
    shared.addEventListener('publication_changed', (e) => {
      try {
        const { plugin, key } = JSON.parse((e as MessageEvent).data);
        for (const l of listeners) l(plugin, key);
      } catch {
        // A malformed frame is not worth tearing the stream down for; the next
        // one will arrive, and a stale block is better than a dead one.
      }
    });
  }
  return () => {
    listeners.delete(fn);
    refCount -= 1;
    if (refCount === 0) {
      shared?.close();
      shared = undefined;
    }
  };
}

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

  const stop = subscribe((changedPlugin, changedKey) => {
    if (changedPlugin === plugin && changedKey === key) void refetch();
  });
  onCleanup(stop);

  return value;
}

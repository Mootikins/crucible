import { approximateBytes, withOfflineLock, type OfflineStore } from './store';

/** A daemon's namespace, over the same IndexedDB. No switch deletes another daemon's data. */
export async function daemonStore(raw: OfflineStore, daemon: string, legacyOwner: string): Promise<OfflineStore> {
  if (!daemon) throw new Error('No daemon identity is available for offline storage');
  const prefix = JSON.stringify(daemon) + '\0';
  const key = (path: string) => prefix + path;
  const scoped: OfflineStore = {
    get: (table, path) => raw.get(table, key(path)),
    put: (table, path, value) => raw.put(table, key(path), value),
    update: (table, path, change) => raw.update(table, key(path), change),
    remove: (table, path) => raw.remove(table, key(path)),
    async list<T>(table: Parameters<OfflineStore['list']>[0], path?: string) {
      return (await raw.list<T>(table, key(path ?? ''))).map(row => ({ key: row.key.slice(prefix.length), value: row.value }));
    },
    async clear(table) {
      for (const row of await scoped.list(table)) await scoped.remove(table, row.key);
    },
    async size(table, path) {
      return (await scoped.list(table, path)).reduce((total, row) => total + approximateBytes(row.value), 0);
    },
  };
  // Copy old data once, without deleting it. A legacy mirror has no identity of its
  // own, so only the daemon remembered BEFORE reconnect may adopt it. Outbox entries
  // do carry an identity and can be recovered independently.
  const marker = 'namespace-import:' + daemon;
  await withOfflineLock(marker, async () => {
    if (!(await raw.get('meta', marker))) {
      for (const table of ['mirror', 'outbox', 'index', 'blobs'] as const) {
        for (const row of await raw.list<Record<string, unknown>>(table)) {
          if (row.key.includes('\0')) continue;
          const owner = table === 'outbox' ? row.value.daemon : legacyOwner;
          if (owner === daemon) await scoped.update(table, row.key, held => held ?? row.value);
        }
      }
      await raw.put('meta', marker, true);
    }
  });
  await scoped.put('meta', 'daemon-identity', daemon);
  return scoped;
}

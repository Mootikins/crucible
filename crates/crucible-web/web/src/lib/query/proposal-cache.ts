import type { QueryClient } from '@tanstack/solid-query';
import { proposalRefreshTargets } from './keys';
import { refreshQueries } from './recovery';

interface Batch {
  ids: Set<string>;
  done: Promise<void>;
}

const pending = new WeakMap<QueryClient, Batch>();

/**
 * Reconcile proposal decisions and events through the same cache rule.
 * Only requests in one microtask batch share a refresh. Once a refresh
 * starts, a newer change must refresh again or an older response could hide
 * it. `refreshQueries` waits for a first fetch in flight, because a plain
 * invalidation would join that fetch and lose the change.
 */
export function invalidateProposal(client: QueryClient, id: string): Promise<void> {
  let batch = pending.get(client);
  if (!batch) {
    const ids = new Set<string>();
    const done = Promise.resolve().then(async () => {
      pending.delete(client);
      await refreshQueries(client, proposalRefreshTargets([...ids]));
    });
    batch = { ids, done };
    pending.set(client, batch);
  }
  batch.ids.add(id);
  return batch.done;
}

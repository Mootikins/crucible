import type { QueryClient } from '@tanstack/solid-query';
import { diffsetKey } from '@/lib/diffset';
import { keys } from './keys';

interface Batch {
  ids: Set<string>;
  done: Promise<void>;
}

const pending = new WeakMap<QueryClient, Batch>();

/**
 * Reconcile proposal decisions and events through the same cache rule.
 * Only requests in one microtask batch share a fetch. Once fetching starts,
 * a newer change must invalidate again or an older response could hide it.
 */
export function invalidateProposal(client: QueryClient, id: string): Promise<void> {
  let batch = pending.get(client);
  if (!batch) {
    const ids = new Set<string>();
    const done = Promise.resolve().then(async () => {
      pending.delete(client);
      await Promise.all([
        client.invalidateQueries({ queryKey: keys.proposals() }),
        ...[...ids].flatMap(value => [
          client.invalidateQueries({ queryKey: keys.proposal(value) }),
          client.invalidateQueries({
            queryKey: keys.diffset(diffsetKey({ kind: 'proposal', id: value })),
          }),
        ]),
      ]);
    });
    batch = { ids, done };
    pending.set(client, batch);
  }
  batch.ids.add(id);
  return batch.done;
}

import type { QueryClient, QueryKey } from '@tanstack/solid-query';

/**
 * A first fetch with no cached data survives ordinary invalidation. Cancel it
 * before refetching so a snapshot started before a gap cannot settle as fresh.
 */
export async function reconcileQueries(client: QueryClient, queryKeys: readonly QueryKey[]): Promise<void> {
  await Promise.all(queryKeys.map(async queryKey => {
    await client.cancelQueries({ queryKey });
    await client.invalidateQueries({ queryKey });
  }));
}

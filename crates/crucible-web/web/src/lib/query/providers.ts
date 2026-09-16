import { useQuery, type QueryClient, type UseQueryResult } from '@tanstack/solid-query';
import { listProviders } from '@/lib/api';
import { readLocalCache, writeLocalCache } from '@/lib/local-cache';
import type { ProviderInfo } from '@/lib/types';
import { getQueryClient } from './client';
import { keys } from './keys';

/**
 * The LLM providers the daemon has configured, and what each one offers.
 *
 * Two readers asked for it separately: `SessionContext` probed on start, to
 * name the fallback model list, and the composer probed again on mount, to
 * name the default model of its chip. The daemon asks every configured
 * provider whether it answers, so the second probe cost the composer a second
 * round of that work.
 *
 * The stale-while-revalidate behaviour of `swrLocal('providers')` is kept,
 * under the same storage key: the last list seeds the cache, so the chip
 * paints a default model on a cold load and the probe corrects it. What is NOT
 * kept is `swrLocal`'s silence — a refused call looked exactly like a daemon
 * with no providers configured, and "no providers" is the sentence the shell
 * puts on screen. The refusal now reaches the caller as the query's `error`.
 */

/** The `swrLocal` key this list has always been stored under. */
const STORAGE_KEY = 'providers';

/** The list a previous run stored, or `undefined` when there is none. */
function storedProviders(): ProviderInfo[] | undefined {
  const stored = readLocalCache<ProviderInfo[]>(STORAGE_KEY);
  return Array.isArray(stored) ? stored : undefined;
}

/** The one probe, which also refreshes what the next cold load paints. */
async function fetchProviders(): Promise<ProviderInfo[]> {
  const providers = await listProviders();
  writeLocalCache(STORAGE_KEY, providers);
  return providers;
}

/** Clients that already carry the stored list. */
const seededClients = new WeakSet<QueryClient>();

/**
 * The cache every provider read goes through, with the stored list already in
 * it.
 *
 * The seed is written with `updatedAt: 0`, so the entry is on screen
 * immediately AND is stale, which means the first observer still probes. A
 * provider whose key was removed since the last run therefore stops being
 * offered as soon as the daemon answers.
 *
 * A test injects its own client, so the seed is tracked per client rather than
 * by a module flag — one test's seeding must not silence the next test's.
 */
function seededClient(): QueryClient {
  const client = getQueryClient();
  if (seededClients.has(client)) return client;
  seededClients.add(client);

  const stored = storedProviders();
  if (stored && client.getQueryData(keys.providers()) === undefined) {
    client.setQueryData(keys.providers(), stored, { updatedAt: 0 });
  }
  return client;
}

/** The provider list as a query, shared by the context and the composer. */
export function useProviders(): UseQueryResult<ProviderInfo[], Error> {
  return useQuery(
    () => ({ queryKey: keys.providers(), queryFn: fetchProviders }),
    () => seededClient(),
  );
}

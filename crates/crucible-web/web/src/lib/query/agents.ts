import { useQuery, type QueryClient, type UseQueryResult } from '@tanstack/solid-query';
import { listAgents } from '@/lib/api';
import { readLocalCache, writeLocalCache } from '@/lib/local-cache';
import type { AgentProfileEntry } from '@/lib/types';
import { getQueryClient } from './client';
import { keys } from './keys';

/**
 * The ACP agent profiles, with the availability the daemon probed.
 *
 * Two pickers ask for this roster — the desktop composer's agent chip and the
 * phone's new-session sheet — and they used to ask separately. The probe is
 * the reason that matters: the daemon looks for each profile's binary on PATH
 * and waits for it to answer, so this is one of the slowest catalogue reads in
 * the shell. Paying for it twice delayed whichever picker opened second.
 *
 * The stale-while-revalidate behaviour of `swrLocal('agents')` is kept, under
 * the same storage key: the last roster seeds the cache, so the chip menu
 * paints real agent names on a cold load and the probe corrects them. What is
 * NOT kept is `swrLocal`'s silence, which made a refused call look exactly
 * like a daemon with no agents installed. The refusal now reaches the caller
 * as the query's `error`.
 */

/** The `swrLocal` key this roster has always been stored under. */
const STORAGE_KEY = 'agents';

/** The roster a previous run stored, or `undefined` when there is none. */
function storedAgents(): AgentProfileEntry[] | undefined {
  const stored = readLocalCache<AgentProfileEntry[]>(STORAGE_KEY);
  return Array.isArray(stored) ? stored : undefined;
}

/** The one probe, which also refreshes what the next cold load paints. */
async function fetchAgents(): Promise<AgentProfileEntry[]> {
  const agents = await listAgents();
  writeLocalCache(STORAGE_KEY, agents);
  return agents;
}

/** Clients that already carry the stored roster. */
const seededClients = new WeakSet<QueryClient>();

/**
 * The cache every agent read goes through, with the stored roster already in
 * it.
 *
 * The seed is written with `updatedAt: 0`, which is the whole point: the entry
 * is on screen immediately AND is stale, so the first observer still probes.
 * Seeding with the current time would paint a roster and never correct it, and
 * an agent uninstalled since the last run would stay in the menu.
 *
 * A test injects its own client, so the seed is tracked per client rather than
 * by a module flag — one test's seeding must not silence the next test's.
 */
function seededClient(): QueryClient {
  const client = getQueryClient();
  if (seededClients.has(client)) return client;
  seededClients.add(client);

  const stored = storedAgents();
  if (stored && client.getQueryData(keys.agents()) === undefined) {
    client.setQueryData(keys.agents(), stored, { updatedAt: 0 });
  }
  return client;
}

/** The agent roster as a query, shared by both pickers. */
export function useAgents(): UseQueryResult<AgentProfileEntry[], Error> {
  return useQuery(
    () => ({ queryKey: keys.agents(), queryFn: fetchAgents }),
    () => seededClient(),
  );
}

import type { Accessor } from 'solid-js';
import {
  useMutation,
  useQuery,
  type QueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from '@tanstack/solid-query';
import { listAllModels, listModels, switchModel } from '@/lib/api';
import { readLocalCache, writeLocalCache } from '@/lib/local-cache';
import { getQueryClient } from './client';
import { keys } from './keys';
import { patchCachedSession } from './sessions';

/**
 * The models a session may run, the whole catalogue, and the write that moves
 * one session from one model to another.
 *
 * Two questions live here and they are not the same question. A SESSION's list
 * is what its agent accepts, which the daemon answers per session; the
 * CATALOGUE is every chat model across every configured provider, which no
 * session owns. They are two keys because a draft surface has no session to
 * ask about, and a bound pane must not offer a model its agent refuses.
 *
 * The session list used to be a bare call behind a hand-written generation
 * counter: two overlapping reads could resolve in reverse order, and the older
 * one won, so the picker went stale or empty after having been correct. The
 * key carries the session id, so a late answer lands on the key of the session
 * that asked for it and cannot overwrite another session's list. That is what
 * retires the counter, exactly as the list key retired the roster's.
 */

/** The `swrLocal` key the catalogue has always been stored under. */
const STORAGE_KEY = 'models';

/** The catalogue a previous run stored, or `undefined` when there is none. */
function storedModels(): string[] | undefined {
  const stored = readLocalCache<string[]>(STORAGE_KEY);
  return Array.isArray(stored) ? stored : undefined;
}

/** The one catalogue read, which also refreshes what the next cold load paints. */
async function fetchAllModels(): Promise<string[]> {
  const models = await listAllModels();
  writeLocalCache(STORAGE_KEY, models);
  return models;
}

/** Clients that already carry the stored catalogue. */
const seededClients = new WeakSet<QueryClient>();

/**
 * The cache the catalogue read goes through, with the stored list already in
 * it.
 *
 * The seed is written with `updatedAt: 0`, so the composer's model chip paints
 * real names on a cold load AND the entry is stale, which means the first
 * observer still fetches. A provider removed since the last run therefore
 * leaves the menu as soon as the daemon answers.
 *
 * A test injects its own client, so the seed is tracked per client rather than
 * by a module flag — one test's seeding must not silence the next test's.
 */
function seededClient(): QueryClient {
  const client = getQueryClient();
  if (seededClients.has(client)) return client;
  seededClients.add(client);

  const stored = storedModels();
  if (stored && client.getQueryData(keys.allModels()) === undefined) {
    client.setQueryData(keys.allModels(), stored, { updatedAt: 0 });
  }
  return client;
}

/**
 * The models one session may run, as a query, or none while the id is null.
 *
 * The id is an accessor because the shell re-points at another session while
 * the picker is mounted. A read that took the value would pin the list of
 * whichever session was current when the component was built.
 */
export function useSessionModels(
  id: Accessor<string | null>,
): UseQueryResult<string[], Error> {
  return useQuery(
    () => {
      const sessionId = id();
      return {
        queryKey: keys.sessionModels(sessionId ?? ''),
        queryFn: () => listModels(sessionId as string),
        enabled: sessionId !== null && sessionId !== '',
      };
    },
    () => getQueryClient(),
  );
}

/**
 * Every chat model the daemon knows, across providers, with no session.
 *
 * The desktop composer and the phone's new-session sheet both read it. The
 * sheet used to fetch it raw on every mount and drop the failure on the floor;
 * the composer kept it in `swrLocal('models')` and could not tell an empty
 * catalogue from a refused call. One key answers both, and the refusal reaches
 * the caller as the query's `error`.
 */
export function useAllModels(): UseQueryResult<string[], Error> {
  return useQuery(
    () => ({ queryKey: keys.allModels(), queryFn: fetchAllModels }),
    () => seededClient(),
  );
}

/**
 * Moves one session to another model.
 *
 * The cached session rows carry `agent_model`, which the rail and the status
 * bar draw, so the write patches them as soon as the daemon accepts the model
 * and without waiting for a refetch. It patches on success and not before,
 * because a model the daemon refuses must not be named anywhere. The session's
 * list is invalidated with it: an agent that accepts a model may offer a
 * different set of them afterwards.
 */
export function useSwitchModel(): UseMutationResult<
  void,
  Error,
  { id: string; modelId: string }
> {
  return useMutation(
    () => ({
      mutationFn: ({ id, modelId }: { id: string; modelId: string }) =>
        switchModel(id, modelId),
      onSuccess: (_result: void, { id, modelId }: { id: string; modelId: string }) => {
        patchCachedSession(id, { agent_model: modelId });
        // One call, not two. The session key is a PREFIX of the session's
        // models key, so this reaches both the row and the list. Naming them
        // separately would refetch the list twice for one switch.
        return getQueryClient().invalidateQueries({ queryKey: keys.session(id) });
      },
    }),
    () => getQueryClient(),
  );
}

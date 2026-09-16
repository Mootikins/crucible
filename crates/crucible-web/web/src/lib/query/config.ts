import { createSignal, type Accessor } from 'solid-js';
import {
  QueryObserver,
  useMutation,
  useQuery,
  type QueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from '@tanstack/solid-query';
import { getConfig, saveConfig, type Config, type ConfigSaveResult } from '@/lib/api';
import { readLocalCache, writeLocalCache } from '@/lib/local-cache';
import { getQueryClient } from './client';
import { keys } from './keys';

/**
 * The daemon's app config, fetched once for the whole shell.
 *
 * Ten surfaces asked for this document — two through `swrLocal('config')`, two
 * through their own `createResource`, six through a bare `getConfig()`. Each
 * one paid for its own request for the same three fields: the default kiln
 * path, the `remote_shell` opt-in and the config root. One query key answers
 * all of them.
 *
 * The stale-while-revalidate behaviour of `swrLocal` is kept, under the same
 * storage key: the last answer seeds the cache, so a cold load paints the real
 * kiln path instead of an empty chip, and a successful fetch writes it back.
 * What is NOT kept is `swrLocal`'s silence. A daemon that refuses the call now
 * reaches the caller as the query's `error` rather than as a config with no
 * fields in it.
 */

/** The `swrLocal` key this document has always been stored under. */
const STORAGE_KEY = 'config';

/** The config a previous run stored, or `undefined` when there is none. */
function storedConfig(): Config | undefined {
  const stored = readLocalCache<Config>(STORAGE_KEY);
  return stored !== null && typeof stored === 'object' ? stored : undefined;
}

/** The one fetch, which also refreshes what the next cold load paints. */
async function fetchConfig(): Promise<Config> {
  const config = await getConfig();
  writeLocalCache(STORAGE_KEY, config);
  return config;
}

/** Clients that already carry the stored config. */
let seededClients = new WeakSet<QueryClient>();

/**
 * The cache every config read goes through, with the stored answer already in
 * it.
 *
 * The seed is written with `updatedAt: 0`, which is the whole point: the entry
 * is on screen immediately AND is stale, so the first observer still fetches.
 * Seeding with the current time would paint a config and never correct it.
 *
 * A test injects its own client, so the seed is tracked per client rather than
 * by a module flag — one test's seeding must not silence the next test's.
 */
function seededClient(): QueryClient {
  const client = getQueryClient();
  if (seededClients.has(client)) return client;
  seededClients.add(client);

  const stored = storedConfig();
  if (stored && client.getQueryData(keys.config()) === undefined) {
    client.setQueryData(keys.config(), stored, { updatedAt: 0 });
  }
  return client;
}

/** The options the hook, the snapshot and the imperative read all share. */
function configQueryOptions() {
  return { queryKey: keys.config(), queryFn: fetchConfig };
}

/**
 * The app config as a query.
 *
 * `data` is `undefined` only while the very first fetch of a browser with no
 * stored config is in flight; every later mount reads the cache synchronously.
 */
export function useConfig(): UseQueryResult<Config, Error> {
  return useQuery(() => configQueryOptions(), () => seededClient());
}

/**
 * Saves one config leaf as the user's durable preference.
 *
 * A refusal is part of the answer, not an error: the caller reads `refused` to
 * name the file that holds the leaf. The mutation invalidates `keys.config()`
 * whatever the daemon decided, because a save changes the provenance rows even
 * when it changes no value, and the pane draws those.
 */
export function useSaveConfig(): UseMutationResult<
  ConfigSaveResult,
  Error,
  Record<string, unknown>
> {
  return useMutation(
    () => ({
      mutationFn: (values: Record<string, unknown>) => saveConfig(values),
      onSuccess: () =>
        void seededClient().invalidateQueries({ queryKey: keys.config() }),
    }),
    () => seededClient(),
  );
}

/**
 * The config as a promise, for the callers that must wait rather than react.
 *
 * `SkillsPanel` resolves a kiln root inside a `createResource` fetcher,
 * `GraphPanel` inside an async `onMount`, and `daemonIdentity` from a plain
 * function with no owner at all. None of them can render a pending state, and
 * all three used to start their own request. They join the hook's fetch
 * instead.
 */
export function fetchConfigOnce(): Promise<Config> {
  return seededClient().ensureQueryData(configQueryOptions());
}

// ---- the non-hook reactive read -------------------------------------------
//
// `terminalAllowed` and `terminalDenied` are read from render paths, from a
// plain module function with no owner. They need the answer reactively and
// cannot mount an observer, so the cache is mirrored into one signal.

const [snapshot, setSnapshot] = createSignal<Config | undefined>(undefined);

let boundClient: QueryClient | null = null;
let unbind: (() => void) | null = null;

/** Mirrors the current client's config entry into the signal, and fetches. */
function bindSnapshot(): void {
  const client = seededClient();
  if (boundClient === client) return;

  unbind?.();
  boundClient = client;
  // An observer, not a cache subscription plus a prefetch: an observer is what
  // marks the entry as in use, and an entry nothing observes is collected the
  // moment its fetch lands.
  //
  // Its errors stay here, as `undefined`. Nothing on this path can render a
  // refusal, and the one caller treats "no answer" and "refused" alike — both
  // mean it did not learn whether the terminal is allowed.
  const observer = new QueryObserver<Config>(client, configQueryOptions());
  setSnapshot(() => observer.getCurrentResult().data);
  unbind = observer.subscribe((result) => setSnapshot(() => result.data));
}

/**
 * The config as a plain reactive accessor, outside any component.
 *
 * Reading it starts the fetch. A failed fetch answers `undefined` and is NOT
 * retried on the next read: the query holds the error, and nothing here polls.
 */
export const configSnapshot: Accessor<Config | undefined> = () => {
  bindSnapshot();
  return snapshot();
};

/**
 * Patches the cached config, for a pane that paints a save before the daemon
 * answers it. The updater is skipped when nothing is cached yet: there is no
 * guess to make about a document that has not arrived.
 */
export function patchCachedConfig(update: (held: Config) => Config): void {
  seededClient().setQueryData(keys.config(), (held: Config | undefined) =>
    held === undefined ? held : update(held),
  );
}

/**
 * Asks again from the start, for the one event that turns a refusal into an
 * answer: a sign-in. It bypasses `staleTime` because new credentials are new
 * information, not a stale copy of the old one.
 *
 * It marks the entry stale rather than cancelling a request in flight, and the
 * app never needs more: the 401 is what raises the token prompt, so a sign-in
 * always follows a refusal that has already landed.
 */
export function refetchConfig(): void {
  void seededClient().invalidateQueries({ queryKey: keys.config() });
}

/** Drops the snapshot's binding so one test's client cannot answer the next. */
export function resetConfigForTests(): void {
  unbind?.();
  unbind = null;
  boundClient = null;
  seededClients = new WeakSet();
  setSnapshot(undefined);
}

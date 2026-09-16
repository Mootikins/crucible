import { createSignal, type Accessor } from 'solid-js';
import { QueryObserver, useQuery, type QueryClient, type UseQueryResult } from '@tanstack/solid-query';
import { listKilns } from '@/lib/api';
import { readLocalCache, writeLocalCache } from '@/lib/local-cache';
import { kilnPathForName } from '@/lib/kiln-registry';
import type { KilnListEntry } from '@/lib/types';
import { getQueryClient } from './client';
import { keys } from './keys';

/**
 * The kiln roster, fetched once for the whole shell.
 *
 * Twelve surfaces used to ask for this list — six through `swrLocal('kilns')`,
 * two through their own `createResource`, four through a bare `listKilns()` in
 * `onMount`. Each one paid for its own request, and a panel that mounted late
 * showed an empty roster until its own fetch answered. One query key answers
 * all of them, and a second caller joins the request the first one started.
 *
 * The stale-while-revalidate behaviour of `swrLocal` is kept, under the same
 * storage key: the last list seeds the cache, so a cold load paints real kiln
 * names instead of "Loading…", and a successful fetch writes the list back.
 * What is NOT kept is `swrLocal`'s silence. `swrLocal` swallowed every fetch
 * failure, so a daemon that refused the call looked exactly like a daemon with
 * no kilns. The failure now reaches the caller as the query's `error`.
 */

/** The `swrLocal` key this list has always been stored under. */
const STORAGE_KEY = 'kilns';

/** The list a previous run stored, or `undefined` when there is none. */
function storedKilns(): KilnListEntry[] | undefined {
  const stored = readLocalCache<KilnListEntry[]>(STORAGE_KEY);
  return Array.isArray(stored) ? stored : undefined;
}

/** The one fetch, which also refreshes what the next cold load paints. */
async function fetchKilns(): Promise<KilnListEntry[]> {
  const kilns = await listKilns();
  writeLocalCache(STORAGE_KEY, kilns);
  return kilns;
}

/** Clients that already carry the stored list. */
let seededClients = new WeakSet<QueryClient>();

/**
 * The cache every kiln read goes through, with the stored list already in it.
 *
 * The seed is written with `updatedAt: 0`, which is the whole point: the entry
 * is on screen immediately AND is stale, so the first observer still fetches.
 * Seeding with the current time would paint a list and never correct it.
 *
 * A test injects its own client, so the seed is tracked per client rather than
 * by a module flag — one test's seeding must not silence the next test's.
 */
function seededClient(): QueryClient {
  const client = getQueryClient();
  if (seededClients.has(client)) return client;
  seededClients.add(client);

  const stored = storedKilns();
  if (stored && client.getQueryData(keys.kilns()) === undefined) {
    client.setQueryData(keys.kilns(), stored, { updatedAt: 0 });
  }
  return client;
}

/** The options the hook, the snapshot and the imperative read all share. */
function kilnsQueryOptions() {
  return { queryKey: keys.kilns(), queryFn: fetchKilns };
}

/**
 * The kiln roster as a query.
 *
 * `data` is `undefined` only while the very first fetch of a browser with no
 * stored list is in flight; every later mount reads the cache synchronously.
 */
export function useKilns(): UseQueryResult<KilnListEntry[], Error> {
  return useQuery(() => kilnsQueryOptions(), () => seededClient());
}

/**
 * The roster as a promise, for the callers that must wait rather than react.
 *
 * `EditorContext` resolves a path's owning kiln inside an async open, and
 * `FileViewerPanel` follows a wikilink from a click. Neither can render a
 * pending state, and both used to keep a private one-shot cache. They join the
 * hook's fetch instead of starting a second one.
 */
export function fetchKilnsOnce(): Promise<KilnListEntry[]> {
  return seededClient().ensureQueryData(kilnsQueryOptions());
}

// ---- the non-hook reactive read -------------------------------------------
//
// `kilnPathOf` runs inside a per-message component's memo, where a hook cannot
// be called: `Message`, `AssistantTurn` and `ChatInput` resolve a session's
// kiln name to a directory on every render, and `note-actions` does it from a
// plain function with no owner at all. They need the list reactively and
// cannot mount an observer, so the cache is mirrored into one signal.

const [snapshot, setSnapshot] = createSignal<KilnListEntry[]>([]);

let boundClient: QueryClient | null = null;
let unbind: (() => void) | null = null;

/**
 * Mirrors the current client's kiln entry into the signal, and starts the
 * fetch.
 *
 * The subscription is permanent for the life of the tab, by design: the signal
 * has no owner to clean it up, and every reader of it is a render path that
 * can appear again at any time. One observer for one roster is the cost.
 */
function bindSnapshot(): void {
  const client = seededClient();
  if (boundClient === client) return;

  unbind?.();
  boundClient = client;
  // An observer, not a cache subscription plus a prefetch: an observer is what
  // marks the entry as in use, and an entry nothing observes is collected the
  // moment its fetch lands.
  //
  // Its errors stay here. Nothing on this path can render a refusal, so a
  // daemon that refuses the call leaves the last-known roster on screen, which
  // is what these callers did before. A caller that must SHOW the refusal
  // calls `useKilns()`.
  const observer = new QueryObserver<KilnListEntry[]>(client, kilnsQueryOptions());
  setSnapshot(observer.getCurrentResult().data ?? []);
  unbind = observer.subscribe((result) => setSnapshot(result.data ?? []));
}

/**
 * The roster as a plain reactive accessor, outside any component.
 *
 * Reading it starts the fetch, the same way `kilnStore.ensureLoaded` did. The
 * first read binds the observer above, and nothing unbinds it until the tab
 * closes or a test calls `resetKilnsForTests`.
 */
export const kilnsSnapshot: Accessor<KilnListEntry[]> = () => {
  bindSnapshot();
  return snapshot();
};

/** Drops the snapshot's binding so one test's client cannot answer the next. */
export function resetKilnsForTests(): void {
  unbind?.();
  unbind = null;
  boundClient = null;
  seededClients = new WeakSet();
  setSnapshot([]);
}

// ---- name ↔ path, for the callers that only need one directory -------------

/**
 * The directory a kiln name points at, or `null` — no such name, or the
 * roster has not answered yet.
 *
 * `null` means "no root": callers skip the query rather than falling back to a
 * wider one. Nothing here ever returns a directory for a name the daemon did
 * not issue.
 */
export function kilnPathOf(name: string | null | undefined): string | null {
  return kilnPathForName(name, kilnsSnapshot());
}

/**
 * The directory of the kiln the daemon touched last, or `null` before the
 * roster answers. A wikilink with no kiln of its own resolves here: a
 * transcript whose session names a kiln the registry cannot map still points
 * at the kiln on screen, and that beats "Note not found" for a note that
 * exists.
 */
export function mostRecentKilnPath(): string | null {
  const list = kilnsSnapshot();
  if (list.length === 0) return null;
  const age = (k: KilnListEntry) => k.last_access_secs_ago ?? Number.POSITIVE_INFINITY;
  return [...list].sort((a, b) => age(a) - age(b))[0].path;
}

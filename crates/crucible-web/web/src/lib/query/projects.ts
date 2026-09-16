import {
  useMutation,
  useQuery,
  type QueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from '@tanstack/solid-query';
import {
  getProject,
  listProjects,
  registerProject,
  scmClone,
  unregisterProject,
  type ScmCloneResponse,
} from '@/lib/api';
import { readLocalCache, writeLocalCache } from '@/lib/local-cache';
import type { Project } from '@/lib/types';
import { getQueryClient } from './client';
import { keys } from './keys';

/**
 * The registered-project roster, fetched once for the whole shell.
 *
 * Ten surfaces asked for this list — the project context through its own store
 * and a hand-written localStorage cache, the composer through
 * `swrLocal('projects')`, the phone sheet and the root dropdown through a bare
 * call. Each one paid for its own request, and each one held its own copy, so
 * a project registered in the files pane stayed invisible in the composer
 * until that component remounted. One query key answers all of them, and every
 * mutation below invalidates that key rather than patching a private list.
 *
 * The stale-while-revalidate behaviour is kept, under the same storage key the
 * context and `swrLocal` shared: the last roster seeds the cache, so a cold
 * load paints real project names instead of an empty rail, and a successful
 * fetch writes it back. What is NOT kept is the silence. A daemon that refuses
 * the call now reaches the caller as the query's `error` rather than as a
 * roster with nothing in it.
 */

/** The `swrLocal` key this roster has always been stored under. */
const STORAGE_KEY = 'projects';

/** The roster a previous run stored, or `undefined` when there is none. */
function storedProjects(): Project[] | undefined {
  const stored = readLocalCache<Project[]>(STORAGE_KEY);
  return Array.isArray(stored) ? stored : undefined;
}

/** The one fetch, which also refreshes what the next cold load paints. */
async function fetchProjects(): Promise<Project[]> {
  const projects = await listProjects();
  writeLocalCache(STORAGE_KEY, projects);
  return projects;
}

/** Clients that already carry the stored roster. */
let seededClients = new WeakSet<QueryClient>();

/**
 * The cache every project read goes through, with the stored roster already in
 * it.
 *
 * The seed is written with `updatedAt: 0`, which is the whole point: the entry
 * is on screen immediately AND is stale, so the first observer still fetches.
 * Seeding with the current time would paint a roster and never correct it.
 * `ProjectProvider` reads that same `updatedAt` to tell a painted roster from
 * an answered one before it pins a project.
 *
 * A test injects its own client, so the seed is tracked per client rather than
 * by a module flag — one test's seeding must not silence the next test's.
 */
function seededClient(): QueryClient {
  const client = getQueryClient();
  if (seededClients.has(client)) return client;
  seededClients.add(client);

  const stored = storedProjects();
  if (stored && client.getQueryData(keys.projects()) === undefined) {
    client.setQueryData(keys.projects(), stored, { updatedAt: 0 });
  }
  return client;
}

/**
 * The registered projects as a query.
 *
 * `data` is `undefined` only while the very first fetch of a browser with no
 * stored roster is in flight; every later mount reads the cache synchronously.
 */
export function useProjects(): UseQueryResult<Project[], Error> {
  return useQuery(
    () => ({ queryKey: keys.projects(), queryFn: fetchProjects }),
    () => seededClient(),
  );
}

/**
 * Asks for the roster again, and for the kiln roster a registration may have
 * changed.
 *
 * The promise is RETURNED rather than dropped, so a mutation settles only once
 * the new roster has landed. Every caller of these three mutations selects the
 * project it just created; a selection made against the roster as it stood
 * before the write finds no row and silently does nothing.
 *
 * Registering a project can add a kiln: the daemon registers the project's own
 * knowledge directory with it, so a stale kiln roster would leave the new
 * kiln out of every picker until something else refreshed it.
 */
function invalidateRoster(client: QueryClient, alsoKilns: boolean): Promise<void> {
  const roster = client.invalidateQueries({ queryKey: keys.projects() });
  if (!alsoKilns) return roster;
  return Promise.all([roster, client.invalidateQueries({ queryKey: keys.kilns() })]).then(
    () => undefined,
  );
}

/** Registers a directory as a project; invalidates the project and kiln rosters. */
export function useRegisterProject(): UseMutationResult<Project, Error, string> {
  return useMutation(
    () => ({
      mutationFn: (path: string) => registerProject(path),
      onSuccess: () => invalidateRoster(seededClient(), true),
    }),
    () => seededClient(),
  );
}

/**
 * Removes a project from the registry; invalidates the project roster.
 *
 * The kiln roster is left alone: unregistering a project does not retract a
 * kiln the user attached, and a kiln that did go with it drops out of the
 * roster on its own next read.
 */
export function useUnregisterProject(): UseMutationResult<void, Error, string> {
  return useMutation(
    () => ({
      mutationFn: (path: string) => unregisterProject(path),
      onSuccess: () => invalidateRoster(seededClient(), false),
    }),
    () => seededClient(),
  );
}

/**
 * Clones a remote repository and registers the checkout as a project.
 *
 * Slow — a network clone, with no timeout beyond `fetch`'s. It invalidates
 * both rosters for the same reason `useRegisterProject` does: the clone ends
 * in a registration.
 */
export function useScmClone(): UseMutationResult<ScmCloneResponse, Error, string> {
  return useMutation(
    () => ({
      mutationFn: (url: string) => scmClone(url),
      onSuccess: () => invalidateRoster(seededClient(), true),
    }),
    () => seededClient(),
  );
}

/**
 * One project by path, as a promise, or `null` when the daemon knows no such
 * project.
 *
 * `ProjectProvider.selectProject` is the only caller: it selects a path that
 * the roster may not list — a window addressed to a worktree registered after
 * this tab loaded, for one. It cannot render a pending state, so it waits on
 * the promise rather than mounting an observer. The answer is cached under its
 * own key, so selecting the same path twice asks once.
 */
export function fetchProjectOnce(path: string): Promise<Project | null> {
  return seededClient().ensureQueryData({
    queryKey: keys.project(path),
    queryFn: () => getProject(path),
  });
}

/** Drops the per-client seeding so one test's client cannot answer the next. */
export function resetProjectsForTests(): void {
  seededClients = new WeakSet();
}

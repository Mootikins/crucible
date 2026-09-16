import type { Accessor } from 'solid-js';
import {
  useMutation,
  useQuery,
  type QueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from '@tanstack/solid-query';
import {
  archiveSession,
  cancelSession,
  createSession,
  deleteSession,
  endSession,
  exportSession,
  getSession,
  listSessions,
  pauseSession,
  resumeSession,
  setSessionTitle,
  unarchiveSession,
} from '@/lib/api';
import { readLocalCache, writeLocalCache } from '@/lib/local-cache';
import type { CreateSessionParams, Session } from '@/lib/types';
import { getQueryClient } from './client';
import { keys } from './keys';

/**
 * The session roster and every write that changes it.
 *
 * Two owners held this list. `SessionContext` kept it in a store with its own
 * localStorage mirror, and `InboxPanel` kept a second, independent copy behind
 * its own `createResource`. A session deleted in the inbox stayed in the rail,
 * because the inbox refetched only itself; a session created in the rail was
 * invisible to the inbox until that panel remounted. One key answers both, and
 * every mutation below writes that key rather than a private array.
 *
 * The archived flag is part of the key, not a filter applied after the fetch:
 * `include_archived=true` and the default are two different questions and two
 * different answers. That is also what retires `SessionContext`'s
 * `lastIncludeArchived` guard — the guard existed because one store held both
 * answers, so a bare refresh could replace the archived view with the active
 * one. Two keys cannot overwrite each other.
 *
 * `kiln` and `workspace` are NOT in the key and not sent. The session tree
 * groups and filters the global list on the client; a scoped fetch here used
 * to clobber that list, and "No project" sessions flashed then vanished.
 */

/** The `crucible:cache:sessions` key the context has always stored under. */
const STORAGE_KEY = 'sessions';

/** The list a previous run stored, or `undefined` when there is none. */
function storedSessions(): Session[] | undefined {
  const stored = readLocalCache<Session[]>(STORAGE_KEY);
  return Array.isArray(stored) ? stored : undefined;
}

/** The one fetch, which also refreshes what the next cold load paints. */
async function fetchSessions(includeArchived: boolean): Promise<Session[]> {
  const sessions = await listSessions({ includeArchived });
  writeLocalCache(STORAGE_KEY, sessions);
  return sessions;
}

/** Clients that already carry the stored list. */
let seededClients = new WeakSet<QueryClient>();

/**
 * The cache every session read goes through, with the stored list already in
 * it.
 *
 * The seed is written with `updatedAt: 0`, so the rail paints the last known
 * sessions immediately AND the entry is stale, which means the first observer
 * still fetches. Both variants are seeded from the one stored list, because
 * storage holds whichever list was read last and neither variant should sit
 * empty while the daemon answers. The fetch corrects both.
 *
 * A test injects its own client, so the seed is tracked per client rather than
 * by a module flag — one test's seeding must not silence the next test's.
 */
function seededClient(): QueryClient {
  const client = getQueryClient();
  if (seededClients.has(client)) return client;
  seededClients.add(client);

  const stored = storedSessions();
  if (stored) {
    for (const includeArchived of [false, true]) {
      const key = keys.sessions(includeArchived);
      if (client.getQueryData(key) === undefined) {
        client.setQueryData(key, stored, { updatedAt: 0 });
      }
    }
  }
  return client;
}

/**
 * The sessions the daemon lists, as a query.
 *
 * The flag is an accessor because the panels change it while they are mounted:
 * the desktop rail asks for the archived rows, the phone tab does not. Passing
 * the value would pin one answer for the life of the component.
 */
export function useSessions(
  includeArchived: Accessor<boolean>,
): UseQueryResult<Session[], Error> {
  return useQuery(
    () => ({
      queryKey: keys.sessions(includeArchived()),
      queryFn: () => fetchSessions(includeArchived()),
    }),
    () => seededClient(),
  );
}

/**
 * One session by id, as a query, or no query at all while the id is null.
 *
 * The status bar follows the focused pane, which may be focused on nothing, so
 * the id is an accessor and a null id asks the daemon nothing.
 */
export function useSession(id: Accessor<string | null>): UseQueryResult<Session, Error> {
  return useQuery(
    () => {
      const sessionId = id();
      return {
        queryKey: keys.session(sessionId ?? ''),
        queryFn: () => getSession(sessionId as string),
        enabled: sessionId !== null,
      };
    },
    () => seededClient(),
  );
}

/**
 * One session as a promise, for a caller that cannot render a pending state.
 *
 * `SessionContext.adoptSession` and `sessionBootstrap` both read the record of
 * the pane being restored, and the status bar reads the same record through
 * `useSession`. One key, so a reload that opens one pane asks once.
 */
export function fetchSessionOnce(id: string): Promise<Session> {
  return seededClient().ensureQueryData({
    queryKey: keys.session(id),
    queryFn: () => getSession(id),
  });
}

/** Both list variants, for a write whose effect the daemon must confirm. */
function invalidateLists(client: QueryClient): Promise<void> {
  return Promise.all([
    client.invalidateQueries({ queryKey: keys.sessions(false) }),
    client.invalidateQueries({ queryKey: keys.sessions(true) }),
  ]).then(() => undefined);
}

/** Rewrites one held list, leaving a list nothing has read alone. */
function writeList(
  client: QueryClient,
  includeArchived: boolean,
  rewrite: (held: Session[]) => Session[],
): void {
  client.setQueryData<Session[]>(keys.sessions(includeArchived), (held) =>
    held ? rewrite(held) : held,
  );
}

/** The row for one id, from whichever list holds it. */
function cachedRow(client: QueryClient, id: string): Session | undefined {
  for (const includeArchived of [false, true]) {
    const row = client
      .getQueryData<Session[]>(keys.sessions(includeArchived))
      ?.find((session) => session.id === id);
    if (row) return row;
  }
  return undefined;
}

/**
 * Applies one patch to every cached copy of a session: both list variants and
 * the single-session key.
 *
 * Exported because a caller outside this module changes a field this module
 * does not own — the model, until Task C2.4 moves that write here — and the
 * rail must redraw without waiting for a refetch.
 */
export function patchCachedSession(id: string, patch: Partial<Session>): void {
  const client = seededClient();
  for (const includeArchived of [false, true]) {
    writeList(client, includeArchived, (held) =>
      held.map((session) => (session.id === id ? { ...session, ...patch } : session)),
    );
  }
  client.setQueryData<Session>(keys.session(id), (held) =>
    held ? { ...held, ...patch } : held,
  );
}

/**
 * Forgets one session, without asking the daemon to delete anything.
 *
 * For the row the daemon has already dropped: the seeded list is the last one
 * this browser saw, and a session deleted from another tab is still in it.
 * Opening that row can only fail, so the read that failed takes it out.
 */
export function dropCachedSession(id: string): void {
  const client = seededClient();
  for (const includeArchived of [false, true]) {
    writeList(client, includeArchived, (held) => held.filter((session) => session.id !== id));
  }
  client.removeQueries({ queryKey: keys.session(id) });
}

/**
 * Moves one row between the two variants.
 *
 * Archiving takes the row out of the default list and flags it in the archived
 * one; restoring does the reverse, and puts the row back at the head because
 * the daemon's own list is newest-first. Both are followed by an invalidation,
 * so the daemon has the last word on the order.
 */
function moveBetweenLists(client: QueryClient, id: string, archived: boolean): void {
  const row = cachedRow(client, id);

  writeList(client, true, (held) =>
    held.map((session) => (session.id === id ? { ...session, archived } : session)),
  );
  writeList(client, false, (held) => {
    if (archived) return held.filter((session) => session.id !== id);
    if (!row || held.some((session) => session.id === id)) return held;
    return [{ ...row, archived: false }, ...held];
  });
}

/**
 * Creates a session, puts it at the head of both lists, then asks again.
 *
 * The append is what the composer used to do to its own store: the draft
 * surface opens the new session immediately, and a rail that waited for the
 * refetch drew a row that was not there yet.
 */
export function useCreateSession(): UseMutationResult<Session, Error, CreateSessionParams> {
  return useMutation(
    () => ({
      mutationFn: (params: CreateSessionParams) => createSession(params),
      onSuccess: (created: Session) => {
        const client = seededClient();
        for (const includeArchived of [false, true]) {
          writeList(client, includeArchived, (held) => [created, ...held]);
        }
        client.setQueryData(keys.session(created.id), created);
        return invalidateLists(client);
      },
    }),
    () => seededClient(),
  );
}

/** Runs one lifecycle write, patches the state it produces, then re-reads. */
function useLifecycleMutation(
  write: (id: string) => Promise<void>,
  state: Session['state'],
): UseMutationResult<void, Error, string> {
  return useMutation(
    () => ({
      mutationFn: write,
      onSuccess: (_result: void, id: string) => {
        patchCachedSession(id, { state });
        // The session key, not the lists: a pause changes one session's state
        // and the rows beside it are still current.
        return seededClient().invalidateQueries({ queryKey: keys.session(id) });
      },
    }),
    () => seededClient(),
  );
}

/** Pauses a session; patches its state and re-reads that session. */
export function usePauseSession(): UseMutationResult<void, Error, string> {
  return useLifecycleMutation(pauseSession, 'paused');
}

/** Resumes a session; patches its state and re-reads that session. */
export function useResumeSession(): UseMutationResult<void, Error, string> {
  return useLifecycleMutation(resumeSession, 'active');
}

/** Ends a session; patches its state and re-reads that session. */
export function useEndSession(): UseMutationResult<void, Error, string> {
  return useLifecycleMutation(endSession, 'ended');
}

/**
 * Deletes a session; takes it out of both lists, then asks the daemon again.
 *
 * Both lists, because the inbox deletes an archived row while the rail is
 * showing the active one. The inbox used to refetch only its own copy, so the
 * rail kept a row the daemon had already dropped.
 */
export function useDeleteSession(): UseMutationResult<void, Error, string> {
  return useMutation(
    () => ({
      mutationFn: (id: string) => deleteSession(id),
      onSuccess: (_result: void, id: string) => {
        dropCachedSession(id);
        return invalidateLists(seededClient());
      },
    }),
    () => seededClient(),
  );
}

/** Archives a session: it leaves the default list and is flagged in the other. */
export function useArchiveSession(): UseMutationResult<void, Error, string> {
  return useMutation(
    () => ({
      mutationFn: (id: string) => archiveSession(id),
      onSuccess: (_result: void, id: string) => {
        const client = seededClient();
        moveBetweenLists(client, id, true);
        return invalidateLists(client);
      },
    }),
    () => seededClient(),
  );
}

/** Restores a session to the default list. */
export function useUnarchiveSession(): UseMutationResult<void, Error, string> {
  return useMutation(
    () => ({
      mutationFn: (id: string) => unarchiveSession(id),
      onSuccess: (_result: void, id: string) => {
        const client = seededClient();
        moveBetweenLists(client, id, false);
        return invalidateLists(client);
      },
    }),
    () => seededClient(),
  );
}

/**
 * Cancels the running turn.
 *
 * It invalidates nothing: the turn's own events reach the cache through the
 * chat stream's route, and the session row is unchanged by a cancel. The
 * daemon's answer says whether there was anything to cancel.
 */
export function useCancelSession(): UseMutationResult<boolean, Error, string> {
  return useMutation(
    () => ({ mutationFn: (id: string) => cancelSession(id) }),
    () => seededClient(),
  );
}

/**
 * Renders a session as markdown.
 *
 * A read the daemon answers over POST, so it is a mutation with no key of its
 * own: the export dialog holds the string in a local signal and drops it when
 * it closes.
 */
export function useExportSession(): UseMutationResult<string, Error, string> {
  return useMutation(
    () => ({ mutationFn: (id: string) => exportSession(id) }),
    () => seededClient(),
  );
}

/** Renames a session; patches the title everywhere it is cached. */
export function useSetSessionTitle(): UseMutationResult<
  void,
  Error,
  { id: string; title: string }
> {
  return useMutation(
    () => ({
      mutationFn: ({ id, title }: { id: string; title: string }) => setSessionTitle(id, title),
      onSuccess: (_result: void, { id, title }: { id: string; title: string }) => {
        patchCachedSession(id, { title });
        return seededClient().invalidateQueries({ queryKey: keys.session(id) });
      },
    }),
    () => seededClient(),
  );
}

/** Drops the per-client seeding so one test's client cannot answer the next. */
export function resetSessionsForTests(): void {
  seededClients = new WeakSet();
}

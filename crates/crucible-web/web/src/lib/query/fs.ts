import type { Accessor } from 'solid-js';
import {
  useMutation,
  useQuery,
  type QueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from '@tanstack/solid-query';
import {
  fsMkdir,
  fsMove,
  fsTrash,
  getFileWithHash,
  listDir,
  saveFileContent,
  type FsMoveOutcome,
} from '@/lib/api';
import type { FsListing } from '@/lib/types';
import { getQueryClient } from './client';
import { keys } from './keys';

/**
 * The filesystem as one cache: a listing per folder, and a file per path.
 *
 * Every entry here is held under an ABSOLUTE path, because that is the only
 * name the daemon's stream uses. `/api/fs/events` says `/kiln/notes/a.md` and
 * nothing else — no root, no relative path — so a listing held under the pair
 * the list route takes (`root`, `rel_path`) is a listing no event can reach.
 * `absFolderPath` is what turns the caller's pair into that name, and its test
 * asserts it against `folderOf`, which is what the stream's route calls.
 *
 * Six panels read files today, and each of them fetched on its own: the tree,
 * the editor, the canvas card, the backlinks snippets, the tool diff and the
 * permission prompt. Two of them looking at one note asked twice, and neither
 * learnt that the other had written to it.
 *
 * A write invalidates the keys it makes wrong ITSELF, and the stream's route
 * invalidates them again a moment later. Both is correct: the user who moved
 * the row is looking at the panel now, the daemon's event is a round trip
 * away, and a root the daemon does not watch never sends one.
 */

/** What one directory read asks for: a root, and a folder inside it. */
export interface DirRequest {
  /** The absolute root the daemon admits: a registered project or a kiln. */
  readonly root: string;
  /** POSIX path of the folder under `root`. Empty names the root itself. */
  readonly relPath?: string;
  /** Whether dotfiles are listed. NOT part of the key — see `dirOptions`. */
  readonly showHidden?: boolean;
}

/** What one move asks for. `kind` picks the daemon's allowlist. */
export interface FsMoveParams {
  readonly root: string;
  readonly kind: 'project' | 'kiln';
  readonly fromRel: string;
  readonly toRel: string;
}

/** What one folder creation or one trash asks for. */
export interface FsPathParams {
  readonly root: string;
  readonly kind: 'project' | 'kiln';
  readonly relPath: string;
}

/** What one whole-file save asks for. */
export interface SaveFileParams {
  readonly path: string;
  readonly content: string;
}

/**
 * The folder one absolute path is in.
 *
 * The stream's route calls it on the path an event names, and `absFolderPath`
 * answers the same strings, so a reader and an event meet on one key. A path
 * directly under the root answers the root itself: an empty string there would
 * name a folder nothing lists, and the event would reach no reader.
 */
export function folderOf(path: string): string {
  const cut = path.lastIndexOf('/');
  if (cut < 0) return '/';
  return cut === 0 ? '/' : path.slice(0, cut);
}

/**
 * The absolute folder one `(root, relPath)` pair names.
 *
 * The daemon's list route takes the pair and answers about the folder; the
 * daemon's stream names the folder and knows no pair. This is the one place
 * the two meet, so a caller that holds a pair keys its listing where an event
 * can find it.
 */
export function absFolderPath(root: string, relPath = ''): string {
  const base = root.replace(/\/+$/, '');
  // A caller spells the folder it is already in as `''` or as `.`, and `./x`
  // is the same folder as `x`. Left alone, a dot names `<root>/.`, which is a
  // folder the daemon never says — so the key would be one no event reaches.
  const rel = relPath.replace(/^\.(?=$|\/)/, '').replace(/^\/+|\/+$/g, '');
  if (!rel) return base || '/';
  return `${base}/${rel}`;
}

/**
 * The options of one listing.
 *
 * `showHidden` is deliberately NOT in the key. The key must be the absolute
 * folder and nothing else, or the stream's route invalidates a key no reader
 * holds. The toggle therefore invalidates the folders it changes the answer
 * for, through `invalidateDirsUnder`.
 */
function dirOptions(request: DirRequest) {
  return {
    queryKey: keys.fsDir(absFolderPath(request.root, request.relPath)),
    queryFn: () => listDir(request.root, request.relPath ?? '', request.showHidden ?? false),
  };
}

/** The options of one file's bytes. */
function fileOptions(path: string) {
  return { queryKey: keys.fsFile(path), queryFn: () => getFileWithHash(path) };
}

/**
 * One folder's entries.
 *
 * The request is an accessor because the browsed root is a control the user
 * changes while the panel is mounted, and `null` is "nothing to list yet",
 * which is a different state from an empty folder.
 */
export function useListDir(
  request: Accessor<DirRequest | null>,
): UseQueryResult<FsListing, Error> {
  return useQuery(() => {
    const asked = request();
    return {
      ...(asked ? dirOptions(asked) : { queryKey: keys.fsDir(''), queryFn: () => listDir('') }),
      enabled: asked !== null,
    };
  }, getQueryClient);
}

/**
 * One folder's entries, as a promise.
 *
 * The file tree is a RECURSION over the folders the user left expanded, so it
 * cannot mount an observer per folder — it does not know their names until the
 * level above answers. It reads the same entries under the same keys, so a
 * folder an observer already holds is not fetched again, and a folder an event
 * made wrong is.
 */
export function fetchDirOnce(request: DirRequest): Promise<FsListing> {
  return getQueryClient().fetchQuery(dirOptions(request));
}

/**
 * Makes every held listing at or under one root wrong.
 *
 * Two callers need it, and neither is a write: the hidden-files toggle, which
 * changes what the SAME folder answers, and the tree's explicit refresh. The
 * key carries the folder alone, so this walks the held keys rather than naming
 * one.
 */
export function invalidateDirsUnder(root: string): Promise<void> {
  const base = absFolderPath(root);
  return getQueryClient()
    .invalidateQueries({
      predicate: (query) => {
        const [family, kind, path] = query.queryKey as unknown[];
        if (family !== 'fs' || kind !== 'dir' || typeof path !== 'string') return false;
        return base === '/' || path === base || path.startsWith(`${base}/`);
      },
    })
    .then(() => undefined);
}

/**
 * One file's bytes and the hash of them.
 *
 * The hash is what a guarded write anchors on, so the read that fills the
 * cache is the one that carries it. `useGetFileContent` selects the text out
 * of the same entry rather than holding a second copy under the same key —
 * two shapes under one key is a reader that crashes on another reader's data.
 */
export function useGetFileWithHash(
  path: Accessor<string | null>,
): UseQueryResult<{ content: string; content_hash: string }, Error> {
  return useQuery(() => {
    const asked = path();
    return { ...fileOptions(asked ?? ''), enabled: asked !== null };
  }, getQueryClient);
}

/** One file's text. */
export function useGetFileContent(path: Accessor<string | null>): UseQueryResult<string, Error> {
  return useQuery(() => {
    const asked = path();
    return {
      ...fileOptions(asked ?? ''),
      enabled: asked !== null,
      select: (file: { content: string }) => file.content,
    };
  }, getQueryClient);
}

/**
 * One file's text, as a promise.
 *
 * For the readers that act rather than render: the tool card building a diff
 * on a click, and the backlinks panel pulling a snippet out of each linking
 * note. Both read under the key the panels observe, so the third reader of a
 * note joins the two that are already holding it.
 */
export function fetchFileContentOnce(path: string): Promise<string> {
  return getQueryClient()
    .fetchQuery(fileOptions(path))
    .then((file) => file.content);
}

/** The file, and the folder whose row carries its size and modified time. */
function invalidateFile(client: QueryClient, path: string): Promise<void> {
  return Promise.all([
    client.invalidateQueries({ queryKey: keys.fsFile(path) }),
    client.invalidateQueries({ queryKey: keys.fsDir(folderOf(path)) }),
  ]).then(() => undefined);
}

/** Every named folder, each asked for once. */
function invalidateFolders(client: QueryClient, folders: string[]): Promise<void> {
  return Promise.all(
    [...new Set(folders)].map((folder) =>
      client.invalidateQueries({ queryKey: keys.fsDir(folder) }),
    ),
  ).then(() => undefined);
}

/**
 * Every path one write moved, and the folder each of them is in.
 *
 * A move and a trash change the BYTES at a path as much as they change the
 * listing around it: the old path holds nothing now, and the new one holds
 * what was at the old one. A reader of either is holding an answer about a
 * file that is no longer there, so this names the file keys as well — the
 * same pair the stream's route names, for the same reason.
 */
function invalidatePaths(client: QueryClient, paths: string[]): Promise<void> {
  const each = [...new Set(paths)];
  return Promise.all([
    ...each.map((path) => client.invalidateQueries({ queryKey: keys.fsFile(path) })),
    invalidateFolders(client, each.map(folderOf)),
  ]).then(() => undefined);
}

/**
 * Writes a whole file, then makes the entries it changed wrong.
 *
 * The daemon writes what it is given, so this is the unguarded path: the
 * canvas card's own document and the tree's "new note". A note the USER is
 * editing goes through the offline outbox instead, which sends the hash it
 * read and lets the daemon refuse a stale write.
 *
 * It is a plain function beside the hook because one caller writes while it is
 * going away: virtualization unmounts a canvas card that leaves the viewport,
 * and that card flushes its queued edit from `onCleanup`. A write that depends
 * on a live observer would make virtualization into data loss.
 */
export function saveFileOnce(path: string, content: string): Promise<void> {
  return saveFileContent(path, content).then(() => invalidateFile(getQueryClient(), path));
}

/** The same write, for a caller that wants its pending and error state. */
export function useSaveFileContent(): UseMutationResult<void, Error, SaveFileParams> {
  return useMutation(
    () => ({
      mutationFn: ({ path, content }: SaveFileParams) => saveFileOnce(path, content),
    }),
    getQueryClient,
  );
}

/**
 * Moves or renames one path inside one root.
 *
 * Both ends are invalidated — the bytes and the listing at each — and a rename
 * inside one folder names that folder once: two invalidations of one listing
 * are two refetches for one move.
 */
export function useFsMove(): UseMutationResult<FsMoveOutcome, Error, FsMoveParams> {
  return useMutation(
    () => ({
      mutationFn: ({ root, kind, fromRel, toRel }: FsMoveParams) =>
        fsMove(root, kind, fromRel, toRel),
      onSuccess: (_outcome: FsMoveOutcome, { root, fromRel, toRel }: FsMoveParams) =>
        invalidatePaths(getQueryClient(), [
          absFolderPath(root, fromRel),
          absFolderPath(root, toRel),
        ]),
    }),
    getQueryClient,
  );
}

/** Creates a folder; the folder that gained a row is asked for again. */
export function useFsMkdir(): UseMutationResult<void, Error, FsPathParams> {
  return useMutation(
    () => ({
      mutationFn: ({ root, kind, relPath }: FsPathParams) => fsMkdir(root, kind, relPath),
      onSuccess: (_result: void, { root, relPath }: FsPathParams) =>
        invalidateFolders(getQueryClient(), [folderOf(absFolderPath(root, relPath))]),
    }),
    getQueryClient,
  );
}

/** Moves one path to the root's trash; its bytes and its folder are asked for again. */
export function useFsTrash(): UseMutationResult<void, Error, FsPathParams> {
  return useMutation(
    () => ({
      mutationFn: ({ root, kind, relPath }: FsPathParams) => fsTrash(root, kind, relPath),
      onSuccess: (_result: void, { root, relPath }: FsPathParams) =>
        invalidatePaths(getQueryClient(), [absFolderPath(root, relPath)]),
    }),
    getQueryClient,
  );
}

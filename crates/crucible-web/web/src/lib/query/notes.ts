import type { Accessor } from 'solid-js';
import { useQuery, type UseQueryResult } from '@tanstack/solid-query';
import {
  getBacklinks,
  getKilnGraph,
  listKilnNotes,
  listNotes,
  resolveNotePath,
} from '@/lib/api';
import type { GraphDto } from '@/lib/graph/types';
import { isMarkdownPath } from '@/lib/markdown-path';
import type { BacklinksResponse, FileEntry, NoteEntry } from '@/lib/types';
import { getQueryClient } from './client';
import { keys } from './keys';

/**
 * The notes of a kiln as one cache: the index, the resolver, the backlinks
 * and the graph.
 *
 * Every entry here carries the KILN. A note name means nothing on its own —
 * two vaults each hold an `Index.md` — so a list held under the bare name
 * would answer the second reader with the first reader's vault. The daemon
 * refuses to resolve across a root for the same reason.
 *
 * Six surfaces read these four routes today, and each read on its own: the
 * command palette, the canvas note picker, the file tree, the editor's link
 * follow, the backlinks panel and the graph. The palette and the picker ask
 * the same question of the same kiln, and asked it twice.
 *
 * There is no mutation here. The web client never writes a note through these
 * routes: a note is a file, and a file is written through `lib/query/fs.ts`
 * or the offline outbox, which invalidate the filesystem keys.
 */

/** What one resolved wikilink target is. `null` is a target that names no note. */
export interface ResolvedNote {
  path: string;
  absolutePath: string;
  title?: string;
}

/**
 * How long a completion list is reused.
 *
 * Deliberately far shorter than the app-wide window. The list exists to
 * coalesce the burst of asks one person makes while typing a link, not to be
 * a store: a long-lived list goes stale the moment a note is created, renamed
 * or moved, and the completion then omits it until the page is reloaded.
 */
export const KILN_NOTES_STALE_MS = 5_000;

/**
 * How long a MISS is reused.
 *
 * Holding a miss is the point of `resolveOptions`: the hover preview asks
 * about every link under the pointer, and the daemon answers a name it cannot
 * place by walking the whole kiln. But the commonest miss is a link to a note
 * that does not exist YET — written a moment before the note is — and holding
 * that for the app-wide five minutes leaves the link broken on screen long
 * after the note is on disk.
 *
 * The filesystem stream drops it properly (`invalidateNotesUnder`, called from
 * that stream's route). This is the belt to that pair of braces: it covers a
 * note created somewhere the daemon does not watch, and the seconds between
 * the write and the event.
 */
export const MISS_STALE_MS = 5_000;

/**
 * The name one target is held under.
 *
 * A wikilink target is case-insensitive — the daemon matches a filename stem
 * by its lowercase form — so `[[Rust]]` and `[[rust]]` are one question. Two
 * entries for them would be two walks of the whole kiln for one answer. The
 * daemon still receives the name the user wrote, because the exact-path branch
 * of its ladder is case-sensitive; only the KEY is folded.
 */
function targetKey(name: string): string {
  return name.toLowerCase();
}

/**
 * The app-wide freshness window, from `lib/query/client.ts`.
 *
 * Named here because one option below has to choose between it and a shorter
 * one, and a query that sets `staleTime` at all loses the default.
 */
const DEFAULT_STALE_MS = 5 * 60 * 1000;

/** The options of one kiln's note index. */
function notesOptions(kiln: string) {
  return { queryKey: keys.notesList(kiln), queryFn: () => listNotes(kiln) };
}

/**
 * The options of one wikilink target.
 *
 * A miss answers `null` rather than throwing, so the cache HOLDS it. The
 * daemon answers a name it cannot place by walking the whole kiln, and the
 * hover preview asks about every link under the pointer; an unheld miss is
 * that walk once per hover, for a link that is simply broken. Both callers
 * already treated every failure as "no such note", so nothing here loses an
 * error a user was shown.
 */
function resolveOptions(kiln: string, name: string) {
  return {
    queryKey: keys.notesResolve(kiln, targetKey(name)),
    queryFn: (): Promise<ResolvedNote | null> => resolveNotePath(kiln, name).catch(() => null),
    // A miss is held for seconds and a hit for the app-wide window: a note
    // does not move under a link that resolved, but a link that resolved to
    // nothing is usually one whose note is about to be written.
    staleTime: (query: { state: { data?: ResolvedNote | null } }) =>
      query.state.data === null ? MISS_STALE_MS : DEFAULT_STALE_MS,
  };
}

/** The options of one note's backlinks. */
function backlinksOptions(kiln: string, note: string) {
  return { queryKey: keys.notesBacklinks(kiln, note), queryFn: () => getBacklinks(kiln, note) };
}

/** The options of one kiln's link graph. */
function graphOptions(kiln: string) {
  return { queryKey: keys.notesGraph(kiln), queryFn: () => getKilnGraph(kiln) };
}

/** The options of one kiln's completion list. See `KILN_NOTES_STALE_MS`. */
function kilnNotesOptions(kiln: string) {
  return {
    queryKey: keys.notesKiln(kiln),
    queryFn: () => listKilnNotes(kiln),
    staleTime: KILN_NOTES_STALE_MS,
  };
}

/**
 * One kiln's note index.
 *
 * The kiln is an accessor because the browsed kiln is a control the user
 * changes while the panel is mounted, and `null` is "no kiln chosen yet",
 * which is a different state from a kiln that holds no notes.
 */
export function useListNotes(kiln: Accessor<string | null>): UseQueryResult<NoteEntry[], Error> {
  return useQuery(() => {
    const asked = kiln();
    return { ...notesOptions(asked ?? ''), enabled: asked !== null };
  }, getQueryClient);
}

/**
 * One kiln's note index, as a promise.
 *
 * For a reader that acts rather than renders. It reads under the key the
 * panels observe, so it joins a list they are already holding.
 */
export function fetchNotesOnce(kiln: string): Promise<NoteEntry[]> {
  return getQueryClient().fetchQuery(notesOptions(kiln));
}

/** Makes one kiln's held index wrong, for the tree's explicit refresh. */
export function invalidateNotes(kiln: string): Promise<void> {
  return getQueryClient()
    .invalidateQueries({ queryKey: keys.notesList(kiln) })
    .then(() => undefined);
}

/** One wikilink target, resolved against one kiln. `null` names no note. */
export function useResolveNotePath(
  kiln: Accessor<string | null>,
  name: Accessor<string | null>,
): UseQueryResult<ResolvedNote | null, Error> {
  return useQuery(() => {
    const askedKiln = kiln();
    const askedName = name();
    const ready = askedKiln !== null && askedName !== null;
    return { ...resolveOptions(askedKiln ?? '', askedName ?? ''), enabled: ready };
  }, getQueryClient);
}

/**
 * One wikilink target, as a promise.
 *
 * Opening a link and previewing it are one resolution ladder, and both act on
 * a gesture rather than on a render, so both read through here. The hover that
 * previewed a target and the click that opens it ask once between them.
 */
export function fetchResolvedNoteOnce(kiln: string, name: string): Promise<ResolvedNote | null> {
  return getQueryClient().fetchQuery(resolveOptions(kiln, name));
}

/** Drops every held resolution, whatever kiln it belongs to. The test seam. */
export function invalidateResolvedNotes(): Promise<void> {
  return getQueryClient()
    .invalidateQueries({ queryKey: ['notes', 'resolve'] })
    .then(() => undefined);
}

/** Whether one absolute path lies inside one kiln root. */
function isUnder(path: string, kiln: string): boolean {
  const root = kiln.replace(/\/+$/, '');
  // The separator is part of the test. Without it `/kiln-archive/a.md` reads
  // as a file inside `/kiln`, and writing in one vault would drop what is held
  // about its similarly named sibling.
  return root !== '' && path.startsWith(`${root}/`);
}

/**
 * Drops everything held about the kilns that these paths are in.
 *
 * The filesystem stream calls it for every markdown file that is written,
 * moved or deleted. Everything here answers a question ABOUT A KILN rather
 * than about one file — which notes it holds, what a link in it resolves to,
 * what links to what — and a note appearing or leaving changes all of them.
 *
 * The one that matters most is a held MISS. A link is written before the note
 * it names, the hover preview resolves it to nothing, and nothing else was
 * ever going to drop that answer: the link read as broken for five minutes
 * after the note was on disk.
 *
 * The kiln comes from the KEY, not from the roster. Every key in this module
 * carries its kiln in the same position, an event carries an absolute path,
 * and containment is a prefix test — so this reaches exactly the entries that
 * are held, and needs no second cache to be loaded first.
 */
export function invalidateNotesUnder(paths: readonly string[]): Promise<void> {
  const notes = paths.filter((path) => isMarkdownPath(path));
  if (notes.length === 0) return Promise.resolve();

  return getQueryClient()
    .invalidateQueries({
      predicate: (query) => {
        const [family, , kiln] = query.queryKey as unknown[];
        if (family !== 'notes' || typeof kiln !== 'string') return false;
        return notes.some((path) => isUnder(path, kiln));
      },
    })
    .then(() => undefined);
}

/** One note's linked and unlinked mentions. */
export function useGetBacklinks(
  kiln: Accessor<string | null>,
  note: Accessor<string | null>,
): UseQueryResult<BacklinksResponse, Error> {
  return useQuery(() => {
    const askedKiln = kiln();
    const askedNote = note();
    const ready = askedKiln !== null && askedNote !== null;
    return { ...backlinksOptions(askedKiln ?? '', askedNote ?? ''), enabled: ready };
  }, getQueryClient);
}

/** One kiln's whole note-link graph. */
export function useGetKilnGraph(kiln: Accessor<string | null>): UseQueryResult<GraphDto, Error> {
  return useQuery(() => {
    const asked = kiln();
    return { ...graphOptions(asked ?? ''), enabled: asked !== null };
  }, getQueryClient);
}

/** One kiln's note files, as the link completion and the autocomplete read them. */
export function useListKilnNotes(
  kiln: Accessor<string | null>,
): UseQueryResult<FileEntry[], Error> {
  return useQuery(() => {
    const asked = kiln();
    return { ...kilnNotesOptions(asked ?? ''), enabled: asked !== null };
  }, getQueryClient);
}

/**
 * One kiln's note files, as a promise.
 *
 * The link completion answers a keystroke, not a render, and the chat
 * autocomplete loads on a trigger character. Both read under the key above, so
 * an editor and the composer share one list for the seconds it stays fresh.
 */
export function fetchKilnNotesOnce(kiln: string): Promise<FileEntry[]> {
  return getQueryClient().fetchQuery(kilnNotesOptions(kiln));
}

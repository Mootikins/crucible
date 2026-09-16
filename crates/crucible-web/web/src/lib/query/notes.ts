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

/** Drops every held resolution. The test seam, and what a rename invalidates. */
export function invalidateResolvedNotes(): Promise<void> {
  return getQueryClient()
    .invalidateQueries({ queryKey: ['notes', 'resolve'] })
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

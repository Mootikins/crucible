/**
 * Shared wikilink → note actions: resolve a `data-note` target to a kiln
 * note, open it in the editor, or fetch a hover preview.
 *
 * Used by chat messages, the editor's wikilink decorations, and the
 * backlinks panel so every surface resolves links the same way.
 */
import { getConfig, getNote, resolveNotePath } from './api';
import { extractFrontmatterBlock } from './frontmatter';
import { openFileInEditor } from './file-actions';
import { notificationActions } from '@/stores/notificationStore';
import type { NoteContent } from './types';

/** Resolve the kiln to search: explicit > configured default. */
async function resolveKiln(kiln?: string): Promise<string> {
  return kiln ?? (await getConfig()).kiln_path;
}

/**
 * Display name for a note payload. GET /api/notes/{name} sends no `name`
 * field, so a tab titled from it directly reads "undefined" — fall through
 * title → name → file stem.
 */
export function noteDisplayName(note: {
  name?: string;
  title?: string | null;
  path: string;
}): string {
  return (
    note.title ??
    note.name ??
    note.path.split('/').pop()?.replace(/\.md$/i, '') ??
    note.path
  );
}

/**
 * Resolve a wikilink target to its kiln file and open it in the editor.
 * Prefers the given kiln (e.g. the chat session's); falls back to the
 * configured default.
 */
export async function openNoteInEditor(name: string, kiln?: string): Promise<void> {
  try {
    const resolvedKiln = await resolveKiln(kiln);

    // Path lookup first. It needs no index, so a kiln that has never been
    // processed still follows its own links instead of falling through to
    // whatever kiln happens to be configured as the default.
    try {
      const hit = await resolveNotePath(resolvedKiln, name);
      openFileInEditor(hit.absolutePath, hit.title ?? name);
      return;
    } catch {
      /* fall through to the index, which also handles fuzzy matches */
    }

    const note = await getNote(name, resolvedKiln);
    openFileInEditor(noteAbsolutePath(note.path, resolvedKiln), noteDisplayName(note));
  } catch (err) {
    const message =
      err instanceof Error && /not found|404/i.test(err.message)
        ? `Note not found: ${name}`
        : `Failed to open note: ${name}`;
    notificationActions.addNotification('warning', message);
  }
}

/**
 * A kiln's notes live at the kiln root, not inside its `.crucible/` config
 * directory. The project registry currently reports a project's kiln as the
 * `.crucible` config dir (e.g. `/vault/.crucible`), which is not where notes
 * live — listing notes there returns nothing. Normalize to the kiln root (the
 * parent of `.crucible`). No-op once the registry reports the root directly.
 */
export function kilnRoot(kilnPath: string): string {
  const trimmed = kilnPath.replace(/\/$/, '');
  return trimmed.replace(/\/\.crucible$/, '');
}

/**
 * Which open kiln owns an absolute path, or `undefined` if none does.
 *
 * Longest match wins, mirroring the daemon's enclosing-root rule: with nested
 * kilns (`/vault` and `/vault/sub`) the inner one owns the file, and crediting
 * the outer one would let that file's links reach everything under `/vault` —
 * wider than "the kiln this file is in".
 *
 * A buffer's links belong to the kiln holding the FILE, never to whichever
 * kiln the status bar is pointing at; that is the whole reason this exists.
 */
export function kilnForPath(
  path: string,
  kilns: readonly { path: string }[],
): string | undefined {
  let best: string | undefined;
  for (const entry of kilns) {
    const root = kilnRoot(entry.path ?? '');
    if (!root) continue;
    // Segment boundary, not a bare prefix: `/vault-archive` is not in `/vault`.
    if (path !== root && !path.startsWith(`${root}/`)) continue;
    if (!best || root.length > best.length) best = root;
  }
  return best;
}

/**
 * Note paths from the daemon are kiln-relative in normal operation, but the
 * file API addresses files absolutely. Join relative paths onto the kiln.
 */
export function noteAbsolutePath(notePath: string, kiln: string): string {
  if (notePath.startsWith('/')) return notePath;
  return `${kiln.replace(/\/$/, '')}/${notePath}`;
}

/** Hover-preview payload for a resolved note. */
export interface NotePreview {
  title: string;
  path: string;
  absPath: string;
}

/** Note body without its YAML frontmatter block (for rendered views). */
export function stripFrontmatter(content: string): string {
  // YAML (---) and TOML (+++) — the daemon parser accepts both, so the web
  // must too, or TOML frontmatter leaks into the rendered body as text.
  const block = extractFrontmatterBlock(content);
  return block ? content.slice(block.bodyStart) : content;
}

/**
 * Wrap an unlinked mention in wikilink syntax inside `content`.
 *
 * Prefers the suggestion's byte offset (valid against the saved file); when
 * the editor buffer has drifted, falls back to the first case-preserving
 * occurrence of the mention text. Returns `null` when the mention can't be
 * located — the caller should refresh suggestions instead of guessing.
 */
export function insertWikilink(
  content: string,
  suggestion: { mention: string; target: string; offset: number },
): string | null {
  const { mention, target, offset } = suggestion;
  let at = -1;
  if (content.slice(offset, offset + mention.length) === mention) {
    at = offset;
  } else {
    at = content.indexOf(mention);
  }
  if (at === -1) return null;

  // Already inside a wikilink? Bail rather than double-wrap.
  const before = content.slice(Math.max(0, at - 2), at);
  const after = content.slice(at + mention.length, at + mention.length + 2);
  if (before === '[[' || after.startsWith(']]') || after.startsWith('|')) return null;

  const link =
    mention.toLowerCase() === target.toLowerCase() ? `[[${mention}]]` : `[[${target}|${mention}]]`;
  return content.slice(0, at) + link + content.slice(at + mention.length);
}

const previewCache = new Map<string, NotePreview | null>();
const PREVIEW_CACHE_MAX = 50;

/** Drop all cached previews (call after note writes; used by tests). */
export function clearNotePreviewCache(): void {
  previewCache.clear();
}

/**
 * Fetch a preview for a wikilink target. Returns `null` when the note
 * doesn't resolve. Results (including misses) are cached per kiln+name.
 */
export async function fetchNotePreview(name: string, kiln?: string): Promise<NotePreview | null> {
  const resolvedKiln = await resolveKiln(kiln);
  const cacheKey = `${resolvedKiln}:${name.toLowerCase()}`;
  if (previewCache.has(cacheKey)) {
    return previewCache.get(cacheKey) ?? null;
  }

  let preview: NotePreview | null = null;
  try {
    // Path lookup first, in the same order openNoteInEditor uses. Two reasons:
    // an unprocessed kiln has no index, so its own notes would preview as
    // missing; and the index matches FUZZILY, so asking it for "Architecture"
    // in a kiln without one returns "Component Architecture" — a preview of a
    // different note than the click opens.
    const hit = await resolveNotePath(resolvedKiln, name);
    preview = { title: hit.title ?? name, path: hit.path, absPath: hit.absolutePath };
  } catch {
    try {
      const note: NoteContent = await getNote(name, resolvedKiln);
      preview = {
        title: noteDisplayName(note),
        path: note.path,
        absPath: noteAbsolutePath(note.path, resolvedKiln),
      };
    } catch {
      preview = null;
    }
  }

  if (previewCache.size >= PREVIEW_CACHE_MAX) {
    previewCache.clear();
  }
  previewCache.set(cacheKey, preview);
  return preview;
}

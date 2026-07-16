/**
 * Shared wikilink → note actions: resolve a `data-note` target to a kiln
 * note, open it in the editor, or fetch a hover-preview excerpt.
 *
 * Used by chat messages, the editor's wikilink decorations, and the
 * backlinks panel so every surface resolves links the same way.
 */
import { getConfig, getNote, getFileContent } from './api';
import { openFileInEditor } from './file-actions';
import { notificationActions } from '@/stores/notificationStore';
import type { NoteContent } from './types';

/** Resolve the kiln to search: explicit > configured default. */
async function resolveKiln(kiln?: string): Promise<string> {
  return kiln ?? (await getConfig()).kiln_path;
}

/**
 * Resolve a wikilink target to its kiln file and open it in the editor.
 * Prefers the given kiln (e.g. the chat session's); falls back to the
 * configured default.
 */
export async function openNoteInEditor(name: string, kiln?: string): Promise<void> {
  try {
    const resolvedKiln = await resolveKiln(kiln);
    const note = await getNote(name, resolvedKiln);
    openFileInEditor(noteAbsolutePath(note.path, resolvedKiln), note.name);
  } catch (err) {
    const message =
      err instanceof Error && /not found|404/i.test(err.message)
        ? `Note not found: ${name}`
        : `Failed to open note: ${name}`;
    notificationActions.addNotification('warning', message);
  }
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
  excerpt: string;
}

/**
 * First ~`maxChars` of note content with YAML frontmatter stripped —
 * enough for a hover card without rendering the whole note.
 */
export function noteExcerpt(content: string, maxChars = 1200): string {
  let body = content;
  if (body.startsWith('---')) {
    const end = body.indexOf('\n---', 3);
    if (end !== -1) {
      body = body.slice(end + 4);
    }
  }
  body = body.trim();
  if (body.length <= maxChars) return body;
  // Cut on a line boundary so we don't render half a markdown construct.
  const cut = body.lastIndexOf('\n', maxChars);
  return body.slice(0, cut > 0 ? cut : maxChars).trimEnd() + '\n…';
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
    const note: NoteContent = await getNote(name, resolvedKiln);
    const absPath = noteAbsolutePath(note.path, resolvedKiln);
    let excerpt = '';
    try {
      const content = await getFileContent(absPath);
      excerpt = noteExcerpt(content);
    } catch {
      // Metadata-only preview when the content read fails.
    }
    preview = {
      title: note.title ?? note.name,
      path: note.path,
      absPath,
      excerpt,
    };
  } catch {
    preview = null;
  }

  if (previewCache.size >= PREVIEW_CACHE_MAX) {
    previewCache.clear();
  }
  previewCache.set(cacheKey, preview);
  return preview;
}

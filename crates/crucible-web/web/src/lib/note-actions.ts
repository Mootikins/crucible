/**
 * Shared wikilink → note actions: resolve a `data-note` target to a kiln
 * note, open it in the editor, or fetch a hover preview.
 *
 * Used by chat messages, the editor's wikilink decorations, and the
 * backlinks panel so every surface resolves links the same way.
 */
import { resolveNotePath } from './api';
import { extractFrontmatterBlock } from './frontmatter';
import { openFileInEditor } from './file-actions';
import { notificationActions } from '@/stores/notificationStore';

/**
 * Normalise the kiln a caller supplied.
 *
 * There is no default. Resolving a link in "whichever kiln is configured" is
 * how a link in one vault opened a same-named note from another; a link belongs
 * to the content that contains it, and content with no known kiln has no links
 * to follow. Callers that genuinely have no kiln get `undefined` and fail.
 */
function resolveKiln(kiln?: string): string | undefined {
  // Normalised the same way `kilnForPath` normalises its candidates. The
  // registry has reported a kiln as its `.crucible` config dir before now, and
  // one helper applying that fix while its sibling does not is how a path ends
  // up as `/vault/.crucible/Note.md`.
  return kiln ? kilnRoot(kiln) : undefined;
}

/**
 * Which kiln do links inside this element resolve in?
 *
 * **The single answer to that question**, shared by hover and click so the two
 * cannot describe different notes. They previously used different mechanisms —
 * an attribute for hover, a prop for click — which agreed only because each
 * surface remembered to set both, and user chat bubbles set neither.
 *
 * # The attribute is a carrier, not a source of truth
 *
 * `data-kiln` must be populated from canonical data: the *file's* own location
 * (via [`kilnForPath`] against the daemon's kiln list) or the *session's* kiln.
 * It must never be populated from whichever kiln happens to be active, and
 * there is deliberately **no fallback to the active kiln here**. Falling back
 * meant switching kilns in the navigator silently re-pointed the links inside
 * an already-open buffer — the content did not move, so its links must not
 * either. An undeclared surface now resolves nothing, which surfaces the
 * omission instead of quietly guessing.
 *
 * This is not a security boundary and is not trying to be one: anything that
 * can write this attribute can already run script in the page. Containment is
 * enforced by the server, which canonicalizes every candidate against the kiln
 * root. This is about being *correct*, not about being tamper-proof.
 */
export function kilnForElement(el: Element | null | undefined): string | undefined {
  return el?.closest?.('[data-kiln]')?.getAttribute('data-kiln') || undefined;
}

/**
 * A wikilink target resolved to a file, or `null` if this kiln has no such note.
 *
 * The ONE resolution ladder. Open and preview were separate copies of this,
 * agreeing by coincidence; the last time they drifted, hovering a link showed
 * a different note than clicking it opened.
 *
 * Resolution is a path lookup — exact relative path, then unique filename stem
 * — performed by walking the kiln. It deliberately does NOT fall back to the
 * note index: the index matches fuzzily, so `[[Architecture]]` in a kiln
 * without one answers with `Component Architecture`. A link that names no note
 * should render as broken, not silently open a different note.
 */
async function resolveTarget(name: string, kiln?: string): Promise<NotePreview | null> {
  const resolvedKiln = resolveKiln(kiln);
  if (!resolvedKiln) return null;
  try {
    const hit = await resolveNotePath(resolvedKiln, name);
    return { title: hit.title ?? name, path: hit.path, absPath: hit.absolutePath };
  } catch {
    return null;
  }
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
    const hit = await resolveTarget(name, kiln);
    if (!hit) throw new Error(`not found: ${name}`);
    openFileInEditor(hit.absPath, hit.title);
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
  const resolvedKiln = resolveKiln(kiln);
  if (!resolvedKiln) return null;
  const cacheKey = `${resolvedKiln}:${name.toLowerCase()}`;
  if (previewCache.has(cacheKey)) {
    return previewCache.get(cacheKey) ?? null;
  }

  // Same ladder as opening — literally the same function, so a preview can
  // never describe a different note than the click will open.
  const preview = await resolveTarget(name, resolvedKiln);

  if (previewCache.size >= PREVIEW_CACHE_MAX) {
    previewCache.clear();
  }
  previewCache.set(cacheKey, preview);
  return preview;
}

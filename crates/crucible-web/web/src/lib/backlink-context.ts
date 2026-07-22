/**
 * Locate the block in a linking note that references a target note — the
 * context snippet shown under Backlinks entries, and the scroll target for
 * hover previews ("show me WHERE this note links to the focused one").
 */

const WIKILINK_RE = /\[\[([^\]|#]+)(?:#[^\]|]*)?(?:\|[^\]]*)?\]\]/g;

/** Case-insensitive match of a wikilink target against a note's identities
 * (file stem, kiln-relative path). Wikilinks may use either, with or
 * without directories. */
export function wikilinkTargetMatches(target: string, keys: string[]): boolean {
  const t = target.trim().toLowerCase();
  const tStem = t.split('/').pop() ?? t;
  return keys.some((key) => {
    const k = key.trim().toLowerCase().replace(/\.md$/, '');
    if (!k) return false;
    const kStem = k.split('/').pop() ?? k;
    return t === k || tStem === kStem;
  });
}

export interface LinkingBlock {
  /** The referencing line, cleaned of list/quote markers, clamped. */
  snippet: string;
  /** 1-based line number of the reference in the linking note. */
  line: number;
}

function clampSnippet(raw: string): string {
  let snippet = raw.replace(/^\s*(?:[-*+]|\d+\.|>)\s*/, '').trim();
  if (snippet.length > 200) {
    // Clamp around the link so it stays visible in the excerpt.
    const at = Math.max(0, snippet.indexOf('[[') - 60);
    snippet = (at > 0 ? '…' : '') + snippet.slice(at, at + 200) + '…';
  }
  return snippet;
}

/**
 * The block containing a BYTE offset — the daemon's link index stores each
 * occurrence's byte span, so backlink rows can jump straight to the
 * referencing line without re-scanning for wikilinks. Byte-accurate:
 * offsets are Rust byte positions, not UTF-16 indices.
 */
export function blockAtByteOffset(content: string, byteOffset: number): LinkingBlock | null {
  if (byteOffset < 0) return null;
  const enc = new TextEncoder();
  const lines = content.split('\n');
  let acc = 0;
  for (let i = 0; i < lines.length; i++) {
    const len = enc.encode(lines[i]).length + 1; // + '\n'
    if (byteOffset < acc + len) {
      return { snippet: clampSnippet(lines[i]), line: i + 1 };
    }
    acc += len;
  }
  return null;
}

/**
 * First line in `content` whose wikilink points at any of `targetKeys`.
 * Returns null when the link exists only via alias/embed forms we don't
 * recognize (caller falls back to no snippet).
 */
export function findLinkingBlock(content: string, targetKeys: string[]): LinkingBlock | null {
  const lines = content.split('\n');
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (!line.includes('[[')) continue;
    WIKILINK_RE.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = WIKILINK_RE.exec(line)) !== null) {
      if (wikilinkTargetMatches(m[1], targetKeys)) {
        return { snippet: clampSnippet(line), line: i + 1 };
      }
    }
  }
  return null;
}

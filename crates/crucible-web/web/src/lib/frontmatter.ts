/**
 * Frontmatter extraction + the shared Properties card.
 *
 * One model for BOTH surfaces: the reading view (MarkdownPreview) and the
 * live-preview editor widget render the same card HTML from the same parse,
 * so frontmatter looks identical everywhere (the chat/notes parity rule,
 * applied to properties). Supports YAML (`---`) and TOML (`+++`) delimiters
 * — the daemon parser accepts both, and the web must not render TOML
 * frontmatter as body text.
 *
 * The value parser is deliberately FLAT and conservative: scalar keys,
 * inline arrays, and YAML dash-lists. Anything it doesn't understand
 * (nested tables/maps, multiline strings) yields `entries: null`, and
 * callers fall back to the raw source instead of showing a wrong card.
 */

export type FrontmatterFormat = 'yaml' | 'toml';

export interface FrontmatterEntry {
  key: string;
  /** Scalar rendered as text; arrays render as pill chips. */
  value: string | string[];
}

export interface FrontmatterBlock {
  format: FrontmatterFormat;
  /** The block INCLUDING both delimiter lines. */
  raw: string;
  /** Inner source (between the delimiters). */
  source: string;
  /** Offset into the original content where the body begins. */
  bodyStart: number;
  /** Parsed key/values, or null when the source is beyond the flat parser. */
  entries: FrontmatterEntry[] | null;
}

const DELIMS: Record<FrontmatterFormat, string> = { yaml: '---', toml: '+++' };

/** Detect a frontmatter block at the very start of a document. */
export function extractFrontmatterBlock(content: string): FrontmatterBlock | null {
  for (const format of ['yaml', 'toml'] as const) {
    const delim = DELIMS[format];
    if (!content.startsWith(delim)) continue;
    const firstLineEnd = content.indexOf('\n');
    // Opening line must be the bare delimiter (allow trailing \r/spaces).
    const opening = firstLineEnd === -1 ? content : content.slice(0, firstLineEnd);
    if (opening.trim() !== delim) continue;
    if (firstLineEnd === -1) return null;

    // Find the closing delimiter on its own line.
    const closeRe = new RegExp(`^\\${delim[0]}{3}\\s*$`, 'm');
    const rest = content.slice(firstLineEnd + 1);
    const m = closeRe.exec(rest);
    if (!m || m.index === undefined) return null;

    const source = rest.slice(0, m.index).replace(/\n$/, '');
    const closeLineEnd = rest.indexOf('\n', m.index);
    const bodyStart =
      firstLineEnd + 1 + (closeLineEnd === -1 ? rest.length : closeLineEnd + 1);
    return {
      format,
      raw: content.slice(0, bodyStart),
      source,
      bodyStart,
      entries: parseFrontmatterEntries(source, format),
    };
  }
  return null;
}

const unquote = (s: string): string => {
  const t = s.trim();
  if (
    (t.startsWith('"') && t.endsWith('"') && t.length >= 2) ||
    (t.startsWith("'") && t.endsWith("'") && t.length >= 2)
  ) {
    return t.slice(1, -1);
  }
  return t;
};

/** `[a, "b c", d]` → items; null when it isn't a flat inline array. */
function parseInlineArray(s: string): string[] | null {
  const t = s.trim();
  if (!t.startsWith('[') || !t.endsWith(']')) return null;
  const inner = t.slice(1, -1).trim();
  if (inner === '') return [];
  if (inner.includes('[') || inner.includes('{')) return null; // nested
  return inner.split(',').map(unquote).filter((x) => x !== '');
}

/**
 * Flat key/value parse. Returns null when any line falls outside the
 * supported shapes — callers then show raw source rather than lie.
 */
export function parseFrontmatterEntries(
  source: string,
  format: FrontmatterFormat,
): FrontmatterEntry[] | null {
  const entries: FrontmatterEntry[] = [];
  const lines = source.split('\n');
  const sep = format === 'toml' ? '=' : ':';
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    const trimmed = line.trim();
    // Blank lines and comments are fine anywhere.
    if (trimmed === '' || trimmed.startsWith('#')) {
      i++;
      continue;
    }
    // TOML tables / nested structures → beyond us.
    if (format === 'toml' && trimmed.startsWith('[')) return null;

    const sepIdx = line.indexOf(sep);
    if (sepIdx <= 0 || /^\s/.test(line)) return null; // indented = nested yaml
    const key = line.slice(0, sepIdx).trim();
    let valueText = line.slice(sepIdx + 1).trim();
    if (!/^[A-Za-z0-9_.-]+$/.test(key)) return null;

    // YAML dash-list under a bare key.
    if (format === 'yaml' && valueText === '') {
      const items: string[] = [];
      let j = i + 1;
      while (j < lines.length && /^\s+-\s+/.test(lines[j])) {
        items.push(unquote(lines[j].replace(/^\s+-\s+/, '')));
        j++;
      }
      // A bare key with no list under it (multiline block etc.) → bail.
      if (items.length === 0) return null;
      entries.push({ key, value: items });
      i = j;
      continue;
    }

    // Strip trailing same-line comments outside quotes (best-effort: only
    // when the value isn't quoted).
    if (!/^["']/.test(valueText)) {
      const hash = valueText.indexOf(' #');
      if (hash !== -1) valueText = valueText.slice(0, hash).trim();
    }

    const arr = parseInlineArray(valueText);
    if (arr) {
      entries.push({ key, value: arr });
    } else if (valueText.includes('{')) {
      return null; // inline tables/maps
    } else {
      entries.push({ key, value: unquote(valueText) });
    }
    i++;
  }
  return entries;
}

const escapeHtml = (s: string): string =>
  s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');

/**
 * The Properties card. Same HTML in the reading view and the live-preview
 * widget; styled by `.fm-card` in index.css. All text is escaped here — the
 * output is safe to assign via innerHTML.
 *
 * Collapsed by default, and built from native `<details>`/`<summary>` rather
 * than a toggle with a click handler. Both surfaces inject this as a string
 * of HTML, not as a component tree, so anything needing JS would have to be
 * wired up twice — once in the reading view and again in the CodeMirror
 * widget — and stay wired as they change. `<details>` toggles itself, keeps
 * keyboard and screen-reader behaviour for free, and survives being
 * re-rendered from scratch.
 */
export function renderFrontmatterCardHtml(entries: FrontmatterEntry[]): string {
  const rows = entries
    .map(({ key, value }) => {
      const val = Array.isArray(value)
        ? value.map((v) => `<span class="fm-pill">${escapeHtml(v)}</span>`).join('')
        : `<span class="fm-text">${escapeHtml(value)}</span>`;
      return `<div class="fm-row"><span class="fm-key">${escapeHtml(key)}</span><span class="fm-val">${val}</span></div>`;
    })
    .join('');
  const count = entries.length;
  const label = `${count} ${count === 1 ? 'property' : 'properties'}`;
  // Collapsed, this is a one-line row across the content column: caret then the
  // count. It replaced a shrink-wrapped square floated into the top-right
  // corner, which collided with the editor's mode toggles — the two were
  // anchored to different right edges (the centred content column vs the pane),
  // so they converged as the window narrowed. A full-width row costs nothing
  // vertically: the square was already 18px tall in a 20.8px line, and it is
  // the padded multi-line block in other editors that wastes space, not the bar.
  //
  // The label is visible text now, so `title`/`aria-label` are gone with it —
  // as a tooltip and an accessible-name override they only duplicated it.
  return (
    `<details class="fm-card" data-testid="fm-card">` +
    `<summary class="fm-summary" data-testid="fm-summary">` +
    `<span class="fm-caret" aria-hidden="true"></span>` +
    `<span class="fm-count">${escapeHtml(label)}</span>` +
    `</summary>` +
    `<div class="fm-rows">${rows}</div>` +
    `</details>`
  );
}

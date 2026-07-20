/**
 * Obsidian-style callouts: `> [!note] Optional title` blockquotes rendered
 * as colored admonition blocks. One canonical variant table drives every
 * surface — the markdown-it transform (reading mode / chat), the CSS
 * classes (index.css `.callout`), and the live-preview line styling.
 *
 * Syntax (Obsidian-compatible):
 *   > [!type] Title text        — styled block, custom title
 *   > [!type]                   — title defaults to the capitalized type
 *   > [!type]- Title            — foldable, collapsed (renders <details>)
 *   > [!type]+ Title            — foldable, open
 * Unknown types fall back to `note` styling (matching Obsidian).
 */
import type MarkdownIt from 'markdown-it';

/** Canonical callout kinds. Aliases resolve into these. */
export const CALLOUT_KINDS = [
  'note',
  'abstract',
  'info',
  'todo',
  'tip',
  'success',
  'question',
  'warning',
  'failure',
  'danger',
  'bug',
  'example',
  'quote',
] as const;
export type CalloutKind = (typeof CALLOUT_KINDS)[number];

const ALIASES: Record<string, CalloutKind> = {
  summary: 'abstract',
  tldr: 'abstract',
  hint: 'tip',
  important: 'tip',
  check: 'success',
  done: 'success',
  help: 'question',
  faq: 'question',
  caution: 'warning',
  attention: 'warning',
  fail: 'failure',
  missing: 'failure',
  error: 'danger',
  cite: 'quote',
};

/** Accent color per kind as `r, g, b` (usable in rgb()/rgba()). Mirrors the
 * values hard-coded in index.css `.callout[data-callout=…]` — keep in sync. */
export const CALLOUT_RGB: Record<CalloutKind, string> = {
  note: '96, 165, 250',
  abstract: '45, 212, 191',
  info: '96, 165, 250',
  todo: '96, 165, 250',
  tip: '45, 212, 191',
  success: '123, 196, 127',
  question: '212, 167, 44',
  warning: '237, 137, 54',
  failure: '239, 68, 68',
  danger: '239, 68, 68',
  bug: '239, 68, 68',
  example: '167, 139, 218',
  quote: '152, 147, 158',
};

/** Resolve a typed callout word (any case, any alias) to its canonical kind. */
export function resolveCalloutKind(raw: string): CalloutKind {
  const lower = raw.toLowerCase();
  if ((CALLOUT_KINDS as readonly string[]).includes(lower)) return lower as CalloutKind;
  return ALIASES[lower] ?? 'note';
}

/** `[!type]± title` on the first line of a blockquote's first paragraph. */
export const CALLOUT_HEAD_RE = /^\[!([a-zA-Z]+)\]([+-])?[ \t]*(.*)$/;

const escapeHtml = (s: string): string =>
  s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');

const capitalize = (s: string): string => (s ? s[0].toUpperCase() + s.slice(1).toLowerCase() : s);

/**
 * markdown-it plugin. Runs after the block pass (before inline), rewriting
 * `blockquote_open … close` pairs whose first inline line is a callout head:
 * the blockquote tags become div/details, a title row is injected, and the
 * head line is removed from the body content.
 */
export function calloutPlugin(md: MarkdownIt): void {
  md.core.ruler.after('block', 'crucible_callouts', (state) => {
    const tokens = state.tokens;
    for (let i = 0; i < tokens.length; i++) {
      if (tokens[i].type !== 'blockquote_open') continue;
      const para = tokens[i + 1];
      const inline = tokens[i + 2];
      if (!para || para.type !== 'paragraph_open' || !inline || inline.type !== 'inline') {
        continue;
      }
      const lines = inline.content.split('\n');
      const m = CALLOUT_HEAD_RE.exec(lines[0]);
      if (!m) continue;

      const kind = resolveCalloutKind(m[1]);
      const fold = m[2] as '+' | '-' | undefined;
      const title = m[3].trim() || capitalize(m[1]);

      // Find the matching close at this nesting level.
      let depth = 0;
      let closeIdx = -1;
      for (let j = i; j < tokens.length; j++) {
        if (tokens[j].type === 'blockquote_open') depth++;
        else if (tokens[j].type === 'blockquote_close' && --depth === 0) {
          closeIdx = j;
          break;
        }
      }
      if (closeIdx === -1) continue;

      const tag = fold ? 'details' : 'div';
      const open = tokens[i];
      open.tag = tag;
      open.attrJoin('class', 'callout');
      open.attrSet('data-callout', kind);
      if (fold === '+') open.attrSet('open', '');
      tokens[closeIdx].tag = tag;

      const titleTag = fold ? 'summary' : 'div';
      const titleTok = new state.Token('html_block', '', 0);
      titleTok.content =
        `<${titleTag} class="callout-title">` +
        `<span class="callout-icon" aria-hidden="true"></span>` +
        `<span class="callout-title-text">${escapeHtml(title)}</span>` +
        `</${titleTag}>\n`;

      // Drop the head line from the body; drop the whole first paragraph if
      // the head was all it contained.
      const rest = lines.slice(1).join('\n');
      if (rest.trim() === '') {
        tokens.splice(i + 1, 3, titleTok);
      } else {
        inline.content = rest;
        tokens.splice(i + 1, 0, titleTok);
      }
    }
  });
}

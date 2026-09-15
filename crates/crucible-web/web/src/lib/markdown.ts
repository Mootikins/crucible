import MarkdownIt from 'markdown-it';
import DOMPurify from 'dompurify';
import { initializeHighlighter, SHIKI_THEMES } from './shiki';
import { theme } from './theme';
import { calloutPlugin } from './callouts';
import { mathPlugin } from './math';
import { fitMermaidViewBox, renderMermaid } from './mermaid';
import { noteStem } from './markdown-path';

/**
 * Fresh global regex matching `[[wikilink]]` bodies (capture group 1 = inner
 * text). A factory, not a shared literal, because `/g` regexes carry mutable
 * `lastIndex` — callers that `matchAll`/`exec` need their own instance.
 */
export const wikilinkRe = (): RegExp => /\[\[([^[\]\n]+)\]\]/g;

const WIKILINK_PATTERN = wikilinkRe();

/**
 * Split a raw wikilink inner text into its resolution target and display text.
 * `[[Note|alias]]` displays "alias" but resolves "Note"; heading/block
 * fragments (`#heading`, `#^block`) are shown but stripped from the target.
 *
 * A markdown extension is part of the TARGET and not part of the TITLE.
 * `[[Getting Started.md]]` resolves the file and reads "Getting Started",
 * which is what the note calls itself and what every other surface — the
 * files panel, the tab, a backlink row — shows. An explicit alias is the
 * author's own words, so it is never stemmed.
 */
export function parseWikilinkInner(inner: string): { target: string; display: string } {
  const [rawTarget, ...aliasParts] = inner.split('|');
  const hash = rawTarget.indexOf('#');
  const target = (hash < 0 ? rawTarget : rawTarget.slice(0, hash)).trim();
  const fragment = hash < 0 ? '' : rawTarget.slice(hash).trim();
  const display =
    aliasParts.length > 0 ? aliasParts.join('|').trim() : `${noteStem(target)}${fragment}`;
  return { target, display };
}
/**
 * Escape user-authored text and turn `[[wikilinks]]` into `.wikilink` anchors,
 * WITHOUT the full markdown pipeline. User bubbles show text verbatim, but a
 * link the user just inserted should still read as (and be) a knowledge link.
 */
export function renderPlainWithWikilinks(content: string): string {
  const re = wikilinkRe();
  let out = '';
  let last = 0;
  let m: RegExpExecArray | null;
  while ((m = re.exec(content)) !== null) {
    out += escapeHtml(content.slice(last, m.index));
    const { target, display } = parseWikilinkInner(m[1]);
    out += `<a class="wikilink" href="#" data-note="${escapeHtml(target)}">${escapeHtml(display)}</a>`;
    last = m.index + m[0].length;
  }
  out += escapeHtml(content.slice(last));
  return out;
}

const CODE_BLOCK_PATTERN = /<pre><code(?: class="language-([^"]+)")?>([\s\S]*?)<\/code><\/pre>/g;

let markdownRenderer: MarkdownIt | null = null;
let docRenderer: MarkdownIt | null = null;

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

function decodeHtml(value: string): string {
  const parser = new DOMParser();
  const doc = parser.parseFromString(value, 'text/html');
  return doc.documentElement.textContent ?? '';
}

/**
 * Inline CSS is a scripting-free injection primitive. `position:fixed;inset:0`
 * paints a full-viewport surface INSIDE the trusted chrome (a login prompt that
 * looks like ours), and `background:url(https://…)` beacons on render — no
 * `<script>`, no event handler, nothing a "no XSS" review looks for. The
 * content need not be ours: an agent saves a fetched page into the kiln and the
 * reading view renders it as a document (`html: true`).
 *
 * DOMPurify allows `style` BY DEFAULT and never inspects its value — `style` is
 * even in its `URI_SAFE_ATTRIBUTES` list, so `url()` is not URL-checked. Taking
 * `'style'` out of `ADD_ATTR` therefore changes nothing; this filter is the
 * enforcement point, and it is a positive allowlist of the presentational
 * properties our own renderers emit:
 *
 *   shiki  — `color`, `background-color`, `font-style`
 *   KaTeX  — box metrics (`height`, `vertical-align`, `top`, `left`, margins,
 *            borders…)
 *
 * Everything else is dropped: an unknown property, a property name hidden
 * behind a CSS escape or comment (exact match only, so neither matches), and
 * any value carrying a `(`, a backslash or a quote — which is how `url()`,
 * `expression()` and `image-set()` would re-enter.
 *
 * What the surviving set can and cannot do, precisely — an overclaiming comment
 * here is how the next reader skips the check that actually matters:
 *
 * - Nothing fetches: no value may contain `(`, so there is no `url()`.
 * - Nothing is POSITIONED. `position` is excluded, so `top`/`bottom`/`left` are
 *   inert offsets on a statically positioned box. They are load-bearing for
 *   KaTeX, which positions those boxes `relative`/`absolute` BY CLASS in its own
 *   stylesheet: `.accent-body` is `width:0;position:relative`, and the inline
 *   `left:-0.2077em` is the whole of what lands an accent over its base glyph.
 * - Nothing moves VERTICALLY. Only the horizontal margins are listed; the
 *   `margin` SHORTHAND is not, because a negative `margin-top` drags an element
 *   up over the content above it with no `position` involved.
 * - Nothing reaches VIEWPORT scale — see SAFE_STYLE_VALUE.
 *
 * What survives is a box that can be wider, taller, or nudged sideways inside
 * its containing block. That is defacement, not a credential prompt.
 *
 * Two declarations our own renderers emit are deliberately excluded, both
 * costing well under a pixel because KaTeX's stylesheet already handles the
 * same job by class: `position:relative` on integral limits (paired with a
 * 0.001em `top`), and the `margin:0 -0.02em` that centres an array's `|`
 * column separator on the column boundary by half a rule thickness.
 */
const SAFE_STYLE_PROPERTIES = new Set([
  // shiki's dual-theme output. A CUSTOM property is allowed here, by exact
  // name and by exact name only: `--shiki-light` is read by one rule in
  // index.css, which assigns it to `color` inside `.shiki`. So the widest
  // thing a hostile document buys with it is the color of its own code text —
  // strictly less than the plain `color` two lines below already grants. No
  // other custom property matches, so a document cannot redefine a shell token.
  '--shiki-light',
  '--shiki-light-font-style',
  'background-color',
  'border-bottom-width',
  'border-right-style',
  'border-right-width',
  'border-style',
  'border-top-width',
  'border-width',
  'bottom',
  'color',
  'font-style',
  'height',
  'left',
  'margin-left',
  'margin-right',
  'min-width',
  'padding-left',
  'top',
  'vertical-align',
  'width',
]);

/** Lengths (`-0.686em`, `0 -0.02em`), hex colours, and bare keywords only. */
const SAFE_STYLE_VALUE = /^[#a-z0-9%. +-]+$/i;

/**
 * Viewport- and container-relative length units, which no renderer here emits
 * (KaTeX and shiki speak `em`, percentages and bare numbers). They are the one
 * way a value that otherwise reads as an ordinary length reaches PAGE scale:
 * `width:100vw`, `height:100vh` and `margin-left:-100vw` all satisfy
 * SAFE_STYLE_VALUE on their own. Container-query units are included because
 * with no containment ancestor they resolve against the small viewport too.
 */
const VIEWPORT_RELATIVE_UNIT = /[\d.](?:[dsl]?v|cq)(?:w|h|i|b|min|max)\b/i;

function filterInlineCss(css: string): string {
  const kept: string[] = [];
  for (const declaration of css.split(';')) {
    const colon = declaration.indexOf(':');
    if (colon === -1) continue;
    const property = declaration.slice(0, colon).trim().toLowerCase();
    const value = declaration.slice(colon + 1).trim();
    if (!SAFE_STYLE_PROPERTIES.has(property)) continue;
    if (!SAFE_STYLE_VALUE.test(value) || VIEWPORT_RELATIVE_UNIT.test(value)) continue;
    kept.push(`${property}:${value}`);
  }
  return kept.join(';');
}

/** Options accepted by {@link runDOMPurify}, plus our own opt-out. */
type PurifyOptions = { rawInlineCss?: boolean } & Record<string, unknown>;

type AttributeHook = (
  node: Element,
  data: { attrName: string; attrValue: string; keepAttr: boolean },
  config: PurifyOptions | null,
) => void;

type Purifier = {
  sanitize: (html: string, options?: unknown) => string;
  addHook: (entryPoint: 'uponSanitizeAttribute', hook: AttributeHook) => void;
};

let purifier: Purifier | null = null;

/** The DOMPurify instance every sanitize in this module goes through, with the
 * inline-CSS filter installed. Resolved once; `null` outside a DOM. */
function getPurifier(): Purifier | null {
  if (purifier) return purifier;

  const direct = DOMPurify as unknown as Purifier;
  const resolved =
    typeof direct.sanitize === 'function'
      ? direct
      : typeof window !== 'undefined'
        ? (DOMPurify as unknown as (windowObj: Window) => Purifier)(window)
        : null;
  if (!resolved) return null;

  resolved.addHook('uponSanitizeAttribute', (_node, data, config) => {
    // DOMPurify lowercases attribute names before the hook; do not depend on it.
    if (data.attrName.toLowerCase() !== 'style') return;
    // Mermaid's own SVG opts out: its diagram styling IS inline `fill`/`stroke`
    // and it has already been through mermaid's strict-mode sanitizer. A future
    // DOMPurify that stops passing our config through fails closed here — the
    // opt-out disappears and diagrams lose colour; nothing gains reach.
    if (config?.rawInlineCss) return;
    const filtered = filterInlineCss(data.attrValue);
    if (filtered) data.attrValue = filtered;
    else data.keepAttr = false;
  });

  purifier = resolved;
  return purifier;
}

function runDOMPurify(value: string, options: PurifyOptions): string {
  const instance = getPurifier();
  if (instance) return instance.sanitize(value, options);

  // No DOM (a bare Node import). Nothing here can parse HTML safely, so strip
  // the two constructs unconditionally rather than pretend to sanitize.
  return value
    .replace(/<script[\s\S]*?>[\s\S]*?<\/script>/gi, '')
    .replace(/\sstyle\s*=\s*("[^"]*"|'[^']*'|[^\s>]+)/gi, '');
}

function sanitizeHtml(value: string): string {
  return runDOMPurify(value, {
    // `align` keeps `<p align="center">` (README demo blocks); `data-copy`
    // marks code-block copy buttons for the reading-view click delegate.
    // `data-callout` carries the admonition kind through to the CSS.
    // `data-plugin-*` names which block a ```plugin fence mounts. They are
    // three inert strings read back by mountPluginBlocks; the daemon decides
    // what a plugin may do, so a hand-authored fence naming one it should not
    // reach is refused there, not here.
    // `style` is NOT listed: DOMPurify allows it by default and listing it here
    // read as a decision to permit arbitrary inline CSS. What actually governs
    // it is filterInlineCss.
    ADD_ATTR: [
      'data-note',
      'data-copy',
      // The SOURCE line a task box came from, so a tap can anchor a one-line
      // edit on it. Note content can forge one, and the blast radius is
      // bounded: the handler re-reads the buffer at that index, does nothing
      // unless the line IS a task line, and anchors the edit on that line's
      // real text. The worst a crafted note achieves is ticking a different
      // checkbox in the same file, which the user can see and undo.
      'data-task-line',
      'data-callout',
      'align',
      'data-plugin-name',
      'data-plugin-block',
      'data-plugin-params',
    ],
    // `style`: DOMPurify empties a bare `<style>` (FORBID_CONTENTS) but keeps
    // one nested in `<svg>` intact — and an SVG `<style>` inside an HTML
    // document is NOT scoped to the SVG, its rules apply to the whole page.
    // That is the same overlay/beacon primitive as the style attribute, only
    // unbounded, so the element goes too. Mermaid keeps its own `<style>` on
    // its own sanitize (below); no renderer here emits one.
    // `form`: the exfiltration half of a phishing overlay. Nothing in this app
    // renders a form, and DOMPurify allows one by default — with it gone an
    // injected `<input>` (the task-list checkbox is ours and stays) has nowhere
    // to post. `<a>`/`<img>` remain the only ways off the origin, both visible.
    FORBID_TAGS: ['style', 'form'],
    // DOMPurify allows every `data-*` attribute by default. The document
    // renderer permits raw HTML, and link resolution reads the nearest
    // `data-kiln` ancestor to decide WHICH KILN a link resolves in — so a note
    // could carry its own marker and redirect its links into another kiln,
    // driven purely by content, needing no script. Only the two attributes
    // named above are actually used; everything else data-* is stripped.
    // `data-kiln` is deliberately NOT in that list: no renderer emits it, and
    // it is set only on trusted component wrappers that never pass through
    // here.
    ALLOW_DATA_ATTR: false,
  });
}

/** Sanitize a mermaid-produced SVG. Mermaid already renders with
 * `securityLevel: 'strict'` (its own DOMPurify pass); this is defense in depth
 * for LLM-authored diagrams in chat. The DEFAULT profile is used, not the
 * svg-only one — the svg profile strips the deeply nested `<tspan>`s that hold
 * node-label text (empty labels) — with `<style>` (diagram theming) and
 * `<foreignObject>` (HTML labels) explicitly kept. Scripts/event handlers are
 * still dropped by the default allowlist. */
function sanitizeMermaidSvg(svg: string): string {
  const purify = (value: string) =>
    runDOMPurify(value, {
      ADD_TAGS: ['style', 'foreignObject'],
      // Mermaid's node/edge styling IS inline `fill`/`stroke`/`max-width`, so
      // this path keeps raw inline CSS (see filterInlineCss). It is the one
      // opt-out and it is not reachable from the HTML pipeline.
      rawInlineCss: true,
    });
  // Sanitize BEFORE fitting — fitMermaidViewBox attaches the markup to the
  // document to measure it, so it must never see unsanitized input — and
  // again AFTER, because fitting reparses and re-serializes: that round trip
  // is exactly the shape mXSS exploits, and svg/foreignObject is where it
  // lives. Both passes are cheap next to rendering the diagram.
  return purify(fitMermaidViewBox(purify(svg)));
}

function wikilinkPlugin(md: MarkdownIt): void {
  md.core.ruler.after('inline', 'crucible_wikilinks', (state) => {
    for (const token of state.tokens) {
      if (token.type !== 'inline' || !token.children) {
        continue;
      }

      const nextChildren = [];

      for (const child of token.children) {
        if (child.type !== 'text') {
          nextChildren.push(child);
          continue;
        }

        const text = child.content;
        WIKILINK_PATTERN.lastIndex = 0;
        let lastIndex = 0;
        let match = WIKILINK_PATTERN.exec(text);

        while (match) {
          const [fullMatch, noteName] = match;
          const start = match.index;
          const end = start + fullMatch.length;

          if (start > lastIndex) {
            const textToken = new state.Token('text', '', 0);
            textToken.content = text.slice(lastIndex, start);
            nextChildren.push(textToken);
          }

          const { target, display } = parseWikilinkInner(noteName);
          const safeText = escapeHtml(display);
          const safeAttr = escapeHtml(target);
          const linkToken = new state.Token('html_inline', '', 0);
          linkToken.content = `<a class="wikilink" href="#" data-note="${safeAttr}">${safeText}</a>`;
          nextChildren.push(linkToken);

          lastIndex = end;
          match = WIKILINK_PATTERN.exec(text);
        }

        if (lastIndex < text.length) {
          const textToken = new state.Token('text', '', 0);
          textToken.content = text.slice(lastIndex);
          nextChildren.push(textToken);
        }
      }

      token.children = nextChildren;
    }
  });
}

/**
 * GFM task lists: turn a `- [ ]` / `- [x]` list item into a styled, disabled
 * checkbox. markdown-it renders the brackets as literal text otherwise. The
 * leading marker is stripped from the item's first text run and replaced with
 * an `<input type=checkbox>`; the `<li>` gets `.task-list-item` for CSS.
 */
const TASK_MARKER_RE = /^\[([ xX])\]\s+/;

function taskListPlugin(md: MarkdownIt): void {
  md.core.ruler.after('inline', 'crucible_tasklists', (state) => {
    const tokens = state.tokens;
    for (let i = 0; i < tokens.length; i++) {
      const inline = tokens[i];
      if (inline.type !== 'inline' || !inline.children) continue;
      // The inline must be the first paragraph of a list item.
      if (tokens[i - 1]?.type !== 'paragraph_open') continue;
      if (tokens[i - 2]?.type !== 'list_item_open') continue;
      const m = TASK_MARKER_RE.exec(inline.content);
      if (!m) continue;

      const checked = m[1] !== ' ';
      tokens[i - 2].attrJoin('class', 'task-list-item');
      inline.content = inline.content.slice(m[0].length);
      const firstText = inline.children.find((c) => c.type === 'text');
      if (firstText) firstText.content = firstText.content.replace(TASK_MARKER_RE, '');

      // The SOURCE line the box came from, so a tap can anchor an edit on it
      // (`lib/task-toggle.ts`). `map` is markdown-it's [start, end) in lines.
      const sourceLine = tokens[i - 2].map?.[0];
      const box = new state.Token('html_inline', '', 0);
      // Disabled by default. A view that can WRITE the file enables them —
      // the same preview renders chat messages and hover cards, where an
      // enabled box would look actionable and do nothing.
      box.content = `<input class="task-checkbox" type="checkbox" disabled${
        checked ? ' checked' : ''
      }${sourceLine === undefined ? '' : ` data-task-line="${sourceLine}"`}>`;
      inline.children.unshift(box);
    }
  });
}

async function getShikiHighlighter() {
  return initializeHighlighter();
}

/** Wrap a rendered code block so the reading view can float a copy button
 * over it. The button carries `data-copy`; the click delegate reads the
 * sibling `<pre>`'s text. Kept as markup (not a component) because the whole
 * render is injected via innerHTML. */
function wrapWithCopyButton(pre: string): string {
  return (
    `<div class="md-codeblock">` +
    `<button class="md-copy" data-copy type="button" aria-label="Copy code">Copy</button>` +
    `${pre}</div>`
  );
}

async function highlightCodeBlocks(
  renderedHtml: string,
  opts: { copyButton?: boolean } = {},
): Promise<string> {
  const highlighter = await getShikiHighlighter();
  const matches = [...renderedHtml.matchAll(CODE_BLOCK_PATTERN)];

  if (matches.length === 0) {
    return renderedHtml;
  }

  let result = '';
  let lastIndex = 0;

  for (const match of matches) {
    const [fullMatch, languageClass, encodedCode] = match;
    const index = match.index ?? 0;
    const language = languageClass && languageClass.length > 0 ? languageClass : 'text';

    result += renderedHtml.slice(lastIndex, index);
    lastIndex = index + fullMatch.length;

    // Mermaid diagrams render async and produce SVG, which the strict HTML
    // sanitizer would strip — defer to renderMermaidBlocks, which runs AFTER
    // sanitizeHtml. Emit a placeholder (source kept HTML-escaped) that
    // survives DOMPurify. No shiki, no copy button.
    if (language === 'mermaid') {
      result += `<div class="mermaid-pending">${encodedCode}</div>`;
      continue;
    }

    // A ```plugin fence names a plugin block. It is not rendered here: unlike
    // mermaid, which produces an inert SVG, a block is live and interactive, so
    // it needs a real component rather than a second string pass. Emit a mount
    // point that survives DOMPurify and let mountPluginBlocks (which runs
    // against the DOM, after sanitizing) put a component in it.
    if (language === 'plugin') {
      result += pluginMountHtml(decodeHtml(encodedCode));
      continue;
    }

    const source = decodeHtml(encodedCode);
    let block: string;
    try {
      // BOTH themes in one pass. Markdown renders to an HTML STRING that is
      // cached and inserted once, so it cannot re-run on a theme toggle;
      // shiki's dual output carries the light color of every token as a
      // `--shiki-light` custom property beside the dark one, and index.css
      // swaps them. `defaultColor: 'dark'` keeps the shell's own theme as the
      // plain `color`, so a surface that never opts in is unaffected.
      block = highlighter.codeToHtml(source, {
        lang: language,
        themes: SHIKI_THEMES,
        defaultColor: 'dark',
      });
    } catch {
      block = fullMatch;
    }
    result += opts.copyButton ? wrapWithCopyButton(block) : block;
  }

  result += renderedHtml.slice(lastIndex);
  return result;
}

/** Render mermaid source to a sanitized SVG string (or null on failure).
 * Shared by the reading-view pipeline and the live-preview editor widget so
 * both go through the same render + sanitize path. */
export async function renderMermaidDiagram(code: string): Promise<string | null> {
  const svg = await renderMermaid(code);
  return svg ? sanitizeMermaidSvg(svg) : null;
}

/**
 * Parse a ```plugin fence into a mount point.
 *
 * The body is deliberately tiny — `plugin/block` on the first line, optional
 * JSON params after it:
 *
 * ```plugin
 * kanban/board
 * { "folder": "tickets" }
 * ```
 *
 * A fence is a thing a person types into a note by hand, so the common case is
 * one line. Anything richer belongs in the plugin, not in the fence: the note
 * says WHICH view, and the plugin says what it looks like. That split is the
 * whole point — a note that carried the layout would be a note that rots when
 * the plugin changes.
 *
 * A malformed fence renders as a visible error, never as nothing.
 */
export function pluginMountHtml(source: string): string {
  const lines = source.trim().split('\n');
  const target = (lines[0] ?? '').trim();
  const slash = target.indexOf('/');
  if (slash <= 0 || slash === target.length - 1) {
    return `<pre class="plugin-block-error"><code>${escapeHtml(
      `plugin: first line must be "plugin/view", got ${JSON.stringify(target)}`,
    )}</code></pre>`;
  }
  const plugin = target.slice(0, slash);
  const view = target.slice(slash + 1);

  const rest = lines.slice(1).join('\n').trim();
  let params = '{}';
  if (rest.length > 0) {
    try {
      params = JSON.stringify(JSON.parse(rest));
    } catch {
      return `<pre class="plugin-block-error"><code>${escapeHtml(
        'plugin: params after the first line must be JSON',
      )}</code></pre>`;
    }
  }

  return `<div class="plugin-mount" data-plugin-name="${escapeHtml(plugin)}" data-plugin-block="${escapeHtml(
    view,
  )}" data-plugin-params="${escapeHtml(params)}"></div>`;
}

/** Placeholder emitted by {@link highlightCodeBlocks} for a ```mermaid fence;
 * inner text is the HTML-escaped diagram source. */
const MERMAID_PENDING_PATTERN = /<div class="mermaid-pending">([\s\S]*?)<\/div>/g;

/**
 * Replace mermaid placeholders with rendered, SVG-sanitized diagrams. Runs as
 * the LAST render step (after the strict sanitize) so the injected SVG isn't
 * stripped. A diagram that fails to parse falls back to its source as a code
 * block, so nothing is lost. No placeholders → the input string is returned
 * untouched (and mermaid is never imported).
 */
async function renderMermaidBlocks(html: string): Promise<string> {
  const matches = [...html.matchAll(MERMAID_PENDING_PATTERN)];
  if (matches.length === 0) return html;

  let result = '';
  let lastIndex = 0;
  for (const match of matches) {
    const index = match.index ?? 0;
    result += html.slice(lastIndex, index);
    lastIndex = index + match[0].length;

    const source = decodeHtml(match[1]);
    const svg = await renderMermaid(source);
    result += svg
      ? `<div class="mermaid-diagram">${sanitizeMermaidSvg(svg)}</div>`
      : `<pre class="mermaid-error"><code>${match[1]}</code></pre>`;
  }
  result += html.slice(lastIndex);
  return result;
}

/**
 * The top-level domains a BARE word.tld may auto-link on, replacing
 * linkify-it's default list.
 *
 * A note filename is not a URL. linkify-it appends every two-letter country
 * code to its TLD table, and `md` is Moldova and `ai` is Anguilla — so a turn
 * that said `Getting Started.md` rendered as the text "Getting " plus an
 * anchor to `http://Started.md`, and `Z.AI Setup.md` broke into two anchors.
 * Filenames outnumber bare country-code domains in a knowledge base by a very
 * wide margin, and the false anchor also broke the link treatment: it painted
 * an external link's underline where the reader expected a wikilink pill.
 *
 * Calling `tlds()` at all is what drops the two-letter block — linkify-it only
 * appends it while its list is untouched — so this list is the stock default,
 * verbatim. An explicit scheme is unaffected: `https://example.md` still
 * links, because a scheme is a statement that the text IS a URL.
 */
const FUZZY_LINK_TLDS = (
  'biz|com|edu|gov|net|org|pro|web|xxx|aero|asia|coop|info|museum|name|shop|рф|' +
  // Modern generic domains a bare `example.dev` must still link. The
  // two-letter country codes stay out, `ai` included: `md` and `ai` turned
  // every `Setup.md` and `Z.AI` in prose into a link. A scheme still links.
  'app|cloud|codes|dev|fm|gg|io|link|ly|me|online|page|run|sh|site|so|tech|to|tools|tv|wiki|xyz'
).split('|');

/**
 * `html` passes raw HTML blocks/inline through markdown-it (still
 * DOMPurify-sanitized downstream). Off for chat/hover (LLM/user text should
 * not inject markup); on for the document Reading view, where authored docs
 * like a README legitimately embed HTML (e.g. a centered `<p align="center">`
 * demo).
 *
 * `breaks` turns single newlines into `<br>`. On for chat (a message's line
 * breaks are meaningful); off for documents, where — like GitHub — soft
 * wraps are whitespace, so consecutive badge lines render inline instead of
 * stacked.
 */
function createMarkdownRenderer(
  opts: { html?: boolean; breaks?: boolean } = {},
): MarkdownIt {
  const renderer = new MarkdownIt({
    breaks: opts.breaks ?? true,
    html: opts.html ?? false,
    linkify: true,
    highlight: (code, lang) => {
      const language = lang || 'text';
      const escapedCode = escapeHtml(code);
      return `<pre><code class="language-${escapeHtml(language)}">${escapedCode}</code></pre>`;
    },
  });
  renderer.linkify.tlds(FUZZY_LINK_TLDS, false);

  wikilinkPlugin(renderer);
  calloutPlugin(renderer);
  taskListPlugin(renderer);
  mathPlugin(renderer);
  return renderer;
}

function getRenderer(): MarkdownIt {
  if (!markdownRenderer) {
    markdownRenderer = createMarkdownRenderer();
  }

  return markdownRenderer;
}

function getDocRenderer(): MarkdownIt {
  if (!docRenderer) {
    docRenderer = createMarkdownRenderer({ html: true, breaks: false });
  }

  return docRenderer;
}

export function renderMarkdown(content: string): string {
  const renderedHtml = getRenderer().render(content);
  return sanitizeHtml(renderedHtml);
}

/** Join a relative POSIX path onto a base dir, resolving `.`/`..`. */
function resolvePath(baseDir: string, rel: string): string {
  const out: string[] = [];
  for (const part of `${baseDir}/${rel}`.split('/')) {
    if (part === '' || part === '.') continue;
    if (part === '..') out.pop();
    else out.push(part);
  }
  return `/${out.join('/')}`;
}

/**
 * Resolve a markdown image `src` to a URL the browser can load. Absolute URLs
 * (http/https/data/blob), already-API paths, and site-absolute paths pass
 * through; a path relative to the document (e.g. a README's `assets/demo.gif`)
 * is resolved against `baseDir` and routed through the raw project-file
 * endpoint. Returns `null` when a relative src can't be resolved (no baseDir).
 */
export function rawImageUrl(src: string, baseDir?: string): string | null {
  if (/^(https?:|data:|blob:|\/)/i.test(src)) return src;
  if (!baseDir) return null;
  return `/api/file/raw?path=${encodeURIComponent(resolvePath(baseDir, src))}`;
}

/**
 * Sanitize an already-HTML fragment and resolve its relative image srcs
 * against `baseDir`. For rendering a raw HTML block (e.g. a README's centered
 * `<p align="center">` demo) that is HTML, not markdown — no markdown parse.
 */
export function sanitizeDocHtml(raw: string, baseDir?: string): string {
  return sanitizeHtml(resolveDocImages(raw, baseDir));
}

/** Rewrite relative `<img src>` in rendered HTML to loadable URLs. */
function resolveDocImages(html: string, baseDir?: string): string {
  return html.replace(/(<img\b[^>]*?\bsrc=")([^"]*)(")/gi, (whole, pre, src, post) => {
    const url = rawImageUrl(src, baseDir);
    return url ? `${pre}${url}${post}` : whole;
  });
}

/**
 * Reading-view render: async pipeline (syntax highlighting + mermaid) that
 * permits embedded HTML (sanitized), floats a copy button over each code
 * block, and resolves
 * relative image srcs against `baseDir` (the document's directory) so local
 * images load. For rendering whole documents (notes, project READMEs) rather
 * than chat turns.
 */
export async function renderMarkdownDocAsync(
  content: string,
  baseDir?: string,
): Promise<string> {
  const renderedHtml = getDocRenderer().render(content);
  const highlightedHtml = await highlightCodeBlocks(renderedHtml, { copyButton: true });
  return renderMermaidBlocks(sanitizeHtml(resolveDocImages(highlightedHtml, baseDir)));
}

/**
 * Chat-turn render with the SAME presentation as the note reading view
 * (`.md-codeblock` code blocks with copy buttons, identical downstream CSS)
 * — an agent writing the same markdown into a message or a note must get the
 * same rendering. Only the semantics differ from the doc pipeline: raw HTML
 * stays off (LLM/user text must not inject markup) and single newlines break
 * (a message's line breaks are meaningful).
 */
export async function renderMarkdownChatAsync(content: string): Promise<string> {
  const renderedHtml = getRenderer().render(content);
  const highlightedHtml = await highlightCodeBlocks(renderedHtml, { copyButton: true });
  return renderMermaidBlocks(sanitizeHtml(highlightedHtml));
}

/**
 * The one prose class both the note reading view and chat turns use — a
 * single source of visual truth so message rendering can never drift from
 * note rendering. `prose-hr:my-3` and the heading margins keep the vertical
 * rhythm tight (stock prose-sm leaves large dead bands around `---` and
 * headings).
 *
 * Theme-INDEPENDENT: every color here is a shell token, which follows the root
 * attribute on its own. The one part that cannot is `prose-invert`, which the
 * typography plugin implements by swapping its own greys — see
 * {@link proseClass}.
 */
const PROSE_CLASS = [
  // IDE-native reading scale: `--text-reading` (12px) at `--leading-reading`
  // (1.6), em-based headings so the whole scale tracks the root size. Tight
  // vertical rhythm — no dead bands around headings/rules/lists — reads dense
  // but calm on the near-black panel, matching a code editor's own density.
  //
  // The size is a TOKEN, not a literal. It used to be `text-[13px]` here and
  // `13px` again in `.user-quote`, and the two were the same number by
  // coincidence — a reading-size change had to find both. Both now read
  // `--text-reading` from the `@theme` block in index.css.
  'prose max-w-none text-reading leading-reading',
  'prose-headings:text-shell-ink prose-headings:font-semibold prose-headings:mt-3.5 prose-headings:mb-1.5',
  'prose-h1:text-[1.45em] prose-h2:text-[1.25em] prose-h3:text-[1.1em] prose-h4:text-[1em]',
  'prose-p:my-2 prose-p:leading-[1.6]',
  'prose-hr:my-4 prose-hr:border-hairline',
  'prose-a:text-primary prose-a:no-underline hover:prose-a:underline',
  // pre bg is enforced in index.css (.prose pre) — shiki inlines its own.
  // A fenced block sits at the reading size too: it is the same 12px, and a
  // code block that shrinks away from the prose around it reads as an aside.
  'prose-pre:bg-surface-elevated prose-pre:rounded-md prose-pre:p-3 prose-pre:text-reading prose-pre:leading-[1.5]',
  'prose-code:bg-surface-elevated prose-code:px-1 prose-code:rounded prose-code:text-[0.9em] prose-code:before:content-none prose-code:after:content-none',
  'prose-ul:my-2 prose-ol:my-2 prose-li:my-0.5 prose-li:leading-[1.6]',
  'prose-blockquote:border-l-2 prose-blockquote:border-hairline prose-blockquote:pl-3 prose-blockquote:italic prose-blockquote:text-muted',
  'prose-strong:text-shell-ink',
].join(' ');

/**
 * {@link PROSE_CLASS}, plus `prose-invert` in the dark theme.
 *
 * The typography plugin does not read our tokens: `prose-invert` swaps
 * `--tw-prose-body` and friends to ITS OWN light greys, so the class left on
 * unconditionally painted #d1d5db body text and white inline code onto a white
 * ground — the one part of a chat turn the light theme could not reach.
 *
 * A function, not a constant, because it has to re-run on a theme switch; the
 * call sites read it in a `class={}` binding, which Solid keeps reactive.
 */
export function proseClass(): string {
  return theme() === 'light' ? PROSE_CLASS : `${PROSE_CLASS} prose-invert`;
}

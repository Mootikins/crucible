import MarkdownIt from 'markdown-it';
import DOMPurify from 'dompurify';
import { initializeHighlighter, SHIKI_THEME } from './shiki';
import { calloutPlugin } from './callouts';

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
 */
export function parseWikilinkInner(inner: string): { target: string; display: string } {
  const [rawTarget, ...aliasParts] = inner.split('|');
  const display = aliasParts.length > 0 ? aliasParts.join('|').trim() : inner.trim();
  const target = (rawTarget.split('#')[0] ?? rawTarget).trim();
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

function sanitizeHtml(value: string): string {
  const sanitizeOptions = {
    // `align` keeps `<p align="center">` (README demo blocks); `data-copy`
    // marks code-block copy buttons for the reading-view click delegate.
    ADD_ATTR: ['data-note', 'data-copy', 'style', 'align'],
  };

  const directSanitize = (DOMPurify as { sanitize?: (html: string, options?: unknown) => string })
    .sanitize;
  if (typeof directSanitize === 'function') {
    return directSanitize(value, sanitizeOptions);
  }

  const domPurifyFactory = DOMPurify as unknown as ((windowObj: Window) => {
    sanitize: (html: string, options?: unknown) => string;
  });

  if (typeof window !== 'undefined') {
    return domPurifyFactory(window).sanitize(value, sanitizeOptions);
  }

  return value.replace(/<script[\s\S]*?>[\s\S]*?<\/script>/gi, '');
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

      const box = new state.Token('html_inline', '', 0);
      box.content = `<input class="task-checkbox" type="checkbox" disabled${
        checked ? ' checked' : ''
      }>`;
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
    const source = decodeHtml(encodedCode);

    result += renderedHtml.slice(lastIndex, index);

    let block: string;
    try {
      block = highlighter.codeToHtml(source, { lang: language, theme: SHIKI_THEME });
    } catch {
      block = fullMatch;
    }
    result += opts.copyButton ? wrapWithCopyButton(block) : block;

    lastIndex = index + fullMatch.length;
  }

  result += renderedHtml.slice(lastIndex);
  return result;
}

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
export function createMarkdownRenderer(
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

  wikilinkPlugin(renderer);
  calloutPlugin(renderer);
  taskListPlugin(renderer);
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

export async function renderMarkdownAsync(content: string): Promise<string> {
  const renderedHtml = getRenderer().render(content);
  const highlightedHtml = await highlightCodeBlocks(renderedHtml);
  return sanitizeHtml(highlightedHtml);
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
 * Reading-view render: like {@link renderMarkdownAsync} but permits embedded
 * HTML (sanitized), floats a copy button over each code block, and resolves
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
  return sanitizeHtml(resolveDocImages(highlightedHtml, baseDir));
}

export async function initializeMarkdownHighlighter(): Promise<void> {
  await initializeHighlighter();
}

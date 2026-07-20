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
 * `allowHtml` passes raw HTML blocks/inline through markdown-it (still
 * DOMPurify-sanitized downstream). Off for chat/hover (LLM/user text should
 * not inject markup); on for the document Reading view, where authored docs
 * like a README legitimately embed HTML (e.g. a centered `<p align="center">`
 * demo).
 */
export function createMarkdownRenderer(allowHtml = false): MarkdownIt {
  const renderer = new MarkdownIt({
    breaks: true,
    html: allowHtml,
    linkify: true,
    highlight: (code, lang) => {
      const language = lang || 'text';
      const escapedCode = escapeHtml(code);
      return `<pre><code class="language-${escapeHtml(language)}">${escapedCode}</code></pre>`;
    },
  });

  wikilinkPlugin(renderer);
  calloutPlugin(renderer);
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
    docRenderer = createMarkdownRenderer(true);
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

/**
 * Reading-view render: like {@link renderMarkdownAsync} but permits embedded
 * HTML (sanitized) and floats a copy button over each code block. For rendering
 * whole documents (notes, project READMEs) rather than chat turns.
 */
export async function renderMarkdownDocAsync(content: string): Promise<string> {
  const renderedHtml = getDocRenderer().render(content);
  const highlightedHtml = await highlightCodeBlocks(renderedHtml, { copyButton: true });
  return sanitizeHtml(highlightedHtml);
}

export async function initializeMarkdownHighlighter(): Promise<void> {
  await initializeHighlighter();
}

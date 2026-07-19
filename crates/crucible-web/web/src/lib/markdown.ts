import MarkdownIt from 'markdown-it';
import DOMPurify from 'dompurify';
import { initializeHighlighter, SHIKI_THEME } from './shiki';

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
    ADD_ATTR: ['data-note', 'style'],
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

async function highlightCodeBlocks(renderedHtml: string): Promise<string> {
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

    try {
      result += highlighter.codeToHtml(source, {
        lang: language,
        theme: SHIKI_THEME,
      });
    } catch {
      result += fullMatch;
    }

    lastIndex = index + fullMatch.length;
  }

  result += renderedHtml.slice(lastIndex);
  return result;
}

export function createMarkdownRenderer(): MarkdownIt {
  const renderer = new MarkdownIt({
    breaks: true,
    html: false,
    linkify: true,
    highlight: (code, lang) => {
      const language = lang || 'text';
      const escapedCode = escapeHtml(code);
      return `<pre><code class="language-${escapeHtml(language)}">${escapedCode}</code></pre>`;
    },
  });

  wikilinkPlugin(renderer);
  return renderer;
}

function getRenderer(): MarkdownIt {
  if (!markdownRenderer) {
    markdownRenderer = createMarkdownRenderer();
  }

  return markdownRenderer;
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

export async function initializeMarkdownHighlighter(): Promise<void> {
  await initializeHighlighter();
}

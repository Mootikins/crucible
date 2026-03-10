import MarkdownIt from 'markdown-it';
import DOMPurify from 'dompurify';
import { createHighlighter } from 'shiki';

const WIKILINK_PATTERN = /\[\[([^\[\]\n]+)\]\]/g;
const CODE_BLOCK_PATTERN = /<pre><code(?: class="language-([^"]+)")?>([\s\S]*?)<\/code><\/pre>/g;
const SHIKI_THEME = 'github-dark';

let markdownRenderer: MarkdownIt | null = null;
let highlighterPromise: ReturnType<typeof createHighlighter> | null = null;

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

          const trimmedName = noteName.trim();
          const safeText = escapeHtml(trimmedName);
          const safeAttr = escapeHtml(trimmedName);
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
  if (!highlighterPromise) {
    highlighterPromise = createHighlighter({
      themes: [SHIKI_THEME],
      langs: ['plaintext', 'text', 'bash', 'json', 'markdown', 'rust', 'typescript', 'javascript'],
    });
  }

  return highlighterPromise;
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
  await getShikiHighlighter();
}

/**
 * Markdown rendering with syntax highlighting
 */

import { marked } from 'marked';
import hljs from 'highlight.js';

// Configure marked with syntax highlighting
marked.setOptions({
  gfm: true,
  breaks: true,
});

// Custom renderer for code blocks with highlighting
const renderer = new marked.Renderer();

renderer.code = ({ text, lang }: { text: string; lang?: string }) => {
  const language = lang && hljs.getLanguage(lang) ? lang : 'plaintext';
  const highlighted = hljs.highlight(text, { language }).value;
  return `<pre><code class="hljs language-${language}">${highlighted}</code></pre>`;
};

marked.use({ renderer });

/**
 * Render markdown to HTML
 */
export function renderMarkdown(content: string): string {
  return marked.parse(content) as string;
}

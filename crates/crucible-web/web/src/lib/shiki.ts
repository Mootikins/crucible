import { createSignal } from 'solid-js';
import { createHighlighter, type Highlighter } from 'shiki';

export const SHIKI_THEME = 'github-dark';

// Eager lang set. Loaded once at boot. ~+200KB gzip vs the previous narrow set;
// matches the long tail of languages a coding agent realistically edits.
export const SHIKI_LANGS = [
  'plaintext',
  'text',
  'bash',
  'sh',
  'json',
  'yaml',
  'toml',
  'markdown',
  'rust',
  'typescript',
  'javascript',
  'tsx',
  'jsx',
  'python',
  'go',
  'html',
  'css',
  'sql',
] as const;

let highlighterPromise: Promise<Highlighter> | null = null;

// Reactive accessor for the loaded Highlighter. Returns null until
// initializeHighlighter() resolves. Components that read this in a reactive
// scope (e.g. inside a Solid render) will re-render when init completes.
const [highlighter, setHighlighter] = createSignal<Highlighter | null>(null);
export { highlighter };

export async function initializeHighlighter(): Promise<Highlighter> {
  if (!highlighterPromise) {
    highlighterPromise = createHighlighter({
      themes: [SHIKI_THEME],
      langs: [...SHIKI_LANGS],
    }).then((h) => {
      setHighlighter(h);
      return h;
    });
  }
  return highlighterPromise;
}

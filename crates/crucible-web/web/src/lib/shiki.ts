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
let highlighterInstance: Highlighter | null = null;

export async function initializeHighlighter(): Promise<Highlighter> {
  if (!highlighterPromise) {
    highlighterPromise = createHighlighter({
      themes: [SHIKI_THEME],
      langs: [...SHIKI_LANGS],
    }).then((h) => {
      highlighterInstance = h;
      return h;
    });
  }
  return highlighterPromise;
}

/** Returns the loaded Highlighter, or null if initializeHighlighter hasn't resolved yet. */
export function getHighlighter(): Highlighter | null {
  return highlighterInstance;
}

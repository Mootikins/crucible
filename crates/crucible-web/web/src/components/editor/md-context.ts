/**
 * Syntax-tree context checks for markdown editors. Wikilink treatment is a
 * PROSE feature — `[[...]]` inside code contexts is code (TOML array-of-
 * tables headers like `[[mcp.upstreams]]` being the canonical false
 * positive), and inside frontmatter it's YAML.
 */
import { syntaxTree } from '@codemirror/language';
import type { EditorState } from '@codemirror/state';

const CODE_CONTEXTS = new Set([
  'FencedCode',
  'CodeBlock',
  'InlineCode',
  'CodeText',
  'Frontmatter',
]);

function hasAncestor(state: EditorState, pos: number, names: Set<string>): boolean {
  let node = syntaxTree(state).resolveInner(pos, 1);
  while (node) {
    if (names.has(node.name)) return true;
    node = node.parent!;
  }
  return false;
}

/** True when `pos` sits inside code or frontmatter — no wikilink treatment. */
export function inCodeContext(state: EditorState, pos: number): boolean {
  return hasAncestor(state, pos, CODE_CONTEXTS);
}

const CODE_OR_TABLE = new Set([...CODE_CONTEXTS, 'Table']);

/** Like {@link inCodeContext} but also true inside tables — revealed table
 * source must stay character-exact (hidden brackets would make visual
 * columns lie about source columns). */
export function inCodeOrTableContext(state: EditorState, pos: number): boolean {
  return hasAncestor(state, pos, CODE_OR_TABLE);
}

/**
 * Read the token layer out of `index.css` so a test can assert a token's VALUE.
 *
 * jsdom applies the cascade but does not substitute `var()`, so
 * `getComputedStyle(el).color` on a themed element returns the literal text
 * `var(--color-…)`, never a colour. A test that wants the colour has to follow
 * that name into the stylesheet itself — which is what this module does.
 *
 * It parses; it does not grep. A source-text gate is satisfied by the presence
 * of a literal, so it keeps passing after the value behind the literal is
 * edited into something wrong, and it can never go red for the reason it was
 * written. The contrast, elevation, canvas and wikilink gates all read the
 * stylesheet through here, so there is one parser and no copy to drift.
 */
import { readFileSync } from 'node:fs';
import { resolve as resolvePath } from 'node:path';

/** vitest serves this module over a non-file URL, so the stylesheet is located
 *  from the project root that the config already anchors. */
const CSS = readFileSync(resolvePath(process.cwd(), 'src/index.css'), 'utf8');

/** Pull one brace-balanced block out of the stylesheet by its selector text. */
function block(selector: string): string {
  const start = CSS.indexOf(selector);
  if (start < 0) throw new Error(`no such block in index.css: ${selector}`);
  let depth = 0;
  for (let i = CSS.indexOf('{', start); i < CSS.length; i++) {
    if (CSS[i] === '{') depth++;
    else if (CSS[i] === '}' && --depth === 0) return CSS.slice(start, i);
  }
  throw new Error(`unbalanced block: ${selector}`);
}

/** `--name: value;` declarations, with comments stripped first so a
 *  commented-out token cannot be read as a live one. */
function declarations(source: string): Map<string, string> {
  const clean = source.replace(/\/\*[\s\S]*?\*\//g, '');
  const out = new Map<string, string>();
  for (const m of clean.matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]+);/g)) {
    out.set(m[1], m[2].trim());
  }
  return out;
}

/** Every `--*` the dark theme declares. */
export const darkTokens: Map<string, string> = declarations(block('@theme'));

/** Every `--*` the light theme re-declares. */
export const lightTokens: Map<string, string> = declarations(
  block(":root[data-theme='light']"),
);

/** Follow `var(--other)` to the value it ultimately names. Throws when the
 *  chain ends at a token the theme never declares — which is the failure this
 *  whole module exists to produce. */
export function resolveToken(tokens: Map<string, string>, name: string): string {
  let value = tokens.get(name);
  for (let hops = 0; value && hops < 8; hops++) {
    const ref = /^var\(\s*(--[a-z0-9-]+)\s*\)$/.exec(value);
    if (!ref) return value;
    value = tokens.get(ref[1]);
  }
  if (!value) throw new Error(`undeclared token: ${name}`);
  return value;
}

/** The first `--token` a CSS value references, or null when it references none.
 *  Use it to turn a computed style — jsdom hands back the literal
 *  `var(--color-primary)` — into the token name the cascade selected. */
export function tokenReferenceIn(value: string): string | null {
  return /var\(\s*(--[a-z0-9-]+)/.exec(value)?.[1] ?? null;
}

/** The literal fallback in `var(--token, fallback)`, or null when there is
 *  none. A fallback is a SECOND copy of a value, so a test has to be able to
 *  compare it against the first. */
export function fallbackIn(value: string): string | null {
  return /var\(\s*--[a-z0-9-]+\s*,\s*([^)]+)\)/.exec(value)?.[1].trim() ?? null;
}

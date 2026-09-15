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
 *
 * THE STYLESHEET HAS TWO LEVELS SINCE THE `--cru-*` CONTRACT LANDED.
 * `@layer cru-theme` holds the values, once per theme. `@theme` holds only
 * aliases, each reading a `--cru-*` name, and Tailwind generates the utilities
 * from those. `darkTokens` and `lightTokens` merge both levels, so a caller
 * that asks for `--color-primary` or for `--cru-color-primary` gets an answer
 * either way and `resolveToken` walks the chain to the literal.
 */
import { readFileSync } from 'node:fs';
import { resolve as resolvePath } from 'node:path';

/** vitest serves this module over a non-file URL, so the stylesheet is located
 *  from the project root that the config already anchors. */
const CSS = readFileSync(resolvePath(process.cwd(), 'src/index.css'), 'utf8');

/** Pull one brace-balanced block out of the stylesheet by its selector text,
 *  starting the search at `from`. */
function block(selector: string, from = 0): string {
  const start = CSS.indexOf(selector, from);
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

/** The layer that holds every default, for both themes. */
const CRU_THEME = block('@layer cru-theme {');

/** `:root` is a PREFIX of `:root[data-theme='light']` and appears earlier in
 *  the file besides, so the dark block is located inside the layer rather than
 *  by its selector alone. */
const CRU_THEME_START = CSS.indexOf(CRU_THEME);

/** The `--cru-*` contract, dark. */
export const contractDark: Map<string, string> = declarations(
  block(':root {', CRU_THEME_START),
);

/** The `--cru-*` contract, light. */
export const contractLight: Map<string, string> = declarations(
  block(":root[data-theme='light']", CRU_THEME_START),
);

/** The `@theme` block: the Tailwind-namespace aliases, and nothing else.
 *  Anchored on the newline and the brace, because the word `@theme` also
 *  appears in the comments that explain why the block holds no literal. */
export const themeAliases: Map<string, string> = declarations(block('\n@theme {'));

const merge = (...maps: Map<string, string>[]) =>
  new Map<string, string>(maps.flatMap((m) => [...m]));

/** Every token a dark-theme element resolves against: the contract plus the
 *  aliases that point into it. */
export const darkTokens: Map<string, string> = merge(themeAliases, contractDark);

/** Every token a light-theme element resolves against.
 *
 *  The light block OVERRIDES; it does not replace. `:root[data-theme='light']`
 *  re-declares the tokens that change with the theme and nothing else, so a
 *  radius or a type size still comes from `:root`. Layering the light block
 *  over the dark one is what the browser does, and a map built from the light
 *  block alone would report a radius as undeclared. */
export const lightTokens: Map<string, string> = merge(
  themeAliases,
  contractDark,
  contractLight,
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

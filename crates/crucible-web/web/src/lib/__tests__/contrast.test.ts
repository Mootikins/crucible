import { describe, expect, it } from 'vitest';
import { contrastRatio, contrastRatioHex, parseHex, relativeLuminance } from '../contrast';
import {
  contractDark,
  contractLight,
  darkTokens,
  lightTokens,
  resolveToken,
  themeAliases,
} from '@/test-utils/css-tokens';
import { readFileSync } from 'node:fs';
import { resolve as resolvePath } from 'node:path';

const indexCss = () => readFileSync(resolvePath(process.cwd(), 'src/index.css'), 'utf8');

/** The `@layer cru-theme { … }` block, brace-balanced. */
function cruThemeLayer(): string {
  const css = indexCss();
  const start = css.indexOf('@layer cru-theme {');
  let depth = 0;
  for (let i = css.indexOf('{', start); i < css.length; i++) {
    if (css[i] === '{') depth++;
    else if (css[i] === '}' && --depth === 0) return css.slice(start, i);
  }
  throw new Error('unbalanced @layer cru-theme');
}

/**
 * The WCAG 2.1 AA gate for the token layer.
 *
 * This test PARSES `index.css` and COMPUTES a ratio from the parsed values. It
 * deliberately does not grep the file for a hex string: a source-text gate is
 * satisfied by the presence of the literal, so it keeps passing after someone
 * edits the literal into something illegible, and it can never go red for the
 * reason it was written.
 *
 * The parser is `@/test-utils/css-tokens`, shared with the canvas, wikilink and
 * elevation gates. A second copy here would let the two drift, and a gate that
 * reads a stale stylesheet proves nothing.
 *
 * Break any ramp value below and this file fails with the measured ratio.
 */

const dark = darkTokens;
const light = lightTokens;

interface Theme {
  name: string;
  tokens: Map<string, string>;
}

const THEMES: Theme[] = [
  { name: 'dark', tokens: dark },
  { name: 'light', tokens: light },
];

/** Every surface a panel-level token can render text on. `--color-control` is
 *  NOT here: it is a control fill, and it carries its own, smaller set. */
const PANEL_SURFACES = [
  '--color-shell-bg',
  '--color-shell-panel',
  '--color-surface-base',
  '--color-surface-elevated',
  '--color-surface-overlay',
];

/** Every ink weight is text, so every ink weight owes 4.5:1. */
const INKS = ['--color-shell-ink', '--color-shell-body', '--color-muted', '--color-muted-dark'];

/** Text sits on a control fill too — a placeholder is text. muted-dark is not
 *  in this list because a control fill is too light for it; it is a rule about
 *  which ink a control may use, and the ink values below prove the rule holds. */
const CONTROL_INKS = ['--color-shell-ink', '--color-shell-body', '--color-muted'];

const AA_TEXT = 4.5;
const AA_NON_TEXT = 3;

describe('contrast math', () => {
  it('reproduces the WCAG reference extremes', () => {
    expect(contrastRatioHex('#ffffff', '#000000')).toBeCloseTo(21, 6);
    expect(contrastRatioHex('#ffffff', '#ffffff')).toBeCloseTo(1, 6);
    expect(relativeLuminance({ r: 255, g: 255, b: 255 })).toBeCloseTo(1, 6);
    expect(relativeLuminance({ r: 0, g: 0, b: 0 })).toBeCloseTo(0, 6);
  });

  it('is symmetric and shorthand-aware', () => {
    const a = parseHex('#fff')!;
    const b = parseHex('#000')!;
    expect(contrastRatio(a, b)).toBeCloseTo(contrastRatio(b, a), 12);
    expect(a).toEqual({ r: 255, g: 255, b: 255 });
  });

  it('refuses a colour it cannot read rather than scoring it', () => {
    expect(parseHex('var(--color-primary)')).toBeNull();
    expect(() => contrastRatioHex('rgba(0,0,0,.5)', '#fff')).toThrow();
  });
});

describe('index.css token layer', () => {
  it('declares the same --color-* set in both themes', () => {
    const names = (t: Map<string, string>) =>
      [...t.keys()].filter((k) => k.startsWith('--color-')).sort();
    expect(light).not.toBe(dark);
    expect(names(light)).toEqual(names(dark));
  });

  it('resolves every --color-* token to a colour, in both themes', () => {
    for (const { name, tokens } of THEMES) {
      for (const key of tokens.keys()) {
        if (!key.startsWith('--color-')) continue;
        const value = resolveToken(tokens, key);
        if (value.startsWith('rgba(') || value.startsWith('rgb(')) continue;
        expect(parseHex(value), `${name} ${key} = ${value}`).not.toBeNull();
      }
    }
  });
});

/**
 * The `--cru-*` contract is PUBLIC (docs/Help/Extending/Web Theme Tokens.md).
 * A plugin ships one stylesheet that re-values these names, so a name that
 * exists in one theme and not the other is a page that half-restyles, and a
 * name that changes is a plugin that breaks.
 */
/** A token whose VALUE changes with the theme. Both blocks must declare it. */
const THEMED = ['color', 'shadow'];
/** A token that does not. The light block must NOT re-declare it: a second
 *  copy of a value that never differs is a copy that can only drift. */
const CONSTANT = ['radius', 'row', 'font', 'measure', 'leading'];

describe('the --cru-* contract', () => {

  const namesIn = (t: Map<string, string>, family: string) =>
    [...t.keys()].filter((k) => k.startsWith(`--cru-${family}-`)).sort();

  for (const family of THEMED) {
    it(`declares the same --cru-${family}-* set in both themes`, () => {
      // A per-family assertion, so a failure names WHICH family lost a token.
      // One list of all seven would report a diff nobody can read.
      expect(namesIn(contractLight, family)).toEqual(namesIn(contractDark, family));
      expect(namesIn(contractDark, family).length).toBeGreaterThan(0);
    });
  }

  for (const family of CONSTANT) {
    it(`declares --cru-${family}-* once, in the base theme only`, () => {
      expect(namesIn(contractDark, family).length, `--cru-${family}-*`).toBeGreaterThan(0);
      expect(namesIn(contractLight, family)).toEqual([]);
    });
  }

  it('every @theme entry is an alias, never a literal', () => {
    // Tailwind INLINES what `@theme` holds. A literal there is baked into the
    // utility and can never follow the contract, which is exactly how the
    // shadow ramp painted near-black on the light theme for months.
    const offenders: string[] = [];
    for (const [name, value] of themeAliases) {
      if (!/^var\(\s*--cru-[a-z0-9-]+\s*\)$/.test(value)) offenders.push(`${name}: ${value}`);
    }
    expect(offenders).toEqual([]);
  });

  it('every alias target is declared by the contract', () => {
    // The realistic failure when the aliases are written by hand is a typo,
    // and a typo resolves to nothing rather than to the wrong colour.
    for (const [name, value] of themeAliases) {
      const target = /var\(\s*(--cru-[a-z0-9-]+)\s*\)/.exec(value)?.[1];
      expect(target, `${name} names no contract token`).toBeDefined();
      expect(contractDark.has(target!), `${name} -> ${target}`).toBe(true);
    }
  });

  it('every --cru-* token resolves to a value, in both themes', () => {
    for (const { name, tokens } of THEMES) {
      for (const key of contractDark.keys()) {
        // `resolveToken` throws on a name the theme never declares AND on a
        // name whose chain ends at one, which covers the empty value too.
        expect(() => resolveToken(tokens, key), `${name} ${key}`).not.toThrow();
      }
    }
  });

  it('a token that does not change with the theme reads the same in both', () => {
    // The CONSTANT families are declared once and inherited by the light
    // block. Resolving both themes proves the inheritance still reaches them:
    // a radius, a row height or a type size that answered differently per
    // theme would mean the light block had quietly grown a second copy.
    const constant = [...contractDark.keys()].filter((k) =>
      CONSTANT.some((family) => k.startsWith(`--cru-${family}-`)),
    );
    expect(constant.length).toBeGreaterThan(0);
    for (const key of constant) {
      expect(resolveToken(lightTokens, key), key).toBe(resolveToken(darkTokens, key));
    }
  });

  it('both theme blocks sit inside @layer cru-theme, and nothing escapes it', () => {
    // One --cru-* declaration left unlayered defeats EVERY plugin override,
    // and a parity test cannot see it: the token is still declared twice.
    const layer = cruThemeLayer();
    expect(layer).toContain("[data-theme='light']");
    // Comments go first: the block above this layer explains the mechanism
    // and names half the contract while doing it. And the search is NOT
    // anchored to the start of a line — `:root { --cru-font-title: 13px; }`
    // on one line is an escape too, and a line-anchored pattern misses it.
    const outside = indexCss().replace(layer, '').replace(/\/\*[\s\S]*?\*\//g, '');
    expect(outside).not.toMatch(/--cru-[a-z0-9-]+\s*:/);
  });

  it('the layer is declared before the layers Tailwind owns', () => {
    expect(indexCss()).toMatch(/@layer\s+cru-theme\s*,\s*theme\s*,/);
  });
});

describe('WCAG 2.1 AA — text on a surface (4.5:1)', () => {
  for (const { name, tokens } of THEMES) {
    for (const ink of INKS) {
      for (const surface of PANEL_SURFACES) {
        it(`${name}: ${ink} on ${surface}`, () => {
          const ratio = contrastRatioHex(resolveToken(tokens, ink), resolveToken(tokens, surface));
          expect(ratio, `${ratio.toFixed(2)}:1`).toBeGreaterThanOrEqual(AA_TEXT);
        });
      }
    }
    for (const ink of CONTROL_INKS) {
      it(`${name}: ${ink} on --color-control`, () => {
        const ratio = contrastRatioHex(resolveToken(tokens, ink), resolveToken(tokens, '--color-control'));
        expect(ratio, `${ratio.toFixed(2)}:1`).toBeGreaterThanOrEqual(AA_TEXT);
      });
    }
  }
});

describe('WCAG 2.1 AA — ink on a solid ember fill (4.5:1)', () => {
  for (const { name, tokens } of THEMES) {
    for (const fill of ['--color-primary', '--color-primary-hover']) {
      it(`${name}: --color-on-primary on ${fill}`, () => {
        const ratio = contrastRatioHex(
          resolveToken(tokens, '--color-on-primary'),
          resolveToken(tokens, fill),
        );
        expect(ratio, `${ratio.toFixed(2)}:1`).toBeGreaterThanOrEqual(AA_TEXT);
      });
    }

    it(`${name}: white on --color-primary is the failure this replaced`, () => {
      const ratio = contrastRatioHex('#ffffff', resolveToken(tokens, '--color-primary'));
      if (name === 'dark') expect(ratio).toBeLessThan(AA_TEXT);
      else expect(ratio).toBeGreaterThanOrEqual(AA_TEXT);
    });
  }
});

describe('WCAG 2.1 AA — focus ring is a non-text indicator (3:1)', () => {
  for (const { name, tokens } of THEMES) {
    for (const surface of [...PANEL_SURFACES, '--color-control']) {
      it(`${name}: --color-focus-ring on ${surface}`, () => {
        const ratio = contrastRatioHex(
          resolveToken(tokens, '--color-focus-ring'),
          resolveToken(tokens, surface),
        );
        expect(ratio, `${ratio.toFixed(2)}:1`).toBeGreaterThanOrEqual(AA_NON_TEXT);
      });
    }
  }
});

describe('the ink ramp keeps four distinct steps', () => {
  for (const { name, tokens } of THEMES) {
    it(`${name}: each step is measurably quieter than the one above`, () => {
      const bg = resolveToken(tokens, '--color-shell-bg');
      const ratios = INKS.map((ink) => contrastRatioHex(resolveToken(tokens, ink), bg));
      for (let i = 1; i < ratios.length; i++) {
        // A step that is not at least 1.2x quieter than its neighbour is not a
        // step; the ramp collapsed into one shade.
        expect(ratios[i - 1] / ratios[i], ratios.map((r) => r.toFixed(2)).join(' / ')).toBeGreaterThan(1.2);
      }
    });
  }
});

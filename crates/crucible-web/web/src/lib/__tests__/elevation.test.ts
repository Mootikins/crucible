import { describe, it, expect } from 'vitest';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { resolve as resolvePath, join as joinPath } from 'node:path';
import { darkTokens, lightTokens } from '@/test-utils/css-tokens';
import { menuContent } from '@/components/ui/menu-style';

/**
 * Elevation, in both themes.
 *
 * `--shadow-sm` … `--shadow-2xl` were declared ONCE, at alpha 0.30–0.60 of
 * near-black, and never re-declared for the light theme — so a near-black
 * shadow painted on #edecf2.
 *
 * The fix has two halves and BOTH are load-bearing. Tailwind v4 does not emit
 * `--shadow-*` as custom properties; it inlines the `@theme` value into each
 * utility at build time. Re-declaring the tokens under the light selector is
 * therefore necessary and NOT sufficient — the utilities have to be re-pointed
 * at them. A test that checked only the tokens would be green while the page
 * still painted black, which is the failure this file is written against.
 */

const CSS = readFileSync(resolvePath(process.cwd(), 'src/index.css'), 'utf8');

const STEPS = ['sm', 'md', 'lg', 'xl', '2xl'] as const;

/** `rgba(r, g, b, a)` → its alpha. Throws on anything else, because a shadow
 *  we cannot measure is a shadow we cannot hold to a floor. */
function alphaOf(value: string): number {
  const m = /rgba\(\s*\d+\s*,\s*\d+\s*,\s*\d+\s*,\s*([0-9.]+)\s*\)/.exec(value);
  if (!m) throw new Error(`not an rgba shadow ink: ${value}`);
  return Number(m[1]);
}

/** The blur radius, in px — the second length in `x y blur spread`. */
function blurOf(value: string): number {
  const m = /^\s*-?[\d.]+(?:px)?\s+-?[\d.]+(?:px)?\s+([\d.]+)px/.exec(value);
  if (!m) throw new Error(`no blur radius in: ${value}`);
  return Number(m[1]);
}

describe('shadow tokens exist in BOTH themes', () => {
  for (const step of STEPS) {
    const name = `--shadow-${step}`;

    it(`${name} is declared light as well as dark`, () => {
      expect(darkTokens.get(name), `dark ${name}`).toBeDefined();
      expect(lightTokens.get(name), `light ${name}`).toBeDefined();
    });

    it(`${name} is far quieter on the light theme`, () => {
      const dark = alphaOf(darkTokens.get(name)!);
      const light = alphaOf(lightTokens.get(name)!);
      // A shadow on a #0e0d11 canvas has nowhere left to darken, so it buys
      // separation with opacity. The same alpha on #edecf2 is a grey smear;
      // the light-theme field (Primer, Material 3, IntelliJ) sits at 0.04–0.24.
      expect(light, `${light} vs dark ${dark}`).toBeLessThan(dark / 2);
      expect(light).toBeLessThanOrEqual(0.24);
      expect(light).toBeGreaterThan(0);
    });

    it(`${name} keeps the same geometry in both themes`, () => {
      // Only the ink changes. A light theme that also moved the offsets would
      // be a second elevation system, not the same one re-inked.
      const strip = (v: string) => v.replace(/rgba?\([^)]*\)/, '').trim();
      expect(strip(lightTokens.get(name)!)).toBe(strip(darkTokens.get(name)!));
    });
  }

  it('the light ramp still rises, step by step', () => {
    const alphas = STEPS.map((s) => alphaOf(lightTokens.get(`--shadow-${s}`)!));
    for (let i = 1; i < alphas.length; i++) {
      expect(alphas[i], alphas.join(' / ')).toBeGreaterThan(alphas[i - 1]);
    }
  });
});

describe('the light tokens actually reach the utilities', () => {
  // Tailwind v4 emits `.shadow-lg{--tw-shadow:0 6px 20px -6px var(--tw-shadow-color,#00000080)}`
  // — the value is INLINED. `--tw-shadow-color` is no way in either: it is
  // registered `@property … inherits:false`, so a value set on `:root` never
  // reaches the element that reads it. Re-pointing `--tw-shadow` per utility,
  // under a selector that outranks the bare class, is what is left.
  for (const step of STEPS) {
    it(`.shadow-${step} is re-pointed under the light theme`, () => {
      const rule = new RegExp(
        `:root\\[data-theme='light'\\]\\s+\\.shadow-${step}\\s*\\{[^}]*--tw-shadow:\\s*var\\(--shadow-${step}\\)`,
      );
      expect(CSS).toMatch(rule);
    });
  }

  it('every shadow utility the app uses is covered', () => {
    // A sixth step used in a component with no light rule would paint black.
    const used = new Set(
      [...CSS.matchAll(/--tw-shadow:\s*var\(--shadow-([a-z0-9]+)\)/g)].map((m) => m[1]),
    );
    expect([...used].sort()).toEqual([...STEPS].sort());
  });
});

describe('native controls follow the theme', () => {
  // Scrollbars, form widgets and the caret read `color-scheme`, not our
  // tokens. There was no declaration at all, so a light theme kept dark
  // scrollbars and a dark date picker.
  it('declares color-scheme for dark and for light', () => {
    expect(CSS).toMatch(/:root\s*\{\s*color-scheme:\s*dark;\s*\}/);
    expect(CSS).toMatch(/:root\[data-theme='light'\]\s*\{\s*color-scheme:\s*light;\s*\}/);
  });
});

/** Every shipped component source, for the structural gates below. */
function componentFiles(): string[] {
  const root = resolvePath(process.cwd(), 'src/components');
  const files: string[] = [];
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir)) {
      if (entry.startsWith('.') || entry === '__tests__') continue;
      const p = joinPath(dir, entry);
      if (statSync(p).isDirectory()) walk(p);
      else if (/\.tsx?$/.test(entry)) files.push(p);
    }
  };
  walk(root);
  return files;
}

describe('menu elevation is in proportion to its edge', () => {
  /**
   * One template, mounted everywhere — so this is measured on the template.
   * It was `border-hairline` (the faintest rule in the palette) under
   * `shadow-lg`, a 20px blur: a 20:1 ratio of soft to hard, which is the shape
   * of a panel given a big drop shadow to make up for an edge that does not
   * read.
   */
  const step = /\bshadow-(sm|md|lg|xl|2xl)\b/.exec(menuContent)?.[1];

  it('names a shadow step from the token layer', () => {
    expect(step).toBeDefined();
  });

  it('keeps the blur within 10x the 1px border it sits under', () => {
    const blur = blurOf(darkTokens.get(`--shadow-${step}`)!);
    expect(blur, `${blur}px blur under a 1px border`).toBeLessThanOrEqual(10);
  });

  it('uses an edge that can be seen, not the faintest one', () => {
    expect(menuContent).toMatch(/\bborder-hairline-strong\b/);
  });

  it('is the ONLY menu panel style — no component rolls its own', () => {
    /**
     * The first spelling of this gate matched `class={…}` alone, and passed
     * while TWO components carried a hand-rolled copy of the template in a
     * `class="…"` STRING — both still on the thin border and the wide shadow
     * this change exists to retire. It now reads the whole attribute, either
     * form, and the offender list carries the class text so a failure says
     * what it found.
     */
    const offenders: string[] = [];
    for (const file of componentFiles()) {
      const src = readFileSync(file, 'utf8');
      for (const m of src.matchAll(/<Menu\.Content\b([^>]*)>/g)) {
        const attrs = m[1];
        const cls = /class=(?:\{([^}]*)\}|"([^"]*)")/.exec(attrs);
        // A Menu.Content with no class at all is unstyled, which is its own
        // bug — count it, rather than letting it slip through as "not a copy".
        if (!cls || !(cls[1] ?? cls[2] ?? '').includes('menuContent')) {
          offenders.push(`${file}: ${(cls?.[1] ?? cls?.[2] ?? '(no class)').trim()}`);
        }
      }
    }
    expect(offenders).toEqual([]);
  });

  it('every menu ROW and separator comes from the template too', () => {
    // The two copies duplicated the row and separator strings as well; a
    // template that only owns the panel is two thirds of a template.
    const ROW = 'px-3 py-1.5 cursor-pointer data-[highlighted]:bg-hover-wash';
    const offenders: string[] = [];
    for (const file of componentFiles()) {
      if (file.endsWith('menu-style.ts')) continue;
      const src = readFileSync(file, 'utf8');
      if (src.includes(ROW)) offenders.push(file);
    }
    expect(offenders).toEqual([]);
  });
});

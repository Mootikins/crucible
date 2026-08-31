import { describe, expect, it } from 'vitest';
import { contrastRatio, contrastRatioHex, parseHex, relativeLuminance } from '../contrast';
import { darkTokens, lightTokens, resolveToken } from '@/test-utils/css-tokens';

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

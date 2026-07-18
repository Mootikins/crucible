import { describe, it, expect } from 'vitest';
import { readFileSync, readdirSync, statSync } from 'fs';
import { resolve, join } from 'path';

/**
 * Design-system contract: the 2026-07-17 styling pass migrated every raw
 * Tailwind gray/status palette class to the semantic tokens in index.css
 * and gave structural surfaces mount animations. These tests keep both
 * from regressing — new code must use tokens, not raw palettes.
 */

const SRC = resolve(__dirname, '../..');

function walk(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir)) {
    // test-harness/ is test scaffolding, not shipped UI.
    if (entry === 'node_modules' || entry === 'test-harness' || entry.startsWith('.')) continue;
    const p = join(dir, entry);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (/\.(tsx|ts)$/.test(entry) && !/\.test\.tsx?$/.test(entry)) out.push(p);
  }
  return out;
}

const read = (rel: string) => readFileSync(resolve(SRC, rel), 'utf-8');

describe('semantic color tokens (no raw palettes)', () => {
  // Allowlist, not a denylist: ANY Tailwind palette color-NNN is an offender
  // unless its hue is approved. A denylist that grows one hue at a time gives a
  // false green — whole families (orange, blue, indigo, teal, …) slipped through.
  // sky-* is deliberately allowed: it marks spawned/running agents (the theme
  // has no blue token yet).
  const ALL_TW_COLORS = [
    'slate', 'gray', 'zinc', 'neutral', 'stone', 'red', 'orange', 'amber',
    'yellow', 'lime', 'green', 'emerald', 'teal', 'cyan', 'sky', 'blue',
    'indigo', 'violet', 'purple', 'fuchsia', 'pink', 'rose',
  ];
  const APPROVED_HUES = new Set([
    'neutral', 'zinc', 'gray', 'slate', 'stone', 'emerald', 'green', 'amber',
    'yellow', 'red', 'purple', 'violet', 'sky',
  ]);
  const PALETTE_RE = new RegExp(`\\b(${ALL_TW_COLORS.join('|')})-[0-9]{2,3}\\b`, 'g');

  it('no component uses an off-token Tailwind palette class', () => {
    const offenders: string[] = [];
    // index.html is outside src/ but carries classes too (the body classes
    // hid a neutral-950 canvas behind the shell for months).
    for (const file of [...walk(SRC), resolve(SRC, '../index.html')]) {
      const lines = readFileSync(file, 'utf-8').split('\n');
      lines.forEach((line, i) => {
        for (const m of line.matchAll(PALETTE_RE)) {
          if (!APPROVED_HUES.has(m[1])) {
            offenders.push(`${file}:${i + 1}: ${line.trim()}`);
            break;
          }
        }
      });
    }
    expect(offenders).toEqual([]);
  });

  it('surfaces use tokens, not white-alpha (bg/text/border)', () => {
    const offenders: string[] = [];
    for (const file of walk(SRC)) {
      const src = readFileSync(file, 'utf-8');
      if (/\b(?:bg|text|border)-white\//.test(src)) offenders.push(file);
    }
    expect(offenders).toEqual([]);
  });
});

describe('motion primitives on structural surfaces', () => {
  it('floating windows pop in', () => {
    expect(read('components/windowing/FloatingWindow.tsx')).toMatch(/cru-anim-pop/);
  });

  it('edge panels slide in from their owning edge', () => {
    const src = read('components/windowing/EdgePanel.tsx');
    expect(src).toMatch(/cru-anim-slide-l/);
    expect(src).toMatch(/cru-anim-slide-r/);
    expect(src).toMatch(/cru-anim-slide-b/);
  });

  it('command palette pops in over a fading overlay', () => {
    const src = read('components/CommandPalette.tsx');
    expect(src).toMatch(/cru-anim-pop/);
    expect(src).toMatch(/cru-anim-fade/);
  });

  it('interaction modal fades its backdrop and pops its card', () => {
    const src = read('components/InteractionModal.tsx');
    expect(src).toMatch(/cru-anim-fade/);
    expect(src).toMatch(/cru-anim-pop/);
  });

  it('autocomplete and hover-preview cards rise in', () => {
    expect(read('components/AutocompletePopup.tsx')).toMatch(/cru-anim-rise/);
    expect(read('components/WikilinkHoverPreview.tsx')).toMatch(/cru-anim-rise/);
  });

  it('keyframes animate scale/translate, never transform (would clobber Tailwind transforms)', () => {
    const css = read('index.css');
    const keyframeBlocks = css.match(/@keyframes cru-[\s\S]*?\n\}/g) ?? [];
    expect(keyframeBlocks.length).toBeGreaterThanOrEqual(6);
    for (const block of keyframeBlocks) {
      expect(block).not.toMatch(/transform:/);
    }
  });
});

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
    if (entry === 'node_modules' || entry.startsWith('.')) continue;
    const p = join(dir, entry);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (/\.(tsx|ts)$/.test(entry) && !/\.test\.tsx?$/.test(entry)) out.push(p);
  }
  return out;
}

const read = (rel: string) => readFileSync(resolve(SRC, rel), 'utf-8');

describe('semantic color tokens (no raw palettes)', () => {
  // sky-* is deliberately exempt: it marks spawned/running agents, distinct
  // from precog (delegation), and the theme has no blue token yet.
  const RAW_PALETTE = /\b(?:neutral|zinc|gray|slate|stone|emerald|green|amber|yellow|red|purple|violet)-[0-9]{2,3}\b/;

  it('no component uses a raw Tailwind gray or status palette class', () => {
    const offenders: string[] = [];
    // index.html is outside src/ but carries classes too (the body classes
    // hid a neutral-950 canvas behind the shell for months).
    for (const file of [...walk(SRC), resolve(SRC, '../index.html')]) {
      const lines = readFileSync(file, 'utf-8').split('\n');
      lines.forEach((line, i) => {
        if (RAW_PALETTE.test(line)) offenders.push(`${file}:${i + 1}: ${line.trim()}`);
      });
    }
    expect(offenders).toEqual([]);
  });

  it('separators use the hairline token, not white-alpha borders', () => {
    const offenders: string[] = [];
    for (const file of walk(SRC)) {
      const src = readFileSync(file, 'utf-8');
      if (/border-white\//.test(src)) offenders.push(file);
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

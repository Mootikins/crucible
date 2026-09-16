import { describe, it, expect } from 'vitest';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';

/**
 * The windowing core knows no app. Every import inside it must stay inside
 * the core, or name one of four allowed entries that carry no product
 * knowledge. A grep is not a gate; this test is.
 *
 * An entry that ends in `/` allows every module under that folder. Any other
 * entry allows that one module only.
 */
const ROOT = join(__dirname, '..');
const ALLOWED = ['@/windowing/', '@/lib/cn', '@/lib/icons', '@/components/ui/'];

/** Every module specifier in the source: static, side-effect and dynamic. */
function specifiers(src: string): string[] {
  const out: string[] = [];
  for (const m of src.matchAll(/(?:from\s+|import\s*\(?\s*)['"]([^'"]+)['"]/g)) {
    out.push(m[1]!);
  }
  return out;
}

/** True when a file in the core may import `spec`. */
function allowed(file: string, spec: string): boolean {
  if (spec.startsWith('.')) {
    const target = resolve(dirname(file), spec);
    return target === ROOT || target.startsWith(ROOT + sep);
  }
  if (spec.startsWith('@/')) {
    return ALLOWED.some((a) => (a.endsWith('/') ? spec.startsWith(a) : spec === a));
  }
  // A package import, for example `solid-js`, carries no app knowledge.
  return true;
}

/** The imports of `src` that break the boundary, for a file at `file`. */
function offendersIn(file: string, src: string): string[] {
  return specifiers(src).filter((spec) => !allowed(file, spec));
}

const SRC = join(ROOT, '..');
const TESTING = join(ROOT, 'testing');

/**
 * The imports of `src` that reach the neutral policy, for a file at `file`.
 *
 * `src/windowing/testing/` is test support. The harness page and the tests
 * may import it. A production module that imports it puts the neutral
 * policy into `dist/`.
 */
function testingImportsIn(file: string, src: string): string[] {
  return specifiers(src).filter((spec) => {
    if (spec.startsWith('.')) {
      const target = resolve(dirname(file), spec);
      return target === TESTING || target.startsWith(TESTING + sep);
    }
    return spec === '@/windowing/testing' || spec.startsWith('@/windowing/testing/');
  });
}

/** True for a file that may use test support: tests, the harness, the support itself. */
function isTestSupport(file: string): boolean {
  const parts = relative(SRC, file).split(sep);
  return (
    parts[0] === 'test-harness' ||
    parts.includes('__tests__') ||
    /\.test\.[^.]+$/.test(file) ||
    file.startsWith(TESTING + sep)
  );
}

function walk(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (/\.(ts|tsx)$/.test(name)) out.push(p);
  }
  return out;
}

describe('windowing core boundary', () => {
  it('imports nothing from the app', () => {
    const offenders: string[] = [];
    // This file holds bad imports as sample text for the matcher test, so
    // the walk skips it.
    for (const file of walk(ROOT).filter((f) => f !== __filename)) {
      for (const spec of offendersIn(file, readFileSync(file, 'utf8'))) {
        offenders.push(`${relative(ROOT, file)} -> ${spec}`);
      }
    }
    expect(offenders).toEqual([]);
  });

  it('keeps the neutral policy out of production code', () => {
    const offenders: string[] = [];
    for (const file of walk(SRC).filter((f) => !isTestSupport(f))) {
      for (const spec of testingImportsIn(file, readFileSync(file, 'utf8'))) {
        offenders.push(`${relative(SRC, file)} -> ${spec}`);
      }
    }
    expect(offenders).toEqual([]);
  });

  describe('the testing matcher', () => {
    it.each([
      ['an alias import', join(SRC, 'App.tsx'), `import { neutralPolicy } from '@/windowing/testing/neutralPolicy';`],
      ['a relative import', join(ROOT, 'store', 'probe.ts'), `import { neutralSeed } from '../testing/neutralPolicy';`],
      ['a dynamic import', join(SRC, 'App.tsx'), `const m = await import('@/windowing/testing/neutralPolicy');`],
    ])('flags %s', (_name, file, src) => {
      expect(testingImportsIn(file, src)).toHaveLength(1);
    });

    it('treats tests, the harness and the support folder as test support', () => {
      expect(isTestSupport(join(SRC, 'test-harness', 'windowing-harness.tsx'))).toBe(true);
      expect(isTestSupport(join(ROOT, '__tests__', 'policy.test.ts'))).toBe(true);
      expect(isTestSupport(join(SRC, 'lib', 'x.test.tsx'))).toBe(true);
      expect(isTestSupport(join(TESTING, 'neutralPolicy.ts'))).toBe(true);
      expect(isTestSupport(join(SRC, 'App.tsx'))).toBe(false);
      expect(isTestSupport(join(ROOT, 'store', 'index.ts'))).toBe(false);
    });
  });

  describe('the matcher', () => {
    const file = join(ROOT, 'model', 'probe.ts');

    it.each([
      ['a single-quoted app import', `import { x } from '@/stores/windowStore';`, '@/stores/windowStore'],
      ['a double-quoted app import', `import { x } from "@/stores/windowStore";`, '@/stores/windowStore'],
      ['a relative path that leaves the core', `import { x } from '../../stores/windowStore';`, '../../stores/windowStore'],
      ['a side-effect import', `import '@/index.css';`, '@/index.css'],
      ['a dynamic import', `const m = await import('@/lib/api');`, '@/lib/api'],
      ['a module that only shares a prefix with an allowed one', `import { x } from '@/lib/icons-extra';`, '@/lib/icons-extra'],
    ])('flags %s', (_name, src, spec) => {
      expect(offendersIn(file, src)).toEqual([spec]);
    });

    it('passes allowed imports', () => {
      const src = [
        `import { cn } from '@/lib/cn';`,
        `import { IconX } from "@/lib/icons";`,
        `import { Button } from '@/components/ui/button';`,
        `import type { Tab } from '@/windowing/model/types';`,
        `import { tree } from './tree';`,
        `import { store } from '../store';`,
        `import { createSignal } from 'solid-js';`,
      ].join('\n');
      expect(offendersIn(file, src)).toEqual([]);
    });
  });
});

import { describe, it, expect } from 'vitest';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';

/**
 * The windowing core knows no app. Every `@/` import inside it must name the
 * core itself or one of three leaf modules that carry no product knowledge.
 * A grep is not a gate; this test is.
 */
const ROOT = join(__dirname, '..');
const ALLOWED = ['@/windowing/', '@/lib/cn', '@/lib/icons', '@/components/ui/'];

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
    for (const file of walk(ROOT)) {
      const src = readFileSync(file, 'utf8');
      for (const m of src.matchAll(/from\s+'(@\/[^']+)'/g)) {
        const spec = m[1]!;
        if (!ALLOWED.some((a) => spec.startsWith(a))) {
          offenders.push(`${relative(ROOT, file)} -> ${spec}`);
        }
      }
    }
    expect(offenders).toEqual([]);
  });
});

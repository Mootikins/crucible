// Architecture gate: every Lucide icon enters the app through `@/lib/icons`.
//
// `lucide-solid`'s entry point re-exports 1943 icons. A dev server has no
// bundle, so ONE `from 'lucide-solid'` anywhere makes the browser fetch all
// 1943 as separate modules before the page can paint. Six components imported
// it directly, and a cold load spent 1948 of its 3427 script requests on
// icons to use 100 of them. Routing them through `lib/icons.ts`, which names
// each icon's own module path, cut the load from 3427 requests and 2.0s to
// first render down to 1586 and 1.0s.
//
// The production build tree-shakes the barrel, so this rule buys nothing
// there. It is a rule about how long the app takes to appear while you work
// on it, which is why no bundle-size check would ever catch it coming back.
//
// `optimizeDeps.include` is not an alternative: vite-plugin-solid keeps Solid
// component libraries out of the esbuild pre-bundle, because the JSX
// transform they need does not survive it. Measured — the include changed
// nothing.
//
// When this fails, import the icon from `@/lib/icons` and add it there if it
// is missing. Do not add an entry to this gate; it has no allowlist.
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const SRC_DIR = resolve(process.cwd(), 'src');

/** All `*.ts` / `*.tsx` files under src/, as paths relative to src/. */
function walk(dir: string, rel = ''): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const here = rel ? `${rel}/${entry.name}` : entry.name;
    if (entry.isDirectory()) out.push(...walk(resolve(dir, entry.name), here));
    else if (/\.tsx?$/.test(entry.name)) out.push(here);
  }
  return out;
}

/** `from 'lucide-solid'` — the barrel itself, not `lucide-solid/icons/x`. */
const BARREL_IMPORT = /from\s+['"]lucide-solid['"]/;

/** The one file allowed to name the package: it maps every icon to its path. */
const ICON_BARREL = 'lib/icons.ts';

/** A spec, or a spec under a `__tests__/` dir. Excluded for the same reason
 * the api-ownership gate excludes them: a test may quote the rule it checks,
 * and this one quotes it in its own assertions. A test ships no icon to a
 * browser, so the cost the rule exists to prevent cannot come from here. */
function isTest(rel: string): boolean {
  return /(^|\/)__tests__\//.test(rel) || /\.test\.tsx?$/.test(rel);
}

describe('icon imports', () => {
  it('nothing imports the lucide-solid barrel', () => {
    const offenders = walk(SRC_DIR)
      .filter((f) => f !== ICON_BARREL && !isTest(f))
      .filter((f) => BARREL_IMPORT.test(readFileSync(resolve(SRC_DIR, f), 'utf8')))
      .map((f) => `src/${f} -> from 'lucide-solid'`);
    expect(offenders).toEqual([]);
  });

  it('the barrel file itself imports every icon by its own path', () => {
    const source = readFileSync(resolve(SRC_DIR, ICON_BARREL), 'utf8');
    const deep = source.match(/from 'lucide-solid\/icons\/[a-z0-9-]+'/g) ?? [];
    expect(deep.length).toBeGreaterThan(90);
    // No bare re-export can hide among them.
    expect(BARREL_IMPORT.test(source)).toBe(false);
  });

  // The regex must actually match the thing it forbids, or the gate is theatre.
  it('recognises a barrel import and spares a deep one', () => {
    expect(BARREL_IMPORT.test(`import { X } from 'lucide-solid';`)).toBe(true);
    expect(BARREL_IMPORT.test(`import X from 'lucide-solid/icons/x';`)).toBe(false);
  });
});

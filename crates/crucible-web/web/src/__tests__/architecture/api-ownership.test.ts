// Architecture gates for who may talk to the daemon, enforced as vitest tests.
//
// These scan the src/ source files (test files are excluded — they stub the
// seam, not the rule) to keep one owner for every server request:
//   G2a — no non-test file outside the sanctioned list imports `@/lib/api`,
//         by alias OR by relative path (a rule an import can dodge by writing
//         `../../lib/api` is aspirational, not enforced). Sanctioned:
//         `lib/query/**` (the hooks), `lib/offline/**` (the document layer),
//         and `lib/api.ts` itself.
//   G2b — no non-test file outside `lib/api-client.ts` and `lib/api.ts` calls
//         `fetch(` or constructs `new EventSource`, so a daemon request
//         cannot bypass the generated client. One frozen allowlist entry:
//         the user-configured EXTERNAL transcription endpoint, which is not
//         daemon traffic and has no route to live behind.
//
// When these fail, route the call through a hook in `lib/query/` (or a route
// function in `lib/api.ts`); do not grow the allowlist — it only shrinks.

import { readFileSync, readdirSync } from 'node:fs';
import { dirname, posix, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

// vitest runs with cwd at the web/ project root (where vite.config lives).
const SRC_DIR = resolve(process.cwd(), 'src');

/** All `*.ts` / `*.tsx` files under src/, as paths relative to src/. */
function walk(dir: string, rel = ''): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const here = rel ? `${rel}/${entry.name}` : entry.name;
    if (entry.isDirectory()) out.push(...walk(`${dir}/${entry.name}`, here));
    else if (/\.[jt]sx?$/.test(entry.name)) out.push(here);
  }
  return out;
}

function read(rel: string): string {
  return readFileSync(`${SRC_DIR}/${rel}`, 'utf8');
}

/** A spec, a spec under a `__tests__/` dir, or the generated contract. */
function isTestOrGenerated(rel: string): boolean {
  return (
    /(^|\/)__tests__\//.test(rel) ||
    /\.test\.[jt]sx?$/.test(rel) ||
    rel === 'lib/api-schema.d.ts'
  );
}

/** Modules that may import the api module: the hooks, the document layer. */
const SANCTIONED_IMPORTERS = ['lib/query/', 'lib/offline/', 'lib/api.ts'];

/** The import specifiers of a file that name the api module, alias or relative. */
function apiImports(rel: string, src: string): string[] {
  const specs = [...src.matchAll(/(?:from\s+|import\s*\(\s*)['"]([^'"]+)['"]/g)].map(
    (m) => m[1]!,
  );
  return specs.filter((spec) => {
    if (spec.startsWith('@/')) return spec.slice(2) === 'lib/api';
    if (!spec.startsWith('.')) return false;
    const target = posix.normalize(posix.join(dirname(rel), spec)).replace(/\.ts$/, '');
    return target === 'lib/api';
  });
}

/** A call of `fetch(` — `refetch(`/`prefetch(` are different words, not matches. */
const FETCH_CALL = /\bfetch\(/;
/** A construction of the browser's SSE client. */
const EVENT_SOURCE = /\bnew\s+EventSource\b/;

// UNIQUE: no eslint config exists in web/ (no eslint dep in package.json); eslint-plugin-import's no-restricted-paths would be the alternative but is not wired in. The vitest source-scan is the only enforcement of the one-owner rule.

describe('api ownership discipline', () => {
  const SOURCE_FILES = walk(SRC_DIR)
    .filter((f) => !isTestOrGenerated(f))
    .sort();

  it('finds the source tree it polices', () => {
    // The gate is inert if the walk goes nowhere; `lib/api.ts` is the one file
    // every rule here names, so it is the witness that the walk found src/.
    expect(SOURCE_FILES, 'expected to scan src/').toContain('lib/api.ts');
    expect(SOURCE_FILES).toContain('components/AuthTokenPrompt.tsx');
  });

  // -- G2a: the api module is imported only through the query layer ---------
  it('only lib/query, lib/offline and lib/api itself import the api module', () => {
    const offenders = SOURCE_FILES.flatMap((f) => {
      if (SANCTIONED_IMPORTERS.some((s) => f === s || f.startsWith(s))) return [];
      const found = apiImports(f, read(f));
      return found.length ? [`${f} -> ${found.join(', ')}`] : [];
    });
    expect(
      offenders,
      `A component imports \`@/lib/api\` instead of reading through the query ` +
        `layer. Take values from a hook in \`lib/query/\`, moved helpers from ` +
        `their new module, or add the route call to \`lib/api.ts\` and wrap it ` +
        `in a hook:\n${offenders.join('\n')}`,
    ).toEqual([]);
  });

  // -- G2b: no daemon request outside the client and the api module ---------
  //
  // Frozen: the transcription endpoint is the app's one HTTP call to a server
  // it did not build — a user-configured Whisper-compatible URL, not a
  // crucible route, so it has no place in `lib/api.ts`. Entries may only be
  // removed (by the endpoint gaining a route), never added.
  const FETCH_ALLOWLIST: Record<string, string> = {
    'lib/transcription.ts': 'external transcription endpoint (not a crucible route)',
  };
  it('no fetch or EventSource outside lib/api-client and lib/api', () => {
    const offenders = SOURCE_FILES.flatMap((f) => {
      if (f === 'lib/api.ts' || f === 'lib/api-client.ts' || f in FETCH_ALLOWLIST) {
        return [];
      }
      const src = read(f);
      const found: string[] = [];
      if (FETCH_CALL.test(src)) found.push(`${f} -> fetch(`);
      if (EVENT_SOURCE.test(src)) found.push(`${f} -> new EventSource`);
      return found;
    });
    expect(
      offenders,
      `A daemon request bypassed the generated client. Route it through ` +
        `\`lib/api-client.ts\`, or make it a route call in \`lib/api.ts\`:\n` +
        offenders.join('\n'),
    ).toEqual([]);
  });


  it.each([
    ['components/Probe.tsx', `import { login } from '@/lib/api';`],
    ['components/Probe.tsx', `import type { Surface } from '@/lib/api';`],
    ['components/Probe.tsx', `import { login } from '../lib/api';`],
    ['components/blocks/Probe.tsx', `import { login } from '../../lib/api';`],
    ['lib/layout-persistence.ts', `import { saveLayout } from './api';`],
    ['lib/query/probe.ts', `import { login } from '../api';`],
    ['components/Probe.tsx', `export { login } from '@/lib/api';`],
    ['components/Probe.tsx', `const mod = await import('@/lib/api');`],
  ])('the import rule refuses %s in %s', (rel, src) => {
    expect(apiImports(rel, src), `${rel}: ${src}`).toHaveLength(1);
  });
  it('the import rule passes the neighbours and the sanctioned', () => {
    const src = [
      `import { client } from '@/lib/api-client';`,
      `import { login } from '@/lib/query/auth';`,
      `import { turnResponseId } from '@/lib/turn';`,
      `import { rawFileUrl } from './paths';`,
      `import { Session } from '../types';`,
    ].join('\n');
    expect(apiImports('lib/query/probe.ts', src)).toEqual([]);
    expect(apiImports('components/Probe.tsx', src)).toEqual([]);
  });

  it('the transport rules flag their calls and pass their neighbours', () => {
    expect(FETCH_CALL.test(`const r = await fetch(url);`)).toBe(true);
    expect(FETCH_CALL.test(`void plugins.refetch();`)).toBe(false);
    expect(FETCH_CALL.test(`await client.fetchQuery(['k'], fn);`)).toBe(false);
    expect(EVENT_SOURCE.test(`const s = new EventSource(url);`)).toBe(true);
    expect(EVENT_SOURCE.test(`const s = new FakeEventSource(url);`)).toBe(false);
  });

  it('no test mocks the api module: routes, not module doubles', () => {
    // G7: a test that replaces `@/lib/api` cannot see the route a hook
    // calls, so it re-states the wire in a fixture nobody checks. Tests
    // answer routes through the fetch seam (`test-utils/mock-fetch.ts`) or
    // an injected QueryClient; pure helpers are mocked at their own module
    // (`@/lib/turn`, `@/lib/paths`). Comment lines that say
    // `No \`vi.mock('@/lib/api')\`` are the tombstones of retired mocks —
    // they are not calls.
    const TEST_FILES = walk(SRC_DIR).filter((f) => isTestOrGenerated(f));
    const offenders = TEST_FILES.flatMap((f) => {
      const calls = read(f)
        .split('\n')
        .map((l) => l.trim())
        .filter((l) => !l.startsWith('//'))
        .filter((l) => /vi\.mock\(\s*['"](@\/lib\/api|\.[./]*api)['"]/.test(l));
      return calls.length ? [`${f} -> ${calls.length} vi.mock block(s)`] : [];
    });
    expect(
      offenders,
      `A test doubles \`@/lib/api\` instead of answering the route. Use the ` +
        `fetch seam or an injected QueryClient:\n${offenders.join('\n')}`,
    ).toEqual([]);
  });
});

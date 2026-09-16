// Architecture gates for the Playwright e2e suite, enforced as vitest tests.
//
// These scan the e2e/ source files (they do not run Playwright — vitest
// excludes e2e/ from execution) to keep the disciplined tiers honest:
//   A5a — no arbitrary sleeps (`page.waitForTimeout`) outside a frozen legacy
//         allowlist; the story and live tiers must never use them.
//   A5b — story specs locate by role/label/text/testid, never by raw CSS
//         class, with a justified CodeMirror exception.
//
// When these fail, fix the spec (wait on a condition, locate by role), do not
// grow the allowlist — it only shrinks.

import { readFileSync, readdirSync } from 'node:fs';
import { dirname, posix, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

// vitest runs with cwd at the web/ project root (where vite.config lives).
const E2E_DIR = resolve(process.cwd(), 'e2e');

/** All `*.spec.ts` / `*.ts` files under a dir, as paths relative to e2e/. */
function walk(dir: string, rel = ''): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const childRel = rel ? `${rel}/${entry.name}` : entry.name;
    if (entry.isDirectory()) {
      out.push(...walk(`${dir}/${entry.name}`, childRel));
    } else if (entry.name.endsWith('.ts')) {
      out.push(childRel);
    }
  }
  return out;
}

function read(rel: string): string {
  return readFileSync(`${E2E_DIR}/${rel}`, 'utf8');
}

const ALL_E2E_FILES = walk(E2E_DIR).sort();

describe('e2e architecture discipline', () => {
  // -- A5a: ban arbitrary sleeps ------------------------------------------
  //
  // Legacy flat specs still using waitForTimeout. Frozen: entries may only be
  // removed (by switching to a condition wait), never added. The story and
  // live tiers are intentionally absent — they must stay sleep-free.
  const WAIT_FOR_TIMEOUT_ALLOWLIST = new Set([
    'empty-state.spec.ts',
    'session-lifecycle.spec.ts',
  ]);

  // UNIQUE: no eslint config exists in web/ (no eslint dep in package.json); eslint-plugin-playwright's no-wait-for-timeout would be the alternative but is not wired in. The vitest source-scan is the only enforcement of the shrinking allowlist invariant.
  it('no new page.waitForTimeout sleeps in e2e/', () => {
    const offenders = ALL_E2E_FILES.filter(
      (f) => !WAIT_FOR_TIMEOUT_ALLOWLIST.has(f) && /\bwaitForTimeout\b/.test(read(f)),
    );
    expect(
      offenders,
      `Replace arbitrary sleeps with condition waits (expect.poll / ` +
        `toBeVisible / waitFor). Do not add to the allowlist — it only shrinks.\n` +
        offenders.join('\n'),
    ).toEqual([]);
  });

  // UNIQUE: tier-specific ban (stories/ and live/) needs file-path globs; without eslint wired in, the vitest source-scan is the only mechanism keeping the story/live tiers sleep-free.
  it('story and live tiers never sleep', () => {
    const offenders = ALL_E2E_FILES.filter(
      (f) => (f.startsWith('stories/') || f.startsWith('live/')) && /\bwaitForTimeout\b/.test(read(f)),
    );
    expect(offenders, `Story/live specs must wait on conditions, not timeouts:\n${offenders.join('\n')}`).toEqual(
      [],
    );
  });

  // -- Core windowing specs know no app -----------------------------------
  //
  // e2e/windowing/ drives the harness page, which mounts the window manager
  // with no app and no server. The specs there may import only the list
  // below: Playwright, the harness driver, another module in e2e/windowing/
  // and the pure geometry helper. Any other import can bring app knowledge.
  // A route mock is not an import, so a second rule bans `page.route(`.
  const CORE_IMPORTS = new Set(['@playwright/test', 'windowing/harness', 'helpers/geometry']);
  const ROUTE_MOCK = /page\.route\(/;

  /** The imports of a file at `rel` (relative to e2e/) that the allow-list refuses. */
  function refusedImports(rel: string, src: string): string[] {
    const specs = [...src.matchAll(/(?:from\s+|import\s*\(?\s*)['"]([^'"]+)['"]/g)].map((m) => m[1]!);
    return specs.filter((spec) => {
      if (!spec.startsWith('.')) return !CORE_IMPORTS.has(spec);
      const target = posix.normalize(posix.join(dirname(rel), spec)).replace(/\.ts$/, '');
      return !(CORE_IMPORTS.has(target) || target.startsWith('windowing/'));
    });
  }

  it('core windowing specs import only the allowed modules and mock no route', () => {
    const coreFiles = ALL_E2E_FILES.filter((f) => f.startsWith('windowing/'));
    expect(coreFiles.length, 'expected to find core windowing specs').toBeGreaterThan(0);
    const offenders = coreFiles.flatMap((f) => {
      const src = read(f);
      const found = refusedImports(f, src).map((spec) => `${f} -> ${spec}`);
      return ROUTE_MOCK.test(src) ? [...found, `${f} -> page.route(`] : found;
    });
    expect(
      offenders,
      `Core windowing specs open the harness. Move an app rule to an app spec in e2e/:\n` +
        offenders.join('\n'),
    ).toEqual([]);
  });

  it.each([
    [`import { setupBasicMocks } from '../helpers/mock-api';`],
    [`import { MOCK_SESSION } from '../helpers/fixtures';`],
    [`import { appReady } from '../helpers/nav';`],
    [`import { windowStore } from '../../src/windowing';`],
    [`import { readFileSync } from 'node:fs';`],
    [`const m = await import('../stories/helpers');`],
  ])('the core import allow-list refuses %s', (src) => {
    expect(refusedImports('windowing/probe.spec.ts', src)).toHaveLength(1);
  });

  it('the core import allow-list passes the allowed modules', () => {
    const src = [
      `import { test, expect, type Page } from '@playwright/test';`,
      `import { act, openHarness } from './harness';`,
      `import { steps } from './shared/steps';`,
      `import { getCenterOf } from '../helpers/geometry';`,
    ].join('\n');
    expect(refusedImports('windowing/probe.spec.ts', src)).toEqual([]);
  });

  it('the route mock rule flags page.route', () => {
    expect(ROUTE_MOCK.test(`await page.route('**/api/layout', handler);`)).toBe(true);
  });

  // -- A5b: story specs use semantic locators -----------------------------
  //
  // CodeMirror (`.cm-*`) and xterm.js (`.xterm`) render their editor/terminal
  // surfaces with framework classes and expose no roles/testids on the
  // rendered layer; those are the only allowed raw-class locators in a story
  // spec (assert `.xterm`/`.cm-*` is present to prove the surface mounted).
  const CM_LOCATOR = /locator\((['"])\.(cm-[^'"]*|xterm[^'"]*)\1/;
  // Any `locator('.foo')` or `locator('[class...]')`.
  const RAW_CLASS_LOCATOR = /locator\((['"])(?:\.[A-Za-z_-]|\[class)/g;

  // UNIQUE: no off-the-shelf eslint rule bans raw CSS class locators in Playwright specs (the CodeMirror/xterm carve-out requires a custom predicate). The vitest source-scan is the only enforcement of the semantic-locator invariant.
  it('story specs locate by role/label/text/testid, not raw CSS class', () => {
    const storyFiles = ALL_E2E_FILES.filter((f) => f.startsWith('stories/'));
    expect(storyFiles.length, 'expected to find story specs to scan').toBeGreaterThan(0);

    const offenders: string[] = [];
    for (const f of storyFiles) {
      const src = read(f);
      for (const match of src.matchAll(RAW_CLASS_LOCATOR)) {
        const snippet = src.slice(match.index, match.index! + 40);
        if (CM_LOCATOR.test(snippet)) continue; // justified CodeMirror internals
        offenders.push(`${f}: ${snippet.split('\n')[0]}`);
      }
    }
    expect(
      offenders,
      `Story specs must use getByRole/getByLabel/getByText/getByTestId. Raw CSS ` +
        `class locators are banned (CodeMirror .cm-* excepted):\n${offenders.join('\n')}`,
    ).toEqual([]);
  });
});

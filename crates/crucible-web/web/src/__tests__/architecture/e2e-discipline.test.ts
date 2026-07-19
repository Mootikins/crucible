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
import { resolve } from 'node:path';
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
    'center-resize.spec.ts',
    'cross-zone-dnd.spec.ts',
    'empty-state.spec.ts',
    'session-lifecycle.spec.ts',
    'tab-reorder.spec.ts',
    'windowing-comprehensive.spec.ts',
  ]);

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

  it('story and live tiers never sleep', () => {
    const offenders = ALL_E2E_FILES.filter(
      (f) => (f.startsWith('stories/') || f.startsWith('live/')) && /\bwaitForTimeout\b/.test(read(f)),
    );
    expect(offenders, `Story/live specs must wait on conditions, not timeouts:\n${offenders.join('\n')}`).toEqual(
      [],
    );
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

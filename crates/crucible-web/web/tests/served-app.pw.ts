import { test, expect, type Locator, type Page } from '@playwright/test';
import { writeFileSync } from 'node:fs';
import path from 'node:path';
import { readState } from '../e2e/live/_state';

/**
 * The built bundle AS THE RUST SERVER HANDS IT OVER — the only tier where the
 * security headers exist at all.
 *
 * Why this file exists: every other Playwright suite runs against the Vite dev
 * server (`baseURL: http://localhost:5273`, mocked backend). That server emits
 * no Content-Security-Policy, no `nosniff` and no `Referrer-Policy`, and never
 * touches an axum route — so a CSP that blocks a stylesheet, a worker or an
 * image passes the whole suite green while the shipped product is broken. That
 * is not hypothetical: both suites passed unchanged while every finding of the
 * hardening review reproduced by hand against a real `cru web`, and the one
 * break they could have caught (kiln SVGs) they missed because no fixture uses
 * an `.svg`.
 *
 * So what is asserted here is deliberately the PRODUCT contract, not the header
 * one: a `securitypolicyviolation` anywhere fails, and the two things the
 * hardening pass actually broke — an SVG served out of the kiln, and the inline
 * offsets KaTeX positions accents with — have to visibly paint.
 *
 * Skips cleanly when globalSetup found no `cru` binary.
 */

const state = readState();

/** A CSP violation, flattened to what a failure message needs. */
type Violation = { directive: string; blocked: string };

declare global {
  interface Window {
    __cspViolations?: Violation[];
  }
}

/**
 * Record every CSP violation the page reports, from before the first byte of
 * app script runs. `securitypolicyviolation` is the only reliable signal: a
 * blocked stylesheet, worker or image raises no exception and no failed
 * request, it just silently does not happen.
 */
async function armCspRecorder(page: Page): Promise<void> {
  await page.addInitScript(() => {
    window.__cspViolations = [];
    document.addEventListener('securitypolicyviolation', (e) => {
      window.__cspViolations?.push({ directive: e.violatedDirective, blocked: e.blockedURI });
    });
  });
}

async function cspViolations(page: Page): Promise<Violation[]> {
  return page.evaluate(() => window.__cspViolations ?? []);
}

/** Load the served app with the recorder armed and wait for the shell. */
async function openApp(page: Page): Promise<void> {
  await armCspRecorder(page);
  await page.goto(state.baseURL!);
  await expect(page.getByTestId('ribbon-cmd-palette')).toBeVisible({ timeout: 20_000 });
}

/**
 * Write `body` (plus a small, script-free `diagram.svg` beside it) into the
 * live kiln, then open it in the real editor's reading view — the product path
 * that runs renderMarkdownDocAsync and the inline-CSS filter.
 */
async function readingViewOf(page: Page, name: string, body: string): Promise<Locator> {
  const kilnDir = state.kilnDir!;
  writeFileSync(
    path.join(kilnDir, 'diagram.svg'),
    '<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20">' +
      '<rect width="40" height="20" fill="#4488ff"/></svg>\n',
  );
  const notePath = path.join(kilnDir, `${name}.md`);
  writeFileSync(notePath, body);

  await openApp(page);
  await page.evaluate(
    ({ filePath, fileName }) => {
      window.dispatchEvent(
        new CustomEvent('crucible:open-file', { detail: { path: filePath, name: fileName } }),
      );
    },
    { filePath: notePath, fileName: `${name}.md` },
  );
  await page.getByTestId('preview-toggle').click();
  const preview = page.getByTestId('markdown-preview');
  await expect(preview).toBeVisible({ timeout: 10_000 });
  return preview;
}

test.describe('the served app (real cru web, built bundle)', () => {
  test.skip(state.skip, `live tier unavailable: ${state.reason ?? ''}`);

  test('boots from the Rust server under its own CSP with no violation', async ({ page }) => {
    await armCspRecorder(page);
    const response = await page.goto(state.baseURL!);
    const headers = response!.headers();

    // These exist on the served document and nowhere in the mock tier.
    expect(headers['content-security-policy']).toContain("default-src 'self'");
    expect(headers['x-content-type-options']).toBe('nosniff');
    expect(headers['referrer-policy']).toBeTruthy();

    // …and the app boots under that policy. A CSP is only correct if the
    // product still works: script, styles, fonts and workers all had to load.
    await expect(page.getByTestId('ribbon-cmd-palette')).toBeVisible({ timeout: 20_000 });
    expect(await cspViolations(page)).toEqual([]);
  });

  test('paints an SVG embedded from the kiln', async ({ page }) => {
    const preview = await readingViewOf(page, 'RenderedSvg', '# Svg\n\n![diagram](diagram.svg)\n');

    // `naturalWidth` is the honest check: a download-forced
    // `application/octet-stream`, a nosniff type mismatch and a CSP-blocked
    // request all leave the <img> in the DOM with naturalWidth 0.
    const img = preview.locator('img');
    await expect(img).toHaveAttribute('src', /\/api\/file\/raw\?path=/);
    await expect
      .poll(() => img.evaluate((el: HTMLImageElement) => el.complete && el.naturalWidth), {
        timeout: 10_000,
      })
      .toBeGreaterThan(0);
    expect(await cspViolations(page)).toEqual([]);
  });

  test('paints a KaTeX accent over its base glyph', async ({ page }) => {
    const preview = await readingViewOf(page, 'RenderedMath', '# Math\n\nThe vector $\\vec{v}$.\n');

    // katex.min.css makes `.accent-body` a ZERO-WIDTH `position:relative` box;
    // the inline `left` is the entire mechanism that slides the arrow back over
    // the `v`. Strip it from the sanitizer's allowlist and the arrow paints
    // beside the glyph instead of on it — a wrong formula, silently.
    await expect(preview.locator('.katex')).toBeVisible();
    const geometry = await preview.locator('.katex-accent .accent-body').evaluate((el) => {
      const style = getComputedStyle(el);
      const arrow = el.querySelector('svg')!.getBoundingClientRect();
      // The base glyph is the other vlist row of the same accent box.
      const base = el
        .closest('.katex-accent')!
        .querySelector('.mord.mathnormal')!
        .getBoundingClientRect();
      return {
        position: style.position,
        left: parseFloat(style.left),
        arrow: { left: arrow.left, right: arrow.right, width: arrow.width },
        base: { left: base.left, right: base.right },
      };
    });

    expect(geometry.position).toBe('relative'); // so `left` is not inert
    expect(geometry.left).toBeLessThan(0); // the offset survived sanitizing
    expect(geometry.arrow.width).toBeGreaterThan(0); // the arrow paints at all
    // Centred on the glyph, not parked off its edge: without the inline `left`
    // the arrow's centre lands |left| px further right, which this bounds out.
    const arrowCentre = (geometry.arrow.left + geometry.arrow.right) / 2;
    const baseCentre = (geometry.base.left + geometry.base.right) / 2;
    expect(Math.abs(arrowCentre - baseCentre)).toBeLessThan(Math.abs(geometry.left));

    expect(await cspViolations(page)).toEqual([]);
  });
});

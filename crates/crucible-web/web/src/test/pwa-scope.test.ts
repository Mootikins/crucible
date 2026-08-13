import { describe, expect, it } from 'vitest';
import { pwaOptions } from '@/pwa-options';

/**
 * A service worker's cache is a same-origin-writable store, so whatever the
 * worker answers is whatever a compromised page can make it answer — for every
 * later load, not just the one that was attacked. The worker must therefore
 * answer as little as possible: the one route this app actually has.
 *
 * Root SCOPE itself is unavoidable (the app is a single document at `/`; a
 * worker scoped deeper controls no client at all) — see the note on
 * `pwaOptions`. What is testable, and what these pin, is the request surface
 * under that scope.
 */

const workbox = pwaOptions.workbox;

/** workbox's navigation-fallback decision, verbatim: allowlist (when present)
 * must match and denylist (when present) must not. */
function servesCachedShell(pathname: string): boolean {
  const allowlist = workbox.navigateFallbackAllowlist as readonly RegExp[] | undefined;
  const denylist = (workbox as { navigateFallbackDenylist?: readonly RegExp[] })
    .navigateFallbackDenylist;
  if (allowlist && !allowlist.some((re) => re.test(pathname))) return false;
  if (denylist && denylist.some((re) => re.test(pathname))) return false;
  return true;
}

describe('service worker request surface', () => {
  it('serves the cached shell for the app route and nothing else', () => {
    expect(servesCachedShell('/')).toBe(true);

    // Every one of these is a path the worker must NOT shadow: an unknown path
    // is either a server route (now or later) or an attacker's deep link, and
    // answering it from cache turns one XSS into a permanent one on that URL.
    for (const path of [
      '/api/chat/stream',
      '/api/file/raw',
      '/api',
      '/health',
      '/.well-known/security.txt',
      '/anything',
      '/notes/some-note',
      '//evil.example/',
    ]) {
      expect(servesCachedShell(path), `must not serve cached shell for ${path}`).toBe(false);
    }
  });

  it('defines no runtime caching, so nothing else is intercepted', () => {
    expect((workbox as { runtimeCaching?: unknown[] }).runtimeCaching).toBeUndefined();
  });

  it('waits for the user before activating a new worker', () => {
    // A worker that skips waiting swaps the bundle under an in-flight turn.
    expect(workbox.skipWaiting).toBe(false);
    expect(pwaOptions.registerType).toBe('prompt');
  });
});

/**
 * `id` is what a browser uses to decide whether an install is THIS app or a new
 * one. Absent, it defaults to `start_url`, which quietly makes `start_url`
 * load-bearing forever: change it later — a mobile shell, a different landing
 * route — and every existing install becomes a second app with its own icon,
 * storage and notification identity, un-migratable remotely.
 */
describe('install identity', () => {
  it('pins an explicit manifest id, independent of start_url', () => {
    expect(pwaOptions.manifest.id).toBe('/');
  });

  it('agrees with index.html about theme-color', async () => {
    // Two sources for one colour, and they had drifted (#0e0d11 vs #0a0a0a).
    // The browser uses the manifest once installed and the meta tag before that,
    // so a mismatch shows as the title bar changing colour on install.
    // Read from the vitest root rather than `import.meta.url`: under the test
    // transform that URL is not a file: URL.
    const fs = await import('node:fs/promises');
    const html = await fs.readFile(`${process.cwd()}/index.html`, 'utf8');
    const meta = html.match(/name="theme-color"\s+content="([^"]+)"/)?.[1];
    expect(meta).toBe(pwaOptions.manifest.theme_color);
  });
});

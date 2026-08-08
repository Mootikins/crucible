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

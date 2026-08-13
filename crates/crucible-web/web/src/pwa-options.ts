/**
 * Service worker + manifest options for `vite.config.ts`.
 *
 * They live under `src/` so the worker's request surface is covered by the
 * typechecker and the unit suite that cover the app (`vite.config.ts` is in a
 * referenced tsconfig project that `tsc -p tsconfig.json` does not check, and a
 * test cannot import across that boundary). Nothing in the app imports this, so
 * it is not in the bundle. Assertions: `src/test/pwa-scope.test.ts`.
 *
 * SCOPE. A worker's scope is what makes it dangerous: everything under it is
 * served through the worker's caches, and a cache is a store any same-origin
 * script can write — so an XSS that reaches one outlives the page that carried
 * it. There is no narrower scope available here: the app is a single document
 * at `/` with no client-side router, so a worker scoped anywhere deeper would
 * control no client at all and the precache and update prompt would be dead
 * code. Root scope is required.
 *
 * What IS narrowed is the surface the worker answers under that scope. It
 * serves the cached shell for exactly the one route the app has, so it does not
 * shadow the rest of the origin (`/.well-known/…`, whatever the Axum server
 * grows later), and it defines no runtimeCaching, so nothing else is
 * intercepted.
 *
 * The residual that root scope carries — a poisoned precache entry surviving
 * reload — is not closable from here; it is bounded by the two controls that
 * keep hostile script off this origin in the first place: `/api/file/raw` must
 * never return content the browser will execute (otherwise an attacker
 * registers their OWN worker and no XSS is needed at all), and the CSP must
 * keep `script-src` to same-origin.
 */
export const pwaOptions = {
  // 'prompt': never yank the page out from under an in-flight turn.
  // A new deploy surfaces as a notification; the update applies when
  // the user reloads (registerSW onNeedRefresh in index.tsx).
  registerType: 'prompt',
  // Dev flow stays untouched: no SW or manifest in `bun run dev`.
  devOptions: { enabled: false },
  // A debug build ships a SW that UNREGISTERS itself and drops the
  // precache, rather than simply omitting one.
  //
  // `prompt` is right in production but ruinous on a rebuild loop: the
  // precache keeps serving the previous bundle until someone accepts the
  // update toast, so a freshly deployed change is invisible and looks like
  // a bug in the change. Omitting the SW would not help either — one
  // already registered in the browser keeps serving the old assets forever.
  // Self-destructing is what actually rescues a browser that has one.
  selfDestroying: process.env.VITE_DISABLE_PWA === '1',
  manifest: {
    name: 'Crucible',
    short_name: 'Crucible',
    description: 'Knowledge-grounded agent runtime',
    // Pinned EXPLICITLY, and worth the line: an absent `id` defaults to
    // `start_url`, which makes `start_url` load-bearing forever. Change it later
    // — a mobile shell, a different landing route — and every existing install
    // becomes a SECOND app rather than an update, with its own icon, its own
    // storage and its own notification identity. Setting it before anyone
    // installs costs nothing; setting it after costs a migration that cannot be
    // done remotely.
    id: '/',
    start_url: '/',
    scope: '/',
    display: 'standalone',
    // neutral-950 — matches the dark UI (`bg-neutral-950` on <body>)
    theme_color: '#0a0a0a',
    background_color: '#0a0a0a',
    icons: [
      { src: '/pwa-192x192.png', sizes: '192x192', type: 'image/png' },
      { src: '/pwa-512x512.png', sizes: '512x512', type: 'image/png' },
      {
        src: '/pwa-maskable-512x512.png',
        sizes: '512x512',
        type: 'image/png',
        purpose: 'maskable',
      },
    ],
  },
  workbox: {
    // Precache the app shell only. The transformers ONNX runtime (~23 MB of
    // `.wasm`) is excluded by extension, not by size, and loads from network
    // exactly as before.
    globPatterns: ['**/*.{js,css,html,svg,png,ico,woff2}'],
    // The shell is a single chunk and has grown past workbox's 2 MiB
    // per-asset default, at which point workbox skips it and the app has no
    // offline shell at all — the one thing this precache exists for. The
    // default is a generic heuristic about accidentally caching huge vendor
    // blobs; here it would drop precisely the asset we mean to cache. 3 MiB
    // keeps the shell in and still refuses anything genuinely oversized.
    maximumFileSizeToCacheInBytes: 3 * 1024 * 1024,
    // Activation waits for the user: registerType 'prompt' only sends
    // SKIP_WAITING when they accept the update toast, so a deploy never
    // reloads a page mid-turn (autoUpdate did exactly that — killing
    // in-flight drafts/streams). The toast keeps stale-forever at bay.
    skipWaiting: false,
    clientsClaim: true,
    cleanupOutdatedCaches: true,
    // The app is ONE document at `/` (no client-side router), so that is the
    // only navigation the shell fallback may answer. An allowlist, not a
    // denylist: a worker that answers every navigation shadows the whole
    // origin — every path the server serves now or later comes back as the
    // cached shell, out of a cache any same-origin script can write. Anything
    // unlisted goes to the network, which is also what keeps /api/* (SSE chat
    // streams included) untouched. generateSW adds no other fetch routes
    // because we define no runtimeCaching, so this is the worker's entire
    // request surface.
    navigateFallback: '/index.html',
    navigateFallbackAllowlist: [/^\/$/],
  },
};

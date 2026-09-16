/* @refresh reload */
import { render } from 'solid-js/web';
import App from './App';
// Geist before index.css (see the note there: CSS @imports of fontsource
// under tailwind v4 lose their font assets; JS imports emit them correctly).
//
// ONE file per family, not one per weight: these are VARIABLE builds with a
// `wght` axis of 100-900, so every weight the app asks for comes out of the
// same woff2. The four static Plex Sans weights plus two Plex Mono weights
// became two requests, and the in-between weights the static set could not
// reach (450, 550) are now available.
import '@fontsource-variable/geist';
import '@fontsource-variable/geist-mono';
// KaTeX math styling; Vite bundles the woff2 fonts its url()s reference.
import 'katex/dist/katex.min.css';
import './index.css';
import { initializeHighlighter } from '@/lib/shiki';
import { initTheme } from '@/lib/theme';
import { installSessionEventRoute } from '@/lib/query/routes/session';
import { installSurfaceEventRoute } from '@/lib/query/routes/surfaces';
import { installFsEventRoute } from '@/lib/query/routes/fs';

// Before any pane opens a stream: a route turns an event into the cache write
// it owes every pane. A pane that subscribes first would otherwise carry its
// own fold only, and the session list, the history, the pending interactions,
// the surface roster and the folder listings would stay as they were read.
installSessionEventRoute();
installSurfaceEventRoute();
installFsEventRoute();

const root = document.getElementById('root');

if (!root) {
  throw new Error('Root element not found');
}

// Fire-and-forget: kick off Shiki download in parallel with first paint.
// DiffViewer reads the reactive `highlighter()` accessor and markdown awaits
// initializeHighlighter(); both fall back to plain text until the promise
// resolves, so this is non-blocking. Surface init failures (offline, corrupt
// WASM, etc.) so diffs/markdown silently degrading to plain text doesn't go
// unnoticed.
void initializeHighlighter().catch((err) => {
  console.error('Shiki highlighter init failed:', err);
});

// PWA service worker: production builds only. The virtual module is emitted
// by vite-plugin-pwa at build time; the PROD guard keeps dev and vitest from
// ever touching it. Prompt registration: a new deploy must never reload the
// page mid-turn, so updates surface as a notification and apply when the
// user clicks it (or on their next manual reload).
if (import.meta.env.PROD) {
  void import('virtual:pwa-register')
    .then(({ registerSW }) => {
      const updateSW = registerSW({
        immediate: true,
        onNeedRefresh() {
          // Actionable notifications never auto-dismiss, so a stale bundle
          // can't silently outlive its 5s toast.
          void import('@/stores/notificationStore').then(({ notificationActions }) => {
            notificationActions.addNotification('info', 'A new version of Crucible is available.', {
              label: 'Reload & update',
              run: () => void updateSW(true),
            });
          });
        },
      });
    })
    .catch((err) => {
      console.error('Service worker registration failed:', err);
    });
}

// Before the first paint: a stored light theme must not flash dark, and a
// light-set machine must not open on dark just because dark is the
// attribute-absent default. The disposer is dropped on purpose — the OS
// watcher lives as long as the document.
initTheme();

render(() => <App />, root);

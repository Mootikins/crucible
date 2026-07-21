/* @refresh reload */
import { render } from 'solid-js/web';
import App from './App';
// IBM Plex before index.css (see the note there: CSS @imports of fontsource
// under tailwind v4 lose their font assets; JS imports emit them correctly).
import '@fontsource/ibm-plex-sans/400.css';
import '@fontsource/ibm-plex-sans/500.css';
import '@fontsource/ibm-plex-sans/600.css';
import '@fontsource/ibm-plex-sans/700.css';
import '@fontsource/ibm-plex-mono/400.css';
import '@fontsource/ibm-plex-mono/500.css';
import './index.css';
import { initializeHighlighter } from '@/lib/shiki';

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
          void import('@/stores/notificationStore').then(({ notificationActions }) => {
            notificationActions.addNotification(
              'info',
              'Update available — click here or reload to apply',
            );
          });
          const apply = () => {
            window.removeEventListener('crucible:apply-sw-update', apply);
            void updateSW(true);
          };
          window.addEventListener('crucible:apply-sw-update', apply);
        },
      });
    })
    .catch((err) => {
      console.error('Service worker registration failed:', err);
    });
}

render(() => <App />, root);
